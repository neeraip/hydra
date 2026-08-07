import { describe, expect, it } from "vitest";
import {
  mapZoomForNode,
  NODE_MIN_MAP_ZOOM,
  orthoCameraForLink,
  orthoCameraForNode,
  regionFlyToSupported,
} from "./flyToCamera";

/**
 * Following a connection from the inspector, locating an extreme from the
 * legend, or picking an element out of the network list all end in one of
 * these: the canvas has to move to something and decide how close to get.
 *
 * It was arithmetic inline in an effect, so the only way to check a cap was
 * to click something and look — and "it zoomed too far", "it zoomed to
 * nothing" and "it kept creeping in" are the reports that follow from
 * getting one wrong.
 */

describe("flying a map to a node", () => {
  /**
   * A floor, not a set zoom. Someone already closer than this asked to see
   * the element, not to be pulled back out to a standard distance.
   */
  it("closes in on a distant view", () => {
    expect(mapZoomForNode(9)).toBe(NODE_MIN_MAP_ZOOM);
  });

  it("leaves a closer view where it is", () => {
    expect(mapZoomForNode(17)).toBe(17);
  });

  it("is already there at the floor", () => {
    expect(mapZoomForNode(NODE_MIN_MAP_ZOOM)).toBe(NODE_MIN_MAP_ZOOM);
  });

  /** A map that cannot say where it is must still produce a usable zoom. */
  it("copes with a map that reports nothing", () => {
    expect(mapZoomForNode(Number.NaN)).toBe(NODE_MIN_MAP_ZOOM);
    expect(mapZoomForNode(Number.POSITIVE_INFINITY)).toBe(NODE_MIN_MAP_ZOOM);
  });
});

describe("framing a node in the schematic", () => {
  it("centres on it", () => {
    expect(orthoCameraForNode([12, -34], 2).target).toEqual([12, -34, 0]);
  });

  /**
   * Relative to the whole-network fit, because an orthographic zoom means
   * nothing on its own — it is log2 of a scale over coordinates the layout
   * invented.
   */
  it("goes one step closer than the whole network", () => {
    expect(orthoCameraForNode([0, 0], 2).zoom).toBe(3);
    expect(orthoCameraForNode([0, 0], -4).zoom).toBe(-3);
  });

  /**
   * The load-bearing cap. A network laid out very small has a high fit
   * zoom, and without a ceiling a single node would be approached
   * arbitrarily far in.
   */
  it("stops at the ceiling however small the network", () => {
    expect(orthoCameraForNode([0, 0], 40).zoom).toBe(10);
  });
});

describe("framing a link in the schematic", () => {
  const viewport = { width: 1000, height: 600 };

  it("centres on the midpoint", () => {
    expect(orthoCameraForLink([0, 0], [100, 50], viewport, 0).target).toEqual([
      50, 25, 0,
    ]);
  });

  /**
   * The zoom is solved so the link spans a fixed share of the smaller
   * viewport dimension. deck's orthographic zoom is log2 of the scale, so
   * the pixels-per-unit wanted becomes a zoom by taking the log.
   */
  it("sizes the link to a share of the viewport", () => {
    // 600px high, 40% of it is 240px, across a link 60 units long.
    const { zoom } = orthoCameraForLink([0, 0], [60, 0], viewport, 99);
    expect(zoom).toBeCloseTo(Math.log2(240 / 60), 10);
  });

  it("measures across both axes", () => {
    const straight = orthoCameraForLink([0, 0], [100, 0], viewport, 99).zoom;
    const diagonal = orthoCameraForLink([0, 0], [60, 80], viewport, 99).zoom;
    // Both links are 100 units long, so both frame identically.
    expect(diagonal).toBeCloseTo(straight, 10);
  });

  /** A short link would otherwise be approached without limit. */
  it("stops three steps past the whole-network fit", () => {
    expect(orthoCameraForLink([0, 0], [0.001, 0], viewport, 2).zoom).toBe(5);
  });

  /**
   * The load-bearing one. A pump between coincident nodes is a real thing
   * in these models, and it has no span to solve for — the division is by
   * zero and the camera goes to infinity.
   */
  it("has an answer for a link with no length", () => {
    const { zoom } = orthoCameraForLink([7, 7], [7, 7], viewport, 2);
    expect(Number.isFinite(zoom)).toBe(true);
    expect(zoom).toBe(4);
  });

  it("caps that answer at the ceiling too", () => {
    expect(orthoCameraForLink([0, 0], [0, 0], viewport, 40).zoom).toBe(10);
  });

  it("frames a zero-length link at its own position", () => {
    expect(orthoCameraForLink([7, 7], [7, 7], viewport, 2).target).toEqual([
      7, 7, 0,
    ]);
  });

  /** The smaller dimension governs, so the link fits either way round. */
  it("takes the share from the smaller dimension", () => {
    const wide = orthoCameraForLink(
      [0, 0],
      [60, 0],
      { width: 2000, height: 600 },
      99,
    );
    const tall = orthoCameraForLink(
      [0, 0],
      [60, 0],
      { width: 600, height: 2000 },
      99,
    );
    expect(wide.zoom).toBeCloseTo(tall.zoom, 10);
  });
});

describe("flying to a region", () => {
  /**
   * Both other views have a ring that could be framed — the model's own in
   * a plan view, the placed glyph in a schematic — but neither has the
   * orthographic camera path for it, so the request is dropped rather than
   * aimed at nothing.
   */
  it("is geographic only", () => {
    expect(regionFlyToSupported("map")).toBe(true);
    expect(regionFlyToSupported("schematic")).toBe(false);
  });
});
