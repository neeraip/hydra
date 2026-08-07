import { describe, expect, it } from "vitest";
import { gridCoversView, gridLines, gridSpacing, visibleBounds } from "./grid";

/**
 * A schematic has no basemap, so without a grid the network floats on a
 * flat ground with nothing to say what kind of surface it is. The grid says
 * it: a diagram, drawn in its own space, not a map of anywhere.
 *
 * Nothing here is labelled, and that is deliberate. In a plan the
 * coordinates are the model's own and the squares are real distance; in a
 * topological layout they are positions the layout invented. An unlabelled
 * grid claims no particular distance, and a labelled one over a topological
 * layout would claim one that does not exist.
 */

const VIEW = { width: 1000, height: 800 };

describe("choosing a spacing", () => {
  /**
   * Snapped to 1, 2 or 5 within each power of ten. Spacing taken straight
   * from the zoom changes by an arbitrary factor every frame of a pinch,
   * which reads as the ground crawling.
   */
  it("uses only round steps", () => {
    for (let zoom = -8; zoom <= 12; zoom += 0.25) {
      const s = gridSpacing(zoom);
      // The step within its power of ten: 1, 2 or 5. A spacing that fell
      // through to the next decade reads as 1 again, which is the point.
      const mantissa = s / 10 ** Math.floor(Math.log10(s));
      expect([1, 2, 5]).toContainEqual(Number(mantissa.toPrecision(6)));
    }
  });

  /** Never closer together than asked, or the lines crowd. */
  it("never crowds tighter than the target", () => {
    for (let zoom = -6; zoom <= 10; zoom += 0.5) {
      expect(gridSpacing(zoom, 110) * 2 ** zoom).toBeGreaterThanOrEqual(110);
    }
  });

  /** Zooming in halves the world distance a line covers. */
  it("halves as the view doubles", () => {
    expect(gridSpacing(3)).toBeLessThanOrEqual(gridSpacing(2));
    expect(gridSpacing(9)).toBeLessThan(gridSpacing(2));
  });

  /** A camera with no usable zoom would ask for an unbounded number. */
  it("gives up on a nonsense zoom", () => {
    expect(gridSpacing(Number.NaN)).toBe(0);
    expect(gridSpacing(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe("the lines themselves", () => {
  const bounds = { minX: 0, maxX: 100, minY: 0, maxY: 50 };

  it("cover the bounds on both axes", () => {
    const lines = gridLines(bounds, 25);
    const vertical = lines.filter((l) => l.from[0] === l.to[0]);
    const horizontal = lines.filter((l) => l.from[1] === l.to[1]);
    expect(vertical).toHaveLength(5); // 0, 25, 50, 75, 100
    expect(horizontal).toHaveLength(3); // 0, 25, 50
  });

  /** Aligned to multiples of the spacing, so the grid stays put as the
   *  camera moves rather than sliding with it. */
  it("sits on multiples of the spacing", () => {
    for (const l of gridLines({ minX: 7, maxX: 90, minY: 3, maxY: 40 }, 10)) {
      const onX = l.from[0] === l.to[0];
      const v = onX ? l.from[0] : l.from[1];
      expect(v % 10).toBeCloseTo(0, 9);
    }
  });

  /**
   * The spacing rule already bounds the count, so an unbounded ask means
   * something upstream is wrong — and a canvas that draws nothing beats one
   * that hangs building a million segments.
   */
  it("refuses an impossible number of lines", () => {
    expect(gridLines({ minX: 0, maxX: 1e9, minY: 0, maxY: 1e9 }, 1)).toEqual(
      [],
    );
  });

  it("draws nothing without a spacing", () => {
    expect(gridLines(bounds, 0)).toEqual([]);
    expect(gridLines(bounds, Number.NaN)).toEqual([]);
  });
});

describe("what the camera can see", () => {
  it("centres on the camera's target", () => {
    const b = visibleBounds([100, 50, 0], 0, VIEW);
    expect((b.minX + b.maxX) / 2).toBeCloseTo(100, 9);
    expect((b.minY + b.maxY) / 2).toBeCloseTo(50, 9);
  });

  it("shrinks as the camera closes in", () => {
    const out = visibleBounds([0, 0, 0], 0, VIEW);
    const inn = visibleBounds([0, 0, 0], 2, VIEW);
    expect(inn.maxX - inn.minX).toBeLessThan(out.maxX - out.minX);
  });
});

describe("when the drawn grid stops covering the view", () => {
  const built = {
    bounds: { minX: -500, maxX: 500, minY: -500, maxY: 500 },
    spacing: gridSpacing(0),
  };

  it("still covers a small pan", () => {
    expect(
      gridCoversView(built, [10, 10, 0], 0, { width: 100, height: 100 }),
    ).toBe(true);
  });

  /**
   * The load-bearing pair. Rebuilding the grid means rebuilding every layer
   * on the canvas, so it must happen when the grid is *wrong* and not
   * merely older — but it must still happen then, or the ground runs out
   * mid-pan.
   */
  it("does not cover a pan past what was drawn", () => {
    expect(
      gridCoversView(built, [5000, 0, 0], 0, { width: 100, height: 100 }),
    ).toBe(false);
  });

  it("does not survive a change of spacing", () => {
    expect(
      gridCoversView(built, [0, 0, 0], 6, { width: 100, height: 100 }),
    ).toBe(false);
  });

  it("has nothing to cover before anything is drawn", () => {
    expect(gridCoversView(null, [0, 0, 0], 0, VIEW)).toBe(false);
  });
});
