/**
 * Reading an inlet coupling from either end.
 *
 * A coupling is a hydraulic connection that is *not* a link: in a dual
 * drainage model, surface flow reaches the buried sewer only by being
 * captured at a street inlet. Because it is not a link, every query that
 * finds neighbours by matching `fromId`/`toId` — which is how the
 * inspector finds connected elements — cannot see it at all. The canvas
 * draws it as a dashed connector, so the relationship is visible on the
 * map and was, until these functions existed, untraversable in the panel
 * beside it.
 *
 * Named functions rather than inline filters because the direction is easy
 * to invert silently: both sides of a coupling are element ids, and
 * matching the wrong field yields a plausible, wrong, and quiet answer.
 */

import type { InletCoupling } from "../../hooks";

/**
 * Ids of the nodes a link's inlets capture into.
 *
 * Usually one — the model format assigns a conduit's inlet a single
 * receiving node — but returned as a list rather than a single id, because
 * nothing in the data forbids a second row naming the same link, and a
 * silent "first one wins" would hide the rest.
 */
export function capturedInto(
  couplings: readonly InletCoupling[],
  linkId: string,
): string[] {
  return couplings.filter((c) => c.link === linkId).map((c) => c.node);
}

/**
 * Ids of the links whose inlets capture into a node.
 *
 * Genuinely many-to-one: a sewer node commonly receives from every street
 * conduit whose inlets sit above it.
 */
export function capturedFrom(
  couplings: readonly InletCoupling[],
  nodeId: string,
): string[] {
  return couplings.filter((c) => c.node === nodeId).map((c) => c.link);
}
