/**
 * Tests for the pure `network-changed` delta path in NetworkDataContext:
 * patched element DTOs replace matching entries in the node/link arrays in
 * place of a full snapshot refetch; patches for unknown ids are stale by
 * construction and dropped (never appended). The perf contract under test:
 * untouched entries keep their object identity, and arrays with no matching
 * patches are returned by reference.
 */
import { describe, expect, it } from "vitest";
import type { Link, Node } from "../types";
import {
  applyElementDeltas,
  normalizeNodes,
  patchAllById,
  patchById,
} from "./NetworkDataContext";

function makeNode(id: string, extra: Partial<Node> = {}): Node {
  return {
    id,
    type: "junction",
    x: 0,
    y: 0,
    pressure: null,
    demand: null,
    ...extra,
  };
}

function makeLink(id: string, extra: Partial<Link> = {}): Link {
  return {
    id,
    type: "pipe",
    fromId: "A",
    toId: "B",
    velocity: 0,
    diameter: 100,
    ...extra,
  };
}

describe("applyElementDeltas", () => {
  it("replaces a node in place, preserving order and untouched identities", () => {
    const nodes = [makeNode("J1"), makeNode("J2"), makeNode("J3")];
    const links = [makeLink("P1")];
    const updated = makeNode("J2", { x: 42 });

    const next = applyElementDeltas(nodes, links, [{ node: updated }]);

    expect(next.nodes.map((n) => n.id)).toEqual(["J1", "J2", "J3"]);
    expect(next.nodes[1]).toBe(updated); // patched slot holds the new DTO
    expect(next.nodes[0]).toBe(nodes[0]); // untouched entries: same references
    expect(next.nodes[2]).toBe(nodes[2]);
    expect(next.nodes).not.toBe(nodes); // new array for React identity
    expect(next.links).toBe(links); // no link patches → same array reference
  });

  it("replaces a link in place and leaves nodes by reference", () => {
    const nodes = [makeNode("J1")];
    const links = [makeLink("P1"), makeLink("P2")];
    const updated = makeLink("P1", { diameter: 250 });

    const next = applyElementDeltas(nodes, links, [{ link: updated }]);

    expect(next.links[0]).toBe(updated);
    expect(next.links[1]).toBe(links[1]);
    expect(next.links).not.toBe(links);
    expect(next.nodes).toBe(nodes);
  });

  it("drops elements with unknown ids instead of appending them", () => {
    // Creates/deletes always emit payload-less events (full-refetch signal),
    // so a delta for an id we don't hold is necessarily stale — appending it
    // used to create "ghost" elements (e.g. resurrecting a deleted node).
    const nodes = [makeNode("J1")];
    const links = [makeLink("P1")];
    const staleNode = makeNode("J9");
    const staleLink = makeLink("P9");

    const next = applyElementDeltas(nodes, links, [
      { node: staleNode },
      { link: staleLink },
    ]);

    expect(next.nodes.map((n) => n.id)).toEqual(["J1"]);
    expect(next.links.map((l) => l.id)).toEqual(["P1"]);
    // Nothing matched → input arrays are returned by reference.
    expect(next.nodes).toBe(nodes);
    expect(next.links).toBe(links);
  });

  it("applies known-id patches while dropping unknown-id ones in the same payload", () => {
    const nodes = [makeNode("J1"), makeNode("J2")];
    const updated = makeNode("J2", { x: 9 });
    const stale = makeNode("J9");

    const next = applyElementDeltas(
      nodes,
      [],
      [{ node: updated }, { node: stale }],
    );

    expect(next.nodes.map((n) => n.id)).toEqual(["J1", "J2"]);
    expect(next.nodes[1]).toBe(updated);
    expect(next.nodes[0]).toBe(nodes[0]);
  });

  it("applies multiple patches from one payload in order", () => {
    const nodes = [makeNode("J1"), makeNode("J2")];
    const first = makeNode("J1", { x: 1 });
    const second = makeNode("J1", { x: 2 });

    const next = applyElementDeltas(
      nodes,
      [],
      [{ node: first }, { node: second }],
    );

    // Later patches for the same id win.
    expect(next.nodes[0]).toBe(second);
    expect(next.nodes).toHaveLength(2);
  });

  it("normalises omitted pressure/demand on patched nodes to null", () => {
    // The backend omits always-null optionals from DTO JSON; consumers use
    // strict `!== null` comparisons, so the delta path must fill them in.
    const existing = { id: "J1", type: "junction", x: 0, y: 0 } as Node;
    const bare = { id: "J1", type: "junction", x: 5, y: 0 } as Node;
    const next = applyElementDeltas([existing], [], [{ node: bare }]);
    expect(next.nodes[0].x).toBe(5);
    expect(next.nodes[0].pressure).toBeNull();
    expect(next.nodes[0].demand).toBeNull();
  });

  it("returns both arrays by reference for an empty delta", () => {
    const nodes = [makeNode("J1")];
    const links = [makeLink("P1")];
    const next = applyElementDeltas(nodes, links, []);
    expect(next.nodes).toBe(nodes);
    expect(next.links).toBe(links);
  });
});

describe("patchById / patchAllById", () => {
  it("patchById replaces by id without mutating the input array", () => {
    const items = [makeNode("A"), makeNode("B")];
    const replacement = makeNode("B", { x: 7 });
    const next = patchById(items, replacement);
    expect(next[1]).toBe(replacement);
    expect(items[1]).not.toBe(replacement); // input untouched
  });

  it("patchById returns the input array unchanged for an unknown id", () => {
    const items = [makeNode("A")];
    expect(patchById(items, makeNode("Z"))).toBe(items);
    expect(items).toHaveLength(1);
  });

  it("patchAllById returns the input array unchanged for no updates", () => {
    const items = [makeNode("A")];
    expect(patchAllById(items, [])).toBe(items);
  });

  it("patchAllById returns the input array unchanged when nothing matches", () => {
    const items = [makeNode("A")];
    expect(patchAllById(items, [makeNode("X"), makeNode("Y")])).toBe(items);
  });
});

describe("normalizeNodes", () => {
  it("fills in undefined pressure/demand, keeps explicit values", () => {
    const n1 = { id: "J1", type: "junction", x: 0, y: 0 } as Node;
    const n2 = makeNode("J2", { pressure: 12.5, demand: 3 });
    const out = normalizeNodes([n1, n2]);
    expect(out[0].pressure).toBeNull();
    expect(out[0].demand).toBeNull();
    expect(out[1].pressure).toBe(12.5);
    expect(out[1].demand).toBe(3);
    // Mutates in place and returns the same array.
    expect(out[0]).toBe(n1);
  });
});
