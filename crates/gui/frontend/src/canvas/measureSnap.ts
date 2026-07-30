/**
 * Snapping geometry for the measure tool.
 *
 * Measure is map-mode only, so every coordinate here is `[lng, lat]` in
 * degrees. Distances are only ever compared against each other — never
 * reported — so they stay in a cheap local planar approximation rather than
 * going through haversine per candidate segment. The *returned point* is exact:
 * it is a true point on the segment, so the measured length that follows is
 * computed from real coordinates.
 *
 * Longitude degrees shrink with latitude (a degree of longitude is ~111 km at
 * the equator and ~79 km at 45°), so comparing raw degree deltas would bias
 * every choice towards north–south segments. Each comparison is therefore made
 * in a space where longitude is scaled by cos(latitude).
 */

export type LngLat = readonly [number, number];

/**
 * One measure point: where it landed, and what it attached to.
 *
 * `target` is `null` when the click hit empty space and kept the raw cursor
 * position — which the readout uses to say whether a measurement is anchored to
 * network geometry or to a bare map location.
 */
export interface MeasurePoint {
  position: [number, number];
  target: {
    kind: "node" | "link";
    id: string;
    /** Specific element type ("junction", "pipe", "pump", …) — `kind` alone
     * cannot drive the letter badge, which distinguishes a pipe from a pump. */
    type: string;
  } | null;
}

/** Result of resolving a click to something to snap to. */
export type SnapResult = MeasurePoint;

/** Scale factor that makes longitude deltas comparable to latitude deltas. */
function lngScaleAt(latDeg: number): number {
  // Clamped away from the poles: cos(90°) is 0, which would collapse longitude
  // entirely and make every point on a segment look equidistant.
  return Math.max(0.01, Math.cos((latDeg * Math.PI) / 180));
}

/**
 * Closest point to `target` on the segment `a`–`b`.
 *
 * `t` is the normalised position along the segment, clamped to [0, 1] so the
 * result is always on the segment itself rather than on its infinite extension.
 */
function nearestOnSegment(
  a: LngLat,
  b: LngLat,
  target: LngLat,
): { point: [number, number]; distSq: number } {
  const k = lngScaleAt(target[1]);
  const ax = a[0] * k;
  const ay = a[1];
  const bx = b[0] * k;
  const by = b[1];
  const px = target[0] * k;
  const py = target[1];

  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy;
  // Degenerate segment (both endpoints identical) — every point on it is `a`.
  const t = lenSq === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / lenSq;
  const clamped = Math.min(1, Math.max(0, t));

  const point: [number, number] = [
    a[0] + (b[0] - a[0]) * clamped,
    a[1] + (b[1] - a[1]) * clamped,
  ];
  const ex = point[0] * k - px;
  const ey = point[1] - py;
  return { point, distSq: ex * ex + ey * ey };
}

/**
 * Closest point to `target` anywhere along `path`.
 *
 * Returns `null` for a path with no segments, so callers fall back to the raw
 * cursor rather than snapping to a point that does not exist.
 *
 * Chosen over snapping to a link's midpoint: on a long main, the midpoint can
 * be a kilometre from where the user clicked, which answers a different
 * question and reads as a broken tool. This is also what snap-to-edge does in
 * every GIS tool the user will have come from.
 */
export function nearestPointOnPath(
  path: readonly LngLat[],
  target: LngLat,
): [number, number] | null {
  if (path.length === 0) return null;
  if (path.length === 1) return [path[0][0], path[0][1]];
  let best: [number, number] | null = null;
  let bestDistSq = Number.POSITIVE_INFINITY;
  for (let i = 0; i < path.length - 1; i++) {
    const { point, distSq } = nearestOnSegment(path[i], path[i + 1], target);
    if (distSq < bestDistSq) {
      bestDistSq = distSq;
      best = point;
    }
  }
  return best;
}
