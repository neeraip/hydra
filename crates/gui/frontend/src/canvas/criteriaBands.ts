/**
 * Reading a criterion as a colour scale.
 *
 * A `banded` variable names the criterion its thresholds come from
 * (hydra-common §6.1) and that criterion says what each region between
 * them means (§7.2). Together those are enough to paint a compliance
 * verdict for any engine — which is the point: this file contains no
 * variable names, no engine names, and no numbers.
 *
 * Before it existed the canvas recognised `pressure`, `velocity` and
 * `flow` by name and read a water-distribution criteria object directly,
 * so drainage variables could not be offered a threshold scale at all
 * even once drainage had criteria of its own.
 */

import type { Criterion, Valuation } from "../components/analysis/criteria";
import type { Verdict } from "./MapCanvas/colorUtils";

/** A criterion resolved against a project's valuation: the thresholds in
 *  ascending order, and what the regions between them mean. */
export interface CriterionBands {
  /** Ascending cut values, in the criterion's SI display unit. */
  cuts: number[];
  /** One more entry than there are cuts (§7.2). */
  severities: Verdict[];
}

/** The cut values a criterion takes under `valuation`, or its defaults. */
export function cutsOf(criterion: Criterion, valuation: Valuation): number[] {
  const saved = valuation[criterion.key];
  if (criterion.kind.type === "value") {
    return [
      typeof saved === "number" && Number.isFinite(saved)
        ? saved
        : criterion.kind.default,
    ];
  }
  const defaults = criterion.kind.cuts.map((c) => c.default);
  if (!Array.isArray(saved) || saved.length !== defaults.length)
    return defaults;
  return saved.map((v, i) => (Number.isFinite(v) ? v : defaults[i]));
}

/**
 * The bands a variable is scaled by, or null when it cannot be.
 *
 * Null covers every way the pair can fail to resolve — the variable is
 * not banded, its criterion is absent from the catalog, the criterion
 * states no severities, or the two disagree about how many regions there
 * are. In each case the caller falls back to a plain magnitude, because a
 * scale that cannot say what its colours mean is worse than no scale.
 *
 * The ascending check is not paranoia: a valuation is user-editable and a
 * band caught mid-edit is legitimately out of order (§7.3 calls it
 * degenerate). Painting a map from cuts that go backwards would report
 * compliance the numbers do not support.
 */
export function bandsFor(
  ramp: { type: string; criterion?: string },
  catalog: readonly Criterion[],
  valuation: Valuation,
): CriterionBands | null {
  if (ramp.type !== "banded" || !ramp.criterion) return null;
  const criterion = catalog.find((c) => c.key === ramp.criterion);
  if (!criterion) return null;
  const severities = criterion.severities ?? [];
  if (severities.length === 0) return null;
  const cuts = cutsOf(criterion, valuation);
  if (severities.length !== cuts.length + 1) return null;
  for (let i = 1; i < cuts.length; i += 1) {
    if (!(cuts[i] > cuts[i - 1])) return null;
  }
  return { cuts, severities: severities as Verdict[] };
}

/**
 * Which region `value` falls in, and therefore how it reads.
 *
 * Cuts are lower edges: a value equal to a cut belongs to the region
 * above it. That matters at the ends people actually set — a conduit at
 * exactly the 80% capacity threshold is *at* capacity, not under it.
 */
export function verdictAt(value: number, bands: CriterionBands): Verdict {
  let region = 0;
  while (region < bands.cuts.length && value >= bands.cuts[region]) region += 1;
  return bands.severities[region];
}

/**
 * The sentence under a banded ramp: where the thresholds sit and what
 * each region is called.
 *
 * Built from the criterion's own cut labels rather than a phrasing per
 * variable, which is what confined this to three water-distribution ids.
 * The reader gets the engine's vocabulary — "self-cleansing", "erosive" —
 * rather than the application's guess at it.
 */
export function annotationFor(
  criterion: Criterion,
  bands: CriterionBands,
  show: (value: number) => string,
): string {
  const labels =
    criterion.kind.type === "band"
      ? criterion.kind.cuts.map((c) => c.label)
      : [criterion.label];
  return bands.cuts
    .map((cut, i) => `${labels[i] ?? ""} ${show(cut)}`.trim())
    .join(" · ");
}
