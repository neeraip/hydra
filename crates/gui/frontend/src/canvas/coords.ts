/** CRS sentinel for a model whose plan coordinates are a local drawing
 * grid rather than a georeferenced system — mirrors `LOCAL_CRS` in the
 * backend. Deliberately not an EPSG code: no EPSG code is true of such a
 * model, so nothing may try to resolve a proj4 definition for it. */
export const LOCAL_CRS = "LOCAL";

/* Coordinate transforms for the canvas.
 *
 * Geographic ("map") positions use WGS84 lon/lat. Schematic positions come
 * from the BFS topological layout computed by `schematicLayout.ts`. Distance
 * and formatting utilities are used by `Annotations.tsx` for measurement
 * overlays.
 *
 * CRS detection and reprojection
 * --------------------------------
 * EPANET [COORDINATES] carry no CRS metadata — they are whatever coordinate
 * system the modeller used (UTM, State Plane, a national grid, or arbitrary
 * unitless schematic units). This module provides:
 *
 *   `sniffCoordCrs`   — fast O(n) check for obvious projected coords.
 *   `reprojectNodes`  — reproject an array of nodes from an EPSG-coded CRS
 *                       to WGS84 using proj4js. Returns new Node objects with
 *                       updated x/y; all other fields are preserved.
 *
 * Usage pattern in CanvasView:
 *   1. Load nodes from backend.
 *   2. Call `sniffCoordCrs(nodes)`. If "projected", prompt the user for an
 *      EPSG code (or auto-switch to schematic).
 *   3. Once the user provides an EPSG code, call `reprojectNodes(nodes, epsg)`.
 *   4. Pass the reprojected nodes to MapCanvas for geo/map mode.
 *
 * Caveats:
 *   • `sniffCoordCrs` returns "wgs84" for coords that happen to fall in
 *     [-180,180]×[-90,90] even if they are e.g. a small UTM zone near the
 *     origin. A CRS selector always overrides the auto-detection.
 *   • Proj4js uses a bundled subset of projections. Common EPSG codes
 *     (WGS84/4326, WebMercator/3857, all UTM zones, most national grids) work
 *     out of the box. Exotic datums may need a custom definition string.
 */

import proj4 from "proj4";
import type { Link, Node, Region } from "../types";

export interface CustomCrsDefinition {
  label: string;
  epsg: string;
  proj4: string;
}

export function normalizeEpsgCode(raw: string): string {
  const val = raw.trim().toUpperCase();
  if (!val) return "";
  if (val.startsWith("EPSG:")) return val;
  if (/^\d+$/.test(val)) return `EPSG:${val}`;
  return val;
}

export function validateCustomCrsDefinition(
  epsgRaw: string,
  projDefRaw: string,
): boolean {
  const epsg = normalizeEpsgCode(epsgRaw);
  const projDef = projDefRaw.trim();
  if (!epsg || !projDef) return false;
  try {
    proj4.defs(epsg, projDef);
    proj4(epsg, "EPSG:4326");
    return true;
  } catch {
    return false;
  }
}

export function registerCustomCrsDefinitions(
  entries: CustomCrsDefinition[],
): void {
  for (const entry of entries) {
    const epsg = normalizeEpsgCode(entry.epsg);
    const projDef = entry.proj4?.trim();
    if (!epsg || !projDef) continue;
    try {
      proj4.defs(epsg, projDef);
    } catch {
      // Ignore invalid entries. The modal validates before save.
    }
  }
}

// Baseline definitions — always available in the frontend.
proj4.defs("EPSG:4326", "+proj=longlat +datum=WGS84 +no_defs");
proj4.defs(
  "EPSG:3857",
  "+proj=merc +a=6378137 +b=6378137 +lat_ts=0 +lon_0=0 +x_0=0 +y_0=0 +k=1 +units=m +nadgrids=@null +wktext +no_defs",
);

/** Euclidean distance between two canvas points scaled to metres.
 *  1 canvas unit ≈ 4 m so a typical pipe reads as ~400 m. */
export function pixelDistance(
  ax: number,
  ay: number,
  bx: number,
  by: number,
): number {
  const dx = bx - ax;
  const dy = by - ay;
  return Math.sqrt(dx * dx + dy * dy) * 4;
}

