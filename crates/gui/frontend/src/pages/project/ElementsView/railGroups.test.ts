import { describe, expect, it } from "vitest";
import { railGroupBreak } from "./railGroups";

/**
 * Where the drainage editor's rail parts.
 *
 * The rule separates the kinds placed on the map from the ones that are
 * not. What is worth pinning is the two cases where the honest answer is
 * "nowhere" — a rule with nothing on one side of it is not a divider.
 */

describe("the rail's group break", () => {
  /** The ordinary catalog: spatial kinds, then the collections. */
  it("falls above the first kind that is not on the map", () => {
    expect(
      railGroupBreak([
        "point",
        "polyline",
        "region",
        "collection",
        "collection",
      ]),
    ).toBe(3);
  });

  /** Nothing to part. */
  it("draws no rule when every kind is spatial", () => {
    expect(railGroupBreak(["point", "polyline"])).toBeNull();
  });

  /**
   * And none when every kind is a collection: a rule above the first
   * entry is not a divider, it is a stray line under the heading.
   */
  it("draws no rule above the very first entry", () => {
    expect(railGroupBreak(["collection", "collection"])).toBeNull();
  });

  /** An empty catalog is the loading state, not an empty model. */
  it("draws no rule for an empty rail", () => {
    expect(railGroupBreak([])).toBeNull();
  });

  /**
   * Exactly one rule, wherever the collections begin — the break is a
   * property of the list, so a second run of spatial kinds after the
   * collections would not earn a second line. The catalog does not order
   * itself that way, and if it ever did, one rule is still the right
   * answer for a rail with one grouping.
   */
  it("marks a single break, not one per boundary", () => {
    const classes = ["point", "collection", "point", "collection"];
    expect(railGroupBreak(classes)).toBe(1);
  });
});
