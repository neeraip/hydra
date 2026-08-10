import { describe, expect, it } from "vitest";
import {
  LEGEND_BAR_STYLE,
  LEGEND_POPOVER_STYLE,
  LEGEND_ROOT_STYLE,
  rampFractionAt,
  rampScaleOf,
  rampValueAt,
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
 * The bar is one shape wearing three different scales, and two of them are
 * not the obvious one. Getting it wrong would not look broken — it would
 * quietly report a number, which is worse than reporting none.
 */
describe("rampScaleOf", () => {
  const seq = { type: "sequential" };
  const div = { type: "diverging" };
  const banded = { type: "banded" };

  it("runs a sequential bar from min to max", () => {
    expect(rampScaleOf(seq, 40.41, 104.22, false)).toEqual({
      kind: "linear",
      min: 40.41,
      max: 104.22,
    });
  });

  it("centres a diverging bar on zero, not on the run's own range", () => {
    // The gradient is built over −1…+1 and the map scales by the larger
    // magnitude, so a run from −2 to +8 draws −8…+8. Reading it as −2…+8
    // would put zero a third of the way along a bar whose middle is zero.
    expect(rampScaleOf(div, -2, 8, false)).toEqual({
      kind: "symmetric",
      scale: 8,
    });
  });

  it("declines a banded bar drawn in a criterion's colours", () => {
    // Those segments are equal widths, not the thresholds they stand for,
    // so a position names a band and not a value.
    expect(rampScaleOf(banded, 0, 5, true)).toBeNull();
  });

  it("reads a banded bar as a magnitude when it is painted as one", () => {
    // No criterion in play: the same variable takes the sequential ramp,
    // and the position means what it looks like it means.
    expect(rampScaleOf(banded, 0, 5, false)).toEqual({
      kind: "linear",
      min: 0,
      max: 5,
    });
  });

  it("declines the cases with nothing to report", () => {
    expect(rampScaleOf({ type: "categorical" }, 0, 5, false)).toBeNull();
    // A flat run: every position is the same value, and a readout sliding
    // across an unchanging number reads as a fault.
    expect(rampScaleOf(seq, 3, 3, false)).toBeNull();
    expect(rampScaleOf(div, 0, 0, false)).toBeNull();
    expect(rampScaleOf(seq, Number.NaN, 5, false)).toBeNull();
  });
});

describe("rampValueAt", () => {
  it("interpolates a linear scale across the bar", () => {
    const scale = { kind: "linear", min: 40, max: 100 } as const;
    expect(rampValueAt(scale, 0)).toBe(40);
    expect(rampValueAt(scale, 0.5)).toBe(70);
    expect(rampValueAt(scale, 1)).toBe(100);
  });

  it("puts zero at the middle of a symmetric scale", () => {
    const scale = { kind: "symmetric", scale: 8 } as const;
    expect(rampValueAt(scale, 0)).toBe(-8);
    expect(rampValueAt(scale, 0.5)).toBe(0);
    expect(rampValueAt(scale, 1)).toBe(8);
  });

  it("clamps a pointer that ran past either end", () => {
    const scale = { kind: "linear", min: 0, max: 10 } as const;
    expect(rampValueAt(scale, -0.4)).toBe(0);
    expect(rampValueAt(scale, 1.7)).toBe(10);
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