export function formatMeters(m: number): string {
  if (m < 1000) return `${m.toFixed(0)} m`;
  return `${(m / 1000).toFixed(2)} km`;
}

/** Haversine great-circle distance in metres between two WGS84 coordinates. */
export function haversineMeters(
  lng1: number,
  lat1: number,
  lng2: number,
  lat2: number,
): number {
  const R = 6_371_000; // Earth radius in metres
  const toRad = (d: number) => (d * Math.PI) / 180;
  const dLat = toRad(lat2 - lat1);
  const dLng = toRad(lng2 - lng1);
  const a =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(toRad(lat1)) * Math.cos(toRad(lat2)) * Math.sin(dLng / 2) ** 2;
  return R * 2 * Math.asin(Math.sqrt(a));
}

/**
 * Sniff whether a set of node coordinates looks like WGS84 (lon/lat) or a
 * projected CRS.
 *
 * Returns `"projected"` as soon as any x is outside [-180, 180] or any y is
 * outside [-90, 90] — those values are unambiguously not WGS84. Returns
 * `"wgs84"` when all values are within bounds. The scan exits early and is
 * O(n) in the worst case.
 */
export function sniffCoordCrs(nodes: Node[]): "wgs84" | "projected" {
  for (const n of nodes) {
    if (n.x < -180 || n.x > 180 || n.y < -90 || n.y > 90) return "projected";
  }
  return "wgs84";
}

/**
 * Canonical list of CRS options shown in the toolbar picker.
 *
 * Each entry has a human label and an EPSG code. Definitions for codes not
 * bundled by default (UTM/MGA zones follow a naming pattern) are generated
 * on-demand by `ensureEpsgDef`.
 */
export const COMMON_CRS: Array<{ label: string; epsg: string }> = [
  { label: "WGS 84 (EPSG:4326)", epsg: "EPSG:4326" },
  { label: "Web Mercator (EPSG:3857)", epsg: "EPSG:3857" },
  { label: "UTM Zone 54N (EPSG:32654)", epsg: "EPSG:32654" },
  { label: "UTM Zone 55N (EPSG:32655)", epsg: "EPSG:32655" },
  { label: "UTM Zone 56N (EPSG:32656)", epsg: "EPSG:32656" },
  { label: "GDA2020 / MGA Zone 54 (EPSG:7854)", epsg: "EPSG:7854" },
  { label: "GDA2020 / MGA Zone 55 (EPSG:7855)", epsg: "EPSG:7855" },
  { label: "GDA94 / MGA Zone 54 (EPSG:28354)", epsg: "EPSG:28354" },
  { label: "GDA94 / MGA Zone 55 (EPSG:28355)", epsg: "EPSG:28355" },
  { label: "NAD83 / UTM Zone 10N (EPSG:26910)", epsg: "EPSG:26910" },
  { label: "NAD83 / UTM Zone 18N (EPSG:26918)", epsg: "EPSG:26918" },
];

/**
 * Ensure a proj4 definition is registered for the given EPSG code.
 *
 * For UTM zones (EPSG:326xx north, EPSG:327xx south) the definition can be
 * generated from the zone number, avoiding the need to hard-code all 120
 * variants. For other codes proj4js's bundled definitions are used.
 *
 * Returns true if the definition is available, false if unknown.
 */
export function ensureEpsgDef(epsg: string): boolean {
  if (proj4.defs(epsg)) return true;

  // Auto-generate WGS84 UTM zone definitions.
  const utmNorth = epsg.match(/^EPSG:326(\d{2})$/);
  if (utmNorth) {
    const zone = parseInt(utmNorth[1], 10);
    proj4.defs(epsg, `+proj=utm +zone=${zone} +datum=WGS84 +units=m +no_defs`);
    return true;
  }
  const utmSouth = epsg.match(/^EPSG:327(\d{2})$/);
  if (utmSouth) {
    const zone = parseInt(utmSouth[1], 10);
    proj4.defs(
      epsg,
      `+proj=utm +zone=${zone} +south +datum=WGS84 +units=m +no_defs`,
    );
    return true;
  }

  // GDA94 / GDA2020 MGA zones: EPSG:283xx, EPSG:78xx
  const mga94 = epsg.match(/^EPSG:2835(\d)$/);
  if (mga94) {
    const zone = 50 + parseInt(mga94[1], 10);
    proj4.defs(
      epsg,
      `+proj=utm +zone=${zone} +south +ellps=GRS80 +towgs84=0,0,0,0,0,0,0 +units=m +no_defs`,
    );
    return true;
  }
  const mga2020 = epsg.match(/^EPSG:785(\d)$/);
  if (mga2020) {
    const zone = 50 + parseInt(mga2020[1], 10);
    proj4.defs(
      epsg,
      `+proj=utm +zone=${zone} +south +ellps=GRS80 +units=m +no_defs`,
    );
    return true;
  }

  return false;
}

