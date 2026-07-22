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

/** Render `err` (usually a Tauri command error string or an `Error`) as
 *  concise human-readable text for toasts/status lines. */
export function formatIpcError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

type IpcErrorHandler = (cmd: string, err: unknown) => void;

let ipcErrorHandler: IpcErrorHandler | null = null;

/**
 * Register a handler for *real* backend errors hit by the otherwise-silent
 * `tryInvoke` (i.e. we are inside a Tauri shell and the command rejected —
 * not the expected "plain browser dev server" case). Lets the app shell
 * surface failures like a corrupted app-data DB instead of rendering them
 * as empty data. Returns an unregister function; the latest registration
 * wins (single-handler by design — the app shell owns user notification).
 */
export function onIpcError(handler: IpcErrorHandler): () => void {
  ipcErrorHandler = handler;
  return () => {
    if (ipcErrorHandler === handler) ipcErrorHandler = null;
  };
}

/** Silent variant — returns `null` instead of throwing. Use for read-only
 *  data fetches where absence of data is an acceptable fallback.
 *
 *  Outside a Tauri shell (plain `vite` dev server) this resolves `null`
 *  silently — expected. Inside Tauri, a rejected command is a real backend
 *  error: it still resolves `null` so callers don't crash, but the error is
 *  reported to the `onIpcError` handler so the UI can surface it. */
export async function tryInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri()) return null;
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (err) {
    // Surface in dev tools and notify the app shell, but don't crash
    // callers — they return null.
    // eslint-disable-next-line no-console
    console.warn(`[ipc] ${cmd} failed:`, err);
    ipcErrorHandler?.(cmd, err);
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
