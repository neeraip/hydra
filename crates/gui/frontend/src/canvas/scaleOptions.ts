/**
 * Which scales the legend offers, and which one is in force.
 *
 * The legend already declines to offer criteria bands to a variable that
 * has none, "so the control never presents a scale that would do nothing".
 * A steady-state run is the same case: with one reporting step, rescaling
 * to *this* step and scaling across the *whole run* are the same scale, and
 * a choice between two identical outcomes is not a choice.
 */

import type { ScaleMode, ScaleOption } from "./legend-primitives";
import { CRITERIA_SCALE_OPTION, DATA_SCALE_OPTIONS } from "./legend-primitives";

/**
 * The options worth showing.
 *
 * @param hasCriteria whether any selected variable has threshold bands.
 * @param multiStep   whether the run has more than one reporting step.
 */
export function scaleOptions(
  hasCriteria: boolean,
  multiStep: boolean,
): readonly ScaleOption[] {
  // "Whole run" survives rather than "This step": on a single step it is
  // the truthful description of both, since the whole run *is* that step.
  const data = multiStep
    ? DATA_SCALE_OPTIONS
    : DATA_SCALE_OPTIONS.filter((o) => o.mode === "run");
  return hasCriteria ? [...data, CRITERIA_SCALE_OPTION] : data;
}

/**
 * Whether the control is worth drawing at all.
 *
 * A segmented control with one segment offers nothing and cannot be turned
 * off, which reads as a broken toggle rather than as an absent choice.
 */
export function scaleControlShown(options: readonly ScaleOption[]): boolean {
  return options.length > 1;
}

/**
 * The scale actually in force, given what is on offer.
 *
 * A project saved while scrubbing a long run carries `step`, and may be
 * reopened on a scenario that resolved to a single snapshot. The stored
 * preference is not wrong — it is simply unreachable — so it resolves to
 * the option that behaves identically rather than leaving the control with
 * nothing selected.
 */
export function effectiveScaleMode(
  stored: ScaleMode,
  options: readonly ScaleOption[],
): ScaleMode {
  if (options.some((o) => o.mode === stored)) return stored;
  return options[0]?.mode ?? "run";
}

/**
 * Whether the legend offers a route to the criteria editor.
 *
 * Criteria are read on the canvas — a colour scale and the band text under
 * a ramp — and authored somewhere else. Until this, nothing on the canvas
 * said where. A reader who thought a band was wrong had to already know
 * which page owned it.
 *
 * Offered whenever the project's engine has criteria at all, not merely
 * while a criteria-backed variable is selected. The scale toggle greys out
 * in that case, and that is exactly the moment someone wants to find out
 * what criteria are — a route that disappears when you need it is worse
 * than none. An engine with no such standard shows nothing, which is the
 * registry's existing answer rather than a new judgement here.
 */
export function criteriaEditShown(
  criteriaVariables: readonly string[] | undefined,
  hasHandler: boolean,
): boolean {
  return hasHandler && (criteriaVariables?.length ?? 0) > 0;
}
