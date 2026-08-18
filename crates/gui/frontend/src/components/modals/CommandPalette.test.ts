/**
 * Tests for the command palette's find-element search (`searchElements`):
 * early-exit at the per-kind limit (the perf fix for ~46k-element scans),
 * nodes-before-links ordering, case handling, and no-match behavior.
 */
import { describe, expect, it } from "vitest";
import type { Link, Node } from "../../hooks";
import { FIND_MAX_PER_TYPE, searchElements } from "./CommandPalette";

function makeNode(id: string, extra: Partial<Node> = {}): Node {
  return {
    id,
    type: "junction",
    x: 10,
    y: 20,
    pressure: null,
    demand: null,
    ...extra,
  };
}

function makeLink(id: string, extra: Partial<Link> = {}): Link {
  return {
    id,
    type: "pipe",
    fromId: "J1",
    toId: "J2",
    velocity: 0,
    diameter: 150,
    ...extra,
  };
}

const range = (n: number) => Array.from({ length: n }, (_, i) => i);

describe("searchElements", () => {
  it("stops at the per-kind limit even when more elements match", () => {
    const nodes = range(30).map((i) => makeNode(`J${i}`));
    const links = range(30).map((i) => makeLink(`J-pipe-${i}`));

    const matches = searchElements(nodes, links, "j");

    expect(matches).toHaveLength(2 * FIND_MAX_PER_TYPE);
    expect(matches.filter((m) => m.elementType === "node")).toHaveLength(
      FIND_MAX_PER_TYPE,
    );
    expect(matches.filter((m) => m.elementType === "link")).toHaveLength(
      FIND_MAX_PER_TYPE,
    );
    // First matches in array order win — no ranking pass.
    expect(matches[0].id).toBe("J0");
    expect(matches[FIND_MAX_PER_TYPE - 1].id).toBe(`J${FIND_MAX_PER_TYPE - 1}`);
  });

  it("respects an explicit maxPerKind", () => {
    const nodes = range(10).map((i) => makeNode(`N${i}`));
    const links = range(10).map((i) => makeLink(`N-link-${i}`));
    const matches = searchElements(nodes, links, "n", 2);
    expect(matches.map((m) => m.id)).toEqual([
      "N0",
      "N1",
      "N-link-0",
      "N-link-1",
    ]);
  });

  it("lists node matches before link matches", () => {
    const matches = searchElements([makeNode("X1")], [makeLink("X2")], "x");
    expect(matches.map((m) => m.elementType)).toEqual(["node", "link"]);
  });

  it("matches element ids case-insensitively against a lowercased query", () => {
    const nodes = [makeNode("PUMP-Station-1")];
    expect(searchElements(nodes, [], "pump-s")).toHaveLength(1);
    expect(searchElements(nodes, [], "station")).toHaveLength(1);
    // The caller lowercases the query before calling; an uppercase query
    // never matches because ids are compared lowercased. Locked-in contract.
    expect(searchElements(nodes, [], "PUMP")).toHaveLength(0);
  });

  it("returns all elements (up to the limit) for the empty query", () => {
    // Typing just "#" in the palette shows the first elements of each kind.
    const nodes = range(3).map((i) => makeNode(`J${i}`));
    const links = range(3).map((i) => makeLink(`P${i}`));
    expect(searchElements(nodes, links, "")).toHaveLength(6);
  });

  it("returns an empty array when nothing matches", () => {
    const matches = searchElements([makeNode("J1")], [makeLink("P1")], "zzz");
    expect(matches).toEqual([]);
  });

  /**
   * The description no longer repeats the element's type: the row renders
   * that as a badge and a name of its own, and saying it a third time in
   * the description crowded out the coordinates and endpoints the line
   * exists to show. `kind` still carries it — the row needs the kind,
   * just not spelled into prose.
   */
  it("builds the documented description strings", () => {
    const matches = searchElements(
      [makeNode("J1", { type: "tank", x: 3, y: 4 })],
      [makeLink("P1", { type: "valve", fromId: "A", toId: "B", diameter: 80 })],
      "1",
    );
    expect(matches[0]).toEqual({
      id: "J1",
      elementType: "node",
      kind: "tank",
      description: "(3, 4)",
    });
    expect(matches[1]).toEqual({
      id: "P1",
      elementType: "link",
      kind: "valve",
      description: "A → B · ⌀80 mm",
    });
  });

  /**
   * These two answer different questions and have to be free to differ.
   * One word used to do both jobs here, and it meant the opposite of what
   * it means in the rest of the app: `kind` picked the focus path while
   * the badge read from `subtype`. The toast paid for it, reporting
   * "Focused node J9" where the reader wanted the kind.
   */
  it("keeps how an element is focused separate from what it is", () => {
    const matches = searchElements(
      [makeNode("X1", { type: "tank" }), makeNode("X2", { type: "reservoir" })],
      [makeLink("X3", { type: "pump" })],
      "x",
    );
    // One focus path, three kinds: the two nodes agree on elementType and
    // disagree on kind, and the link disagrees on both.
    expect(matches.map((m) => m.elementType)).toEqual(["node", "node", "link"]);
    expect(matches.map((m) => m.kind)).toEqual(["tank", "reservoir", "pump"]);
  });
});
