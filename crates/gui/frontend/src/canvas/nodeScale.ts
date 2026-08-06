/**
 * How big a node is drawn, relative to the network it sits in.
 *
 * A node's radius was 7 metres: an absolute size with no relationship to
 * the model. On a dense urban network, where junctions are tens of metres
 * apart, that reads as a junction. On a transmission main, where they are
 * kilometres apart, it is a rounding error — so it pinned to the pixel
 * floor and stayed there through most of the zoom range, and a network
 * zoomed in far enough to show two nodes still showed them as dots.
 *
 * What looks wrong there is the ratio of node size to link length, and
 * that ratio is exactly what zoom cannot change: zoom scales both
 * together. `schematicAspect` makes the same argument for its own control,
 * and notes that it leaves radii alone. This is the part it left.
 *
 * So the radius is derived from the network's own geometry, and the slider
 * is a multiplier on that rather than a size. A multiplier means the same
 * thing on every model; a size in metres would have a different useful
 * range on each one, which is the original problem wearing a control.
 */

/** Slider positions, with the derived size at the midpoint. */
export const NODE_SCALE_MIN = 0;
export const NODE_SCALE_MAX = 100;
export const NODE_SCALE_DEFAULT = 50;

/** The multiplier at each end of the slider. */
const MIN_FACTOR = 0.4;
const MAX_FACTOR = 3;

/**
 * Node radius as a fraction of the typical link length.
 *
 * Chosen so a dense network lands near the 7 metres this replaced: with
 * junctions about 60m apart, the derived radius is about 7m and nothing
 * visibly changes for the models that already looked right.
 */
const RADIUS_PER_LINK = 0.12;

/**
 * A sane radius for a network with no usable geometry.
 *
 * A model can have one node, or every node stacked at the same point.
 * There is no typical length to scale from, and the pixel clamps will
 * govern anyway.
 */
const FALLBACK_RADIUS = 7;

/**
 * The multiplier a slider position means.
 *
 * Geometric rather than linear, so a step left shrinks by as much as a
 * step right grows. On a linear scale the midpoint of 0.4 and 3 is not 1,
 * and the neutral position would not be neutral.
 */
export function nodeScaleFactor(position: number): number {
  const clamped = Math.min(
    NODE_SCALE_MAX,
    Math.max(
      NODE_SCALE_MIN,
      Number.isFinite(position) ? position : NODE_SCALE_DEFAULT,
    ),
  );
  const t =
    (clamped - NODE_SCALE_DEFAULT) / (NODE_SCALE_MAX - NODE_SCALE_DEFAULT);
  return t >= 0 ? MAX_FACTOR ** t : MIN_FACTOR ** -t;
}

/**
 * The typical distance between connected nodes, in model units.
 *
 * The median, not the mean: a handful of long transmission links must not
 * drag a dense network's nodes up, and a network is usually mostly its
 * ordinary links.
 *
 * Measured between drawn positions rather than taken from a pipe's length
 * attribute: what governs how this looks is where the nodes are on screen.
 *
 * Only meaningful for a geographic layout. A schematic's distances are
 * chosen by the layout rather than by the model, and they move with the
 * aspect slider — measuring them would tie node size to a control that has
 * nothing to do with it.
 */
export function typicalLinkLength(
  links: ReadonlyArray<{ from: [number, number]; to: [number, number] }>,
): number | null {
  const lengths: number[] = [];
  for (const l of links) {
    const dx = l.to[0] - l.from[0];
    const dy = l.to[1] - l.from[1];
    const d = Math.hypot(dx, dy);
    // Zero-length links are real — a pump between coincident nodes — and
    // say nothing about spacing.
    if (Number.isFinite(d) && d > 0) lengths.push(d);
  }
  if (lengths.length === 0) return null;
  lengths.sort((a, b) => a - b);
  return lengths[Math.floor(lengths.length / 2)];
}

/**
 * The radius to draw nodes at, in the same units the positions are in.
 *
 * @param typical  the network's typical link length, or `null` when it has
 *                 no geometry to speak of.
 * @param position the slider's position.
 */
export function nodeRadius(typical: number | null, position: number): number {
  const base =
    typical === null || !Number.isFinite(typical) || typical <= 0
      ? FALLBACK_RADIUS
      : typical * RADIUS_PER_LINK;
  return base * nodeScaleFactor(position);
}
