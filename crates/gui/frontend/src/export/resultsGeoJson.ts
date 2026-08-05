/**
 * The results GeoJSON export: network geometry plus whatever the current
 * step reports for it.
 *
 * A named function rather than a body inside the command palette's click
 * handler, because the thing worth reviewing here is a rule, not a
 * serialisation: **every element class carries its result values, or none
 * does.** That rule was silently broken for areal elements — nodes and
 * links spread `resultValues`, subcatchments did not — so one file gave two
 * different answers to "does this export include results", and nothing
 * could catch it because the decision had no name.
 *
 * Absent attributes are omitted rather than exported as `0`: a downstream
 * consumer can tell "not reported" from "reported as zero" only if the key
 * is missing.
 */

import type { Link, Node, Region } from "../types/network";

/** A GeoJSON property bag: ids, static attributes, and result values. */
export type FeatureProperties = Record<string, unknown>;

export interface GeoJsonFeature {
  type: "Feature";
  geometry:
    | { type: "Point"; coordinates: number[] }
    | { type: "LineString"; coordinates: number[][] }
    | { type: "Polygon"; coordinates: number[][][] };
  properties: FeatureProperties;
}

export interface GeoJsonFeatureCollection {
  type: "FeatureCollection";
  features: GeoJsonFeature[];
}

/**
 * Build the export's feature collection from the three element classes.
 *
 * Callers pass the **sim-merged** arrays when a run exists — those are the
 * ones carrying `resultValues` (engine-generic, keyed by §6 catalog
 * variable id, in SI). The plain network arrays are the correct fallback
 * and simply produce geometry with static attributes.
 */
export function buildResultsGeoJson(
  nodes: Node[],
  links: Link[],
  regions: Region[],
): GeoJsonFeatureCollection {
  const nodeCoords = new Map(
    nodes.map((n) => [n.id, [n.x, n.y] as [number, number]]),
  );
  return {
    type: "FeatureCollection",
    features: [
      ...nodes.map(
        (n): GeoJsonFeature => ({
          type: "Feature",
          geometry: { type: "Point", coordinates: [n.x, n.y] },
          properties: {
            id: n.id,
            type: n.type,
            // Static attributes, then result values when available.
            ...(n.elevation != null ? { elevation: n.elevation } : {}),
            ...(n.pressure != null ? { pressure: n.pressure } : {}),
            ...(n.head != null ? { head: n.head } : {}),
            ...(n.demand != null ? { demand: n.demand } : {}),
            ...(n.quality != null ? { quality: n.quality } : {}),
            ...(n.resultValues ?? {}),
          },
        }),
      ),
      ...links.map((l): GeoJsonFeature => {
        const from = nodeCoords.get(l.fromId) ?? [0, 0];
        const to = nodeCoords.get(l.toId) ?? [0, 0];
        return {
          type: "Feature",
          geometry: {
            type: "LineString",
            // Intermediate vertices included — a straight from→to line
            // flattened every polyline conduit.
            coordinates: [from, ...(l.vertices ?? []), to],
          },
          properties: {
            id: l.id,
            type: l.type,
            ...(l.diameter != null && l.diameter > 0
              ? { diameter: l.diameter }
              : {}),
            ...(l.length != null && l.length > 0 ? { length: l.length } : {}),
            // `velocity` is only meaningful once sim data is merged in —
            // gate it on `flow` like the other result values.
            ...(l.flow != null ? { flow: l.flow, velocity: l.velocity } : {}),
            ...(l.status != null ? { status: l.status } : {}),
            ...(l.quality != null ? { quality: l.quality } : {}),
            ...(l.resultValues ?? {}),
          },
        };
      }),
      // Areal elements as closed polygons. A ring of fewer than three
      // points is not a polygon and is dropped rather than exported as
      // degenerate geometry.
      ...regions
        .filter((r) => r.ring.length >= 3)
        .map(
          (r): GeoJsonFeature => ({
            type: "Feature",
            geometry: {
              type: "Polygon",
              coordinates: [[...r.ring, r.ring[0]]],
            },
            properties: {
              id: r.id,
              type: r.type,
              ...(r.outletId != null ? { outlet: r.outletId } : {}),
              ...(r.resultValues ?? {}),
            },
          }),
        ),
    ],
  };
}
