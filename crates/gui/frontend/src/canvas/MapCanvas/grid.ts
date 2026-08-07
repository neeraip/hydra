/**
 * The grid drawn under a diagram.
 *
 * A schematic has no basemap, and without one it is a network floating on a
 * flat ground with nothing to say what kind of surface it is. A grid says
 * it: this is a diagram, drawn in its own space, not a map of anywhere.
 *
 * It is also a scale reference the schematic otherwise lacks. In a plan the
 * coordinates are the model's own, so the squares are real distance. In a
 * topological layout they are positions the layout invented, and the grid
 * is texture rather than measure — which is why nothing here is labelled.
 * An unlabelled grid claims no particular distance; a labelled one over a
 * topological layout would claim one that does not exist.
 */

/** How far apart the lines should be on screen, in pixels. */
const TARGET_SPACING_PX = 110;

/**
 * The steps a grid is allowed to use, within each power of ten.
 *
 * A grid whose spacing is whatever the zoom happens to make it changes by
 * an arbitrary factor every frame of a pinch, which reads as the ground
 * crawling. Snapping to these means it holds still and then steps once,
 * and every step is a number a reader can hold — 1, 2, 5, 10, 20, 50.
 */
const STEPS = [1, 2, 5];

/**
 * Grid spacing in world units for this zoom.
 *
 * deck's orthographic zoom is `log2` of the scale, so `2 ** zoom` is pixels
 * per world unit. The wanted spacing in world units follows, and is then
 * rounded up to the next allowed step so the lines never crowd closer than
 * asked.
 */
export function gridSpacing(
  zoom: number,
  targetPx: number = TARGET_SPACING_PX,
): number {
  const pixelsPerUnit = 2 ** zoom;
  // A camera with no usable zoom would ask for a spacing of zero or
  // infinity, and either would try to draw an unbounded number of lines.
  if (!Number.isFinite(pixelsPerUnit) || pixelsPerUnit <= 0) return 0;
  const wanted = targetPx / pixelsPerUnit;
  if (!Number.isFinite(wanted) || wanted <= 0) return 0;
  const magnitude = 10 ** Math.floor(Math.log10(wanted));
  for (const step of STEPS) {
    if (step * magnitude >= wanted) return step * magnitude;
  }
  return 10 * magnitude;
}

export interface GridBounds {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
}

/**
 * The world rectangle an orthographic camera can see.
 *
 * Padded by one spacing so the lines run past the edges rather than
 * stopping short of them as the view moves.
 */
export function visibleBounds(
  target: readonly [number, number, ...number[]],
  zoom: number,
  viewport: { width: number; height: number },
  pad = 0,
): GridBounds {
  const pixelsPerUnit = 2 ** zoom;
  const halfW = viewport.width / 2 / pixelsPerUnit + pad;
  const halfH = viewport.height / 2 / pixelsPerUnit + pad;
  return {
    minX: target[0] - halfW,
    maxX: target[0] + halfW,
    minY: target[1] - halfH,
    maxY: target[1] + halfH,
  };
}

/** One grid line, as deck wants it. */
export interface GridLine {
  from: [number, number];
  to: [number, number];
}

/**
 * The most lines to draw, whatever the arithmetic asks for.
 *
 * The spacing rule already bounds this — lines about 110px apart across any
 * plausible viewport is a few dozen — so reaching this cap means something
 * upstream is wrong, and a canvas that draws nothing is a better outcome
 * than one that hangs building a million segments.
 */
const MAX_LINES = 400;

/** The grid lines covering these bounds. */
export function gridLines(bounds: GridBounds, spacing: number): GridLine[] {
  if (!(spacing > 0) || !Number.isFinite(spacing)) return [];
  const firstX = Math.ceil(bounds.minX / spacing) * spacing;
  const firstY = Math.ceil(bounds.minY / spacing) * spacing;
  const countX = Math.floor((bounds.maxX - firstX) / spacing) + 1;
  const countY = Math.floor((bounds.maxY - firstY) / spacing) + 1;
  if (countX < 0 || countY < 0) return [];
  if (countX + countY > MAX_LINES) return [];

  const lines: GridLine[] = [];
  for (let i = 0; i < countX; i += 1) {
    const x = firstX + i * spacing;
    lines.push({ from: [x, bounds.minY], to: [x, bounds.maxY] });
  }
  for (let i = 0; i < countY; i += 1) {
    const y = firstY + i * spacing;
    lines.push({ from: [bounds.minX, y], to: [bounds.maxX, y] });
  }
  return lines;
}

/**
 * The grid's hue: a mid grey.
 *
 * Neutral on purpose, so it reads as a faint lightening on a dark ground
 * and a faint darkening on a light one without needing to know which it is
 * on. The network is drawn over it at full strength, so the grid never
 * competes with the thing it sits under.
 */
const GRID_RGB: [number, number, number] = [128, 132, 140];

/**
 * How present the grid is, normally.
 *
 * The grid is there to say what kind of surface this is, not to be read. At
 * this weight it registers as texture and the eye passes over it.
 */
const GRID_ALPHA = 20;

/**
 * And under "High contrast".
 *
 * Someone who has asked for more contrast has asked to be able to see
 * things, including this. A faint grid is the first thing to disappear for
 * a reader who needed it drawn plainly in the first place.
 */
const GRID_ALPHA_HIGH_CONTRAST = 38;

/** The grid's colour, at the weight this reader has asked for. */
export function gridRgba(
  highContrast: boolean,
): [number, number, number, number] {
  return [
    ...GRID_RGB,
    highContrast ? GRID_ALPHA_HIGH_CONTRAST : GRID_ALPHA,
  ] as [number, number, number, number];
}

/**
 * How much beyond the viewport the grid is drawn.
 *
 * A grid built to the visible rectangle needs rebuilding on every frame of
 * a pan, and a rebuild here means rebuilding every layer on the canvas —
 * which on a large network is not something to do at 60Hz for a background
 * texture. Built a viewport's worth wider in each direction, most panning
 * happens inside what is already drawn and costs nothing.
 */
export const GRID_OVERDRAW = 1;

/** What the grid currently on screen was built to cover. */
export interface GridCoverage {
  bounds: GridBounds;
  spacing: number;
}

/**
 * Whether the grid already drawn still covers the view.
 *
 * False when the camera has panned past what was drawn, or when the zoom
 * has crossed into a different spacing — the two things that make the grid
 * wrong rather than merely older.
 */
export function gridCoversView(
  built: GridCoverage | null,
  target: readonly [number, number, ...number[]],
  zoom: number,
  viewport: { width: number; height: number },
): boolean {
  if (!built) return false;
  if (gridSpacing(zoom) !== built.spacing) return false;
  const view = visibleBounds(target, zoom, viewport);
  return (
    view.minX >= built.bounds.minX &&
    view.maxX <= built.bounds.maxX &&
    view.minY >= built.bounds.minY &&
    view.maxY <= built.bounds.maxY
  );
}