/**
 * Reproject an array of nodes from `fromEpsg` to WGS84 (EPSG:4326).
 *
 * Returns a new array of Node objects with updated `x` (longitude) and `y`
 * (latitude) fields. All other fields are passed through unchanged.
 *
 * Throws if `fromEpsg` is unknown and cannot be auto-generated. The caller
 * should catch this and show the user a meaningful error.
 */
/**
 * Forward plan-coordinate transform for canvas layers that draw raw model
 * coordinates (the 2D surface mesh, whose vertices never pass through the
 * node pipeline). Mirrors the node path exactly: a local grid flips Y once
 * for the orthographic view, WGS84 passes through, a projected CRS goes
 * through the same proj4 forward as {@link reprojectNodes}. `null` when
 * the CRS has no registered definition yet — the caller shows nothing
 * rather than something misplaced.
 */
export function planProjector(
  sourceCrs: string,
): ((x: number, y: number) => [number, number]) | null {
  if (sourceCrs === LOCAL_CRS) return (x, y) => [x, -y];
  if (sourceCrs === "EPSG:4326") return (x, y) => [x, y];
  if (!ensureEpsgDef(sourceCrs)) return null;
  const converter = proj4(sourceCrs, "EPSG:4326");
  return (x, y) => converter.forward([x, y]) as [number, number];
}

export function reprojectNodes(nodes: Node[], fromEpsg: string): Node[] {
  if (fromEpsg === "EPSG:4326") return nodes; // no-op

  if (!ensureEpsgDef(fromEpsg)) {
    throw new Error(
      `Unknown CRS: ${fromEpsg}. Provide a proj4 definition string or use a supported EPSG code.`,
    );
  }

  const converter = proj4(fromEpsg, "EPSG:4326");
  return nodes.map((n) => {
    const [lon, lat] = converter.forward([n.x, n.y]);
    return { ...n, x: lon, y: lat };
  });
}

/**
 * Reproject regions' boundary rings from `fromEpsg` to WGS84, the same
 * forward transform {@link reprojectNodes} applies to node coordinates.
 * Region counts are small (dozens, not tens of thousands), so no identity
 * cache is needed. Throws like {@link reprojectNodes} for unknown codes.
 */
export function reprojectRegions(
  regions: Region[],
  fromEpsg: string,
): Region[] {
  if (fromEpsg === "EPSG:4326") return regions; // no-op
  if (!ensureEpsgDef(fromEpsg)) {
    throw new Error(
      `Unknown CRS: ${fromEpsg}. Provide a proj4 definition string or use a supported EPSG code.`,
    );
  }
  const converter = proj4(fromEpsg, "EPSG:4326");
  return regions.map((r) => ({
    ...r,
    ring: r.ring.map(([x, y]) => {
      const [lon, lat] = converter.forward([x, y]);
      return [lon, lat] as [number, number];
    }),
  }));
}

/**
 * Inverse-project a single WGS84 [lon, lat] point back to the network's
 * source CRS — the exact inverse of the {@link reprojectNodes} forward path,
 * built from the same proj4 converter so a drag round-trips losslessly.
 *
 * Used when committing map-space edits (node drag drop, add-node click) to
 * the backend coordinate store, which holds source-CRS values.
 *
 * Identity fast-path: when `toEpsg` is EPSG:4326 the input point is returned
 * unchanged (the store is already WGS84).
 *
 * Throws when the CRS is unknown/unregistered or when the inverse transform
 * fails or produces a non-finite coordinate (out-of-domain point). Callers
 * must NOT commit on throw — a raw WGS84 value in a projected store corrupts
 * the coordinate.
 */
