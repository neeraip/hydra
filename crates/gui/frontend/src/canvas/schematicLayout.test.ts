import { describe, expect, it } from "vitest";
import type { Link, Node } from "../types";
import { computeSchematicLayout } from "./schematicLayout";

// ── helpers ────────────────────────────────────────────────────────────────────

function junction(id: string): Node {
  return { id, type: "junction", x: 0, y: 0, pressure: null, demand: null };
}

function reservoir(id: string): Node {
  return { id, type: "reservoir", x: 0, y: 0, pressure: null, demand: null };
}

function tank(id: string): Node {
  return { id, type: "tank", x: 0, y: 0, pressure: null, demand: null };
}

function pipe(id: string, from: string, to: string): Link {
  return {
    id,
    type: "pipe",
    fromId: from,
    toId: to,
    velocity: 0,
    diameter: 100,
  };
}

function getLayoutPoint(
  layout: Map<string, [number, number]>,
  id: string,
): [number, number] {
  const point = layout.get(id);
  if (!point) {
    throw new Error(`Missing layout point for ${id}`);
  }
  return point;
}

// ── empty input ───────────────────────────────────────────────────────────────

describe("computeSchematicLayout – empty input", () => {
  it("returns an empty map for no nodes and no links", () => {
    const layout = computeSchematicLayout([], []);
    expect(layout.size).toBe(0);
  });
});

// ── single node ───────────────────────────────────────────────────────────────

describe("computeSchematicLayout – single node", () => {
  it("assigns a position to the lone node", () => {
    const layout = computeSchematicLayout([junction("j1")], []);
    expect(layout.has("j1")).toBe(true);
    const [x, y] = getLayoutPoint(layout, "j1");
    expect(typeof x).toBe("number");
    expect(typeof y).toBe("number");
  });
});

// ── linear chain ─────────────────────────────────────────────────────────────

describe("computeSchematicLayout – linear chain R → J1 → J2", () => {
  const nodes = [reservoir("R"), junction("J1"), junction("J2")];
  const links = [pipe("P1", "R", "J1"), pipe("P2", "J1", "J2")];
  const layout = computeSchematicLayout(nodes, links);

  it("assigns a position to every node", () => {
    expect(layout.size).toBe(3);
    for (const n of nodes) expect(layout.has(n.id)).toBe(true);
  });

  it("reservoir is at depth 0 (leftmost x)", () => {
    const [rX] = getLayoutPoint(layout, "R");
    const [j1X] = getLayoutPoint(layout, "J1");
    const [j2X] = getLayoutPoint(layout, "J2");
    expect(rX).toBeLessThan(j1X);
    expect(j1X).toBeLessThan(j2X);
  });

  it("nodes at different depths have strictly increasing x", () => {
    const xs = ["R", "J1", "J2"].map((id) => getLayoutPoint(layout, id)[0]);
    for (let i = 1; i < xs.length; i++) {
      expect(xs[i]).toBeGreaterThan(xs[i - 1]);
    }
  });
});

// ── branching network ─────────────────────────────────────────────────────────

describe("computeSchematicLayout – branching network", () => {
  //   R ─ J1 ─ J2
  //        └─ J3
  const nodes = [
    reservoir("R"),
    junction("J1"),
    junction("J2"),
    junction("J3"),
  ];
  const links = [
    pipe("P1", "R", "J1"),
    pipe("P2", "J1", "J2"),
    pipe("P3", "J1", "J3"),
  ];
  const layout = computeSchematicLayout(nodes, links);

  it("assigns a position to all 4 nodes", () => {
    expect(layout.size).toBe(4);
  });

  it("J2 and J3 are at the same BFS depth (same x)", () => {
    const [x2] = getLayoutPoint(layout, "J2");
    const [x3] = getLayoutPoint(layout, "J3");
    expect(x2).toBe(x3);
  });

  it("J2 and J3 are at different y positions", () => {
    const [, y2] = getLayoutPoint(layout, "J2");
    const [, y3] = getLayoutPoint(layout, "J3");
    expect(y2).not.toBe(y3);
  });
});

// ── disconnected graph ────────────────────────────────────────────────────────

describe("computeSchematicLayout – disconnected graph", () => {
  // Two totally separate sub-networks.
  const nodes = [
    reservoir("R1"),
    junction("J1"),
    reservoir("R2"),
    junction("J2"),
  ];
  const links = [pipe("P1", "R1", "J1"), pipe("P2", "R2", "J2")];
  const layout = computeSchematicLayout(nodes, links);

  it("assigns a position to every node even when disconnected", () => {
    expect(layout.size).toBe(4);
    for (const n of nodes) expect(layout.has(n.id)).toBe(true);
  });
});

