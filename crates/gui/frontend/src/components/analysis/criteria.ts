/** Criterion wire shapes (hydra-common §7.2, serde camelCase) and the
 * pure decisions of editing a valuation — engine-neutral by construction:
 * every label, quantity, and default arrives from the engine's catalog,
 * so one editor serves every engine without a hardcoded unit table.
 *
 * Values are SI display units of each criterion's quantity (§7.3); the
 * quantity descriptor rides along in the DTO, so display conversion is
 * the affine map the engine published, not a frontend lookup.
 */

import type { UnitSystem } from "../../units";

export interface QuantityInfo {
  key: string;
  siLabel: string;
  usLabel: string;
  siToUsScale: number;
  siToUsOffset: number;
  siDecimals: number;
  usDecimals: number;
}

export type CriterionKind =
  | { type: "value"; default: number }
  | {
      type: "band";
      cuts: Array<{ key: string; label: string; default: number }>;
    };

export interface Criterion {
  key: string;
  label: string;
  help: string;
  quantity?: QuantityInfo;
  kind: CriterionKind;
}

/** A valuation: criterion key → SI number (value) or SI list (band). */
export type Valuation = Record<string, number | number[]>;

/** SI value → the active display system, by the engine's affine map. */
export function toDisplayValue(
  si: number,
  quantity: QuantityInfo | undefined,
  sys: UnitSystem,
): number {
  if (!quantity || sys === "si") return si;
  return si * quantity.siToUsScale + quantity.siToUsOffset;
}

/** Display value → SI, inverting the engine's affine map. */
export function fromDisplayValue(
  display: number,
  quantity: QuantityInfo | undefined,
  sys: UnitSystem,
): number {
  if (!quantity || sys === "si") return display;
  return (display - quantity.siToUsOffset) / quantity.siToUsScale;
}

/** The criterion's unit text in the active system, or empty when
 * dimensionless. */
export function criterionUnit(
  quantity: QuantityInfo | undefined,
  sys: UnitSystem,
): string {
  if (!quantity) return "";
  return sys === "us" ? quantity.usLabel : quantity.siLabel;
}

/** The catalog's default valuation: what an engine means by "the
 * conventional standard" (§7.3: absent keys mean exactly this). */
export function defaultValuation(catalog: Criterion[]): Valuation {
  const values: Valuation = {};
  for (const c of catalog) {
    values[c.key] =
      c.kind.type === "value"
        ? c.kind.default
        : c.kind.cuts.map((cut) => cut.default);
  }
  return values;
}

/** One criterion's current value, falling back to its defaults — the
 * §7.3 absent-key rule, applied for display. */
export function criterionValue(
  c: Criterion,
  values: Valuation,
): number | number[] {
  const held = values[c.key];
  if (c.kind.type === "value") {
    return typeof held === "number" ? held : c.kind.default;
  }
  const defaults = c.kind.cuts.map((cut) => cut.default);
  return Array.isArray(held) && held.length === defaults.length
    ? held
    : defaults;
}

function trimmed(n: number): string {
  const decimals = Math.abs(n) >= 100 ? 0 : Math.abs(n) >= 10 ? 1 : 2;
  return String(Number(n.toFixed(decimals)));
}

/** The chip's read-only line: every criterion, values in the active
 * system, bands as endpoint ranges. */
export function valuationSummary(
  catalog: Criterion[],
  values: Valuation,
  sys: UnitSystem,
): string {
  return catalog
    .map((c) => {
      const unit = criterionUnit(c.quantity, sys);
      const suffix = unit ? ` ${unit}` : "";
      const v = criterionValue(c, values);
      if (typeof v === "number") {
        return `${c.label} ${trimmed(toDisplayValue(v, c.quantity, sys))}${suffix}`;
      }
      const first = toDisplayValue(v[0], c.quantity, sys);
      const last = toDisplayValue(v[v.length - 1], c.quantity, sys);
      return `${c.label} ${trimmed(first)}–${trimmed(last)}${suffix}`;
    })
    .join("  ·  ");
}
