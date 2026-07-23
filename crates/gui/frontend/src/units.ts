/**
 * Display-unit system for the GUI.
 *
 * The engine, every IPC payload, and all persisted files (INP, CSV, GeoJSON)
 * are ALWAYS in SI units. This module is purely a presentation layer: values
 * are converted at the render boundary with {@link toDisplay} /
 * {@link formatQty}, and user-entered values are converted back to SI with
 * {@link fromDisplay} before being staged or patched. Stored/staged values
 * must never be mutated to display units.
 *
 * The selected system is a module-level store persisted to localStorage and
 * exposed to React via {@link useUnitSystem} (useSyncExternalStore) — no
 * context/provider needed.
 */

import { useSyncExternalStore } from "react";

export type UnitSystem = "si" | "us";

/** Physical quantities the GUI displays. `demand` ≡ `flow` and
 * `elevation`/`head` ≡ `length` numerically, but they are kept distinct so
 * call sites stay self-documenting. */
export type Quantity =
  | "length"
  | "elevation"
  | "head"
  | "diameter"
  | "flow"
  | "velocity"
  | "pressure"
  | "headloss"
  | "volume"
  | "demand";

// ── Store ────────────────────────────────────────────────────────────────────

const STORAGE_KEY = "hydra2-unit-system";

function readStored(): UnitSystem {
  try {
    if (typeof localStorage !== "undefined") {
      const v = localStorage.getItem(STORAGE_KEY);
      if (v === "us" || v === "si") return v;
    }
  } catch {
    // localStorage unavailable (tests, privacy mode) — fall through.
  }
  return "si";
}

let current: UnitSystem = readStored();
const listeners = new Set<() => void>();

export function getUnitSystem(): UnitSystem {
  return current;
}

export function setUnitSystem(sys: UnitSystem): void {
  if (sys === current) return;
  current = sys;
  try {
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEY, sys);
  } catch {
    // Persistence is best-effort.
  }
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Current display-unit system; re-renders the caller when it changes. */
export function useUnitSystem(): UnitSystem {
  return useSyncExternalStore(subscribe, getUnitSystem, getUnitSystem);
}

// ── Conversion ───────────────────────────────────────────────────────────────

/**
 * SI → US multiplication factor per quantity.
 *
 * headloss is the deliberate odd one out: the SI unit is m/km and the US unit
 * is ft/kft. Both are length-per-1000-lengths, so the ratio is dimensionless
 * and numerically identical — only the label changes, the value does not
 * (factor 1.0).
 */
const SI_TO_US: Record<Quantity, number> = {
  length: 3.28084, // m → ft
  elevation: 3.28084, // m → ft
  head: 3.28084, // m → ft
  diameter: 0.0393701, // mm → in
  flow: 15.850323, // L/s → gpm
  demand: 15.850323, // L/s → gpm
  velocity: 3.28084, // m/s → ft/s
  pressure: 1.4219702, // m (head) → psi
  headloss: 1.0, // m/km → ft/kft (numerically identical, see above)
  volume: 264.172, // m³ → gal
};

const SI_LABEL: Record<Quantity, string> = {
  length: "m",
  elevation: "m",
  head: "m",
  diameter: "mm",
  flow: "L/s",
  demand: "L/s",
  velocity: "m/s",
  pressure: "m",
  headloss: "m/km",
  volume: "m³",
};

const US_LABEL: Record<Quantity, string> = {
  length: "ft",
  elevation: "ft",
  head: "ft",
  diameter: "in",
  flow: "gpm",
  demand: "gpm",
  velocity: "ft/s",
  pressure: "psi",
  headloss: "ft/kft",
  volume: "gal",
};

/** Convert a stored SI value to the given display system. */
export function toDisplay(v: number, q: Quantity, sys: UnitSystem): number {
  return sys === "us" ? v * SI_TO_US[q] : v;
}

/** Convert a user-entered display value back to SI for storage/patching. */
export function fromDisplay(v: number, q: Quantity, sys: UnitSystem): number {
  return sys === "us" ? v / SI_TO_US[q] : v;
}

/** Unit label for the quantity in the given display system. */
export function unitLabel(q: Quantity, sys: UnitSystem): string {
  return sys === "us" ? US_LABEL[q] : SI_LABEL[q];
}

/** Sensible default decimal places per quantity and system. */
export function defaultDecimals(q: Quantity, sys: UnitSystem): number {
  if (sys === "us") {
    switch (q) {
      case "diameter":
        return 2; // in
      case "flow":
      case "demand":
        return 1; // gpm
      case "pressure":
        return 1; // psi
      case "velocity":
        return 2; // ft/s
      case "volume":
        return 0; // gal
      default:
        return 1; // ft, ft/kft
    }
  }
  switch (q) {
    case "diameter":
      return 0; // mm
    case "flow":
    case "demand":
      return 2; // L/s
    case "velocity":
      return 2; // m/s
    case "volume":
      return 0; // m³
    default:
      return 1; // m, m/km
  }
}

/** Convert + format an SI value with its display unit label appended. */
export function formatQty(
  v: number,
  q: Quantity,
  sys: UnitSystem,
  decimals?: number,
): string {
  const d = decimals ?? defaultDecimals(q, sys);
  return `${toDisplay(v, q, sys).toFixed(d)} ${unitLabel(q, sys)}`;
}

/**
 * Like {@link formatQty}, but in SI the raw value is passed through with no
 * rounding — used where the UI previously rendered the model's own precision
 * (`${value} m`) and that rendering must be preserved.
 */
export function formatQtyRaw(v: number, q: Quantity, sys: UnitSystem): string {
  if (sys === "si") return `${v} ${unitLabel(q, sys)}`;
  return formatQty(v, q, sys);
}

/**
 * Measure-tool distance readout: m/km in SI, ft/mi in US customary.
 * Input is always metres.
 */
export function formatDistance(m: number, sys: UnitSystem): string {
  if (sys === "us") {
    const ft = m * 3.28084;
    if (ft < 5280) return `${ft.toFixed(0)} ft`;
    return `${(m / 1609.344).toFixed(2)} mi`;
  }
  if (m < 1000) return `${m.toFixed(0)} m`;
  return `${(m / 1000).toFixed(2)} km`;
}
