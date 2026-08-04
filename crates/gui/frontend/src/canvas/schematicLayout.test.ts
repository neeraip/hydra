import { describe, expect, it } from "vitest";
import type { Link, Node, Region } from "../types";
import { computeSchematicLayout } from "./schematicLayout";

/** Positions only — most assertions here are about coordinates. */
const layoutOf = (
  nodes: Node[],
  links: Link[],
  scale?: { x: number; y: number },
) => computeSchematicLayout(nodes, links, scale).positions;

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
    const layout = layoutOf([], []);
    expect(layout.size).toBe(0);
  });
});

// ── single node ───────────────────────────────────────────────────────────────

describe("computeSchematicLayout – single node", () => {
  it("assigns a position to the lone node", () => {
    const layout = layoutOf([junction("j1")], []);
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
  const layout = layoutOf(nodes, links);

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
  const layout = layoutOf(nodes, links);

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
  const layout = layoutOf(nodes, links);

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
    const layout = layoutOf(nodes, links);
    const [rX] = getLayoutPoint(layout, "R");
    expect(rX).toBe(0);
  });

  it("tanks are also valid BFS roots", () => {
    const nodes = [tank("T"), junction("J1")];
    const links = [pipe("P1", "T", "J1")];
    const layout = layoutOf(nodes, links);
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
    const layout = layoutOf(nodes, links);
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
    const base = layoutOf(nodes, links);
    const explicit = layoutOf(nodes, links, { x: 1, y: 1 });
    for (const [id, [x, y]] of base) {
      expect(explicit.get(id)).toEqual([x, y]);
    }
  });

  it("scales each axis independently and linearly", () => {
    // The design rests on this: scaling the spacing constants is the same as
    // scaling the output per axis, so no BFS re-run is needed, and X can move
    // without dragging Y with it.
    const base = layoutOf(nodes, links);
    for (const [kx, ky] of [
      [0.25, 1],
      [1, 0.25],
      [4, 1],
      [1, 4],
      [0.5, 2],
    ] as const) {
      const scaled = layoutOf(nodes, links, { x: kx, y: ky });
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
    const base = extent(layoutOf(nodes, links));
    const widened = extent(layoutOf(nodes, links, { x: 4, y: 1 }));
    const heightened = extent(layoutOf(nodes, links, { x: 1, y: 4 }));
    expect(widened).toBeGreaterThan(base);
    expect(heightened).toBeLessThan(base);

    const uniform = extent(layoutOf(nodes, links, { x: 3, y: 3 }));
    expect(uniform).toBeCloseTo(base, 10);
  });
});

// ── detached parts form their own group below the network ────────────────────

describe("computeSchematicLayout – detached nodes", () => {
  const at = (m: Map<string, [number, number]>, id: string) =>
    m.get(id) ?? [Number.NaN, Number.NaN];

  it("puts an orphaned node right of the connected network, not among the sources", () => {
    // The case that prompted this: deleting a leaf's only link. At depth 0 it
    // landed in the leftmost column beside the reservoirs, which on a wide
    // network reads as the node having vanished.
    const nodes = [
      reservoir("R1"),
      junction("J1"),
      junction("J2"),
      junction("ORPHAN"),
    ];
    const links = [pipe("P1", "R1", "J1"), pipe("P2", "J1", "J2")];
    const layout = layoutOf(nodes, links);

    expect(layout.size).toBe(4);
    const connectedMaxX = Math.max(
      ...["R1", "J1", "J2"].map((id) => at(layout, id)[0]),
    );
    expect(at(layout, "ORPHAN")[0]).toBeGreaterThan(connectedMaxX);
  });

  it("separates the group by more than a column on a wide network", () => {
    // A fixed few columns of gap is invisible beside a layout thousands of units
    // wide — the size at which finding a stray node matters most.
    const nodes = [reservoir("R1"), junction("ORPHAN")];
    const links = [];
    // A 60-deep chain: one very wide connected network.
    for (let i = 0; i < 60; i++) {
      nodes.push(junction(`J${i}`));
      links.push(pipe(`P${i}`, i === 0 ? "R1" : `J${i - 1}`, `J${i}`));
    }
    const layout = layoutOf(nodes, links);

    const xs = [...layout.entries()]
      .filter(([id]) => id !== "ORPHAN")
      .map(([, [x]]) => x);
    const connectedMaxX = Math.max(...xs);
    const connectedWidth = connectedMaxX - Math.min(...xs);
    const gap = at(layout, "ORPHAN")[0] - connectedMaxX;
    expect(gap).toBeGreaterThan(connectedWidth * 0.05);
  });

  it("keeps a detached sub-network's own shape", () => {
    // A chain with no source is still a chain: it must not collapse into a
    // single column just because nothing feeds it.
    const nodes = [
      reservoir("R1"),
      junction("J1"),
      junction("D1"),
      junction("D2"),
      junction("D3"),
    ];
    const links = [
      pipe("P1", "R1", "J1"),
      pipe("D-a", "D1", "D2"),
      pipe("D-b", "D2", "D3"),
    ];
    const layout = layoutOf(nodes, links);

    expect(at(layout, "D2")[0]).toBeGreaterThan(at(layout, "D1")[0]);
    expect(at(layout, "D3")[0]).toBeGreaterThan(at(layout, "D2")[0]);
  });

  it("stacks several detached components in shared columns", () => {
    // Offsetting each component separately would march the group rightwards
    // once per orphan on a network with many of them.
    const nodes = [
      reservoir("R1"),
      junction("J1"),
      junction("A"),
      junction("B"),
      junction("C"),
    ];
    const layout = layoutOf(nodes, [pipe("P1", "R1", "J1")]);

    const xs = ["A", "B", "C"].map((id) => at(layout, id)[0]);
    expect(new Set(xs).size).toBe(1);
    const ys = ["A", "B", "C"].map((id) => at(layout, id)[1]);
    expect(new Set(ys).size).toBe(3);
  });

  it("anchors the group at the origin when nothing is connected", () => {
    // No sources and no links: every node is detached, so there is no connected
    // block to sit beneath.
    const layout = layoutOf([junction("A"), junction("B")], []);
    expect(layout.size).toBe(2);
    for (const [, [x, y]] of layout) {
      expect(Number.isFinite(x)).toBe(true);
      expect(Number.isFinite(y)).toBe(true);
    }
  });

  it("leaves a fully connected network untouched", () => {
    // The group must cost nothing when there is nothing detached.
    const nodes = [reservoir("R1"), junction("J1"), junction("J2")];
    const links = [pipe("P1", "R1", "J1"), pipe("P2", "J1", "J2")];
    const layout = layoutOf(nodes, links);
    expect([...layout.values()].map(([x]) => x)).toEqual([0, 120, 240]);
  });
});

// ── Inlet couplings (dual drainage) ────────────────────────────────────────────
//
// Topology taken from a real SWMM model whose surface street network the
// layout reported as a detached group of six: the streets touch the sewer
// only through inlet capture, never through a link.

describe("computeSchematicLayout – inlet couplings", () => {
  const outfall = (id: string): Node => ({
    id,
    type: "outfall",
    x: 0,
    y: 0,
    pressure: null,
    demand: null,
  });
  const streets = ["Aux1", "Aux2", "Aux4", "Aux5", "Aux6", "Aux7"];
  const nodes = [
    ...streets.map(junction),
    ...["J1", "J2", "J2a", "J11"].map(junction),
    outfall("O1"),
  ];
  const links = [
    pipe("Street1", "Aux1", "Aux2"),
    pipe("Street2", "Aux2", "Aux4"),
    pipe("Street3", "Aux4", "Aux5"),
    pipe("Street4", "Aux5", "Aux6"),
    pipe("Street5", "Aux6", "Aux7"),
    pipe("P2", "J2a", "J2"),
    pipe("P3", "J2", "J11"),
    pipe("C11", "J11", "O1"),
  ];
  const couplings = [
    { link: "Street1", node: "J1" },
    { link: "Street3", node: "J2a" },
    { link: "Street4", node: "J2" },
    { link: "Street5", node: "J11" },
  ];

  it("calls the street chain separate when couplings are unknown", () => {
    const layout = computeSchematicLayout(nodes, links);
    expect([...layout.detachedIds].sort()).toEqual([...streets, "J1"].sort());
  });

  it("counts inlet capture as connectivity, leaving nothing separate", () => {
    const layout = computeSchematicLayout(
      nodes,
      links,
      { x: 1, y: 1 },
      couplings,
    );
    expect([...layout.detachedIds]).toEqual([]);
    // Every node still gets a position.
    expect(layout.positions.size).toBe(nodes.length);
  });

  it("ignores a coupling naming an element that is not present", () => {
    const layout = computeSchematicLayout(nodes, links, { x: 1, y: 1 }, [
      { link: "NoSuchStreet", node: "J2" },
      { link: "Street1", node: "NoSuchNode" },
    ]);
    expect(layout.detachedIds.has("Aux1")).toBe(true);
  });
});

// ── catchment glyphs ──────────────────────────────────────────────────────────

describe("computeSchematicLayout – region glyphs", () => {
  /** A 2:1 rectangle, so aspect is checkable after placement. */
  function catchment(id: string, outletId: string | null): Region {
    return {
      id,
      type: "subcatchment",
      outletId,
      ring: [
        [1000, 2000],
        [1400, 2000],
        [1400, 2200],
        [1000, 2200],
      ],
    };
  }

  const nodes = [junction("J1"), junction("J2"), reservoir("R")];
  const links = [pipe("P1", "R", "J1"), pipe("P2", "J1", "J2")];

  it("anchors a catchment near the node it drains to", () => {
    const layout = computeSchematicLayout(nodes, links, undefined, [], false, [
      catchment("S1", "J1"),
    ]);
    const ring = layout.regionRings.get("S1");
    expect(ring).toBeDefined();
    if (!ring) return;
    const outlet = getLayoutPoint(layout.positions, "J1");
    // Within one layer's spacing of the outlet in each axis: the glyph is a
    // placement beside the node, not a plan position half a network away.
    for (const [x, y] of ring) {
      expect(Math.abs(x - outlet[0])).toBeLessThan(120 * 1.5);
      expect(Math.abs(y - outlet[1])).toBeLessThan(80 * 3);
    }
  });

  it("preserves the catchment's aspect ratio", () => {
    const layout = computeSchematicLayout(nodes, links, undefined, [], false, [
      catchment("S1", "J1"),
    ]);
    const ring = layout.regionRings.get("S1") ?? [];
    const xs = ring.map(([x]) => x);
    const ys = ring.map(([, y]) => y);
    const w = Math.max(...xs) - Math.min(...xs);
    const h = Math.max(...ys) - Math.min(...ys);
    expect(w / h).toBeCloseTo(2, 5);
  });

  it("fans catchments sharing one outlet so they do not coincide", () => {
    const layout = computeSchematicLayout(nodes, links, undefined, [], false, [
      catchment("S1", "J1"),
      catchment("S2", "J1"),
    ]);
    const a = layout.regionRings.get("S1") ?? [];
    const b = layout.regionRings.get("S2") ?? [];
    expect(a.length).toBeGreaterThan(0);
    expect(b.length).toBeGreaterThan(0);
    expect(a[0]).not.toEqual(b[0]);
  });

  it("omits a catchment with no node to hang off", () => {
    const layout = computeSchematicLayout(nodes, links, undefined, [], false, [
      catchment("S1", null),
      catchment("S2", "NoSuchNode"),
    ]);
    expect(layout.regionRings.size).toBe(0);
    expect(layout.regionLeaders.size).toBe(0);
  });

  it("keeps real coordinates when the layout is a plan, not a schematic", () => {
    const layout = computeSchematicLayout(nodes, links, undefined, [], true, [
      catchment("S1", "J1"),
    ]);
    expect(layout.regionRings.get("S1")?.[0]).toEqual([1000, 2000]);
  });
});
