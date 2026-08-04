/**
 * Tests for the pure canvas colour functions. These lock in the exact RGBA
 * outputs the map legend documents: status code groupings, threshold
 * banding for flow/velocity, pump-always-amber, and per-variable dispatch.
 */
import { describe, expect, it } from "vitest";
import type { Link, Node } from "../../hooks";
import {
  BAND_STEPS,
  baseNodeRgba,
  flowMagnitudeRgba,
  headlossRgba,
  LINK_HEADLOSS_MAX,
  linkQualityRgba,
  linkRgba,
  NO_DATA_RGB,
  nodeRgba,
  qualityRgba,
  seqRgb,
  sequentialRgba,
  statusLabel,
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
  it("marks missing data as missing", () => {
    expect(flowMagnitudeRgba(null, 10)).toEqual([...NO_DATA_RGB, 200]);
    expect(flowMagnitudeRgba(undefined, 10, 123)).toEqual([
      ...NO_DATA_RGB,
      123,
    ]);
  });

  it("bands |flow| against thresholds when provided", () => {
    // Thresholds make it a judgement, so it speaks the banded ramp rather
    // than a warm palette of its own.
    const thresholds = { low: 10, target: 20, high: 30 };
    expect(flowMagnitudeRgba(5, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[0],
      200,
    ]);
    expect(flowMagnitudeRgba(15, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[2],
      200,
    ]);
    expect(flowMagnitudeRgba(25, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[3],
      200,
    ]);
    expect(flowMagnitudeRgba(35, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[4],
      200,
    ]); // excessive
    // Band edges are exclusive lower bounds: exactly `low` is moderate.
    expect(flowMagnitudeRgba(10, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[2],
      200,
    ]);
    // Sign is ignored — reverse flow bands by magnitude.
    expect(flowMagnitudeRgba(-35, 100, 200, thresholds)).toEqual([
      ...BAND_STEPS[4],
      200,
    ]);
  });

  it("takes the sequential ramp by |flow|/maxFlow without thresholds", () => {
    // Links take the polyline hue family, so a link and a node at the same
    // fraction of their range are told apart by hue rather than by memory.
    expect(flowMagnitudeRgba(0, 10)).toEqual([...seqRgb(0, "polyline"), 200]);
    expect(flowMagnitudeRgba(10, 10)).toEqual([...seqRgb(1, "polyline"), 200]);
    expect(flowMagnitudeRgba(-10, 10)).toEqual([...seqRgb(1, "polyline"), 200]); // |flow|
    expect(flowMagnitudeRgba(5, 0)).toEqual([...seqRgb(0, "polyline"), 200]);
  });
});

describe("velocityRgba", () => {
  it("bands against thresholds when provided", () => {
    const thresholds = { low: 0.5, target: 1.0, high: 2.0 };
    expect(velocityRgba(0.2, thresholds)).toEqual([...BAND_STEPS[0], 220]);
    expect(velocityRgba(0.7, thresholds)).toEqual([...BAND_STEPS[2], 220]);
    expect(velocityRgba(1.5, thresholds)).toEqual([...BAND_STEPS[3], 220]);
    expect(velocityRgba(2.5, thresholds)).toEqual([...BAND_STEPS[4], 220]);
    // Exact threshold values fall into the next band up (strict `<`).
    expect(velocityRgba(0.5, thresholds)).toEqual([...BAND_STEPS[2], 220]);
    expect(velocityRgba(2.0, thresholds)).toEqual([...BAND_STEPS[4], 220]);
  });

  it("takes the sequential ramp, capped at 1.5 m/s, without thresholds", () => {
    expect(velocityRgba(0)).toEqual([...seqRgb(0, "polyline"), 220]);
    expect(velocityRgba(1.5)).toEqual([...seqRgb(1, "polyline"), 220]);
    expect(velocityRgba(99)).toEqual([...seqRgb(1, "polyline"), 220]);
  });
});

