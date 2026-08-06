import { describe, expect, it } from "vitest";
import {
  effectiveScaleMode,
  scaleControlShown,
  scaleOptions,
} from "./scaleOptions";

/**
 * Which scales the legend offers.
 *
 * The legend already declines to offer criteria bands to a variable with
 * none, so the control never presents a scale that would do nothing. A
 * steady-state run is the same case one step further: with a single
 * reporting step, "this step" and "the whole run" are one scale.
 */

const modes = (o: ReturnType<typeof scaleOptions>) => o.map((x) => x.mode);

describe("the scales on offer", () => {
  it("offers both data scales over a run with several steps", () => {
    expect(modes(scaleOptions(false, true))).toEqual(["run", "step"]);
  });

  /** The reported case. */
  it("offers one data scale for a single step", () => {
    expect(modes(scaleOptions(false, false))).toEqual(["run"]);
  });

  /**
   * "Whole run" is the survivor rather than "This step": on a single step
   * it describes both truthfully, because the whole run is that step.
   */
  it("keeps the option whose label is still true", () => {
    expect(modes(scaleOptions(true, false))).toEqual(["run", "criteria"]);
  });

  it("still offers criteria alongside both data scales", () => {
    expect(modes(scaleOptions(true, true))).toEqual([
      "run",
      "step",
      "criteria",
    ]);
  });
});

describe("whether to draw the control", () => {
  /** One segment offers nothing and cannot be turned off, which reads as a
   *  broken toggle rather than an absent choice. */
  it("hides a control with a single option", () => {
    expect(scaleControlShown(scaleOptions(false, false))).toBe(false);
  });

  it("shows it whenever there is a choice", () => {
    expect(scaleControlShown(scaleOptions(true, false))).toBe(true);
    expect(scaleControlShown(scaleOptions(false, true))).toBe(true);
  });
});

describe("the scale in force", () => {
  it("keeps a stored preference that is still offered", () => {
    expect(effectiveScaleMode("step", scaleOptions(false, true))).toBe("step");
  });

  /**
   * A project saved while scrubbing a long run carries `step`, and may be
   * reopened on a scenario that resolved to one snapshot. The preference is
   * unreachable rather than wrong, so it resolves to the option that
   * behaves identically instead of leaving nothing selected.
   */
  it("falls back when the stored preference is no longer offered", () => {
    expect(effectiveScaleMode("step", scaleOptions(false, false))).toBe("run");
    expect(effectiveScaleMode("criteria", scaleOptions(false, true))).toBe(
      "run",
    );
  });
});
