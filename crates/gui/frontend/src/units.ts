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
 * # Which system, and who decides
 *
 * Two settings, resolved by {@link resolveUnitSystem}:
 *
 * - an app-wide **default** (Settings), a module-level store persisted to
 *   localStorage — `source`, `si`, or `us`;
 * - an optional per-project **override** in the project's `meta.json`,
 *   beside `source_crs`, because it is the same kind of decision: how to
 *   read this model, not a fact about it.
 *
 * `source` means "whatever system the model's own file declares", which is
 * also what reports use — so it is the least surprising default and the one
 * that makes the Canvas and the Report tab agree.
 *
 * The *resolved* system depends on the active project, so
 * {@link useUnitSystem} reads it from a context that `ProjectPage` provides.
 * Outside a project (Settings, Home) there is no model to follow and the
 * app-wide default resolves on its own.
 *
 * Deliberately project-scoped and never scenario-scoped: switching scenario
 * chips is the app's one built-in comparison, and units differing between
 * them would show a 3.28× jump as though the model had changed.
 */

import { createContext, useContext, useSyncExternalStore } from "react";

/** A resolved system — what every conversion in this module takes. */
export type UnitSystem = "si" | "us";

/**
 * A *chosen* system, which may defer to the model.
 *
 * Distinct from {@link UnitSystem} because `source` is not a system: it is
 * a rule for picking one, exactly as a theme setting of "system" is not a
 * theme. Conversions only ever see the resolved value.
 */
export type UnitPreference = "source" | "si" | "us";

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

// ── App-wide default ─────────────────────────────────────────────────────────

const STORAGE_KEY = "hydra2-unit-system";

/**
 * The stored default.
 *
 * The key predates `source` and held `"si"` / `"us"`, both of which are
 * still valid preferences — so an existing choice migrates by being read
 * unchanged. Only the *absence* of a stored value now means something
 * different: it used to mean SI, and now means "follow the model", which is
 * the answer someone who never opened Settings was most likely wanting.
 */
function readStored(): UnitPreference {
  try {
    if (typeof localStorage !== "undefined") {
      const v = localStorage.getItem(STORAGE_KEY);
      if (v === "us" || v === "si" || v === "source") return v;
    }
  } catch {
    // localStorage unavailable (tests, privacy mode) — fall through.
  }
  return "source";
}

let current: UnitPreference = readStored();
const listeners = new Set<() => void>();

export function getUnitPreference(): UnitPreference {
  return current;
}

