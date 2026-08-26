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
  const ALL_ON = { point: true, polyline: true, region: true, surface: false };
  const ALL_OFF = {
    point: false,
    polyline: false,
    region: false,
    surface: false,
  };

  it("reads the current per-class key", () => {
    expect(readCriteriaScale({ criteriaScale: ALL_ON })).toEqual(ALL_ON);
    expect(
      readCriteriaScale({
        criteriaScale: {
          point: true,
          polyline: false,
          region: false,
          surface: false,
        },
      }),
    ).toEqual({ point: true, polyline: false, region: false, surface: false });
  });

  it("fills in a class the stored object does not mention", () => {
    // Written by an older build, or by hand. An absent class is not
    // judging: the map shows a magnitude, which is what it did before
    // anyone asked for a verdict.
    expect(readCriteriaScale({ criteriaScale: { point: true } })).toEqual({
      point: true,
      polyline: false,
      region: false,
      surface: false,
    });
  });

  it("turns every class on for a reader who was judging before", () => {
    // Three earlier shapes all meant "judge whatever can be judged": a
    // `colorMode` of threshold, a three-valued `scaleMode` of criteria,
    // and the single boolean that briefly replaced it.
    expect(readCriteriaScale({ criteriaScale: true })).toEqual(ALL_ON);
    expect(readCriteriaScale({ scaleMode: "criteria" })).toEqual(ALL_ON);
    expect(readCriteriaScale({ colorMode: "threshold" })).toEqual(ALL_ON);
    expect(
      readCriteriaScale({ colorMode: "threshold", rangeMode: "step" }),
    ).toEqual(ALL_ON);
  });

  it("is off for every shape that was not judging", () => {
    expect(readCriteriaScale({ criteriaScale: false })).toEqual(ALL_OFF);
    expect(readCriteriaScale({ scaleMode: "step" })).toEqual(ALL_OFF);
    expect(readCriteriaScale({ colorMode: "relative" })).toEqual(ALL_OFF);
    expect(readCriteriaScale(null)).toEqual(ALL_OFF);
    expect(readCriteriaScale({})).toEqual(ALL_OFF);
    expect(readCriteriaScale("nonsense")).toEqual(ALL_OFF);
  });

  it("prefers the current key over the shapes it replaced", () => {
    expect(
      readCriteriaScale({ criteriaScale: ALL_OFF, scaleMode: "criteria" }),
    ).toEqual(ALL_OFF);
  });
});