// ── reservoir / tank priority as BFS root ─────────────────────────────────────

describe("computeSchematicLayout – reservoir/tank is BFS root", () => {
  it("places the reservoir at x = 0 (depth 0) for a simple chain", () => {
    const nodes = [junction("J1"), junction("J2"), reservoir("R")];
    const links = [pipe("P1", "R", "J1"), pipe("P2", "J1", "J2")];
    const layout = computeSchematicLayout(nodes, links);
    const [rX] = getLayoutPoint(layout, "R");
    expect(rX).toBe(0);
  });

  it("tanks are also valid BFS roots", () => {
    const nodes = [tank("T"), junction("J1")];
    const links = [pipe("P1", "T", "J1")];
    const layout = computeSchematicLayout(nodes, links);
    const [tX] = getLayoutPoint(layout, "T");
    const [jX] = getLayoutPoint(layout, "J1");
    expect(tX).toBeLessThan(jX);
  });
});

// ── no source nodes falls back to first node as root ─────────────────────────

describe("computeSchematicLayout – all junctions (no reservoir/tank)", () => {
  it("still assigns positions to all nodes", () => {
    const nodes = [junction("J1"), junction("J2"), junction("J3")];
    const links = [pipe("P1", "J1", "J2"), pipe("P2", "J2", "J3")];
    const layout = computeSchematicLayout(nodes, links);
    expect(layout.size).toBe(3);
    for (const n of nodes) expect(layout.has(n.id)).toBe(true);
  });
});

// ── spacing multiplier ───────────────────────────────────────────────────────

describe("computeSchematicLayout – spacing", () => {
  const nodes = [reservoir("R1"), junction("J1"), junction("J2")];
  const links = [pipe("P1", "R1", "J1"), pipe("P2", "R1", "J2")];

  it("defaults to the layout it has always produced", () => {
    // Anyone who never touches the sliders must see the original layout, so the
    // identity scale has to be exactly 1 rather than merely close to it.
    const base = computeSchematicLayout(nodes, links);
    const explicit = computeSchematicLayout(nodes, links, { x: 1, y: 1 });
    for (const [id, [x, y]] of base) {
      expect(explicit.get(id)).toEqual([x, y]);
    }
  });

  it("scales each axis independently and linearly", () => {
    // The design rests on this: scaling the spacing constants is the same as
    // scaling the output per axis, so no BFS re-run is needed, and X can move
    // without dragging Y with it.
    const base = computeSchematicLayout(nodes, links);
    for (const [kx, ky] of [
      [0.25, 1],
      [1, 0.25],
      [4, 1],
      [1, 4],
      [0.5, 2],
    ] as const) {
      const scaled = computeSchematicLayout(nodes, links, { x: kx, y: ky });
      expect(scaled.size).toBe(base.size);
      for (const [id, [x, y]] of base) {
        const got = scaled.get(id);
        expect(got?.[0]).toBeCloseTo(x * kx, 10);
        expect(got?.[1]).toBeCloseTo(y * ky, 10);
      }
    }
  });

  it("reshapes the layout — an equal scale would only be a zoom", () => {
    // Stretching one axis must change the bounding box's proportions. Scaling
    // both equally cannot: that is exactly what zoom already does.
    const extent = (m: Map<string, [number, number]>) => {
      const xs = [...m.values()].map(([x]) => x);
      const ys = [...m.values()].map(([, y]) => y);
      const w = Math.max(...xs) - Math.min(...xs);
      const h = Math.max(...ys) - Math.min(...ys);
      return h === 0 ? Number.POSITIVE_INFINITY : w / h;
    };
    const base = extent(computeSchematicLayout(nodes, links));
    const widened = extent(
      computeSchematicLayout(nodes, links, { x: 4, y: 1 }),
    );
    const heightened = extent(
      computeSchematicLayout(nodes, links, { x: 1, y: 4 }),
    );
    expect(widened).toBeGreaterThan(base);
    expect(heightened).toBeLessThan(base);

    const uniform = extent(
      computeSchematicLayout(nodes, links, { x: 3, y: 3 }),
    );
    expect(uniform).toBeCloseTo(base, 10);
  });
});
