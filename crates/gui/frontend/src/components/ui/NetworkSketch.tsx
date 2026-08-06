/**
 * A project's network, drawn small enough to recognise it by.
 *
 * Engineers know their models by outline before they read a name, and no
 * other screen shows one below full canvas size. This is what makes the
 * home page's cards worth more than rows: without the drawing they are
 * rows with padding, and the projects table does rows better.
 *
 * Inline SVG rather than an image, so it costs no file, scales to any card,
 * and takes its colour from the theme like everything else.
 */

import type React from "react";

/** One link, reduced to a straight line, in a unit box (see the backend). */
export interface SketchSegment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface Sketch {
  segments: SketchSegment[];
  /** Catchment outlines, in the same box and to the same extent. Absent on
   *  drawings made before catchments were included. */
  areas?: SketchSegment[];
  aspect: number;
}

/**
 * Stroke width for a drawing of this many segments, as a fraction of the
 * box.
 *
 * A fraction, not a screen width. An earlier version set
 * `vector-effect: non-scaling-stroke`, which reinterprets these numbers as
 * device pixels — so a 600-segment network drew at six thousandths of a
 * pixel and looked like it was fading out.
 *
 * A sparse network drawn at a hairline disappears; a dense one drawn thick
 * fills in to a solid block. Neither is recognisable, which is the only
 * thing the drawing is for, so the weight follows the count.
 */
export function strokeFor(segmentCount: number): number {
  if (segmentCount <= 40) return 0.012;
  if (segmentCount <= 200) return 0.008;
  return 0.005;
}

/**
 * Place a sketch's segments in the unit box at its true proportions.
 *
 * Done in JavaScript rather than with an SVG transform, which is what this
 * replaces. A transform scales the stroke along with the geometry, and it
 * scales it by a different amount on each axis: a network thirty times
 * wider than it is tall was drawn through `scale(1, 0.03)`, which left
 * every near-horizontal line three percent of its intended weight. The
 * result was a row of broken dashes where the model is one continuous run.
 */
export function placeSegments(
  sketch: Sketch,
  which: "segments" | "areas" = "segments",
): SketchSegment[] {
  const aspect =
    Number.isFinite(sketch.aspect) && sketch.aspect > 0 ? sketch.aspect : 1;
  const w = aspect >= 1 ? 1 : aspect;
  const h = aspect >= 1 ? 1 / aspect : 1;
  const ox = (1 - w) / 2;
  const oy = (1 - h) / 2;
  // The same placement for both, because they were normalised against one
  // extent — placing them differently is how a catchment ends up beside
  // its network instead of around it.
  return (sketch[which] ?? []).map((s) => ({
    x1: ox + s.x1 * w,
    y1: oy + s.y1 * h,
    x2: ox + s.x2 * w,
    y2: oy + s.y2 * h,
  }));
}

/**
 * How many segments a drawing may have before its nodes stop being drawn.
 *
 * A sparse network is mostly nodes: five of them joined by four pipes reads
 * as a chain of dots on the canvas and as a faint scratch without them,
 * especially when the run is nearly straight and the whole drawing is a
 * band a few pixels tall. Past this count the dots merge into the lines and
 * only cost drawing time.
 */
const NODE_DOTS_UP_TO = 60;

/** The distinct endpoints of a placed drawing, for drawing nodes. */
export function endpointsOf(
  segments: SketchSegment[],
): Array<[number, number]> {
  const seen = new Set<string>();
  const out: Array<[number, number]> = [];
  for (const s of segments) {
    for (const [x, y] of [
      [s.x1, s.y1],
      [s.x2, s.y2],
    ]) {
      const k = `${x.toFixed(4)},${y.toFixed(4)}`;
      if (seen.has(k)) continue;
      seen.add(k);
      out.push([x, y]);
    }
  }
  return out;
}

export function NetworkSketch({
  sketch,
  style,
}: {
  sketch: Sketch;
  style?: React.CSSProperties;
}) {
  const segments = placeSegments(sketch);
  const areas = placeSegments(sketch, "areas");
  const stroke = strokeFor(segments.length);
  const nodes = segments.length <= NODE_DOTS_UP_TO ? endpointsOf(segments) : [];
  return (
    <svg
      // Decorative: the card names the project beside it, and a list of
      // line segments read aloud would be noise.
      aria-hidden
      focusable="false"
      viewBox={`0 0 1 1`}
      preserveAspectRatio="xMidYMid meet"
      style={{ display: "block", width: "100%", height: "100%", ...style }}
    >
      <title>Network outline</title>
      <g>
        {/* Catchments first and fainter, so the conveyance reads on top of
            them rather than competing with them. Same order and the same
            relationship the canvas uses. */}
        {areas.map((s) => (
          <line
            key={`a${s.x1},${s.y1},${s.x2},${s.y2}`}
            x1={s.x1}
            y1={s.y1}
            x2={s.x2}
            y2={s.y2}
            stroke="currentColor"
            strokeOpacity={0.35}
            strokeWidth={stroke * 0.7}
            strokeLinecap="round"
          />
        ))}
        {segments.map((s) => (
          <line
            key={`${s.x1},${s.y1},${s.x2},${s.y2}`}
            x1={s.x1}
            y1={s.y1}
            x2={s.x2}
            y2={s.y2}
            stroke="currentColor"
            strokeWidth={stroke}
            strokeLinecap="round"
          />
        ))}
        {nodes.map(([x, y]) => (
          <circle
            key={`${x},${y}`}
            cx={x}
            cy={y}
            r={stroke * 1.1}
            fill="currentColor"
          />
        ))}
      </g>
    </svg>
  );
}
