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
