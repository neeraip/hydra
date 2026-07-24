/**
 * Tests for the scenario-comparison delta computation (compare.ts). These
 * lock in: element-wise (active − baseline) subtraction, per-field max |Δ|
 * with the MIN_MAX_ABS floor, the quality-optional handling, and the
 * topology-drift guards that make comparison unavailable rather than
 * pairing unrelated elements.
 */
import { describe, expect, it } from "vitest";
import type { PeriodResults } from "../hooks";
import { type CompareDeltas, computeDeltas, MIN_MAX_ABS } from "./compare";

/** Build a PeriodResults with the given node/link counts. Field values are
 * `base + index` per array so element pairing is easy to assert; overrides
 * replace whole arrays. */
function makePeriod(
  nNodes: number,
  nLinks: number,
  overrides: Partial<PeriodResults> = {},
): PeriodResults {
  const fill = (n: number, base: number) =>
    Float32Array.from({ length: n }, (_, i) => base + i);
  return {
    nodeDemand: fill(nNodes, 10),
    nodeHead: fill(nNodes, 20),
    nodePressure: fill(nNodes, 30),
    linkFlow: fill(nLinks, 40),
    linkVelocity: fill(nLinks, 50),
    linkHeadloss: fill(nLinks, 60),
    linkStatus: fill(nLinks, 3),
    ...overrides,
  };
}

/** computeDeltas that must succeed — fails the test instead of returning null. */
function mustCompute(
  active: PeriodResults,
  baseline: PeriodResults,
  nodeCount: number,
  linkCount: number,
): CompareDeltas {
  const result = computeDeltas(active, baseline, nodeCount, linkCount);
  expect(result).not.toBeNull();
  if (!result) throw new Error("unreachable");
  return result;
}

