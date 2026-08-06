/**
 * A network to show for a project that has not been drawn yet.
 *
 * A project only gets a real sketch once it has been opened, so an install
 * that predates the feature shows a grid of cards with nothing in them. An
 * engine's mark filled that space before, and read as an error rather than
 * as "not yet".
 *
 * These are invented networks, not real ones, and they are shaped like the
 * kind of thing each engine models: a distribution network is a trunk main
 * with dense residential branches hanging off it, while a drainage network
 * is a dendritic run of channels gathering from a catchment. Someone who
 * knows the domain reads which engine a card belongs to before reading the
 * badge.
 *
 * Drawn rather than shipped as images, so they take the theme, scale to any
 * card, and cost no files. They are deliberately faint at the call site:
 * the point is to fill the frame, not to be mistaken for the real model.
 */

import type { Sketch, SketchSegment } from "./NetworkSketch";

/**
 * Deterministic jitter in `-1..1`.
 *
 * A hash rather than `Math.random`, so a card does not redraw differently
 * on every render, and so these can be asserted in a test.
 */
function wobble(seed: number): number {
  const x = Math.sin(seed * 12.9898) * 43758.5453;
  return (x - Math.floor(x)) * 2 - 1;
}

/** A trunk main with residential branches: what a supply network looks like. */
function distribution(): SketchSegment[] {
  const out: SketchSegment[] = [];
  // The main, running slightly off-horizontal so it does not read as a rule.
  out.push({ x1: 0.04, y1: 0.62, x2: 0.96, y2: 0.5 });

  // Branches alternate above and below, each ending in a short run of
  // streets. Even spacing with a little jitter, because a laid-out network
  // is regular without being a grid.
  for (let i = 0; i < 9; i += 1) {
    const t = 0.1 + i * 0.095;
    const mainY = 0.62 - t * 0.12;
    const up = i % 2 === 0;
    const reach = 0.22 + wobble(i) * 0.06;
    const endY = up ? mainY - reach : mainY + reach;
    out.push({ x1: t, y1: mainY, x2: t + wobble(i + 7) * 0.03, y2: endY });

    // Streets off the branch, the dense part a supply network is mostly of.
    for (let j = 1; j <= 3; j += 1) {
      const y = mainY + (endY - mainY) * (j / 3.4);
      const w = 0.035 + wobble(i * 3 + j) * 0.012;
      out.push({ x1: t - w, y1: y, x2: t + w, y2: y });
    }
  }
  return out;
}

/** A dendritic channel network gathering from a catchment. */
function drainage(): SketchSegment[] {
  const out: SketchSegment[] = [];
  // The outfall run, falling left to right toward a single outlet.
  const spine: Array<[number, number]> = [
    [0.06, 0.2],
    [0.26, 0.36],
    [0.45, 0.46],
    [0.66, 0.58],
    [0.94, 0.74],
  ];
  for (let i = 0; i < spine.length - 1; i += 1) {
    out.push({
      x1: spine[i][0],
      y1: spine[i][1],
      x2: spine[i + 1][0],
      y2: spine[i + 1][1],
    });
  }

  // Tributaries joining the spine, each splitting once. Drainage branches
  // upward into the catchment rather than fanning both ways.
  for (let i = 0; i < 6; i += 1) {
    const t = 0.14 + i * 0.14;
    const y = 0.24 + t * 0.52;
    const dir = i % 2 === 0 ? -1 : 1;
    const midX = t - 0.08 + wobble(i) * 0.03;
    const midY = y + dir * (0.16 + wobble(i + 3) * 0.05);
    out.push({ x1: t, y1: y, x2: midX, y2: midY });
    for (const k of [-1, 1]) {
      out.push({
        x1: midX,
        y1: midY,
        x2: midX - 0.07 + k * 0.05 + wobble(i * 5 + k) * 0.02,
        y2: midY + dir * (0.1 + wobble(i + k) * 0.03),
      });
    }
  }
  return out;
}

/**
 * Hold a segment inside the box.
 *
 * The generators above place branches by arithmetic, and a jittered branch
 * near an edge can reach past it. Clamping here rather than tuning the
 * arithmetic keeps the invariant true whatever anyone changes later, and
 * the difference is invisible at this size.
 */
function inBox(s: SketchSegment): SketchSegment {
  const c = (v: number) => Math.min(1, Math.max(0, v));
  return { x1: c(s.x1), y1: c(s.y1), x2: c(s.x2), y2: c(s.y2) };
}

const BY_ENGINE: Record<string, SketchSegment[]> = {
  wds: distribution().map(inBox),
  uds: drainage().map(inBox),
};

/**
 * The stand-in network for an engine, or `null` where none is drawn.
 *
 * `null` rather than a generic shape for an unknown engine: inventing a
 * picture for something this build does not understand would claim a
 * character it has no basis for.
 */
export function placeholderSketch(
  engineKey: string | undefined,
): Sketch | null {
  const segments = engineKey ? BY_ENGINE[engineKey] : undefined;
  if (!segments) return null;
  return { segments, aspect: 16 / 9 };
}
