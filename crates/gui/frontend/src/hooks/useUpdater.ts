/**
 * In-app self-update flow (tauri-plugin-updater).
 *
 * On first use the hook asks the backend whether this install can
 * self-update at all (`updater_supported` — false in dev builds and on
 * Linux deb/rpm installs, which update via their package manager), then
 * polls the updater endpoint once per app session; Settings can trigger
 * additional checks via `checkNow`. State lives in a module-level store so
 * a download keeps its progress when pages unmount and remount.
 *
 * Phases: idle → checking → (upToDate | checkFailed | available) →
 * downloading → ready (restart), with error as a retryable side exit from
 * the download. The home page renders only the actionable phases
 * (available onward); Settings surfaces the passive ones too.
 *
 * Dev QA: set `localStorage["hydra2-updater-mock"] = "9.9.9"` to simulate
 * an available update; install animates fake progress and restart clears
 * the key and reloads. No plugin call is made in mock mode.
 */

import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useSyncExternalStore } from "react";
import { tryInvokeOr } from "./ipc";

const MOCK_KEY = "hydra2-updater-mock";

export type UpdaterState =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "upToDate" }
  | { phase: "checkFailed"; message: string }
  | { phase: "available"; version: string }
  | { phase: "downloading"; version: string; percent: number | null }
  | { phase: "ready"; version: string }
  | { phase: "installing"; version: string }
  /** Installed, but the automatic restart did not happen. The update *is*
   * applied — only a manual restart is outstanding. */
  | { phase: "installedNeedsRestart"; version: string }
  | { phase: "error"; version: string; message: string };

// ── Pure helpers (unit-tested) ───────────────────────────────────────────────

/** Whole-number download percentage, clamped to 0–100; null when the total
 * is unknown (indeterminate progress). */
export function downloadPercent(
  downloaded: number,
  total: number | null,
): number | null {
  if (total === null || !Number.isFinite(total) || total <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((downloaded / total) * 100)));
}

/** Validate a raw mock-marker value into a version string ("9.9.9" style);
 * null for absent or malformed values so a stray key can't fake states. */
export function mockUpdateVersion(raw: string | null): string | null {
  if (raw === null) return null;
  return /^\d+(\.\d+){0,2}$/.test(raw.trim()) ? raw.trim() : null;
}

// ── Module-level store ───────────────────────────────────────────────────────

let state: UpdaterState = { phase: "idle" };
/** Whether this install can self-update at all; null until known. Settings
 * hides its updates row unless this is true. */
let supported: boolean | null = null;
const listeners = new Set<() => void>();

function setState(next: UpdaterState): void {
  state = next;
  for (const l of listeners) l();
}

function setSupported(next: boolean): void {
  supported = next;
  for (const l of listeners) l();
}

function subscribe(onChange: () => void): () => void {
  listeners.add(onChange);
  return () => {
    listeners.delete(onChange);
  };
}

function readMockVersion(): string | null {
  try {
    if (typeof localStorage === "undefined") return null;
    return mockUpdateVersion(localStorage.getItem(MOCK_KEY));
  } catch {
    return null;
  }
}

/** The plugin's Update handle for the pending real update (never set in
 * mock mode). Kept outside state — it is not renderable data. */
let pendingUpdate: Update | null = null;
let checkStarted = false;

async function runCheck(): Promise<void> {
  // Never clobber an in-flight check or an update mid-install.
  if (
    state.phase === "checking" ||
    state.phase === "downloading" ||
    state.phase === "ready" ||
    state.phase === "installing" ||
    state.phase === "installedNeedsRestart"
  ) {
    return;
  }

  const mock = readMockVersion();
  if (mock !== null) {
    // Drop any handle from an earlier real check: `install()` picks its branch
    // by whether this is set, so a stale one would run the real installer for a
    // version that does not exist.
    pendingUpdate = null;
    setSupported(true);
    setState({ phase: "available", version: mock });
    return;
  }

  if (supported === null) {
    setSupported(
      await tryInvokeOr<boolean>("updater_supported", undefined, false),
    );
  }
  if (!supported) return;

  setState({ phase: "checking" });
  try {
    const update = await check();
    if (update) {
      pendingUpdate = update;
      setState({ phase: "available", version: update.version });
    } else {
      setState({ phase: "upToDate" });
    }
  } catch (err) {
    // Offline or endpoint unavailable. The home page renders nothing for
    // this phase; Settings shows the message next to its retry button.
    setState({
      phase: "checkFailed",
      message: err instanceof Error ? err.message : String(err),
    });
  }
}

