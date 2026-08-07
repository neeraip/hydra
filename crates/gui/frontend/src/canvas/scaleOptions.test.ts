import { describe, expect, it } from "vitest";
import {
  criteriaEditShown,
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

/**
 * Criteria are read on the canvas and authored elsewhere, and until this
 * nothing on the canvas said where. A reader who thought a band was wrong
 * had to already know which page owned it.
 */
describe("the route to the criteria editor", () => {
  it("is offered when the engine has criteria", () => {
    expect(criteriaEditShown(["pressure", "velocity", "flow"], true)).toBe(
      true,
    );
  });

  /**
   * The load-bearing one. The Criteria *scale* greys out when the selected
   * variable has no bands, and that is exactly when someone wants to find
   * out what criteria are. A route that vanishes when you need it is worse
   * than no route, so this does not depend on the selection at all.
   */
  it("does not depend on which variable is selected", () => {
    expect(criteriaEditShown(["pressure"], true)).toBe(true);
  });

  /** An engine with no such standard — the registry's existing answer. */
  it("is absent for an engine with no criteria", () => {
    expect(criteriaEditShown([], true)).toBe(false);
    expect(criteriaEditShown(undefined, true)).toBe(false);
  });

  /** And absent where the host offers nowhere to go. */
  it("is absent without a handler", () => {
    expect(criteriaEditShown(["pressure"], false)).toBe(false);
  });
});