describe("computeDeltas", () => {
  it("subtracts baseline from active element-wise for every coloured field", () => {
    const active = makePeriod(3, 2, {
      nodePressure: Float32Array.from([35, 30, 25]),
      linkFlow: Float32Array.from([5, -5]),
    });
    const baseline = makePeriod(3, 2, {
      nodePressure: Float32Array.from([30, 30, 40]),
      linkFlow: Float32Array.from([2, 5]),
    });
    const { deltas, maxAbs } = mustCompute(active, baseline, 3, 2);
    expect(Array.from(deltas.nodePressure)).toEqual([5, 0, -15]);
    expect(Array.from(deltas.linkFlow)).toEqual([3, -10]);
    // Fields built from identical fill patterns cancel to zero.
    expect(Array.from(deltas.nodeHead)).toEqual([0, 0, 0]);
    expect(Array.from(deltas.nodeDemand)).toEqual([0, 0, 0]);
    expect(Array.from(deltas.linkVelocity)).toEqual([0, 0]);
    expect(Array.from(deltas.linkHeadloss)).toEqual([0, 0]);
    // maxAbs is the max |Δ| per field.
    expect(maxAbs.nodePressure).toBe(15);
    expect(maxAbs.linkFlow).toBe(10);
  });

  it("returns fresh arrays (never views over the inputs)", () => {
    const active = makePeriod(2, 1);
    const baseline = makePeriod(2, 1);
    const { deltas } = mustCompute(active, baseline, 2, 1);
    deltas.nodePressure[0] = 999;
    expect(active.nodePressure[0]).toBe(30);
    expect(baseline.nodePressure[0]).toBe(30);
  });

  it("floors max |Δ| at MIN_MAX_ABS so identical results cannot divide by zero", () => {
    const { maxAbs } = mustCompute(makePeriod(2, 2), makePeriod(2, 2), 2, 2);
    for (const v of Object.values(maxAbs)) {
      expect(v).toBe(MIN_MAX_ABS);
    }
  });

  it("skips non-finite deltas in the max |Δ| scan but keeps them in the array", () => {
    const active = makePeriod(3, 1, {
      nodePressure: Float32Array.from([Number.NaN, 4, 1]),
    });
    const baseline = makePeriod(3, 1, {
      nodePressure: Float32Array.from([1, 2, 1]),
    });
    const { deltas, maxAbs } = mustCompute(active, baseline, 3, 1);
    expect(Number.isNaN(deltas.nodePressure[0])).toBe(true);
    expect(maxAbs.nodePressure).toBe(2);
  });

  it("returns null when either side mismatches the network node count", () => {
    expect(computeDeltas(makePeriod(3, 2), makePeriod(4, 2), 3, 2)).toBeNull();
    expect(computeDeltas(makePeriod(4, 2), makePeriod(3, 2), 3, 2)).toBeNull();
    // Both sides agree with each other but not with the network.
    expect(computeDeltas(makePeriod(4, 2), makePeriod(4, 2), 3, 2)).toBeNull();
  });

  it("returns null when either side mismatches the network link count", () => {
    expect(computeDeltas(makePeriod(3, 1), makePeriod(3, 2), 3, 2)).toBeNull();
    expect(computeDeltas(makePeriod(3, 2), makePeriod(3, 1), 3, 2)).toBeNull();
  });

  it("computes quality deltas only when BOTH sides carry quality arrays", () => {
    const q = (vals: number[]) => Float32Array.from(vals);
    const withQ = makePeriod(2, 1, {
      nodeQuality: q([3, 1]),
      linkQuality: q([7]),
    });
    const baselineQ = makePeriod(2, 1, {
      nodeQuality: q([1, 1]),
      linkQuality: q([2]),
    });
    const both = mustCompute(withQ, baselineQ, 2, 1);
    expect(Array.from(both.deltas.nodeQuality ?? [])).toEqual([2, 0]);
    expect(Array.from(both.deltas.linkQuality ?? [])).toEqual([5]);
    expect(both.maxAbs.nodeQuality).toBe(2);
    expect(both.maxAbs.linkQuality).toBe(5);

    // Baseline without quality → no quality deltas, floored maxAbs.
    const noQ = mustCompute(withQ, makePeriod(2, 1), 2, 1);
    expect(noQ.deltas.nodeQuality).toBeUndefined();
    expect(noQ.deltas.linkQuality).toBeUndefined();
    expect(noQ.maxAbs.nodeQuality).toBe(MIN_MAX_ABS);
    expect(noQ.maxAbs.linkQuality).toBe(MIN_MAX_ABS);
    // Active without quality, baseline with → same.
    const noQ2 = mustCompute(makePeriod(2, 1), baselineQ, 2, 1);
    expect(noQ2.deltas.nodeQuality).toBeUndefined();
    expect(noQ2.deltas.linkQuality).toBeUndefined();
  });

  // Two runs with different quality modes (chemical mg/L vs age hours) carry
  // same-length quality arrays of DIFFERENT physical quantities — the caller
  // flags that via `qualityComparable=false` and quality deltas are omitted,
  // while every hydraulic field still compares normally.
  it("omits quality deltas when qualityComparable is false", () => {
    const q = (vals: number[]) => Float32Array.from(vals);
    const active = makePeriod(2, 1, {
      nodeQuality: q([0.4, 0.2]), // mg/L
      linkQuality: q([0.3]),
      nodePressure: q([35, 30]),
    });
    const baseline = makePeriod(2, 1, {
      nodeQuality: q([6, 12]), // hours
      linkQuality: q([9]),
      nodePressure: q([30, 30]),
    });
    const result = computeDeltas(active, baseline, 2, 1, false);
    expect(result).not.toBeNull();
    if (!result) throw new Error("unreachable");
    expect(result.deltas.nodeQuality).toBeUndefined();
    expect(result.deltas.linkQuality).toBeUndefined();
    expect(result.maxAbs.nodeQuality).toBe(MIN_MAX_ABS);
    expect(result.maxAbs.linkQuality).toBe(MIN_MAX_ABS);
    // Hydraulic deltas are unaffected.
    expect(Array.from(result.deltas.nodePressure)).toEqual([5, 0]);

    // Explicit `true` matches the default (quality compared).
    const comparable = computeDeltas(active, baseline, 2, 1, true);
    expect(Array.from(comparable?.deltas.nodeQuality ?? [])).toEqual([
      Float32Array.from([0.4 - 6])[0],
      Float32Array.from([0.2 - 12])[0],
    ]);
  });
});