describe("headlossRgba", () => {
  it("returns grey for missing data", () => {
    expect(headlossRgba(null)).toEqual([...NO_DATA_RGB, 200]);
    expect(headlossRgba(undefined)).toEqual([...NO_DATA_RGB, 200]);
  });

  it("ramps the sequential scale over the fixed 0..LINK_HEADLOSS_MAX range", () => {
    expect(headlossRgba(0)).toEqual(
      sequentialRgba(0, 0, LINK_HEADLOSS_MAX, 220, "polyline"),
    );
    expect(headlossRgba(0)).toEqual([...seqRgb(0, "polyline"), 220]);
    expect(headlossRgba(5)).toEqual(
      sequentialRgba(5, 0, LINK_HEADLOSS_MAX, 220, "polyline"),
    );
    expect(headlossRgba(LINK_HEADLOSS_MAX)).toEqual([
      ...seqRgb(1, "polyline"),
      220,
    ]);
    // Values above the cap clamp to the bright end.
    expect(headlossRgba(99)).toEqual([...seqRgb(1, "polyline"), 220]);
  });

  it("colours by magnitude (reverse-flow headloss sign is ignored)", () => {
    expect(headlossRgba(-5)).toEqual(headlossRgba(5));
  });
});

describe("linkQualityRgba", () => {
  it("returns grey for missing data", () => {
    expect(linkQualityRgba(null, 0, 1)).toEqual([...NO_DATA_RGB, 200]);
    expect(linkQualityRgba(undefined, 0, 1)).toEqual([...NO_DATA_RGB, 200]);
  });

  it("reuses the node quality ramp normalised to the result range", () => {
    expect(linkQualityRgba(0, 0, 1)).toEqual(qualityRgba(0, "polyline"));
    expect(linkQualityRgba(0.5, 0, 1)).toEqual(qualityRgba(0.5, "polyline"));
    expect(linkQualityRgba(1, 0, 1)).toEqual(qualityRgba(1, "polyline"));
    // Non-trivial range: 15 of [10, 20] → t = 0.5.
    expect(linkQualityRgba(15, 10, 20)).toEqual(qualityRgba(0.5, "polyline"));
    // Degenerate range guards against divide-by-zero.
    expect(linkQualityRgba(5, 5, 5)).toEqual(qualityRgba(0, "polyline"));
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
  it("lets a pump show its own results", () => {
    // A pump used to be painted a fixed amber before the ramp was
    // consulted, so it could never show its flow, velocity or status — the
    // one link kind whose data was hidden to signal what kind it was.
    // Identity comes from role and size; the colour belongs to the data.
    const pump = makeLink({ type: "pump", flow: 999, velocity: 99, status: 2 });
    expect(linkRgba(pump, "flow", 10)).toEqual(flowMagnitudeRgba(999, 10));
    expect(linkRgba(pump, "velocity", 10)).toEqual(velocityRgba(99));
    expect(linkRgba(pump, "status", 10)).toEqual(statusRgba(2));
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
      ...NO_DATA_RGB,
      200,
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

  it("dispatches to headlossRgba for the headloss variable", () => {
    expect(linkRgba(makeLink({ headloss: 5 }), "headloss", 0)).toEqual(
      headlossRgba(5),
    );
    expect(linkRgba(makeLink(), "headloss", 0)).toEqual([...NO_DATA_RGB, 200]);
    // A pump's headloss is its headloss, like anything else's.
    expect(
      linkRgba(makeLink({ type: "pump", headloss: 5 }), "headloss", 0),
    ).toEqual(headlossRgba(5));
  });

  it("dispatches to linkQualityRgba for the quality variable", () => {
    expect(
      linkRgba(
        makeLink({ quality: 15 }),
        "quality",
        0,
        undefined,
        undefined,
        10,
        20,
      ),
    ).toEqual(linkQualityRgba(15, 10, 20));
    // Range defaults to [0, 1] when not supplied.
    expect(linkRgba(makeLink({ quality: 0.5 }), "quality", 0)).toEqual(
      linkQualityRgba(0.5, 0, 1),
    );
    expect(linkRgba(makeLink(), "quality", 0)).toEqual([...NO_DATA_RGB, 200]);
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
  role?: string,
) => nodeRgba(node, nodeVar, 0, 100, 0, 10, 0, 1, undefined, role);

describe("nodeRgba", () => {
  it("shows the at-rest palette for kinds carrying no value", () => {
    // A tank and a reservoir have no pressure, head or demand of their own
    // to plot, so they keep the network-at-rest appearance whatever
    // variable is active — and it comes from the role the engine declared,
    // not from this file knowing what a tank is.
    const tank = makeNode({ type: "tank", pressure: 5 });
    const reservoir = makeNode({ type: "reservoir", pressure: 5 });
    for (const v of ["pressure", "head", "demand", "quality"] as const) {
      expect(rgbaOf(tank, v, "boundary")).toEqual(baseNodeRgba("boundary"));
      expect(rgbaOf(reservoir, v, "boundary")).toEqual(
        baseNodeRgba("boundary"),
      );
    }
  });

  it("separates the three roles by lightness, not hue", () => {
    const lum = (c: number[]) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    const conveyance = baseNodeRgba("conveyance");
    const boundary = baseNodeRgba("boundary");
    const control = baseNodeRgba("control");
    // A boundary is where the model is fed or drained, so it reads first.
    expect(lum(boundary)).toBeGreaterThan(lum(control));
    expect(lum(control)).toBeGreaterThan(lum(conveyance));
    // Neutral: no channel may run away from the others.
    for (const c of [conveyance, boundary, control]) {
      expect(
        Math.max(c[0], c[1], c[2]) - Math.min(c[0], c[1], c[2]),
      ).toBeLessThan(40);
    }
  });

  it("gives a kind with no role the quietest treatment", () => {
    expect(baseNodeRgba(undefined)).toEqual(baseNodeRgba("conveyance"));
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

  it("marks a missing reading as missing, not as a resting element", () => {
    // A reading that should exist and does not is a different statement
    // from a network with no results at all, so it gets its own dim
    // neutral rather than the at-rest palette.
    const missing = [110, 116, 126, 190];
    expect(rgbaOf(makeNode({ pressure: null }), "pressure")).toEqual(missing);
    expect(rgbaOf(makeNode({ quality: null }), "quality")).toEqual(missing);
    expect(missing).not.toEqual(baseNodeRgba("conveyance"));
  });

  it("uses the sequential ramp for head and demand", () => {
    expect(rgbaOf(makeNode({ head: 0 }), "head")).toEqual([...seqRgb(0), 220]);
    expect(rgbaOf(makeNode({ demand: 10 }), "demand")).toEqual([
      ...seqRgb(1),
      220,
    ]);
  });

  it("keeps the sequential ramp monotonic in lightness", () => {
    // What a rainbow could not promise: every step darker than the last,
    // so the ramp survives greyscale and colour-vision deficiency, and
    // invents no boundary the data does not have.
    const lum = (c: number[]) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    // Brighter as the value rises: the canvas ground is near-black, so a
    // ramp that darkened toward its maximum hid the values these maps are
    // read for.
    let previous = Number.NEGATIVE_INFINITY;
    for (let i = 0; i <= 10; i++) {
      const here = lum(sequentialRgba(i, 0, 10));
      expect(here).toBeGreaterThan(previous);
      previous = here;
    }
  });
});

// ── statusLabel ──────────────────────────────────────────────────────────────

describe("statusLabel", () => {
  // The codes come from the engine's `status_to_f32`. They are NOT a 0/1
  // open/closed flag, which is precisely what a duplicate mapping in the
  // hover chip assumed — labelling every open link (code 3) as "cv".
  it("maps every OUT-file status code the writer can emit", () => {
    expect(statusLabel(0)).toBe("Closed (XHead)");
    expect(statusLabel(1)).toBe("Temp Closed");
    expect(statusLabel(2)).toBe("Closed");
    expect(statusLabel(3)).toBe("Open");
    expect(statusLabel(4)).toBe("Active");
    expect(statusLabel(6)).toBe("Active (XFcv)");
    expect(statusLabel(7)).toBe("Active (XPressure)");
  });

  it("never reports a check valve, which is not a simulated status", () => {
    // `cv` is a model-side flag on a pipe; the runtime LinkStatus enum has no
    // such variant, so no status code may ever produce it as a label. Matched
    // whole-string, since "Active (XFcv)" legitimately contains those letters.
    for (const code of [0, 1, 2, 3, 4, 5, 6, 7, 8]) {
      expect(statusLabel(code).trim().toLowerCase()).not.toBe("cv");
    }
  });

  it("falls back for absent or unknown codes", () => {
    expect(statusLabel(null)).toBe("—");
    expect(statusLabel(undefined)).toBe("—");
    expect(statusLabel(5)).toBe("—");
  });

  it("agrees with statusRgba on which codes are closed, open and active", () => {
    const closed = [201, 64, 64, 200];
    const active = [212, 160, 23, 200];
    const open = [120, 150, 185, 180];
    for (const c of [0, 1, 2]) {
      expect(statusLabel(c)).toMatch(/Closed/);
      expect(statusRgba(c)).toEqual(closed);
    }
    expect(statusLabel(3)).toBe("Open");
    expect(statusRgba(3)).toEqual(open);
    for (const c of [4, 6, 7]) {
      expect(statusLabel(c)).toMatch(/Active/);
      expect(statusRgba(c)).toEqual(active);
    }
  });
});
