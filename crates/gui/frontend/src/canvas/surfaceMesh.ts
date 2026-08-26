/**
 * The 2D surface as deck.gl binary attributes — every decision between
 * the decoded sidecar payloads (`hooks/surface.ts`) and the layer that
 * draws them.
 *
 * The mesh renders as one SolidPolygonLayer over pre-triangulated cells:
 * vertices are duplicated per cell so each triangle takes one flat
 * colour (a cell is the solver's unit of state; smoothing across cells
 * would draw claims the marcher never made). Colours come from
 * `genericRgba`, the same decision every network element renders
 * through, and a cell holding less than the display drying depth is
 * fully transparent so the network stays legible underneath.
 */

import type { GenericVariable } from "../hooks/results";
import type { SurfaceGeometry } from "../hooks/surface";
import { genericRgba, seqRgb } from "./MapCanvas/colorUtils";

/**
 * Below this depth (m) a cell renders fully transparent. A display
 * decision, not physics: the marcher's own drying threshold is not
 * carried in the sidecar, and one millimetre of water on a surface is
 * visual noise at any zoom a network is read at.
 */
export const SURFACE_DRY_DEPTH_M = 0.001;

/** Fill alpha for wet cells. Below opaque so the basemap's context and
 * the flooding's extent read together. */
export const SURFACE_ALPHA = 200;

/**
 * Fill alpha for a mesh with no run behind it: present, and plainly
 * carrying no data. Faint enough that the network reads through it,
 * strong enough to answer "does this model have a 2D surface" without
 * simulating anything first.
 */
export const SURFACE_FOOTPRINT_ALPHA = 46;

/** Binary polygon data for deck.gl's SolidPolygonLayer. */
export interface SurfacePolygonData {
  length: number;
  startIndices: Uint32Array;
  attributes: {
    getPolygon: { value: Float64Array; size: 2 };
  };
}

/**
 * Per-triangle-vertex plan positions, cells duplicated so colours stay
 * flat per cell. `project` maps model-plan coordinates to the canvas's
 * coordinate space (WGS84 in map mode); the identity serves a local
 * grid.
 */
export function surfacePolygonData(
  geometry: SurfaceGeometry,
  project: (x: number, y: number) => [number, number] = (x, y) => [x, y],
): SurfacePolygonData {
  const { nCells, positions, triangles } = geometry;
  const value = new Float64Array(6 * nCells);
  const startIndices = new Uint32Array(nCells);
  for (let ci = 0; ci < nCells; ci++) {
    startIndices[ci] = 3 * ci;
    for (let k = 0; k < 3; k++) {
      const vi = triangles[3 * ci + k];
      const [x, y] = project(positions[3 * vi], positions[3 * vi + 1]);
      value[6 * ci + 2 * k] = x;
      value[6 * ci + 2 * k + 1] = y;
    }
  }
  return {
    length: nCells,
    startIndices,
    attributes: { getPolygon: { value, size: 2 } },
  };
}

/**
 * Per-triangle-vertex RGBA for a mesh with no values: every cell the
 * same faint tint, so the surface's extent is visible and its silence
 * about depth is obvious.
 */
export function surfaceFootprintColors(nCells: number): Uint8Array {
  const out = new Uint8Array(12 * nCells);
  // The family's midpoint, so a footprint reads as the same material the
  // coloured surface is made of rather than as a fourth kind of thing.
  const [r, g, b] = seqRgb(0.5, "surface");
  for (let i = 0; i < nCells * 3; i++) {
    const at = 4 * i;
    out[at] = r;
    out[at + 1] = g;
    out[at + 2] = b;
    out[at + 3] = SURFACE_FOOTPRINT_ALPHA;
  }
  return out;
}

/**
 * Per-triangle-vertex RGBA for one instant: the selected variable's
 * value through the engine's ramp, dry cells transparent. `depth` masks
 * whatever variable is on show — a dry cell has no water surface or
 * speed worth colouring either.
 */
export function surfaceFillColors(
  values: Float32Array,
  depth: Float32Array,
  variable: GenericVariable,
): Uint8Array {
  const nCells = values.length;
  const out = new Uint8Array(12 * nCells);
  for (let ci = 0; ci < nCells; ci++) {
    if (!(depth[ci] > SURFACE_DRY_DEPTH_M)) continue; // alpha stays 0
    const [r, g, b, a] = genericRgba(
      values[ci],
      variable,
      SURFACE_ALPHA,
      // The surface's own hue family — the same key the legend samples,
      // so the swatch and the map cannot disagree.
      "surface",
    );
    for (let k = 0; k < 3; k++) {
      const at = 12 * ci + 4 * k;
      out[at] = r;
      out[at + 1] = g;
      out[at + 2] = b;
      out[at + 3] = a;
    }
  }
  return out;
}
