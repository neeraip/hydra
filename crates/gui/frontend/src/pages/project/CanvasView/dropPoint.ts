/**
 * Turning a point the canvas reported into a coordinate the model stores.
 *
 * The backend's coordinate store always holds source-CRS values. What
 * arrives from the canvas does not: a georeferenced model is drawn on a
 * basemap and reports WGS84, while a local grid is drawn orthographically
 * at its own coordinates and reports those. One needs inverse-projecting
 * and the other must not be touched — projecting a grid coordinate as if
 * it were a longitude is how a plan view would quietly ruin a model.
 *
 * Split out of the two handlers that used to assume WGS84 so the rule is
 * stated once, and can be tested without a map.
 */

import type { CanvasPoint } from "../../../canvas/types";

/**
 * The source-CRS coordinate to store for `at`.
 *
 * `project` is the WGS84 → source-CRS conversion (`wgs84ToSourceCrs`),
 * passed in rather than imported so this stays a decision about spaces
 * rather than a dependency on proj4. It throws on an unconvertible point,
 * and that throw is deliberately not caught here: the callers turn it into
 * a refusal the user can see, and a silently swallowed failure would
 * commit a wrong coordinate.
 */
export function sourceCoordinate(
  at: CanvasPoint,
  project: (lngLat: [number, number]) => [number, number],
): [number, number] {
  return at.space === "source" ? [at.x, at.y] : project([at.x, at.y]);
}
