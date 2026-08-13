import { describe, expect, it } from "vitest";
import { railGroupBreak, railHeadings } from "./railGroups";

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

/**
 * The headings, which are the engine's word rather than the rail's.
 *
 * The rule and the heading answer different questions — one is derivable
 * and the other is only the engine's to say — so they are asserted apart.
 */
describe("the rail's headings", () => {
  const rail = [
    { class: "point", group: "Nodes" },
    { class: "point", group: "Nodes" },
    { class: "polyline", group: "Links" },
    { class: "collection", group: "Curves and patterns" },
    { class: "collection", group: "Curves and patterns" },
    { class: "collection", group: "Controls" },
  ];

  it("draws a heading only where the group changes", () => {
    expect(railHeadings(rail).map((h) => h.label)).toEqual([
      "Nodes",
      null,
      "Links",
      "Curves and patterns",
      null,
      "Controls",
    ]);
  });

  it("puts the rule where the map kinds stop, wherever the groups fall", () => {
    // The two marks are independent: a group ends at the third entry and
    // the rule falls at the fourth, and neither moved the other.
    expect(railHeadings(rail).map((h) => h.division)).toEqual([
      false,
      false,
      false,
      true,
      false,
      false,
    ]);
  });

  it("gives a run of one its heading", () => {
    // A lone kind under no heading, in a rail where everything else has
    // one, reads as an oversight rather than as a group with one member.
    expect(
      railHeadings([
        { class: "point", group: "Nodes" },
        { class: "point", group: "Rain gages" },
        { class: "polyline", group: "Links" },
      ]).map((h) => h.label),
    ).toEqual(["Nodes", "Rain gages", "Links"]);
  });

  it("draws none for an engine that names no groups", () => {
    // The field is optional, and a rail that ignores it is flat and
    // correct — which is what a catalog written before §4.2.1 gets.
    expect(
      railHeadings([{ class: "point" }, { class: "collection" }]).map(
        (h) => h.label,
      ),
    ).toEqual([null, null]);
  });
});
