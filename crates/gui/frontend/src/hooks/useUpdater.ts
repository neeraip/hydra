/**
 * In-app self-update flow (tauri-plugin-updater).
 *
 * One check per app session: on first use the hook asks the backend whether
 * this install can self-update at all (`updater_supported` — false in dev
 * builds and on Linux deb/rpm installs, which update via their package
 * manager), then polls the updater endpoint once. State lives in a
 * module-level store so a download keeps its progress when the home page
 * unmounts and remounts.
 *
 * Phases: idle → available → downloading → ready (restart), with error as
 * a retryable side exit. `idle` covers "no update", "unsupported", and
 * "check failed" alike — the UI simply shows nothing.
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
  | { phase: "available"; version: string }
  | { phase: "downloading"; version: string; percent: number | null }
  | { phase: "ready"; version: string }
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
const listeners = new Set<() => void>();

function setState(next: UpdaterState): void {
  state = next;
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

async function checkOnce(): Promise<void> {
  if (checkStarted) return;
  checkStarted = true;

  const mock = readMockVersion();
  if (mock !== null) {
    setState({ phase: "available", version: mock });
    return;
  }

  const supported = await tryInvokeOr<boolean>(
    "updater_supported",
    undefined,
    false,
  );
  if (!supported) return;

  try {
    const update = await check();
    if (update) {
      pendingUpdate = update;
      setState({ phase: "available", version: update.version });
    }
  } catch {
    // Offline or endpoint unavailable — stay idle; next app launch retries.
  }
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
    await pendingUpdate.downloadAndInstall((event) => {
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
  try {
    await relaunch();
  } catch (err) {
    setState({
      phase: "error",
      version: state.version,
      message: err instanceof Error ? err.message : String(err),
    });
  }
}

// ── Hook ─────────────────────────────────────────────────────────────────────

/** Self-update state plus the two user actions. `install` also serves as
 * the retry action from the error phase. */
export function useUpdater(): {
  updater: UpdaterState;
  install: () => void;
  restart: () => void;
} {
  const updater = useSyncExternalStore(subscribe, () => state);

  useEffect(() => {
    void checkOnce();
  }, []);

  return {
    updater,
    install: useCallback(() => {
      void install();
    }, []),
    restart: useCallback(() => {
      void restart();
    }, []),
  };
}
