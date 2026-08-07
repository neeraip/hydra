// maplibre-gl 6 dropped its default export; the namespace import keeps
// every `maplibregl.X` usage unchanged.
import * as maplibregl from "maplibre-gl";
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
 * options so the map never renders at an arbitrary default before the initial
 * fit runs. The formula is intentionally simple — `fitMapExtents` will refine
 * it via `map.cameraForBounds` once the style is loaded, but since we're
 * already roughly centered the user won't see any perceivable movement.
 *
 * When no real coordinates exist (all-sentinel network) falls back to a
 * world-level view at zoom 1 centered on 0°E / 20°N.
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
  opts: {
    animate?: boolean;
    /** Flight time in ms. MapLibre's own default scales with the distance
     *  travelled, which across a national network is several seconds. */
    duration?: number;
    padding?: maplibregl.PaddingOptions;
  } = {},
): void {
  const bounds = geoBounds(nodes);
  if (!bounds) return;
  // Silently bail when coordinates are outside WGS84 range (e.g. UTM).
  const [[minLon, minLat], [maxLon, maxLat]] = bounds;
  if (minLon < -180 || maxLon > 180 || minLat < -90 || maxLat > 90) return;
  let camera: ReturnType<typeof map.cameraForBounds>;
  try {
    camera = map.cameraForBounds(bounds, {
      padding: opts.padding ?? 48,
      maxZoom: 18,
    });
  } catch {
    return;
  }
  if (!camera?.center) return;
  const center = maplibregl.LngLat.convert(camera.center);
  if (opts.animate) {
    map.flyTo({
      center: [center.lng, center.lat],
      zoom: camera.zoom ?? 12,
      curve: 1,
      duration: opts.duration,
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

/**
 * The chrome floating over the map, as MapLibre edge padding.
 *
 * A fit that ignores it centres the network in the *container*, which is
 * correct arithmetic and the wrong answer: the rail covers the left edge,
 * the inspector the right, the toolbar the top, and the legend and viewport
 * controls the bottom, so the network lands visibly off to one side of the
 * part you can see.
 *
 * The two side panels publish their occupied width; the toolbar and legend
 * do not, so their heights are constants here — they are fixed by
 * `--tool-btn-size` and the legend bar's own `minHeight`, and being a few
 * pixels out only costs a little breathing room.
 *
 * **Charge a corner overlay to the edge it is thin against.** MapLibre
 * padding is a frame, not a set of rectangles, so reserving an overlay's
 * *height* on the bottom withholds that height across the entire width.
 * The viewport controls are a tall, narrow column in one corner: billing
 * their ~164px height to the bottom edge cost a quarter of the container's
 * height everywhere, and the network sat high above a wide empty band.
 * Reserving their ~40px *width* on the right instead is both cheaper and
 * strictly safer — no content is placed in their column at all, so nothing
 * can end up behind them.
 */
export function visibleMapPadding(
  map: maplibregl.Map,
): maplibregl.PaddingOptions {
  const style = getComputedStyle(document.documentElement);
  const px = (name: string): number => {
    const value = Number.parseFloat(style.getPropertyValue(name));
    return Number.isFinite(value) ? value : 0;
  };
  /** Breathing room between the network and whatever bounds it. */
  const MARGIN = 24;
  const button = px("--tool-btn-size") || 30;
  /** `.canvas-toolbar`: one row of buttons plus its 4px padding and border,
   * sitting 12px down from the top. */
  const TOOLBAR = 12 + button + 10;
  /** The legend bar's `minHeight`, 14px up from the bottom. Present only
   * once results exist, so this over-pads a model that has not run — in the
   * direction of more breathing room, not less. */
  const LEGEND = 14 + 32;
  /** The viewport controls' *width*: one button column inside the toolbar's
   * 4px padding and 1px border, sitting 12px in from the right edge. Their
   * height is deliberately not charged to the bottom — see the note above. */
  const CONTROLS_W = 12 + button + 10;

  const padding = {
    left: px("--rail-effective-w") + MARGIN,
    right: px("--inspector-effective-w") + CONTROLS_W + MARGIN,
    top: TOOLBAR + MARGIN,
    // The legend bar genuinely spans this edge horizontally, so its height
    // is the honest cost here.
    bottom: LEGEND + MARGIN,
  };
  // Padding that exceeds the container leaves no viewport to fit into, and
  // MapLibre's camera maths degenerates. A container that small has no room
  // for a considered fit anyway, so fall back to a plain even margin.
  const canvas = map.getCanvas();
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  if (
    padding.left + padding.right >= width ||
    padding.top + padding.bottom >= height
  ) {
    return { top: 8, bottom: 8, left: 8, right: 8 };
  }

  // A last slice of whatever space is left over, so a fit never runs the
  // network right up to the edge it was fitted into. Proportional rather
  // than fixed: the allowance then means the same thing on a laptop and on
  // a large display, and it absorbs the places these constants are a few
  // pixels optimistic — an outermost node's own radius and label among them,
  // neither of which a bounds fit knows about, since bounds are computed
  // from coordinates and drawn with width.
  const SLACK = 0.05;
  const freeWidth = width - padding.left - padding.right;
  const freeHeight = height - padding.top - padding.bottom;
  return {
    left: padding.left + freeWidth * SLACK,
    right: padding.right + freeWidth * SLACK,
    top: padding.top + freeHeight * SLACK,
    bottom: padding.bottom + freeHeight * SLACK,
  };
}
