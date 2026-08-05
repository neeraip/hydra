import { describe, expect, it } from "vitest";
import { UNKNOWN_CURVE_AXES } from "../../hooks";
import { stagedCurveRole } from "./CurveEditor";

describe("curve axes resolution", () => {
  /**
   * The frontend half of a cross-boundary claim: a curve created here is a
   * pump-head curve. The editor stages the add under this role and looks
   * its axes up by it, so if `create_curve` ever made something else, a new
   * curve would render under the wrong axes until saved. The Rust half is
   * `a_created_curve_is_the_kind_the_editor_stages_it_as`.
   */
  it("stages a new curve as the kind the backend creates", () => {
    expect(stagedCurveRole).toBe("pump-head");
  });

  /**
   * The fallback must never be a guessed unit.
   *
   * A staged curve used to carry hand-written axes with `quantity:
   * undefined`, which reads as "unitless": on a US project its points were
   * shown unconverted under bare labels, and a value typed as gpm was
   * stored as L/s. Bare X/Y is the honest answer when the kind is unknown —
   * it converts nothing and promises nothing.
   */
  it("falls back to unitless magnitudes, not to pump axes", () => {
    expect(UNKNOWN_CURVE_AXES.map((a) => a.label)).toEqual(["X", "Y"]);
    for (const axis of UNKNOWN_CURVE_AXES) {
      expect(axis.quantity).toBeUndefined();
    }
  });
});
