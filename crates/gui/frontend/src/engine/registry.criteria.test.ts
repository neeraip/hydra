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
   * Finding an element and changing it are two different capabilities,
   * and the registry has a flag for each. Drainage had them conflated:
   * because its model was read-only, "Open in editor" was hidden — so a
   * drainage user could see a conduit on the map and had no way to reach
   * its row, for a reason that was never about editing.
   *
   * A second assertion here used to pin the independence by example,
   * naming drainage as the engine that focused elements and could not
   * edit them. That example expired when drainage learned to edit, and
   * it is gone rather than rewritten: with both engines doing
   * everything, any version of it either restates a literal or invents
   * an engine to assert about. The separation now lives where it is
   * real — `editorFocusesElements` is its own field, read by the
   * inspector on its own, and this test fails if an engine stops
   * setting it.
   */
  it("is offered by every engine with an Editor to focus in", () => {
    for (const key of ["wds", "uds"]) {
      expect(engineComponents(key).editorFocusesElements).toBe(true);
    }
  });
});

describe("hasStarterModel", () => {
  /**
   * The frontend half of a two-sided invariant: the backend's
   * `engine_has_starter_model` refuses to create a project for an
   * engine with nothing to start from, and this is what decides whether
   * the wizard offers that path at all. Neither side can see the other,
   * so a drift only shows if both are pinned — and for one commit they
   * did drift, when this was derived from `editing.create` and drainage
   * learned to create.
   */
  it("is not the same question as whether elements can be created", () => {
    const uds = engineComponents("uds");
    expect(uds.editing.create).toBe(true);
    expect(uds.hasStarterModel).toBe(false);
  });

  it("is true only where a starter model exists", () => {
    expect(engineComponents("wds").hasStarterModel).toBe(true);
    expect(engineComponents("uds").hasStarterModel).toBe(false);
  });
});

describe("undoableEdits", () => {
  /**
   * The undo stack stores its inverses as water-distribution commands —
   * a position patch, a node recreate, a link recreate. Replaying one
   * against another engine's model fails.
   *
   * It was capturing them for drainage anyway. `recreateSpecsForDelete`
   * recognises a node by a hardcoded set of kind names that includes
   * "junction", and finds a conduit in the links array like any other
   * link — so deleting a drainage junction or conduit pushed an entry
   * that read as undoable and refused when applied. Moving a drainage
   * node did the same. Both said "Deleted J2" in the history and then
   * could not do it.
   */
  it("is false for an engine whose edits the stack cannot replay", () => {
    expect(engineComponents("uds").undoableEdits).toBe(false);
  });

  it("is not implied by being able to edit at all", () => {
    // Drainage edits its model completely — geometry, names, values,
    // creation, removal — and none of it is undoable. The two questions
    // were never the same one.
    const uds = engineComponents("uds");
    expect(uds.editing.geometry).toBe(true);
    expect(uds.editing.delete).toBe(true);
    expect(uds.undoableEdits).toBe(false);
  });

  it("is true where the stack was built", () => {
    expect(engineComponents("wds").undoableEdits).toBe(true);
  });
});
