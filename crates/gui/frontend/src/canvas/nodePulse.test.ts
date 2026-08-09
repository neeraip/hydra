import { describe, expect, it } from "vitest";
import { engineComponents } from "../engine/registry";
import { animatesVariable, isMoving } from "./linkPulse";
import { ringApplies, ringRate } from "./nodePulse";

/**
 * The node ring animates rates crossing a node's boundary, on the same
 * terms the link pulse animates rates along a conveyance: motion where it
 * says something the colour cannot, and never where the number is a state.
 *
 * The sparsity is load-bearing, not incidental. A still node draws nothing
 * at all, so a network where almost nothing is flooding costs almost
 * nothing to animate — which is what makes the ring affordable on the
 * element counts this canvas has to hold.
 */

describe("ringRate", () => {
  it("scales with the share of the run's range", () => {
    expect(ringRate(5, 10)).toBeCloseTo(0.5);
    expect(ringRate(10, 10)).toBe(1);
  });

  it("never exceeds full rate", () => {
    // A period beyond the range the run reported still reads as full.
    expect(ringRate(40, 10)).toBe(1);
  });

  it("reads magnitude, not sign", () => {
    // A ring has one honest direction — outward, away from the node —
    // because that is the shape of water reaching the surface. An inward
    // ring would animate water being un-flooded, which nothing means.
    expect(ringRate(-6, 10)).toBe(ringRate(6, 10));
  });

  it("holds a quiet node still", () => {
    expect(ringRate(0, 10)).toBe(0);
    // Solver noise is not a flood.
    expect(ringRate(1e-12, 10)).toBe(0);
  });

  it("holds still where the results say nothing", () => {
    expect(ringRate(undefined, 10)).toBe(0);
    expect(ringRate(Number.NaN, 10)).toBe(0);
  });

  it("survives a degenerate range without dividing by zero", () => {
    expect(Number.isFinite(ringRate(1, 0))).toBe(true);
  });
});

describe("ringApplies", () => {
  /** The filter that keeps the animated set as small as the event is. */
  it("selects exactly the nodes with something crossing the boundary", () => {
    const flooding = [0, 0, 0.4, 0, 12];
    expect(flooding.filter((v) => ringApplies(v, 12))).toEqual([0.4, 12]);
  });
});

describe("what each engine rings", () => {
  it("drainage rings flooding and nothing else", () => {
    const uds = engineComponents("uds").animatedVariables;
    expect(animatesVariable("flooding", uds.point)).toBe(true);
    // Depth, head and stored volume are states: a full manhole is not a
    // fast one, and a ring pacing itself against one would assert a rate
    // the number does not have.
    expect(animatesVariable("depth", uds.point)).toBe(false);
    expect(animatesVariable("head", uds.point)).toBe(false);
    expect(animatesVariable("volume", uds.point)).toBe(false);
  });

  it("water distribution rings nothing yet, and still pulses its links", () => {
    const wds = engineComponents("wds").animatedVariables;
    expect(wds.point).toEqual([]);
    expect(animatesVariable("flow", wds.polyline)).toBe(true);
  });

  it("keeps the two classes separate", () => {
    // One flat list had the links' answer standing in for every class.
    const uds = engineComponents("uds").animatedVariables;
    expect(animatesVariable("flow", uds.point)).toBe(false);
    expect(animatesVariable("flooding", uds.polyline)).toBe(false);
  });
});

/**
 * "Still" has to mean one thing on a canvas that pulses links and rings
 * nodes at the same time. This module wrote its own test for it and drifted
 * to a thousand times stricter than the link pulse's, so a node reading
 * 0.00000482 against a peak of a few units sat perfectly motionless while
 * the number beside it said it was flooding.
 */
describe("what counts as still", () => {
  it("moves for a real value that is merely small", () => {
    // The reported case: tiny, and nothing to do with arithmetic noise.
    expect(ringRate(0.00000482, 5)).toBeGreaterThan(0);
    expect(ringApplies(0.00000482, 5)).toBe(true);
  });

  it("agrees with the link pulse on where the floor is", () => {
    // One definition, shared — not two that can drift apart again.
    for (const [value, scale] of [
      [0.00000482, 5],
      [1e-12, 10],
      [0, 10],
      [3, 10],
    ] as const) {
      expect(ringRate(value, scale) > 0).toBe(isMoving(value, scale));
    }
  });

  it("still rejects arithmetic noise", () => {
    // Below a billionth of the run's peak is the solve talking, not water.
    expect(ringRate(1e-12, 10)).toBe(0);
  });
});
