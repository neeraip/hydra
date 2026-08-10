import { describe, expect, it } from "vitest";
import {
  effectiveScaleMode,
  scaleControlShown,
  scaleOptions,
} from "./scaleOptions";

/**
 * Which ranges the legend offers.
 *
 * With a single reporting step, "this step" and "the whole run" are one
 * scale, and a choice between two identical outcomes is not a choice.
 *
 * Criteria used to be a third option here. It answers a different question
 * — magnitude or verdict, rather than which range — and rides its own
 * toggle now, so it neither appears nor disappears with these.
 */

const modes = (o: ReturnType<typeof scaleOptions>) => o.map((x) => x.mode);

describe("the ranges on offer", () => {
  it("offers both over a run with several steps", () => {
    expect(modes(scaleOptions(true))).toEqual(["run", "step"]);
  });

  /** The reported case. */
  it("offers one for a single step", () => {
    expect(modes(scaleOptions(false))).toEqual(["run"]);
  });

  /**
   * "Whole run" is the survivor rather than "This step": on a single step
   * it describes both truthfully, because the whole run is that step.
   */
  it("keeps the option whose label is still true", () => {
    expect(modes(scaleOptions(false))).toEqual(["run"]);
  });

  it("never offers criteria among them", () => {
    // Judging is not a range. Offered here it came and went with the
    // selected variable, so the control displayed one scale while the
    // preference remembered another.
    expect(modes(scaleOptions(true))).not.toContain("criteria");
    expect(modes(scaleOptions(false))).not.toContain("criteria");
  });
});

describe("whether to draw the range control", () => {
  /** One segment offers nothing and cannot be turned off, which reads as a
   *  broken toggle rather than an absent choice. The legend draws the row
   *  anyway when there is a criteria toggle to put beside it. */
  it("hides a control with a single option", () => {
    expect(scaleControlShown(scaleOptions(false))).toBe(false);
  });

  it("shows it whenever there is a choice", () => {
    expect(scaleControlShown(scaleOptions(true))).toBe(true);
  });
});

describe("the scale in force", () => {
  it("keeps a stored preference that is still offered", () => {
    expect(effectiveScaleMode("step", scaleOptions(true))).toBe("step");
  });

  /**
   * A project saved while scrubbing a long run carries `step`, and may be
   * reopened on a scenario that resolved to one snapshot. The preference is
   * unreachable rather than wrong, so it resolves to the option that
   * behaves identically instead of leaving nothing selected.
   */
  it("falls back when the stored preference is no longer offered", () => {
    expect(effectiveScaleMode("step", scaleOptions(false))).toBe("run");
  });
});

/**
 * Criteria are read on the canvas and authored elsewhere, and until this
 * nothing on the canvas said where. A reader who thought a band was wrong
 * had to already know which page owned it.
 */
