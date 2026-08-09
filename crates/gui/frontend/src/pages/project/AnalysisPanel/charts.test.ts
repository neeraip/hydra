import { describe, expect, it } from "vitest";
import { barLabelGutter } from "./charts";

/**
 * The bar chart's label gutter is sized from the labels themselves. The
 * fixed 88-unit gutter fit the wds bin labels but let the uds band
 * sentences paint out of the SVG across the neighbouring card — the
 * chart deliberately draws with visible overflow, so the gutter is the
 * only thing keeping text inside.
 */

describe("barLabelGutter", () => {
  it("keeps the original width for short bin labels", () => {
    expect(barLabelGutter(["< 20 psi", "20–35 psi", "> 35 psi"])).toBe(88);
  });

  it("widens for sentence labels so they stay inside the chart", () => {
    const gutter = barLabelGutter([
      "Below self-cleansing",
      "Self-cleansing range",
      "Above erosive",
    ]);
    // 20 glyphs at ~6 units plus padding — comfortably past the old 88.
    expect(gutter).toBe(130);
  });

  it("an empty chart keeps the floor", () => {
    expect(barLabelGutter([])).toBe(88);
  });
});
