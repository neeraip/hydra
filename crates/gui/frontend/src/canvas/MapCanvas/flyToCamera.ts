/**
 * Where to put the camera to show one element.
 *
 * Following a connection from the inspector, locating an extreme from the
 * legend, or picking an element out of the network list all end here: the
 * canvas has to move to something and decide how close to get.
 *
 * The arithmetic differs by renderer. A geographic view has a zoom scale
 * with an agreed meaning, so a node is a floor on the current zoom and a
 * link is a bounds fit. An orthographic view has neither — its zoom is
 * `log2` of an arbitrary scale over coordinates the layout invented — so
 * every answer there is relative to the zoom that frames the whole network,
 * and capped, or repeated visits ratchet inward with nothing to stop them.
 *
 * All of it was inline in one effect, where the only way to check a cap was
 * to click something and look.
 */

/** A point in whichever space the renderer is using. */
export type Point2 = [number, number];

/** An orthographic camera, in deck's terms. */
export interface OrthoCamera {
  target: [number, number, number];
  zoom: number;
}

/**
 * How close a geographic view gets to a single node.
 *
 * A floor rather than a set zoom: someone already closer than this asked to
 * see the element, not to be pulled back out to a standard distance.
 */
export const NODE_MIN_MAP_ZOOM = 14;

/** Where to start from when the map cannot say where it is. */
const MAP_ZOOM_FALLBACK = 12;

/**
 * The zoom to fly a geographic view to for a node.
 *
 * Reads the live zoom rather than any stored camera: MapLibre's pans and
 * zooms are its own, and nothing here mirrors them.
 */
export function mapZoomForNode(currentZoom: number): number {
  const from = Number.isFinite(currentZoom) ? currentZoom : MAP_ZOOM_FALLBACK;
  return Math.max(from, NODE_MIN_MAP_ZOOM);
}

/** How far past the whole-network fit a single node may be zoomed. */
const NODE_ZOOM_OVER_FIT = 1;

/** And the ceiling that applies however far out the fit was. */
const NODE_ZOOM_CEILING = 10;

/**
 * The orthographic camera for a single node.
 *
 * Relative to the fit and capped: an orthographic zoom means nothing on its
 * own, and without a ceiling a network laid out very small would zoom
 * arbitrarily far in.
 */
export function orthoCameraForNode(
  target: Point2,
  fitZoom: number,
): OrthoCamera {
  return {
    target: [target[0], target[1], 0],
    zoom: Math.min(fitZoom + NODE_ZOOM_OVER_FIT, NODE_ZOOM_CEILING),
  };
}

/** The share of the smaller viewport dimension a framed link should span. */
const LINK_SPAN_FRACTION = 0.4;

/** How far past the whole-network fit a link may be zoomed. */
const LINK_ZOOM_OVER_FIT = 3;

/** The fallback when a link has no length to frame. */
const ZERO_LENGTH_ZOOM_OVER_FIT = 2;

/**
 * The orthographic camera for a link, sized so the link spans a fixed share
 * of the viewport.
 *
 * Centred on the link's midpoint, with the zoom solved from the span it
 * should occupy: deck's orthographic zoom is `log2` of the scale, so the
 * pixels-per-unit wanted becomes a zoom by taking the log.
 *
 * A zero-length link — a pump between coincident nodes, which is a real
 * thing in these models — has no span to solve for, so it falls back to a
 * fixed step past the fit. Without that the division is by zero and the
 * camera goes to infinity.
 */
export function orthoCameraForLink(
  from: Point2,
  to: Point2,
  viewport: { width: number; height: number },
  fitZoom: number,
): OrthoCamera {
  const target: [number, number, number] = [
    (from[0] + to[0]) / 2,
    (from[1] + to[1]) / 2,
    0,
  ];
  const linkUnits = Math.hypot(to[0] - from[0], to[1] - from[1]);
  const targetSpanPx =
    Math.min(viewport.width, viewport.height) * LINK_SPAN_FRACTION;
  const zoom =
    linkUnits > 0
      ? Math.min(
          Math.log2(targetSpanPx / linkUnits),
          fitZoom + LINK_ZOOM_OVER_FIT,
        )
      : Math.min(fitZoom + ZERO_LENGTH_ZOOM_OVER_FIT, NODE_ZOOM_CEILING);
  return { target, zoom };
}

/**
 * Whether a request to fly to a region can be honoured.
 *
 * Geographic only. Both other views have a ring that could be framed — the
 * model's own in a plan view, the placed glyph in a schematic — but neither
 * has the orthographic camera path for it, so the request is dropped rather
 * than aimed at nothing.
 */
export function regionFlyToSupported(viewMode: "map" | "schematic"): boolean {
  return viewMode === "map";
}
