import { describe, expect, it } from "vitest";
import type { InletCoupling } from "../../hooks";
import { capturedFrom, capturedInto } from "./couplings";

/** Two streets draining into one sewer node, and a third into another. */
const COUPLINGS: InletCoupling[] = [
  { link: "Street1", node: "J5" },
  { link: "Street2", node: "J5" },
  { link: "Street3", node: "J9" },
];

describe("inlet couplings", () => {
  /**
   * The direction is the whole point, and it is the thing easiest to get
   * silently backwards: both sides of a coupling are element ids, so
   * matching the wrong field returns a plausible list of the wrong kind of
   * element — link ids where node ids belong — and the inspector would
   * cheerfully offer them as places to go.
   */
  it("reads a coupling from each end without confusing the two", () => {
    expect(capturedInto(COUPLINGS, "Street1")).toEqual(["J5"]);
    expect(capturedFrom(COUPLINGS, "J5")).toEqual(["Street1", "Street2"]);
  });

  it("finds every street feeding one node", () => {
    expect(capturedFrom(COUPLINGS, "J9")).toEqual(["Street3"]);
  });

  /**
   * A coupling joins elements that share no endpoint, so neither an
   * unrelated element nor one that is merely *connected* to a coupled one
   * has a coupling of its own. Returning something here would put a
   * "captures into" section on every conduit in the model.
   */
  it("has nothing to say about an element with no coupling", () => {
    expect(capturedInto(COUPLINGS, "Conduit7")).toEqual([]);
    expect(capturedFrom(COUPLINGS, "J1")).toEqual([]);
  });

  /**
   * A node id is not a link id. Asking for a node's captures — or a link's
   * capturers — must find nothing rather than matching the other column.
   */
  it("does not match an id against the opposite column", () => {
    expect(capturedInto(COUPLINGS, "J5")).toEqual([]);
    expect(capturedFrom(COUPLINGS, "Street1")).toEqual([]);
  });

  /** Nothing to read before the fetch resolves, and no throw either. */
  it("answers empty for a model with no couplings", () => {
    expect(capturedInto([], "Street1")).toEqual([]);
    expect(capturedFrom([], "J5")).toEqual([]);
  });

  /**
   * The format assigns a conduit's inlet one receiving node, but nothing
   * in the data forbids a second row naming the same link — so the answer
   * is a list, not a first-one-wins.
   */
  it("keeps every node a link captures into", () => {
    const many: InletCoupling[] = [
      { link: "Street1", node: "J5" },
      { link: "Street1", node: "J6" },
    ];
    expect(capturedInto(many, "Street1")).toEqual(["J5", "J6"]);
  });
});
