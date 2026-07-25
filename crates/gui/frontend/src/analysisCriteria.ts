/**
 * Analysis criteria — user-configurable thresholds that define compliance in
 * the Results view (currently the minimum service pressure).
 *
 * The value is a module-level store persisted to localStorage and exposed to
 * React via {@link useMinPressure} (useSyncExternalStore), mirroring `units.ts`.
 * It is stored in SI (metres) — the same unit the backend analytics command
 * takes — and converted to the display system only at the input/label edges.
 */

import { useSyncExternalStore } from "react";

const STORAGE_KEY = "hydra2-min-pressure-m";
/** EPANET/AWWA-typical minimum service pressure, ~20 psi. */
export const DEFAULT_MIN_PRESSURE_M = 14;

function readInitial(): number {
  try {
    if (typeof localStorage !== "undefined") {
      const v = localStorage.getItem(STORAGE_KEY);
      if (v !== null) {
        const n = Number(v);
        if (Number.isFinite(n) && n >= 0) return n;
      }
    }
  } catch {
    // localStorage unavailable (tests, privacy mode) — fall through.
  }
  return DEFAULT_MIN_PRESSURE_M;
}

let minPressureM = readInitial();
const listeners = new Set<() => void>();

export function getMinPressure(): number {
  return minPressureM;
}

export function setMinPressure(m: number): void {
  const next = Number.isFinite(m) && m >= 0 ? m : DEFAULT_MIN_PRESSURE_M;
  if (next === minPressureM) return;
  minPressureM = next;
  try {
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEY, String(next));
  } catch {
    // Ignore persistence failures.
  }
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** React hook: the current minimum-pressure criterion in SI metres. */
export function useMinPressure(): number {
  return useSyncExternalStore(subscribe, getMinPressure, getMinPressure);
}
