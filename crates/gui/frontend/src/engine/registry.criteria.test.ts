import { describe, expect, it } from "vitest";
import { engineComponents } from "./registry";

/**
 * `criteriaVariables` is gone from the registry.
 *
 * It named three water-distribution variables, which is why a drainage
 * project — with criteria of its own since the contract landed — was
 * never offered the threshold scale. The answer now comes from the two
 * contracts instead: a variable names the criterion that bands it
 * (hydra-common §6.1) and the criterion says what its regions mean
 * (§7.2), so the resolution is `bandsFor` and its tests, and the engines'
 * own tests hold that every banded variable resolves.
 *
 * This is left as a reminder rather than deleted outright: re-adding a
 * per-engine list here would silently take the scale away from every
 * engine not on it, which is exactly the failure it caused.
 */
describe("the retired criteria list", () => {
  it("is not something the registry answers any more", () => {
    expect("criteriaVariables" in engineComponents("wds")).toBe(false);
    expect("criteriaVariables" in engineComponents("uds")).toBe(false);
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
    expect(uds.editing.structure).toBe(false);
    expect(uds.editorFocusesElements).toBe(true);
  });

  it("is offered by every engine with an Editor to focus in", () => {
    for (const key of ["wds", "uds"]) {
      expect(engineComponents(key).editorFocusesElements).toBe(true);
    }
  });
});
