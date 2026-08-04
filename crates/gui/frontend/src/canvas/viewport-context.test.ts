import { describe, expect, it } from "vitest";
import {
  pathIntersectsBox,
  pointInBox,
  ringIntersectsBox,
  segmentIntersectsBox,
  type ViewportBox,
} from "./viewport-context";

/** A 10×10 box at the origin. */
const BOX: ViewportBox = { west: 0, south: 0, east: 10, north: 10 };

describe("pointInBox", () => {
  it("accepts a point inside and on the edge", () => {
    expect(pointInBox(5, 5, BOX)).toBe(true);
    expect(pointInBox(0, 0, BOX)).toBe(true);
    expect(pointInBox(10, 10, BOX)).toBe(true);
  });

  it("rejects a point outside on each side", () => {
    expect(pointInBox(-1, 5, BOX)).toBe(false);
    expect(pointInBox(11, 5, BOX)).toBe(false);
    expect(pointInBox(5, -1, BOX)).toBe(false);
    expect(pointInBox(5, 11, BOX)).toBe(false);
  });
});

describe("segmentIntersectsBox", () => {
  it("accepts a segment crossing clean through, both ends outside", () => {
    // The case endpoint testing gets wrong: a trunk main spanning the view.
    expect(segmentIntersectsBox(-5, 5, 15, 5, BOX)).toBe(true);
    expect(segmentIntersectsBox(5, -5, 5, 15, BOX)).toBe(true);
  });

  it("accepts a diagonal crossing corner to corner", () => {
    expect(segmentIntersectsBox(-5, -5, 15, 15, BOX)).toBe(true);
  });

  it("accepts a segment with one end inside", () => {
    expect(segmentIntersectsBox(5, 5, 50, 50, BOX)).toBe(true);
  });

  it("accepts a segment wholly inside", () => {
    expect(segmentIntersectsBox(2, 2, 8, 8, BOX)).toBe(true);
  });

  it("rejects a segment that misses the corner but whose bbox overlaps", () => {
    // The line x+y=21 passes beyond the north-east corner: at y=10 it is at
    // x=11, at x=10 it is at y=11. Yet the segment's own bounding box
    // ([-5,16] × [5,26]) does overlap the viewport, so a bounding-box test
    // would wrongly call this visible. Clipping gets it right.
    expect(segmentIntersectsBox(-5, 26, 16, 5, BOX)).toBe(false);
    // One unit closer and it does clip the corner.
    expect(segmentIntersectsBox(-1, 12, 12, -1, BOX)).toBe(true);
  });

  it("rejects a segment entirely to one side", () => {
    expect(segmentIntersectsBox(-5, -5, -1, 15, BOX)).toBe(false);
    expect(segmentIntersectsBox(11, 0, 20, 10, BOX)).toBe(false);
  });

  it("handles a degenerate zero-length segment", () => {
    expect(segmentIntersectsBox(5, 5, 5, 5, BOX)).toBe(true);
    expect(segmentIntersectsBox(50, 50, 50, 50, BOX)).toBe(false);
  });

  it("handles axis-parallel segments outside the slab", () => {
    expect(segmentIntersectsBox(-5, 20, 15, 20, BOX)).toBe(false);
    expect(segmentIntersectsBox(20, -5, 20, 15, BOX)).toBe(false);
  });
});

describe("pathIntersectsBox", () => {
  it("accepts a polyline whose middle segment crosses, no vertex inside", () => {
    // Every vertex is outside the box; only the segment between two of them
    // enters it. Vertex-only testing would miss this.
    const path: Array<[number, number]> = [
      [-20, 5],
      [-5, 5],
      [15, 5],
      [30, 5],
    ];
    expect(pathIntersectsBox(path, BOX)).toBe(true);
  });

  it("rejects a polyline that stays clear", () => {
    const path: Array<[number, number]> = [
      [-20, 20],
      [-10, 30],
      [20, 40],
    ];
    expect(pathIntersectsBox(path, BOX)).toBe(false);
  });

  it("handles a single-point and an empty path", () => {
    expect(pathIntersectsBox([[5, 5]], BOX)).toBe(true);
    expect(pathIntersectsBox([[50, 50]], BOX)).toBe(false);
    expect(pathIntersectsBox([], BOX)).toBe(false);
  });
});

describe("ringIntersectsBox", () => {
  it("accepts a ring overlapping the box", () => {
    const ring: Array<[number, number]> = [
      [5, 5],
      [50, 5],
      [50, 50],
      [5, 50],
    ];
    expect(ringIntersectsBox(ring, BOX)).toBe(true);
  });

  it("accepts a ring that entirely contains the box", () => {
    const ring: Array<[number, number]> = [
      [-50, -50],
      [50, -50],
      [50, 50],
      [-50, 50],
    ];
    expect(ringIntersectsBox(ring, BOX)).toBe(true);
  });

  it("rejects a ring clear of the box", () => {
    const ring: Array<[number, number]> = [
      [20, 20],
      [30, 20],
      [30, 30],
    ];
    expect(ringIntersectsBox(ring, BOX)).toBe(false);
  });

  it("rejects an empty ring", () => {
    expect(ringIntersectsBox([], BOX)).toBe(false);
  });
});
