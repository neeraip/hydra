import { describe, expect, it } from "vitest";
import {
  endpointsOf,
  placeSegments,
  type Sketch,
  strokeFor,
} from "./NetworkSketch";
import { placeholderSketch } from "./placeholderSketch";

/**
 * How heavily a network outline is drawn.
 *
 * The drawing exists to be recognised at card size and nothing else. A
 * sparse network at a hairline vanishes; a dense one at the same weight
 * fills in to a solid block. Either way the card stops saying which
 * project it is, which is its only job.
 */

describe("a sketch's stroke weight", () => {
  it("thins as the network gets denser", () => {
    expect(strokeFor(20)).toBeGreaterThan(strokeFor(120));
    expect(strokeFor(120)).toBeGreaterThan(strokeFor(600));
  });

  /**
   * Every weight is a usable fraction of the box. The bug this replaces
   * set `vector-effect: non-scaling-stroke`, which reads these as device
   * pixels, so the densest networks drew at six thousandths of a pixel and
   * looked like they were fading out.
   */
  it("is a fraction of the box that can actually be seen", () => {
    for (const n of [3, 40, 200, 5000]) {
      expect(strokeFor(n)).toBeGreaterThanOrEqual(0.004);
    }
  });

  /** Never zero, or a large network draws nothing at all. */
  it("always has a weight", () => {
    for (const n of [0, 1, 40, 41, 200, 201, 5000]) {
      expect(strokeFor(n)).toBeGreaterThan(0);
    }
  });
});

/**
 * The stand-in networks.
 *
 * A card with nothing drawn for it used to show the engine's mark, which
 * read as an error rather than as "not opened yet". These fill the frame
 * with something shaped like what the engine models.
 */
describe("a stand-in network", () => {
  it("has one for each engine that ships", () => {
    expect(placeholderSketch("wds")?.segments.length).toBeGreaterThan(20);
    expect(placeholderSketch("uds")?.segments.length).toBeGreaterThan(10);
  });

  /**
   * Inventing a picture for an engine this build does not understand would
   * claim a character there is no basis for.
   */
  it("has none for an engine it does not know", () => {
    expect(placeholderSketch("och")).toBeNull();
    expect(placeholderSketch(undefined)).toBeNull();
  });

  /** The two must not be interchangeable, or the picture says nothing. */
  it("draws the two engines differently", () => {
    expect(placeholderSketch("wds")?.segments).not.toEqual(
      placeholderSketch("uds")?.segments,
    );
  });

  /** Stable across renders: a card that redrew differently each time would
   *  be worse than an empty frame. */
  it("is the same drawing every time", () => {
    expect(placeholderSketch("wds")).toEqual(placeholderSketch("wds"));
  });

  /** Inside the box, like a real sketch, or it clips at the frame. */
  it("stays inside the unit box", () => {
    for (const key of ["wds", "uds"]) {
      for (const s of placeholderSketch(key)?.segments ?? []) {
        for (const v of [s.x1, s.y1, s.x2, s.y2]) {
          expect(v).toBeGreaterThanOrEqual(0);
          expect(v).toBeLessThanOrEqual(1);
        }
      }
    }
  });
});

/**
 * Placing a drawing at its true proportions.
 *
 * This replaced an SVG transform, which scaled the stroke along with the
 * geometry and did it by a different amount on each axis. A network thirty
 * times wider than tall went through `scale(1, 0.03)`, leaving every
 * near-horizontal line at three percent of its weight: the model drew as a
 * row of broken dashes where it is one continuous run.
 */
describe("placing a sketch at its proportions", () => {
  const wide: Sketch = {
    aspect: 4,
    segments: [{ x1: 0, y1: 0, x2: 1, y2: 1 }],
  };

  it("uses the full width and a proportional height", () => {
    const [s] = placeSegments(wide);
    expect(s.x1).toBe(0);
    expect(s.x2).toBe(1);
    expect(s.y2 - s.y1).toBeCloseTo(0.25, 6);
  });

  it("centres the drawing rather than pinning it to a corner", () => {
    const [s] = placeSegments(wide);
    expect((s.y1 + s.y2) / 2).toBeCloseTo(0.5, 6);
  });

  it("gives a tall network the full height", () => {
    const [s] = placeSegments({ ...wide, aspect: 0.25 });
    expect(s.y1).toBe(0);
    expect(s.y2).toBe(1);
    expect(s.x2 - s.x1).toBeCloseTo(0.25, 6);
  });

  it("leaves a square network alone", () => {
    expect(placeSegments({ ...wide, aspect: 1 })[0]).toEqual(wide.segments[0]);
  });

  /** A sketch written before `aspect` existed, or one that lost it. */
  it("treats a missing or absurd aspect as square", () => {
    for (const aspect of [0, -3, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(placeSegments({ ...wide, aspect })[0]).toEqual(wide.segments[0]);
    }
  });

  /** Everything stays in the box, whatever the proportions. */
  it("keeps every placed point inside the box", () => {
    for (const aspect of [0.05, 0.5, 1, 8, 40]) {
      for (const s of placeSegments({ ...wide, aspect })) {
        for (const v of [s.x1, s.y1, s.x2, s.y2]) {
          expect(v).toBeGreaterThanOrEqual(0);
          expect(v).toBeLessThanOrEqual(1);
        }
      }
    }
  });
});

describe("a sparse network's nodes", () => {
  /** Five nodes joined by four pipes is mostly nodes. Without them it is a
   *  scratch, especially when the run is nearly straight. */
  it("counts each junction once, however many links meet there", () => {
    const chain = [
      { x1: 0, y1: 0.5, x2: 0.5, y2: 0.5 },
      { x1: 0.5, y1: 0.5, x2: 1, y2: 0.5 },
    ];
    expect(endpointsOf(chain)).toHaveLength(3);
  });

  it("finds nothing to draw in an empty network", () => {
    expect(endpointsOf([])).toHaveLength(0);
  });
});

/**
 * Catchments and conveyance are placed identically.
 *
 * They were normalised against one extent in the backend, so placing them
 * by different rules here is how a catchment ends up beside its network
 * rather than around it.
 */
describe("placing catchment outlines", () => {
  const s: Sketch = {
    aspect: 4,
    segments: [{ x1: 0, y1: 0, x2: 1, y2: 1 }],
    areas: [{ x1: 0, y1: 0, x2: 1, y2: 1 }],
  };

  it("places an outline exactly where it places a pipe", () => {
    expect(placeSegments(s, "areas")).toEqual(placeSegments(s, "segments"));
  });

  /** Drawings written before catchments existed have none, and must not
   *  throw on the way to being drawn. */
  it("reads a drawing that has no outlines", () => {
    const older: Sketch = { aspect: 1, segments: s.segments };
    expect(placeSegments(older, "areas")).toEqual([]);
  });
});
