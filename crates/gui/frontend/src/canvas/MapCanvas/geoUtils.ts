import maplibregl from "maplibre-gl";
import type { Node } from "../../hooks";

/** Bounding box of all node coordinates. Returns null when nodes is empty. */
export function geoBounds(
  nodes: Node[],
): [[number, number], [number, number]] | null {
  if (nodes.length === 0) return null;
  // Iterative min/max avoids Math.min(...spread) which stack-overflows on
  // large networks (> ~100k nodes) because of JS argument-count limits.
  // Nodes with x===0 && y===0 are the backend sentinel for "no [COORDINATES]"
  // entry — exclude them so they don't skew the bounding box.
  let minLon = Infinity,
    maxLon = -Infinity;
  let minLat = Infinity,
    maxLat = -Infinity;
  let seen = false;
  for (const n of nodes) {
    if (n.x === 0 && n.y === 0) continue;
    seen = true;
    if (n.x < minLon) minLon = n.x;
    if (n.x > maxLon) maxLon = n.x;
    if (n.y < minLat) minLat = n.y;
    if (n.y > maxLat) maxLat = n.y;
  }
  if (!seen) return null;
  // Expand degenerate single-point bounds so cameraForBounds doesn't over-zoom.
  const padLon = minLon === maxLon ? 0.002 : 0;
  const padLat = minLat === maxLat ? 0.002 : 0;
  return [
    [minLon - padLon, minLat - padLat],
    [maxLon + padLon, maxLat + padLat],
  ];
}

/**
 * Rough initial geo viewState derived from node bounding-box extents.
 *
 * Used to seed both the deck.gl viewState ref and the MapLibre `center`/`zoom`
 * options so the map never renders at an arbitrary default before `doFit` runs.
 * The formula is intentionally simple — `fitGeoExtents` will refine it via
 * `map.cameraForBounds` once the style is loaded, but since we're already
 * roughly centered the user won't see any perceivable movement.
 *
 * When no real coordinates exist (all-sentinel network) falls back to a
 * world-level view at zoom 1 centered on 0°N / 20°N.
 */
export function roughGeoViewState(nodes: Node[]): {
  longitude: number;
  latitude: number;
  zoom: number;
  pitch: number;
  bearing: number;
} {
  const bounds = geoBounds(nodes);
  if (!bounds)
    return { longitude: 0, latitude: 20, zoom: 1, pitch: 0, bearing: 0 };
  const longitude = (bounds[0][0] + bounds[1][0]) / 2;
  const latitude = (bounds[0][1] + bounds[1][1]) / 2;
  // Guard against non-WGS84 coordinates (e.g. UTM) crashing MapLibre — if the
  // computed centre is outside valid lon/lat range, fall back to world view.
  // The CRS error is surfaced via the toolbar picker, not a canvas crash.
  if (longitude < -180 || longitude > 180 || latitude < -90 || latitude > 90) {
    return { longitude: 0, latitude: 20, zoom: 1, pitch: 0, bearing: 0 };
  }
  const dLon = Math.max(bounds[1][0] - bounds[0][0], 0.004);
  const dLat = Math.max(bounds[1][1] - bounds[0][1], 0.004);
  // Fit whichever dimension is larger, targeting ~70% of a typical viewport.
  // At zoom z, the number of degrees visible horizontally ≈ 360 / 2^z.
  const span = Math.max(dLon, dLat * 1.5); // rough aspect correction
  const zoom = Math.max(1, Math.min(18, Math.log2(270 / span)));
  return { longitude, latitude, zoom, pitch: 0, bearing: 0 };
}

/**
 * Fit the deck.gl + MapLibre cameras to the full network extents.
 * Uses `map.cameraForBounds` so the zoom accounts for the actual container size.
 */
export function fitMapExtents(
  nodes: Node[],
  map: maplibregl.Map,
  opts: { animate?: boolean } = {},
): void {
  const bounds = geoBounds(nodes);
  if (!bounds) return;
  // Silently bail when coordinates are outside WGS84 range (e.g. UTM).
  const [[minLon, minLat], [maxLon, maxLat]] = bounds;
  if (minLon < -180 || maxLon > 180 || minLat < -90 || maxLat > 90) return;
  let camera: ReturnType<typeof map.cameraForBounds>;
  try {
    camera = map.cameraForBounds(bounds, { padding: 48, maxZoom: 18 });
  } catch {
    return;
  }
  if (!camera) return;
  const center = maplibregl.LngLat.convert(camera.center!);
  if (opts.animate) {
    map.flyTo({
      center: [center.lng, center.lat],
      zoom: camera.zoom ?? 12,
      curve: 1,
    });
  } else {
    map.jumpTo({ center: [center.lng, center.lat], zoom: camera.zoom ?? 12 });
  }
}

export function orthoCenterFromMap(coords: Map<string, [number, number]>): {
  target: [number, number, number];
  zoom: number;
} {
  if (coords.size === 0) return { target: [0, 0, 0], zoom: 0 };
  let minX = Infinity,
    maxX = -Infinity,
    minY = Infinity,
    maxY = -Infinity;
  for (const [x, y] of coords.values()) {
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  const span = Math.max(maxX - minX, maxY - minY);
  const zoom = span > 0 ? Math.log2(600 / span) : 0;
  return { target: [cx, cy, 0], zoom };
}
