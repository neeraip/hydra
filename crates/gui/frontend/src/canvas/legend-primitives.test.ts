import { describe, expect, it } from "vitest";
import {
  LEGEND_BAR_STYLE,
  LEGEND_POPOVER_STYLE,
  LEGEND_ROOT_STYLE,
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
