import { describe, expect, it } from "vitest";
import type { InletCoupling } from "../../hooks";
import { capturedFrom, capturedInto } from "./couplings";

/** Two streets draining into one sewer node, and a third into another. */
const COUPLINGS: InletCoupling[] = [
  { link: "Street1", node: "J5", design: "Grate_P50" },
  { link: "Street2", node: "J5", design: "ComboA" },
  { link: "Street3", node: "J9", design: "Grate_P50" },
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
    expect(capturedInto(COUPLINGS, "Street1")).toEqual([
      { node: "J5", design: "Grate_P50" },
    ]);
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

  /**
   * The design travels with the node, because "captures into J5" says
   * where and not how — and how is what decides how much. It is also an
   * `inlet` registry entry, so the name is a place the reader can go.
   */
  it("carries the inlet design alongside the node", () => {
    expect(capturedInto(COUPLINGS, "Street2")).toEqual([
      { node: "J5", design: "ComboA" },
    ]);
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
      { link: "Street1", node: "J5", design: "Grate_P50" },
      { link: "Street1", node: "J6", design: "ComboA" },
    ];
    expect(capturedInto(many, "Street1").map((c) => c.node)).toEqual([
      "J5",
      "J6",
    ]);
  });
});
