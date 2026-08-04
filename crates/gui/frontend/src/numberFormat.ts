/**
 * How many digits to show.
 *
 * A decimal count is absolute precision, and absolute precision is only ever
 * right for one order of magnitude. Two decimals renders a 0.000471 m³/s
 * lateral inflow as "0.00" and a 1513.3612 m head as "1513.36" — the same
 * rule, one useless and one fine, decided entirely by how big the number
 * happened to be.
 *
 * What a reader needs is *relative* precision, and which relative precision
 * depends on what the number is doing:
 *
 *   a value on its own   enough digits to be worth reading — significant
 *                        figures, so the width is bounded whatever the
 *                        magnitude (`significantDecimals`).
 *   a value in a series  enough digits to resolve the *spread*. A trend
 *                        running 1513.36 → 1514.00 needs two decimals not
 *                        because the values are precise but because without
 *                        them every point reads 1513 and the line looks flat
 *                        (`rangeDecimals`).
 *
 * Both take the owning quantity's declared decimals as a *floor*, never as
 * the answer: the engine says how coarse it is willing to be read at, and
 * the application may always be more precise when the data needs it.
 *
 * None of this belongs anywhere near the report or CSV renderers. Those are
 * compatibility surfaces with a spec'd precision and golden files, and a
 * heuristic that varies with the data would make their output depend on the
 * data rather than on the format.
 */

/** Significant figures a standalone value is shown to. Four is the most a
 * reader takes in at a glance, and it bounds the width. */
const SIGNIFICANT_FIGURES = 4;

/** Decimals never exceed this, whatever the magnitude — past it a value is
 * better served by `toExponential` than by a run of zeros. */
const MAX_DECIMALS = 6;

/** Outside this band a fixed-point rendering is either a wall of zeros or
 * longer than any column wants, so exponential notation takes over. */
const SMALL_LIMIT = 1e-4;
const LARGE_LIMIT = 1e7;

/**
 * Decimals needed to show `value` to `sig` significant figures, never fewer
 * than `floor`.
 *
 * Zero and non-finite values fall back to the floor: there is no magnitude
 * to derive anything from.
 */
export function significantDecimals(
  value: number,
  floor = 0,
  sig: number = SIGNIFICANT_FIGURES,
): number {
  if (!Number.isFinite(value) || value === 0) return floor;
  const magnitude = Math.floor(Math.log10(Math.abs(value)));
  const needed = sig - 1 - magnitude;
  return Math.min(MAX_DECIMALS, Math.max(floor, needed, 0));
}

/**
 * Decimals needed to tell one step of a series from the next.
 *
 * Derived from the span rather than the values, which is the whole point: a
 * series is read for its variation, and a rule that looks at magnitude alone
 * rounds the variation away exactly when the variation is small.
 *
 * `steps` is how many distinguishable levels the span should resolve into —
 * roughly the number of gridlines or points a reader might compare.
 */
export function rangeDecimals(
  min: number,
  max: number,
  floor = 0,
  steps = 20,
): number {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return floor;
  const span = Math.abs(max - min);
  // A constant series has no spread to resolve; show the value itself well.
  if (span <= 0) return significantDecimals(max, floor);
  const step = span / Math.max(steps, 1);
  const needed = Math.ceil(-Math.log10(step));
  return Math.min(MAX_DECIMALS, Math.max(floor, needed, 0));
}

/**
 * Render a number at a given decimal count, escaping to exponential notation
 * where fixed point would be unreadable.
 *
 * Thousands separators are applied above 10 000, where digit-grouping starts
 * earning its keep, and never to the fractional part.
 */
export function formatDecimal(value: number, decimals: number): string {
  if (!Number.isFinite(value)) return "—";
  const magnitude = Math.abs(value);
  if (
    magnitude !== 0 &&
    (magnitude < SMALL_LIMIT || magnitude >= LARGE_LIMIT)
  ) {
    return value.toExponential(2);
  }
  return value.toLocaleString(undefined, {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
    useGrouping: magnitude >= 10_000,
  });
}

/** A standalone value at significant-figure precision. */
export function formatSignificant(value: number, floor = 0): string {
  return formatDecimal(value, significantDecimals(value, floor));
}
