/**
 * Keeping the legend and the canvas on the same variable when a run has no
 * quality results.
 *
 * The choice of variable is held twice: `genericSelection`, which is what
 * the legend's picker shows, and `nodeVar`/`linkVar`, which is what the
 * canvas paints and what the hover chip and the inspector read. Two stores
 * for one decision, kept in step by the select handler writing both.
 *
 * One path wrote only one of them. A result with no quality analysis has to
 * move the selection off quality — the picker was otherwise left on an
 * option with no data behind it, every element rendering the null-quality
 * grey — and that correction moved `linkVar` to velocity while leaving the
 * legend saying Quality. So the legend named one variable and everything
 * else showed another, which is worse than either being wrong: the reader
 * has no way to tell which of the two is lying.
 *
 * The correction is one function over all four values, so a store cannot be
 * left behind by being forgotten.
 */

import type { LinkVariable, NodeVariable } from "./types";

/** What a node falls back to. Pressure is what a distribution model is for. */
export const NODE_WITHOUT_QUALITY: NodeVariable = "pressure";

/** And a link. Velocity, for the same reason. */
export const LINK_WITHOUT_QUALITY: LinkVariable = "velocity";

/**
 * The id both stores use for quality.
 *
 * The engine's catalog and the canvas's own typed names agree on this
 * string, which is what lets one picker drive both without either side
 * naming an engine.
 */
const QUALITY = "quality";

/** The legend's selection: which variable each element class shows. */
export interface VariableSelection {
  /** The node class's selected variable id. */
  point: string;
  /** The link class's. */
  polyline: string;
}

/**
 * Move the selection off quality when the run has none, and leave it alone
 * when it does.
 *
 * Returns the argument itself when nothing needs changing, so a caller can
 * compare by identity and skip the update.
 *
 * Generic over the selection, so the region class — and anything added
 * beside it — travels through untouched rather than being dropped by a
 * function that only knew about two of the three.
 */
export function withQualityAvailability<T extends VariableSelection>(
  sel: T,
  qualityAvailable: boolean,
): T {
  if (qualityAvailable) return sel;
  if (sel.point !== QUALITY && sel.polyline !== QUALITY) return sel;
  return {
    ...sel,
    point: sel.point === QUALITY ? NODE_WITHOUT_QUALITY : sel.point,
    polyline: sel.polyline === QUALITY ? LINK_WITHOUT_QUALITY : sel.polyline,
  };
}
