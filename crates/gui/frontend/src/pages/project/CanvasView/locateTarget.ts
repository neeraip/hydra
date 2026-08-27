/**
 * Which array the legend's "locate the extreme" search runs over, for a
 * class the legend offers it on.
 *
 * The legend speaks in element classes; the extremes search is indexed
 * by the wds period result's node and link arrays. Two of the four
 * classes have no such array, so they have no answer — and answering
 * anyway is the whole point of naming this: the branch this replaced
 * refused regions by name and sent *everything else* to the link
 * arrays, which made the 2D surface's own extreme a link. Nothing
 * reaches that today (the control is offered only where a wds period
 * result is loaded, and a surface only exists in a drainage model), so
 * it would have stayed wrong until the day it was seen.
 */
import type { GenericClassKey } from "../../../canvas/GenericLegend";

export function locateTarget(cls: GenericClassKey): "node" | "link" | null {
  switch (cls) {
    case "point":
      return "node";
    case "polyline":
      return "link";
    // Areal elements and the 2D surface are not in those arrays.
    case "region":
    case "surface":
      return null;
  }
}
