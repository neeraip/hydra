/**
 * Thin wrapper around `@tauri-apps/api/core` `invoke()`.
 *
 * In a Tauri-hosted window, `invoke<T>(cmd, args)` calls the Rust command
 * registered via `tauri::generate_handler![...]`. In a plain browser dev
 * server (`vite` only, no Tauri shell), `invoke` rejects with a useful
 * error — `tryInvoke` catches that case and returns `null`, allowing each
 * `data/` hook to return null without breaking the dev loop.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Silent variant — swallows errors and returns `null`. Use for read-only
 *  data fetches where absence of data is an acceptable fallback. */
export async function tryInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri()) return null;
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (err) {
    // Surface in dev tools but don't crash callers — they return null.
    // eslint-disable-next-line no-console
    console.warn(`[ipc] ${cmd} failed:`, err);
    return null;
  }
}

/** Throwing variant — propagates backend errors to the caller. Use for
 *  commands where the error message must reach the UI (e.g. run_simulation). */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) throw new Error("Not running inside Tauri");
  return tauriInvoke<T>(cmd, args);
}
