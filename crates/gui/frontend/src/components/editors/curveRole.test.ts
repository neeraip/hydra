import { describe, expect, it } from "vitest";
import { curveRoleLabel } from "./curveRole";

describe("curveRoleLabel", () => {
  /**
   * The engine sends every role its curve payload can carry. Each must
   * read as what the curve is for — the editor previously showed
   * `single-point`/`three-point`/`multi-point`, which described the
   * curve's shape, so a tank volume curve and a pump head curve were
   * indistinguishable on screen if they happened to have the same number
   * of points.
   */
  it("names every role the engine's curve payload declares", () => {
    const roles = [
      "pump-head",
      "pump-efficiency",
      "tank-volume",
      "gpv-headloss",
      "pcv-loss-ratio",
      "generic",
    ];
    for (const role of roles) {
      const label = curveRoleLabel(role);
      expect(label).not.toBe(role);
      expect(label.length).toBeGreaterThan(0);
    }
  });

  /** Roles are engine vocabulary, so the engine may grow one this map has
   * never seen. It renders as itself — slightly ugly beats blank. */
  it("falls back to the raw role for one it has never seen", () => {
    expect(curveRoleLabel("pump-npsh")).toBe("pump-npsh");
  });

  // An imported curve the model references from nowhere is not a pump
  // curve and must not read as one. (A curve *created* here is: the
  // backend's `create_curve` makes a pump-head curve, so the editor stages
  // adds as `pump-head`, not as this.)
  it("does not present an unreferenced curve as a pump curve", () => {
    expect(curveRoleLabel("generic")).toBe("Unassigned");
    expect(curveRoleLabel("generic").toLowerCase()).not.toContain("pump");
  });
});
