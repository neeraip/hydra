import { describe, expect, it } from "vitest";
import { readCriteriaScale, readScaleMode } from "./CanvasView/canvasPrefs";

/**
 * Three shapes of saved canvas preference, read as two settings.
 *
 * The oldest held `colorMode` (relative | threshold) beside `rangeMode`
 * (run | step). Those were merged into one three-valued `scaleMode`, which
 * has now split back into a range plus a criteria toggle — because they
 * were two questions after all, and the merge dropped the answer to one of
 * them whenever both were given.
 *
 * What must hold across all of it: a reader who left their canvas judging
 * against criteria finds it still judging, and one who left it scaled to
 * the step finds it still scaled to the step. Getting this wrong resets a
 * canvas silently on upgrade, which nobody reports and everybody notices.
 */

describe("readScaleMode", () => {
  it("reads the current key", () => {
    expect(readScaleMode({ scaleMode: "step" })).toBe("step");
    expect(readScaleMode({ scaleMode: "run" })).toBe("run");
  });

  it("gives a saved criteria mode the range it behaved as", () => {
    // It was never a range: nothing but `step` ever rescaled, so a canvas
    // saved in criteria mode was drawing whole-run ranges underneath.
    expect(readScaleMode({ scaleMode: "criteria" })).toBe("run");
  });

  it("migrates the oldest range key", () => {
    expect(readScaleMode({ colorMode: "relative", rangeMode: "step" })).toBe(
      "step",
    );
  });

  it("keeps the range that the merge used to discard", () => {
    // The pre-merge shape could say both. The merge had one slot and chose
    // criteria, losing the step; the split can honour it.
    expect(readScaleMode({ colorMode: "threshold", rangeMode: "step" })).toBe(
      "step",
    );
  });

  it("falls back to the whole run for missing or corrupt prefs", () => {
    expect(readScaleMode(null)).toBe("run");
    expect(readScaleMode({})).toBe("run");
    expect(readScaleMode("nonsense")).toBe("run");
    expect(readScaleMode({ scaleMode: "sideways" })).toBe("run");
  });
});

describe("readCriteriaScale", () => {
  it("reads the current key", () => {
    expect(readCriteriaScale({ criteriaScale: true })).toBe(true);
    expect(readCriteriaScale({ criteriaScale: false })).toBe(false);
  });

  it("keeps a reader judging who was judging under either older shape", () => {
    expect(readCriteriaScale({ scaleMode: "criteria" })).toBe(true);
    expect(readCriteriaScale({ colorMode: "threshold" })).toBe(true);
    expect(
      readCriteriaScale({ colorMode: "threshold", rangeMode: "step" }),
    ).toBe(true);
  });

  it("is off for every shape that was not judging", () => {
    expect(readCriteriaScale({ scaleMode: "step" })).toBe(false);
    expect(readCriteriaScale({ colorMode: "relative" })).toBe(false);
    expect(readCriteriaScale(null)).toBe(false);
    expect(readCriteriaScale({})).toBe(false);
    expect(readCriteriaScale("nonsense")).toBe(false);
  });

  it("prefers the current key over the shapes it replaced", () => {
    // A canvas saved after the split, whose older keys happen to linger.
    expect(
      readCriteriaScale({ criteriaScale: false, scaleMode: "criteria" }),
    ).toBe(false);
  });
});
