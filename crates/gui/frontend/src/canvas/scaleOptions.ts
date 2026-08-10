/**
 * Which ranges the legend offers, and which one is in force.
 *
 * With one reporting step, rescaling to *this* step and scaling across the
 * *whole run* are the same scale, and a choice between two identical
 * outcomes is not a choice.
 *
 * Judging against criteria is no longer among these: it answers a
 * different question and rides its own toggle, so it neither appears nor
 * disappears with the range options.
 */

import type { ScaleMode, ScaleOption } from "./legend-primitives";
import { DATA_SCALE_OPTIONS } from "./legend-primitives";

/**
 * The ranges worth showing.
 *
 * @param multiStep whether the run has more than one reporting step.
 */
export function scaleOptions(multiStep: boolean): readonly ScaleOption[] {
  // "Whole run" survives rather than "This step": on a single step it is
  // the truthful description of both, since the whole run *is* that step.
  return multiStep
    ? DATA_SCALE_OPTIONS
    : DATA_SCALE_OPTIONS.filter((o) => o.mode === "run");
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
