import { describe, expect, it } from "vitest";
import { tooltipTextLayout } from "./TooltipPortal";

/**
 * Whether a tooltip is a label or a sentence.
 *
 * Every tooltip used to be `nowrap`, which is right for a label and wrong
 * for an explanation: the legend's motion note is a sentence now that it
 * has stopped being three lines of the panel, and on one line it would run
 * off both edges of the window and be clamped rather than read.
 *
 * The difference is invisible to the other test layers — jsdom measures
 * every box as zero — so this asserts the decision rather than the result.
 */

describe("tooltipTextLayout", () => {
  it("keeps a label on one line", () => {
    for (const label of ["Close", "Basemap", "Edit / move nodes (E)"]) {
      expect(tooltipTextLayout(label)).toEqual({ whiteSpace: "nowrap" });
    }
  });

  it("lets a sentence wrap, within a measure", () => {
    const sentence =
      "Motion follows the water — Flow, Velocity, Status, Unit headloss and " +
      "Quality. Anything else on this map is a still reading.";
    const layout = tooltipTextLayout(sentence);
    expect(layout.whiteSpace).toBe("normal");
    expect(layout.maxWidth).toBeGreaterThan(0);
  });

  it("keeps a criteria summary on one line", () => {
    // The toolbar chip reads its whole standard back on hover, and it is a
    // row of values meant to be scanned across, not prose. It sits just
    // under the threshold on purpose.
    expect(tooltipTextLayout("≥ 14 m · V 0.1–1.5 m/s · Q 0.1–10 L/s")).toEqual({
      whiteSpace: "nowrap",
    });
  });
});
