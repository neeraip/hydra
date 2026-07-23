/**
 * Tests for the pure canvas colour functions. These lock in the exact RGBA
 * outputs the map legend documents: status code groupings, threshold
 * banding for flow/velocity, pump-always-amber, and per-variable dispatch.
 */
import { describe, expect, it } from "vitest";
import type { Link, Node } from "../../hooks";
import {
  flowMagnitudeRgba,
  linkRgba,
  nodeRgba,
  nodeTypeRgba,
  statusRgba,
  velocityRgba,
} from "./colorUtils";

const RED = [201, 64, 64, 200];
const AMBER = [212, 160, 23, 200];
const BLUE_GREY = [120, 150, 185, 180];

describe("statusRgba", () => {
  it("maps closed variants (0=XHead, 1=TempClosed, 2=Closed) to red", () => {
    expect(statusRgba(0)).toEqual(RED);
    expect(statusRgba(1)).toEqual(RED);
    expect(statusRgba(2)).toEqual(RED);
  });

  it("maps active/controlled (4=Active, 6=XFcv, 7=XPressure) to amber", () => {
    expect(statusRgba(4)).toEqual(AMBER);
    expect(statusRgba(6)).toEqual(AMBER);
    expect(statusRgba(7)).toEqual(AMBER);
  });

  it("maps open (3) and missing/unknown codes to blue-grey", () => {
    expect(statusRgba(3)).toEqual(BLUE_GREY);
    expect(statusRgba(null)).toEqual(BLUE_GREY);
    expect(statusRgba(undefined)).toEqual(BLUE_GREY);
    expect(statusRgba(5)).toEqual(BLUE_GREY); // unused code falls through
  });
});

describe("flowMagnitudeRgba", () => {
  it("returns grey for missing data", () => {
    expect(flowMagnitudeRgba(null, 10)).toEqual([100, 100, 100, 200]);
    expect(flowMagnitudeRgba(undefined, 10, 123)).toEqual([100, 100, 100, 123]);
  });

  it("bands |flow| against thresholds when provided", () => {
    const thresholds = { low: 10, target: 20, high: 30 };
    expect(flowMagnitudeRgba(5, 100, 200, thresholds)).toEqual([
      61, 175, 117, 200,
    ]); // good
    expect(flowMagnitudeRgba(15, 100, 200, thresholds)).toEqual([
      212, 160, 23, 200,
    ]); // moderate
    expect(flowMagnitudeRgba(25, 100, 200, thresholds)).toEqual([
      201, 120, 64, 200,
    ]); // elevated
    expect(flowMagnitudeRgba(35, 100, 200, thresholds)).toEqual([
      201, 64, 64, 200,
    ]); // excessive
    // Band edges are exclusive lower bounds: exactly `low` is moderate.
    expect(flowMagnitudeRgba(10, 100, 200, thresholds)).toEqual([
      212, 160, 23, 200,
    ]);
    // Sign is ignored — reverse flow bands by magnitude.
    expect(flowMagnitudeRgba(-35, 100, 200, thresholds)).toEqual([
      201, 64, 64, 200,
    ]);
  });

  it("ramps cyan → orange by |flow|/maxFlow without thresholds", () => {
    expect(flowMagnitudeRgba(0, 10)).toEqual([80, 200, 247, 200]); // t=0
    expect(flowMagnitudeRgba(10, 10)).toEqual([255, 80, 47, 200]); // t=1
    expect(flowMagnitudeRgba(-10, 10)).toEqual([255, 80, 47, 200]); // |flow|
    expect(flowMagnitudeRgba(5, 0)).toEqual([80, 200, 247, 200]); // maxFlow=0 → t=0
  });
});

describe("velocityRgba", () => {
  it("bands against thresholds when provided", () => {
    const thresholds = { low: 0.5, target: 1.0, high: 2.0 };
    expect(velocityRgba(0.2, thresholds)).toEqual([61, 175, 117, 220]); // good
    expect(velocityRgba(0.7, thresholds)).toEqual([212, 160, 23, 220]); // moderate
    expect(velocityRgba(1.5, thresholds)).toEqual([201, 120, 64, 220]); // elevated
    expect(velocityRgba(2.5, thresholds)).toEqual([201, 64, 64, 220]); // excessive
    // Exact threshold values fall into the next band up (strict `<`).
    expect(velocityRgba(0.5, thresholds)).toEqual([212, 160, 23, 220]);
    expect(velocityRgba(2.0, thresholds)).toEqual([201, 64, 64, 220]);
  });

  it("ramps blue → red capped at 1.5 m/s without thresholds", () => {
    expect(velocityRgba(0)).toEqual([74, 144, 217, 220]); // t=0
    expect(velocityRgba(1.5)).toEqual([201, 80, 23, 220]); // t=1
    expect(velocityRgba(99)).toEqual([201, 80, 23, 220]); // clamped
  });
});

// ── linkRgba dispatch ────────────────────────────────────────────────────────

