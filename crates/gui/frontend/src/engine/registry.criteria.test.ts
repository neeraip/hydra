import { describe, expect, it } from "vitest";
import { engineComponents } from "./registry";

describe("criteriaVariables", () => {
  // The project criteria file is a water-distribution compliance standard.
  it("offers the wds bands to wds", () => {
    expect(engineComponents("wds").criteriaVariables).toEqual([
      "pressure",
      "velocity",
      "flow",
    ]);
  });

  // Both engines publish a variable called `flow` and mean different
  // quantities by it, so matching on id alone handed a drainage map the
  // water-distribution bands — a Criteria scale annotated with numbers
  // from another domain.
  it("offers none to drainage, which has no such standard", () => {
    expect(engineComponents("uds").criteriaVariables).toEqual([]);
  });

  it("falls back to wds for unknown or absent engine keys", () => {
    for (const key of [null, undefined, "och"]) {
      expect(engineComponents(key).criteriaVariables).toContain("pressure");
    }
  });
});

describe("editorFocusesElements", () => {
  /**
   * Finding an element and changing it are two different capabilities, and
   * the registry has a flag for each. Drainage had them conflated: because
   * its model is read-only, "Open in editor" was hidden — so a drainage
   * user could see a conduit on the map and had no way to reach its row,
   * for a reason that was never about editing.
   *
   * If these two ever agree for every engine again, this assertion is the
   * one that notices.
   */
  it("is independent of whether the engine's model can be edited", () => {
    const uds = engineComponents("uds");
    expect(uds.modelEditable).toBe(false);
    expect(uds.editorFocusesElements).toBe(true);
  });

  it("is offered by every engine with an Editor to focus in", () => {
    for (const key of ["wds", "uds"]) {
      expect(engineComponents(key).editorFocusesElements).toBe(true);
    }
  });
});
