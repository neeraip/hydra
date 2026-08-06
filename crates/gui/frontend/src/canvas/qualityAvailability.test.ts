import { describe, expect, it } from "vitest";
import {
  LINK_WITHOUT_QUALITY,
  NODE_WITHOUT_QUALITY,
  type VariableSelection,
  withQualityAvailability,
} from "./qualityAvailability";

/**
 * The reported defect: the legend showed Quality while the hover chip and
 * the element inspector both showed velocity.
 *
 * It was held in two places — what the legend's picker shows and what the
 * canvas paints — and the correction that moves a selection off quality on
 * a run without it moved only the second. Neither store looked wrong on its
 * own, which is why it survived: the legend named one variable, everything
 * else showed another, and nothing on screen said which to believe.
 *
 * The canvas now derives its variable from this selection, so there is
 * nothing left for the correction to leave behind. What is asserted here is
 * what remains its own decision: which variable a run without quality falls
 * back to, and that everything else in the selection survives the trip.
 */

const on = { point: "quality", polyline: "quality", region: "runoff" };

describe("a run with quality results", () => {
  it("leaves the selection alone", () => {
    expect(withQualityAvailability(on, true)).toBe(on);
  });

  it("leaves a non-quality selection alone too", () => {
    const v = { point: "head", polyline: "flow", region: "" };
    expect(withQualityAvailability(v, true)).toBe(v);
    expect(withQualityAvailability(v, false)).toBe(v);
  });
});

describe("a run with no quality results", () => {
  it("moves both classes off quality", () => {
    const out = withQualityAvailability(on, false);
    expect(out.point).toBe(NODE_WITHOUT_QUALITY);
    expect(out.polyline).toBe(LINK_WITHOUT_QUALITY);
  });

  it("moves one class without disturbing the other", () => {
    const out = withQualityAvailability(
      { point: "head", polyline: "quality", region: "" },
      false,
    );
    expect(out.point).toBe("head");
    expect(out.polyline).toBe(LINK_WITHOUT_QUALITY);
  });

  /**
   * Regions are selected too, and a correction that only knew about the two
   * classes it was written for would drop that choice on the way past.
   */
  it("carries the rest of the selection through", () => {
    expect(withQualityAvailability(on, false).region).toBe("runoff");
  });

  /**
   * An untouched selection must come back by identity, not as an equal
   * copy: the effect that applies this depends on the state it sets, and a
   * fresh object every time would never settle.
   */
  it("returns the same object when nothing needs changing", () => {
    const v = { point: "pressure", polyline: "flow", region: "" };
    expect(withQualityAvailability(v, false)).toBe(v);
  });

  /** The legend starts empty, before a catalog has been chosen from. */
  it("leaves an empty selection empty", () => {
    const v: VariableSelection = { point: "", polyline: "" };
    expect(withQualityAvailability(v, false)).toBe(v);
  });
});
