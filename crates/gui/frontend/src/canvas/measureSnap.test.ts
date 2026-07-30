import { describe, expect, it } from "vitest";
import { type LngLat, nearestPointOnPath } from "./measureSnap";

describe("nearestPointOnPath", () => {
  it("projects onto the interior of a segment", () => {
    // Snapping to the nearest point is the whole reason this exists: a click
    // beside the middle of a pipe must return the middle, not an endpoint.
    const path: LngLat[] = [
      [0, 0],
      [10, 0],
    ];
    const got = nearestPointOnPath(path, [4, 1]);
    expect(got?.[0]).toBeCloseTo(4, 9);
    expect(got?.[1]).toBeCloseTo(0, 9);
  });

  it("clamps to an endpoint rather than the segment's extension", () => {
    // Without clamping, a click past the end of a pipe would snap to a point
    // in open space along the line the pipe happens to lie on.
    const path: LngLat[] = [
      [0, 0],
      [10, 0],
    ];
    expect(nearestPointOnPath(path, [-50, 0])?.[0]).toBeCloseTo(0, 9);
    expect(nearestPointOnPath(path, [50, 0])?.[0]).toBeCloseTo(10, 9);
  });

  it("picks the closest segment of a multi-vertex path", () => {
    // Map-mode links carry intermediate vertices, so the nearest point is not
    // necessarily on the first or last segment.
    const path: LngLat[] = [
      [0, 0],
      [0, 10],
      [10, 10],
    ];
    const got = nearestPointOnPath(path, [5, 11]);
    expect(got?.[0]).toBeCloseTo(5, 9);
    expect(got?.[1]).toBeCloseTo(10, 9);
  });

  it("returns a point that lies exactly on the path", () => {
    // The measured length is computed from this point, so it has to be a real
    // point on the segment — not an approximation from the scaled space used
    // for comparison.
    const a: LngLat = [-71.1, 42.3];
    const b: LngLat = [-71.0, 42.4];
    const got = nearestPointOnPath([a, b], [-71.02, 42.39]);
    if (!got) throw new Error("expected a snap");
    // Colinearity via the cross product of (b−a) and (got−a).
    const cross =
      (b[0] - a[0]) * (got[1] - a[1]) - (b[1] - a[1]) * (got[0] - a[0]);
    expect(Math.abs(cross)).toBeLessThan(1e-12);
  });

  it("does not bias towards north-south segments at high latitude", () => {
    // A degree of longitude is ~79km at 45° but ~111km of latitude. Comparing
    // raw degree deltas would make the east-west segment look further away
    // than it is, and the click would snap to the wrong pipe.
    const eastWest: LngLat[] = [
      [-1, 60],
      [1, 60],
    ];
    // Click 0.05° north of the east-west line. In scaled space that is further
    // than 0.05° of longitude would be, which is the point of the correction.
    const got = nearestPointOnPath(eastWest, [0, 60.05]);
    expect(got?.[1]).toBeCloseTo(60, 9);
    expect(got?.[0]).toBeCloseTo(0, 6);
  });

  it("survives degenerate input", () => {
    expect(nearestPointOnPath([], [0, 0])).toBeNull();
    // A zero-length link (both ends at one place) still has a position.
    expect(
      nearestPointOnPath(
        [
          [3, 4],
          [3, 4],
        ],
        [9, 9],
      ),
    ).toEqual([3, 4]);
    // A single-vertex path is that vertex.
    expect(nearestPointOnPath([[7, 8]], [0, 0])).toEqual([7, 8]);
  });
});
