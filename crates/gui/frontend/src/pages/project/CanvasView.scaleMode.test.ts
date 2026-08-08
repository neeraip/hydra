import { describe, expect, it } from "vitest";
import { readScaleMode } from "./CanvasView/canvasPrefs";

describe("readScaleMode", () => {
  it("reads the merged key", () => {
    expect(readScaleMode({ scaleMode: "step" })).toBe("step");
    expect(readScaleMode({ scaleMode: "criteria" })).toBe("criteria");
  });

  // The two pre-merge keys asked one question with two of the three
  // answers; a user who left the canvas in threshold mode must find it
  // still pinned to their bands after the upgrade, not silently reset.
  it("migrates the pre-merge colorMode", () => {
    expect(readScaleMode({ colorMode: "threshold" })).toBe("criteria");
    expect(readScaleMode({ colorMode: "relative" })).toBe("run");
  });

  it("migrates the pre-merge rangeMode when no criteria mode was set", () => {
    expect(readScaleMode({ colorMode: "relative", rangeMode: "step" })).toBe(
      "step",
    );
  });

  // Criteria ignores the data range entirely, so it cannot be combined
  // with a range mode — the pinned scale is what the user chose to see.
  it("prefers criteria over a stored range mode", () => {
    expect(readScaleMode({ colorMode: "threshold", rangeMode: "step" })).toBe(
      "criteria",
    );
  });

  it("falls back to the whole run for missing or corrupt prefs", () => {
    expect(readScaleMode(null)).toBe("run");
    expect(readScaleMode({})).toBe("run");
    expect(readScaleMode("nonsense")).toBe("run");
    expect(readScaleMode({ scaleMode: "sideways" })).toBe("run");
  });
});