export function setUnitPreference(pref: UnitPreference): void {
  if (pref === current) return;
  current = pref;
  try {
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // Persistence is best-effort.
  }
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** The app-wide default preference — what Settings edits. */
export function useUnitPreference(): UnitPreference {
  return useSyncExternalStore(subscribe, getUnitPreference, getUnitPreference);
}

// ── Resolution ───────────────────────────────────────────────────────────────

/**
 * Which system to display in, from the three inputs that decide it.
 *
 * A named function because it is the decision the whole feature turns on,
 * and because two of its rules are easy to get subtly wrong:
 *
 * - a project override of `null` means **follow the default**, which is not
 *   the same as an override that happens to equal the default. The first
 *   tracks a later change in Settings; the second pins against one. That
 *   distinction is the entire difference between the menu's two `Source`
 *   entries.
 * - `source` with no model to read — a project with no network yet, an
 *   engine that declares no units, or before the fetch resolves — falls
 *   back to SI rather than guessing US. Values are stored in SI, so SI is
 *   the reading that converts nothing.
 */
export function resolveUnitSystem(
  projectOverride: UnitPreference | null | undefined,
  appDefault: UnitPreference,
  modelSystem: UnitSystem | null | undefined,
): UnitSystem {
  const chosen = projectOverride ?? appDefault;
  if (chosen === "source") return modelSystem ?? "si";
  return chosen;
}

/**
 * The resolved system for the active project, supplied by `ProjectPage`.
 *
 * `null` outside a project: {@link useUnitSystem} then resolves the app-wide
 * default alone, which is all that Settings and Home can mean by it.
 */
export const ResolvedUnitSystem = createContext<UnitSystem | null>(null);

/** Current display-unit system; re-renders the caller when it changes. */
export function useUnitSystem(): UnitSystem {
  const resolved = useContext(ResolvedUnitSystem);
  const appDefault = useUnitPreference();
  return resolved ?? resolveUnitSystem(null, appDefault, null);
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
  volume: 35.314667, // m³ → ft³ (an INP's volumes are cubic feet)
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
  volume: "ft³",
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
        return 0; // ft³
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
 * Convert + format an SI value as a bare number string — no unit label.
 *
 * Used by table cells whose column header already carries the unit (via
 * {@link unitLabel}), so the cell shows just the number and the edit pre-fill
 * is directly parseable. Both systems round to a fixed number of decimals —
 * `decimals` when given, otherwise {@link defaultDecimals}.
 */
export function formatQtyValue(
  v: number,
  q: Quantity,
  sys: UnitSystem,
  decimals?: number,
): string {
  // Both unit systems get a fixed decimal count. SI used to fall through to
  // raw number-to-string, which produced ragged columns in the editor tables
  // — "42.21" above "42.4" above "42.07", right-aligned so nothing lined up.
  // A fixed count is what makes a numeric column scannable.
  const d = decimals ?? defaultDecimals(q, sys);
  if (sys === "si") return v.toFixed(d);
  return toDisplay(v, q, sys).toFixed(d);
}

/**
 * Format a raw `[COORDINATES]` value for a table cell.
 *
 * Coordinates are not a unit `Quantity` — they are plain numbers in whatever
 * CRS the project declares — but they still need a fixed decimal count, or a
 * right-aligned column renders "6594418.2" next to "6594308.45" with nothing
 * lined up. Two decimals is well inside the precision of any projected CRS
 * Hydra supports and comfortably sub-millimetre for degrees.
 */
export function formatCoordValue(v: number): string {
  return Number.isFinite(v) ? v.toFixed(2) : "";
}

/**
 * Format a WGS84 position for display, latitude first: `42.36789, -71.05673`.
 *
 * Five decimals is ~1.1 m of latitude — finer than the click that produced the
 * point, and short enough to sit two of them side by side. `formatCoordValue`
 * cannot be reused: its two decimals are sub-millimetre in the projected CRS
 * units it was written for, but in degrees they are over a kilometre.
 */
export function formatLatLng(lng: number, lat: number): string {
  if (!Number.isFinite(lng) || !Number.isFinite(lat)) return "—";
  return `${lat.toFixed(5)}, ${lng.toFixed(5)}`;
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

// ── Numeric input parsing ───────────────────────────────────────────────────

/** Result of parsing user-typed numeric input. */
export type NumericInput =
  | { kind: "number"; value: number }
  | { kind: "empty" }
  | { kind: "invalid" };

/** Strict number with an optional trailing unit token: the numeric part must
 * be a complete well-formed number, and the suffix (if any) must be a plain
 * unit-ish run with no digits — so "8.62 m" and "1e3" parse, while prefix
 * salvage like "8F.6G2Y" (which parseFloat happily reads as 8) is rejected. */
const NUMERIC_WITH_UNIT =
  /^([+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)\s*([a-zA-Zµ°%²³/·]*)$/;

/**
 * Parse user input for a numeric field. Values pasted with a display unit
 * ("8.62 m", "300mm") normalise to their number; interleaved garbage is
 * `invalid` rather than prefix-parsed; whitespace-only input is `empty`
 * (callers treat it as an abandoned edit, not a zero).
 */
export function parseNumericInput(raw: string): NumericInput {
  const trimmed = raw.trim();
  if (trimmed === "") return { kind: "empty" };
  const m = NUMERIC_WITH_UNIT.exec(trimmed);
  if (!m) return { kind: "invalid" };
  const value = Number(m[1]);
  if (!Number.isFinite(value)) return { kind: "invalid" };
  return { kind: "number", value };
}

// ── Byte sizes ────────────────────────────────────────────────────────────────

/**
 * Human-readable file size, e.g. "84 bytes", "12.4 MB".
 *
 * Decimal units (1 kB = 1000 bytes) to match what macOS and Windows report,
 * so a figure shown here agrees with the one in Finder or Explorer rather
 * than being ~7% smaller. Unit-system independent: bytes are bytes.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 bytes";
  if (bytes < 1000) return `${Math.round(bytes)} bytes`;
  const units = ["kB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  // One decimal below 10 (12.4 MB reads better than 12 MB); none above, where
  // the extra digit is noise.
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}