async function checkOnce(): Promise<void> {
  if (checkStarted) return;
  checkStarted = true;
  await runCheck();
}

async function install(): Promise<void> {
  if (state.phase !== "available" && state.phase !== "error") return;
  const version = state.version;

  if (pendingUpdate === null) {
    // Mock mode: animate fake progress so the full flow is QA-able.
    for (let percent = 0; percent <= 100; percent += 20) {
      setState({ phase: "downloading", version, percent });
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
    setState({ phase: "ready", version });
    return;
  }

  setState({ phase: "downloading", version, percent: null });
  let total: number | null = null;
  let downloaded = 0;
  try {
    // Download only. `downloadAndInstall` also runs the installer, and on
    // Windows that installer is launched with NSIS `/R` (restart) and the
    // plugin then calls `process::exit(0)` — so the app was replaced and
    // relaunched the moment the download finished, and the "ready" phase and
    // its "Restart to update" button were unreachable there. Installing is
    // deferred to `restart()`, which is what the user actually clicks.
    await pendingUpdate.download((event) => {
      if (event.event === "Started") {
        total = event.data.contentLength ?? null;
        downloaded = 0;
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        setState({
          phase: "downloading",
          version,
          percent: downloadPercent(downloaded, total),
        });
      }
    });
    setState({ phase: "ready", version });
  } catch (err) {
    setState({
      phase: "error",
      version,
      message: err instanceof Error ? err.message : String(err),
    });
  }
}

async function restart(): Promise<void> {
  if (state.phase !== "ready") return;
  const version = state.version;
  if (pendingUpdate === null) {
    // Mock mode: drop the marker and reload in place.
    try {
      localStorage.removeItem(MOCK_KEY);
    } catch {
      // Best-effort.
    }
    window.location.reload();
    return;
  }
  // Marks the phase before awaiting: the button stays visible throughout the
  // install, and a second click would run the installer a second time.
  setState({ phase: "installing", version });
  try {
    // Install swaps the bundle. On Windows it hands off to the NSIS installer
    // and exits the process, so nothing below runs there — the installer
    // restarts the app itself.
    await pendingUpdate.install();
  } catch (err) {
    setState({
      phase: "error",
      version,
      message: err instanceof Error ? err.message : String(err),
    });
    return;
  }
  try {
    await relaunch();
  } catch {
    // The install already succeeded — only the restart did not happen. Calling
    // this an error would tell the user the update failed when it is applied
    // and waiting for them to reopen the app.
    setState({ phase: "installedNeedsRestart", version });
  }
}

// ── Hook ─────────────────────────────────────────────────────────────────────

/** Self-update state plus the user actions. `install` also serves as the
 * retry action from the error phase; `checkNow` re-polls the endpoint on
 * demand (Settings) and is a no-op while checking or mid-install. */
export function useUpdater(): {
  updater: UpdaterState;
  supported: boolean | null;
  install: () => void;
  restart: () => void;
  checkNow: () => void;
} {
  const updater = useSyncExternalStore(subscribe, () => state);
  const supportedNow = useSyncExternalStore(subscribe, () => supported);

  useEffect(() => {
    void checkOnce();
  }, []);

  return {
    updater,
    supported: supportedNow,
    install: useCallback(() => {
      void install();
    }, []),
    restart: useCallback(() => {
      void restart();
    }, []),
    checkNow: useCallback(() => {
      void runCheck();
    }, []),
  };
}
