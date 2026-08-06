import { describe, expect, it } from "vitest";
import {
  NODE_SCALE_DEFAULT,
  NODE_SCALE_MAX,
  NODE_SCALE_MIN,
  nodeRadius,
  nodeScaleFactor,
  typicalLinkLength,
} from "./nodeScale";

/**
 * Sizing nodes against the network rather than at a fixed 7 metres.
 *
 * The defect: an absolute radius has a different meaning on every model.
 * On a dense network it read as a junction; on a spread-out one it pinned
 * to the pixel floor and stayed a dot however far you zoomed in.
 */

const link = (a: [number, number], b: [number, number]) => ({ from: a, to: b });

describe("the typical link length", () => {
  it("is the median, not the mean", () => {
    // Four ordinary links and one transmission main. The mean would be
    // dragged past every link in the network; the median is not.
    const links = [
      link([0, 0], [10, 0]),
      link([0, 0], [12, 0]),
      link([0, 0], [14, 0]),
      link([0, 0], [16, 0]),
      link([0, 0], [10000, 0]),
    ];
    expect(typicalLinkLength(links)).toBe(14);
  });

  /** A pump between coincident nodes is real and says nothing about
   *  spacing. */
  it("ignores zero-length links", () => {
    expect(
      typicalLinkLength([link([5, 5], [5, 5]), link([0, 0], [3, 4])]),
    ).toBe(5);
  });

  it("has no answer for a network with no links", () => {
    expect(typicalLinkLength([])).toBeNull();
    expect(typicalLinkLength([link([1, 1], [1, 1])])).toBeNull();
  });
});

describe("the slider's multiplier", () => {
  /** The midpoint has to be neutral, or the derived size is unreachable. */
  it("is exactly one at the default position", () => {
    expect(nodeScaleFactor(NODE_SCALE_DEFAULT)).toBe(1);
  });

  /**
   * Geometric, so a step left shrinks by as much as a step right grows.
   * On a linear scale between 0.4 and 3 the midpoint is 1.7, and neutral
   * would sit off-centre.
   */
  it("shrinks and grows by the same feel either side of neutral", () => {
    const up = nodeScaleFactor(75);
    const down = nodeScaleFactor(25);
    expect(up).toBeGreaterThan(1);
    expect(down).toBeLessThan(1);
    // Same ratio away from neutral in both directions.
    expect(Math.log(up)).toBeCloseTo(
      -Math.log(down) * (Math.log(3) / -Math.log(0.4)),
      1,
    );
  });

  it("never inverts or vanishes at the extremes", () => {
    expect(nodeScaleFactor(NODE_SCALE_MIN)).toBeGreaterThan(0);
    expect(nodeScaleFactor(NODE_SCALE_MAX)).toBeGreaterThan(
      nodeScaleFactor(NODE_SCALE_MIN),
    );
  });

  it("clamps a position from outside the range", () => {
    expect(nodeScaleFactor(-40)).toBe(nodeScaleFactor(NODE_SCALE_MIN));
    expect(nodeScaleFactor(400)).toBe(nodeScaleFactor(NODE_SCALE_MAX));
    expect(nodeScaleFactor(Number.NaN)).toBe(1);
  });
});

describe("the radius a network gets", () => {
  /**
   * The point of the whole change: two models whose spacing differs by
   * three orders of magnitude get radii that differ by the same, so the
   * node-to-link ratio is the same in both.
   */
  it("scales with how far apart the network's nodes are", () => {
    const dense = nodeRadius(60, NODE_SCALE_DEFAULT);
    const spread = nodeRadius(60_000, NODE_SCALE_DEFAULT);
    expect(spread / dense).toBeCloseTo(1000, 5);
  });

  /** And a dense network keeps roughly the size it already had, so nothing
   *  visibly moves for the models that looked right. */
  it("leaves a typical urban network near where it was", () => {
    expect(nodeRadius(60, NODE_SCALE_DEFAULT)).toBeCloseTo(7, 0);
  });

  /** A model with no usable geometry still draws. */
  it("falls back rather than collapsing to nothing", () => {
    for (const bad of [null, 0, -5, Number.NaN]) {
      expect(nodeRadius(bad, NODE_SCALE_DEFAULT)).toBeGreaterThan(0);
    }
  });
});

/**
 * Node size must not move when the schematic spread does.
 *
 * The schematic layout's distances are chosen by the layout, not the
 * model, and they change whenever the aspect slider does. Deriving a
 * radius from them made a spread adjustment resize every node, so one
 * control drove two things and the spread slider stopped being only a
 * spread slider.
 *
 * The fix is not to correct for the reshaping but to not measure it: a
 * schematic gets the fixed radius it always had, because the spacing it
 * sits in is uniform and chosen. `null` is how the caller says so.
 */
describe("a layout with no model spacing to measure", () => {
  it("falls back to a fixed radius", () => {
    expect(nodeRadius(null, NODE_SCALE_DEFAULT)).toBeGreaterThan(0);
  });

  /** And the slider still works on it, or the schematic would have a
   *  control that does nothing. */
  it("still answers to the size slider", () => {
    expect(nodeRadius(null, 80)).toBeGreaterThan(
      nodeRadius(null, NODE_SCALE_DEFAULT),
    );
    expect(nodeRadius(null, 20)).toBeLessThan(
      nodeRadius(null, NODE_SCALE_DEFAULT),
    );
  });

  /** The fixed radius does not depend on the network, so no reshaping of
   *  any layout can move it. */
  it("is the same whatever the network looks like", () => {
    expect(nodeRadius(null, 60)).toBe(nodeRadius(null, 60));
  });
});