function makeLink(extra: Partial<Link> = {}): Link {
  return {
    id: "P1",
    type: "pipe",
    fromId: "J1",
    toId: "J2",
    velocity: 0,
    diameter: 100,
    ...extra,
  };
}

describe("linkRgba", () => {
  it("always colours pumps amber, regardless of the active variable", () => {
    const pump = makeLink({ type: "pump", flow: 999, velocity: 99, status: 2 });
    expect(linkRgba(pump, "flow", 10)).toEqual([212, 160, 23, 220]);
    expect(linkRgba(pump, "velocity", 10)).toEqual([212, 160, 23, 220]);
    expect(linkRgba(pump, "status", 10)).toEqual([212, 160, 23, 220]);
  });

  it("dispatches to flowMagnitudeRgba for the flow variable", () => {
    const link = makeLink({ flow: 10 });
    expect(linkRgba(link, "flow", 10)).toEqual(flowMagnitudeRgba(10, 10));
    // flow alpha is fixed at 200 and flow thresholds are forwarded.
    const thresholds = { low: 1, target: 2, high: 3 };
    expect(linkRgba(link, "flow", 10, undefined, thresholds)).toEqual(
      flowMagnitudeRgba(10, 10, 200, thresholds),
    );
    expect(linkRgba(makeLink({ flow: null }), "flow", 10)).toEqual([
      100, 100, 100, 200,
    ]);
  });

  it("dispatches to velocityRgba for the velocity variable", () => {
    const link = makeLink({ velocity: 1.5 });
    expect(linkRgba(link, "velocity", 0)).toEqual(velocityRgba(1.5));
    const thresholds = { low: 0.5, target: 1, high: 2 };
    expect(linkRgba(link, "velocity", 0, thresholds)).toEqual(
      velocityRgba(1.5, thresholds),
    );
  });

  it("dispatches to statusRgba for the status variable", () => {
    expect(linkRgba(makeLink({ status: 2 }), "status", 0)).toEqual(RED);
    expect(linkRgba(makeLink({ status: 3 }), "status", 0)).toEqual(BLUE_GREY);
    expect(linkRgba(makeLink(), "status", 0)).toEqual(BLUE_GREY); // no result
  });
});

// ── nodeRgba dispatch ────────────────────────────────────────────────────────

type CanvasNode = Node & { position: [number, number] };

function makeNode(extra: Partial<Node> = {}): CanvasNode {
  return {
    id: "J1",
    type: "junction",
    x: 0,
    y: 0,
    position: [0, 0],
    pressure: null,
    demand: null,
    ...extra,
  };
}

const rgbaOf = (
  node: CanvasNode,
  nodeVar: "pressure" | "head" | "demand" | "quality",
) => nodeRgba(node, nodeVar, 0, 100, 0, 10, 0, 1);

describe("nodeRgba", () => {
  it("always uses the type colour for tanks and reservoirs", () => {
    const tank = makeNode({ type: "tank", pressure: 5 });
    const reservoir = makeNode({ type: "reservoir", pressure: 5 });
    for (const v of ["pressure", "head", "demand", "quality"] as const) {
      expect(rgbaOf(tank, v)).toEqual(nodeTypeRgba("tank"));
      expect(rgbaOf(reservoir, v)).toEqual(nodeTypeRgba("reservoir"));
    }
    expect(nodeTypeRgba("tank")).toEqual([61, 175, 117, 255]);
    expect(nodeTypeRgba("reservoir")).toEqual([74, 144, 217, 255]);
  });

  it("colours junctions by pressure thresholds (default 24/35/45)", () => {
    expect(rgbaOf(makeNode({ pressure: 10 }), "pressure")).toEqual([
      201, 64, 64, 255,
    ]); // below low
    expect(rgbaOf(makeNode({ pressure: 30 }), "pressure")).toEqual([
      212, 160, 23, 255,
    ]); // low–required
    expect(rgbaOf(makeNode({ pressure: 40 }), "pressure")).toEqual([
      61, 175, 117, 255,
    ]); // required–high
    expect(rgbaOf(makeNode({ pressure: 50 }), "pressure")).toEqual([
      74, 144, 217, 255,
    ]); // above high
  });

  it("falls back to muted junction grey when the variable has no data", () => {
    expect(rgbaOf(makeNode({ pressure: null }), "pressure")).toEqual([
      180, 195, 215, 180,
    ]);
    expect(rgbaOf(makeNode({ quality: null }), "quality")).toEqual([
      180, 195, 215, 180,
    ]);
  });

  it("uses the sequential ramp for head and demand", () => {
    // head 0 of [0, 100] → t=0 → deep blue end of the ramp.
    expect(rgbaOf(makeNode({ head: 0 }), "head")).toEqual([0, 0, 255, 220]);
    // demand 10 of [0, 10] → t=1 → orange end of the ramp.
    expect(rgbaOf(makeNode({ demand: 10 }), "demand")).toEqual([
      255, 0, 0, 220,
    ]);
  });
});
