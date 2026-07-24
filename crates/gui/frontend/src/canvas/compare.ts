/**
 * Scenario-comparison delta computation for the canvas Δ overlay.
 *
 * Pure logic: given the active period result and a baseline period result,
 * produce per-field delta arrays (active − baseline) plus the max |Δ| per
 * field used to scale the diverging colour ramp. All values stay in SI —
 * unit conversion happens at the render boundary (Legend labels).
 */

import type { PeriodResults } from "../hooks";

/** Fields the canvas colours by — link status is deliberately excluded
 * (a categorical code; "active − baseline" is not meaningful). */
export type DeltaField =
  | "nodePressure"
  | "nodeHead"
  | "nodeDemand"
  | "nodeQuality"
  | "linkFlow"
  | "linkVelocity"
  | "linkHeadloss"
  | "linkQuality";

/** Delta arrays (active − baseline), network order. Quality entries are
 * present only when BOTH result sets carry quality arrays. */
export interface DeltaArrays {
  nodePressure: Float32Array;
  nodeHead: Float32Array;
  nodeDemand: Float32Array;
  nodeQuality?: Float32Array;
  linkFlow: Float32Array;
  linkVelocity: Float32Array;
  linkHeadloss: Float32Array;
  linkQuality?: Float32Array;
}

export interface CompareDeltas {
  deltas: DeltaArrays;
  /** Max |Δ| per field, floored at {@link MIN_MAX_ABS}. Quality entries fall
   * back to the floor when no quality delta exists. */
  maxAbs: Record<DeltaField, number>;
}

/** Floor for max |Δ| so a zero-difference field never divides by zero when
 * normalising the diverging ramp. */
export const MIN_MAX_ABS = 1e-6;

/** Subtract two equal-length arrays; returns the deltas and their max |Δ|.
 * Non-finite inputs produce a non-finite delta (rendered as "no data") and
 * are skipped for the max |Δ| scan. */
function diff(
  active: Float32Array,
  baseline: Float32Array,
): { delta: Float32Array; maxAbs: number } {
  const n = active.length;
  const delta = new Float32Array(n);
  let maxAbs = 0;
  for (let i = 0; i < n; i++) {
    const d = active[i] - baseline[i];
    delta[i] = d;
    if (Number.isFinite(d)) {
      const a = Math.abs(d);
      if (a > maxAbs) maxAbs = a;
    }
  }
  return { delta, maxAbs: Math.max(maxAbs, MIN_MAX_ABS) };
}

/**
 * Compute per-field deltas (active − baseline) for the fields the canvas
 * colours by.
 *
 * Returns `null` when either side does not match the network's node/link
 * counts (topology drift between the active model and the baseline — the
 * flat arrays are keyed by network order, so element-wise subtraction would
 * silently pair unrelated elements).
 *
 * `qualityComparable` (default `true`) must be `false` when the two runs used
 * different quality modes (e.g. chemical mg/L vs water age hours): both sides
 * then carry quality arrays of the right length, but subtracting them would
 * silently mix physical quantities — quality deltas are omitted instead
 * (rendered as "no data", like a run without quality).
 */
export function computeDeltas(
  active: PeriodResults,
  baseline: PeriodResults,
  nodeCount: number,
  linkCount: number,
  qualityComparable = true,
): CompareDeltas | null {
  if (
    active.nodePressure.length !== nodeCount ||
    baseline.nodePressure.length !== nodeCount ||
    active.linkFlow.length !== linkCount ||
    baseline.linkFlow.length !== linkCount
  ) {
    return null;
  }

  const nodePressure = diff(active.nodePressure, baseline.nodePressure);
  const nodeHead = diff(active.nodeHead, baseline.nodeHead);
  const nodeDemand = diff(active.nodeDemand, baseline.nodeDemand);
  const linkFlow = diff(active.linkFlow, baseline.linkFlow);
  const linkVelocity = diff(active.linkVelocity, baseline.linkVelocity);
  const linkHeadloss = diff(active.linkHeadloss, baseline.linkHeadloss);

  // Quality deltas only when the modes match (see `qualityComparable`) and
  // both result sets have quality data of the right length (quality is
  // optional per run; comparing "chemical" against a run without quality
  // would just render all-grey noise).
  const nodeQuality =
    qualityComparable &&
    active.nodeQuality &&
    baseline.nodeQuality &&
    active.nodeQuality.length === nodeCount &&
    baseline.nodeQuality.length === nodeCount
      ? diff(active.nodeQuality, baseline.nodeQuality)
      : null;
  const linkQuality =
    qualityComparable &&
    active.linkQuality &&
    baseline.linkQuality &&
    active.linkQuality.length === linkCount &&
    baseline.linkQuality.length === linkCount
      ? diff(active.linkQuality, baseline.linkQuality)
      : null;

  const deltas: DeltaArrays = {
    nodePressure: nodePressure.delta,
    nodeHead: nodeHead.delta,
    nodeDemand: nodeDemand.delta,
    linkFlow: linkFlow.delta,
    linkVelocity: linkVelocity.delta,
    linkHeadloss: linkHeadloss.delta,
  };
  if (nodeQuality) deltas.nodeQuality = nodeQuality.delta;
  if (linkQuality) deltas.linkQuality = linkQuality.delta;

  return {
    deltas,
    maxAbs: {
      nodePressure: nodePressure.maxAbs,
      nodeHead: nodeHead.maxAbs,
      nodeDemand: nodeDemand.maxAbs,
      nodeQuality: nodeQuality?.maxAbs ?? MIN_MAX_ABS,
      linkFlow: linkFlow.maxAbs,
      linkVelocity: linkVelocity.maxAbs,
      linkHeadloss: linkHeadloss.maxAbs,
      linkQuality: linkQuality?.maxAbs ?? MIN_MAX_ABS,
    },
  };
}
