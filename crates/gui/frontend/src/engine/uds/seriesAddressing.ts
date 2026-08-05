/**
 * How a time-series request is addressed: which catalog variables describe
 * an element class, and where that element sits in the snapshot order the
 * backend indexes by.
 *
 * Named functions rather than ternaries inside the card, because a
 * two-branch ternary keyed on a three-value class is a silent bug: adding
 * `region` to the class union made every `kind === "node" ? … : …` treat
 * areal elements as polylines, which would have charted a subcatchment's
 * runoff against the conduit catalog and found the wrong element's index.
 * A lookup that must be exhaustive should be written so that it is.
 */

import type {
  ElementSeriesKind,
  GenericResultMeta,
  GenericVariable,
} from "../../hooks/results";

/**
 * The §6 catalog variables published for `kind`, or `[]` before the
 * engine's generic metadata has loaded (or for an engine that publishes
 * none for this class — an engine with no areal elements has no region
 * variables, and the card then renders nothing).
 */
export function seriesVariables(
  generic: GenericResultMeta | null | undefined,
  kind: ElementSeriesKind,
): GenericVariable[] {
  if (!generic) return [];
  switch (kind) {
    case "node":
      return generic.pointVars;
    case "link":
      return generic.polylineVars;
    case "region":
      return generic.regionVars;
  }
}

/**
 * Position of `elementId` within its class's snapshot array — the index the
 * backend addresses series by — or `-1` when the element is not in it.
 */
export function seriesIndex(
  arrays: {
    nodes: Array<{ id: string }>;
    links: Array<{ id: string }>;
    regions: Array<{ id: string }>;
  },
  kind: ElementSeriesKind,
  elementId: string,
): number {
  const arr =
    kind === "node"
      ? arrays.nodes
      : kind === "link"
        ? arrays.links
        : arrays.regions;
  return arr.findIndex((el) => el.id === elementId);
}
