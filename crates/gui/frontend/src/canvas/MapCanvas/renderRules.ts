/**
 * Small rules the canvas applies while building a frame.
 *
 * Each is one line where it is used, which is why none of them had a name.
 * Together they decide what can be clicked, what a size means, and what a
 * change of view mode owes the camera — and a one-line rule buried in an
 * argument list is exactly as unreachable as a long one.
 */

import type { CanvasTool, ViewMode } from "../types";

/**
 * Whether links accept pointer picking under this tool.
 *
 * Skipping the pick pass for the tools that cannot use it halves the
 * per-mousemove GPU cost. Measure is in the list because snapping to a link
 * needs it, even though measure's own interaction goes through the map's
 * click and mousemove rather than the layer's.
 */
export function linksPickableFor(tool: CanvasTool): boolean {
  return tool === "select" || tool === "edit" || tool === "measure";
}

/**
 * What a radius or a width is measured in.
 *
 * A geographic view has metres, so a size given in them means the same
 * thing at every zoom. A schematic has no metres — its coordinates are the
 * layout's own — so sizes there are in deck's common units.
 */
export function sizeUnitsFor(isSchematic: boolean): "common" | "meters" {
  return isSchematic ? "common" : "meters";
}

/** What a change of view mode means for the camera. */
export type ViewTransition =
  /** Arriving at the geographic view: its camera has to be put back, or
   *  framed if there is none to put back. */
  | "entering-map"
  /** Leaving it: keep the camera before the map goes out of sight. */
  | "leaving-map"
  /** Neither — a first render, or a switch between the two orthographic
   *  layouts, which the framing pass handles instead. */
  | "staying";

/**
 * Classify a view-mode change.
 *
 * Written out at the top of the effect that acts on it, where the two
 * conditions are near-mirrors of each other and easy to get subtly
 * different — and where being wrong means either a camera not kept or a
 * camera not restored, both of which read as the canvas re-framing itself
 * for no reason.
 */
export function classifyViewTransition(
  previous: ViewMode | null,
  next: ViewMode,
): ViewTransition {
  if (next === "map" && previous !== "map") return "entering-map";
  if (next !== "map" && previous === "map") return "leaving-map";
  return "staying";
}
