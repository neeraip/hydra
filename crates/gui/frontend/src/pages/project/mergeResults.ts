/**
 * Attaching a reporting period's results to the elements that carry them.
 *
 * The merge used to be written out field by field in `CanvasView`, once
 * for nodes and once for links, and the link list was one field short: it
 * set flow, velocity, status and quality, and never set `headloss`. So
 * every surface fed by that merge — the inspector's result cards above
 * all — saw a link whose head loss was permanently absent, whatever the
 * run had computed. The canvas looked right because it does its own,
 * complete merge from the same arrays.
 *
 * That is a list-of-fields bug, so the lists are no longer written by
 * hand. Both tables below are `Record<…Variable, …>` over the variable
 * unions, which makes a variable added to a union and forgotten here a
 * compile error rather than a value that silently never appears.
 */

import type { LinkVariable, NodeVariable } from "../../canvas/types";
import type { PeriodResults } from "../../hooks/results";

/** Which array in a period's results holds each node variable. */
export const NODE_RESULT_FIELDS: Record<NodeVariable, keyof PeriodResults> = {
  pressure: "nodePressure",
  demand: "nodeDemand",
  head: "nodeHead",
  quality: "nodeQuality",
};

/** Which array in a period's results holds each link variable. */
export const LINK_RESULT_FIELDS: Record<LinkVariable, keyof PeriodResults> = {
  flow: "linkFlow",
  velocity: "linkVelocity",
  status: "linkStatus",
  headloss: "linkHeadloss",
  quality: "linkQuality",
};

/**
 * Read one element's values out of a period.
 *
 * A missing array yields `null` rather than being skipped, which is the
 * distinction the quality arrays need: they are absent entirely when no
 * quality simulation ran, and a card reading "no value" is right where a
 * card reading "0" would be a fabrication.
 */
function valuesAt<V extends string>(
  fields: Record<V, keyof PeriodResults>,
  period: PeriodResults,
  index: number,
): Record<V, number | null> {
  const out = {} as Record<V, number | null>;
  for (const variable of Object.keys(fields) as V[]) {
    const array = period[fields[variable]] as Float32Array | undefined;
    const value = array?.[index];
    out[variable] = value == null || !Number.isFinite(value) ? null : value;
  }
  return out;
}

/** Every node variable's value at `index`, for the given period. */
export function nodeResultsAt(period: PeriodResults, index: number) {
  return valuesAt(NODE_RESULT_FIELDS, period, index);
}

/** Every link variable's value at `index`, for the given period. */
export function linkResultsAt(period: PeriodResults, index: number) {
  return valuesAt(LINK_RESULT_FIELDS, period, index);
}
