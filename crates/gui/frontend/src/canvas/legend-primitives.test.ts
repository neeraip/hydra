import { describe, expect, it } from "vitest";
import {
  LEGEND_BAR_STYLE,
  LEGEND_POPOVER_STYLE,
  LEGEND_ROOT_STYLE,
  rampFractionAt,
  rampReadingAt,
  rampScaleOf,
} from "./legend-primitives";

/**
 * The legend floats over the canvas, and hit testing works on a box rather
 * than on painted pixels. Its root shrink-wraps around both the control
 * bar and the ramp popover, so with the popover open the empty rectangle
 * beside the narrower of the two was still swallowing every click, drag
 * and hover meant for the map behind it — the canvas simply stopped
 * responding over a wide strip whenever the legend was expanded.
 *
 * Asserted against the exported styles themselves so the test cannot drift
 * from what ships.
 */

describe("the legend's pointer surface", () => {
  it("lets the pointer through its container", () => {
    expect(LEGEND_ROOT_STYLE.pointerEvents).toBe("none");
  });

  it("keeps the parts a reader actually touches solid", () => {
    // Both, and each on its own: the bar is present whenever the legend
    // is, the popover only when expanded, and neither can rely on the
    // other for its events now that the root passes them through.
    expect(LEGEND_BAR_STYLE.pointerEvents).toBe("auto");
    expect(LEGEND_POPOVER_STYLE.pointerEvents).toBe("auto");
  });
});

/**
 * Reading a value off the colour bar.
 *
 * The bar is one shape wearing two different meanings, and the second one
 * would not look broken if it were wrong — it would quietly report a
 * number, which is worse than reporting none.
 */
describe("rampScaleOf", () => {
  const seq = { type: "sequential" };
  const div = { type: "diverging" };
  const banded = { type: "banded" };

  it("runs a sequential bar from min to max", () => {
    expect(rampScaleOf(seq, 40.41, 104.22, null)).toEqual({
      kind: "linear",
      min: 40.41,
      max: 104.22,
    });
  });

  it("reads a diverging bar as its own range too", () => {
    // Its gradient is clipped to the same range the end labels state, so
    // the bar carries exactly those values. Drawn whole it did not: a run
    // of 40…104 was painted over a ramp whose left edge is −104, and the
    // hover readout said so out loud.
    expect(rampScaleOf(div, 40.41, 104.22, null)).toEqual({
      kind: "linear",
      min: 40.41,
      max: 104.22,
    });
  });

  it("reads a banded bar as its regions", () => {
    // Equal-width segments standing for cuts that are not evenly spaced,
    // so a position names a region and interpolation would invent values.
    expect(rampScaleOf(banded, 0, 5, [0.6, 3])).toEqual({
      kind: "bands",
      cuts: [0.6, 3],
    });
  });

  it("reads a banded bar as a magnitude when painted as one", () => {
    // No criterion in play: the same variable takes the sequential ramp.
    expect(rampScaleOf(banded, 0, 5, null)).toEqual({
      kind: "linear",
      min: 0,
      max: 5,
    });
  });

  it("declines the cases with nothing to report", () => {
    expect(rampScaleOf({ type: "categorical" }, 0, 5, null)).toBeNull();
    // A flat run: every position is the same value, and a readout sliding
    // across an unchanging number reads as a fault.
    expect(rampScaleOf(seq, 3, 3, null)).toBeNull();
    expect(rampScaleOf(seq, Number.NaN, 5, null)).toBeNull();
  });
});

describe("rampReadingAt", () => {
  it("interpolates a linear scale across the bar", () => {
    const scale = { kind: "linear", min: 40, max: 100 } as const;
    expect(rampReadingAt(scale, 0)).toEqual({ kind: "value", value: 40 });
    expect(rampReadingAt(scale, 0.5)).toEqual({ kind: "value", value: 70 });
    expect(rampReadingAt(scale, 1)).toEqual({ kind: "value", value: 100 });
  });

  it("names the region a banded position falls in", () => {
    // Two cuts, three equal-width regions: the ends are open, because
    // there is no further cut to bound them.
    const scale = { kind: "bands", cuts: [0.6, 3] } as const;
    expect(rampReadingAt(scale, 0.1)).toEqual({
      kind: "band",
      from: null,
      to: 0.6,
    });
    expect(rampReadingAt(scale, 0.5)).toEqual({
      kind: "band",
      from: 0.6,
      to: 3,
    });
    expect(rampReadingAt(scale, 0.9)).toEqual({
      kind: "band",
      from: 3,
      to: null,
    });
  });

  it("clamps a pointer that ran past either end", () => {
    const scale = { kind: "linear", min: 0, max: 10 } as const;
    expect(rampReadingAt(scale, -0.4)).toEqual({ kind: "value", value: 0 });
    expect(rampReadingAt(scale, 1.7)).toEqual({ kind: "value", value: 10 });
    // Including the last band, which the naive floor() would overrun.
    expect(rampReadingAt({ kind: "bands", cuts: [1] }, 1)).toEqual({
      kind: "band",
      from: 1,
      to: null,
    });
  });
});

describe("rampFractionAt", () => {
  const rect = (left: number, width: number) => ({ left, width }) as DOMRect;

  it("measures from the bar's own left edge", () => {
    expect(rampFractionAt(120, rect(100, 200))).toBeCloseTo(0.1);
    expect(rampFractionAt(200, rect(100, 200))).toBeCloseTo(0.5);
  });

  it("clamps rather than reporting outside the bar", () => {
    expect(rampFractionAt(40, rect(100, 200))).toBe(0);
    expect(rampFractionAt(400, rect(100, 200))).toBe(1);
  });

  it("answers nothing for a box that has not been laid out", () => {
    // The bar is measured from the live document and a hover can arrive
    // before layout; dividing by zero would report NaN as a value.
    expect(rampFractionAt(120, rect(0, 0))).toBeNull();
  });
});
