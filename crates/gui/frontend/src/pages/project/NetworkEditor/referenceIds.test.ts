import { describe, expect, it } from "vitest";
import { referenceError, referenceIds } from "./referenceIds";

describe("referenceIds", () => {
  /**
   * The reason this is draft-aware rather than a read of the saved
   * network. Curves and patterns are created in the same unsaved draft as
   * the elements pointing at them, so a curve added minutes ago is a
   * perfectly good reference — and one staged for deletion is not.
   * Answering from the saved list alone gets both backwards.
   */
  it("counts staged additions and discounts staged deletions", () => {
    const ids = referenceIds(["C1", "C2"], ["C3"], new Set(["C1"]));
    expect(ids).toEqual(["C2", "C3"]);
  });

  it("is stable in order rather than in creation sequence", () => {
    expect(referenceIds(["b", "a"], ["c"], new Set())).toEqual(["a", "b", "c"]);
  });

  it("does not repeat an id that is both saved and re-added", () => {
    expect(referenceIds(["C1"], ["C1"], new Set())).toEqual(["C1"]);
  });

  /** A deletion wins over an addition of the same id: the draft's last
   * word on that id is that it is going away. */
  it("lets a deletion remove a staged addition", () => {
    expect(referenceIds([], ["C1"], new Set(["C1"]))).toEqual([]);
  });
});

describe("referenceError", () => {
  const allowed = ["Pat1", "Pat2"];

  /** The defect this exists for: a typed id that names nothing committed a
   * dangling reference, and the cell that took it said nothing. */
  it("rejects an id that does not exist", () => {
    expect(referenceError("Pat3", allowed)).toBe("No such id");
    expect(referenceError("pat1", allowed)).toBe("No such id");
  });

  it("accepts one that does", () => {
    expect(referenceError("Pat1", allowed)).toBeNull();
    expect(referenceError("  Pat2  ", allowed)).toBeNull();
  });

  /**
   * Empty is not an error. These references are optional, and clearing the
   * cell is how you say a reservoir has no head pattern — rejecting it
   * would make the field impossible to unset.
   */
  it("accepts empty, which is how a reference is cleared", () => {
    expect(referenceError("", allowed)).toBeNull();
    expect(referenceError("   ", allowed)).toBeNull();
  });

  /** With nothing to reference, every name is wrong — but clearing still
   * works, so the cell never becomes a trap. */
  it("rejects any name when nothing exists to name", () => {
    expect(referenceError("Pat1", [])).toBe("No such id");
    expect(referenceError("", [])).toBeNull();
  });
});