export function wgs84ToSourceCrs(
  point: [number, number],
  toEpsg: string,
): [number, number] {
  if (toEpsg === "EPSG:4326") return point; // identity — store is WGS84

  if (!ensureEpsgDef(toEpsg)) {
    throw new Error(
      `Unknown CRS: ${toEpsg}. Provide a proj4 definition string or use a supported EPSG code.`,
    );
  }

  // Same converter construction as the forward path; .inverse runs
  // WGS84 → source CRS.
  const converter = proj4(toEpsg, "EPSG:4326");
  const [x, y] = converter.inverse(point);
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    throw new Error(
      `Coordinate (${point[0]}, ${point[1]}) cannot be projected to ${toEpsg}.`,
    );
  }
  return [x, y];
}

/**
 * Reproject with a per-node identity cache. When a node's *source object* is
 * unchanged since the previous call, the previously produced output object is
 * returned (same identity, no proj4 call, no allocation) — so re-running
 * after a single-element network patch costs O(changed nodes) proj4 work
 * instead of O(N). The caller owns `cache` and must reset it when the CRS
 * changes.
 *
 * Throws like {@link reprojectNodes} for unknown CRS codes.
 */
export function reprojectNodesCached(
  nodes: Node[],
  fromEpsg: string,
  cache: Map<string, { src: Node; out: Node }>,
): Node[] {
  if (fromEpsg === "EPSG:4326") return nodes; // no-op

  if (!ensureEpsgDef(fromEpsg)) {
    throw new Error(
      `Unknown CRS: ${fromEpsg}. Provide a proj4 definition string or use a supported EPSG code.`,
    );
  }

  const converter = proj4(fromEpsg, "EPSG:4326");
  const result = nodes.map((n) => {
    const hit = cache.get(n.id);
    if (hit && hit.src === n) return hit.out;
    const [lon, lat] = converter.forward([n.x, n.y]);
    const out = { ...n, x: lon, y: lat };
    cache.set(n.id, { src: n, out });
    return out;
  });
  // Evict entries for deleted nodes so long editing sessions don't grow the
  // cache unboundedly. Only runs when deletions have actually happened.
  if (cache.size > nodes.length) {
    const live = new Set(nodes.map((n) => n.id));
    for (const id of cache.keys()) {
      if (!live.has(id)) cache.delete(id);
    }
  }
  return result;
}

/**
 * Reproject link polyline vertices from `fromEpsg` to WGS84, mirroring
 * {@link reprojectNodesCached}: links whose *source object* is unchanged since
 * the previous call reuse the previously produced output object (identity, no
 * proj4 work), and links without vertices pass through untouched. When no
 * link in the array carries vertices (or the CRS is already WGS84) the input
 * array itself is returned so downstream identity-keyed memos stay stable.
 *
 * Throws like {@link reprojectNodes} for unknown CRS codes.
 */
export function reprojectLinkVerticesCached(
  links: Link[],
  fromEpsg: string,
  cache: Map<string, { src: Link; out: Link }>,
): Link[] {
  if (fromEpsg === "EPSG:4326") return links; // no-op

  if (!ensureEpsgDef(fromEpsg)) {
    throw new Error(
      `Unknown CRS: ${fromEpsg}. Provide a proj4 definition string or use a supported EPSG code.`,
    );
  }

  const converter = proj4(fromEpsg, "EPSG:4326");
  let anyVertices = false;
  const result = links.map((l) => {
    if (!l.vertices || l.vertices.length === 0) return l;
    anyVertices = true;
    const hit = cache.get(l.id);
    if (hit && hit.src === l) return hit.out;
    const vertices = l.vertices.map(
      ([x, y]) => converter.forward([x, y]) as [number, number],
    );
    const out = { ...l, vertices };
    cache.set(l.id, { src: l, out });
    return out;
  });
  if (!anyVertices) return links;
  if (cache.size > links.length) {
    const live = new Set(links.map((l) => l.id));
    for (const id of cache.keys()) {
      if (!live.has(id)) cache.delete(id);
    }
  }
  return result;
}
