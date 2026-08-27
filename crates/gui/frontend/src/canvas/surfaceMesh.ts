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
 * Fill alpha for a mesh with nothing to colour it by: present, and
 * plainly carrying no data. Faint enough that the network reads through
 * it, strong enough to answer "does this model have a 2D surface".
 *
 * Not the picture before a run — that is the ground, which the mesh
 * carries itself. This is the fallback for a mesh whose variables have
 * not arrived (or an engine that publishes none), where a tint that
 * says nothing beats a ramp that would say something untrue.
 */
export const SURFACE_FOOTPRINT_ALPHA = 46;

/**
 * How many pixels a cell edge must span before its edges are drawn.
 *
 * Edges say what a footprint cannot — where the cells are, and where the
 * mesh is refined — but only while they can be told apart. A real mesh
 * viewed whole is 100k+ cells across a few hundred pixels, where every
 * edge drawn is a solid dark wash that hides the surface instead of
 * describing it. So the mesh shows its structure on a coarse mesh, and
 * on a fine one as soon as you zoom into a part of it.
 */
export const MESH_EDGE_MIN_PIXELS = 8;

/** Binary polygon data for deck.gl's SolidPolygonLayer. */
export interface SurfacePolygonData {
  length: number;
  startIndices: Uint32Array;
  attributes: {
    getPolygon: { value: Float64Array; size: 2 };
  };
  /** The mesh's projected extent, `[minX, minY, maxX, maxY]`, so a
   * camera fit can frame the surface and not only the network it
   * accompanies. `null` for a mesh with no cells. */
  bounds: [number, number, number, number] | null;
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
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (let ci = 0; ci < nCells; ci++) {
    startIndices[ci] = 3 * ci;
    for (let k = 0; k < 3; k++) {
      const vi = triangles[3 * ci + k];
      const [x, y] = project(positions[3 * vi], positions[3 * vi + 1]);
      value[6 * ci + 2 * k] = x;
      value[6 * ci + 2 * k + 1] = y;
      // Taken here rather than from the vertices: the extent that
      // matters is the one on screen, after projection, and an unused
      // vertex is not on screen.
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
  return {
    length: nCells,
    startIndices,
    attributes: { getPolygon: { value, size: 2 } },
    bounds: nCells > 0 ? [minX, minY, maxX, maxY] : null,
  };
}

/**
 * Per-triangle-vertex RGBA for a mesh with nothing to colour it by:
 * every cell the same faint tint, so the surface's extent is visible
 * and its silence about what is on it is obvious.
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

/** Binary line data for deck.gl's LineLayer, plus what the legibility
 * decision needs to know about the mesh it came from. */
export interface SurfaceEdgeData {
  length: number;
  attributes: {
    getSourcePosition: { value: Float64Array; size: 2 };
    getTargetPosition: { value: Float64Array; size: 2 };
  };
  /** Median edge length in projected units — the mesh's own sense of
   * scale, which is what decides whether its edges are legible. */
  medianLength: number;
}

/** Edges sampled when taking the median: a mesh's scale is a property
 * of the whole, and reading every one of 180k edges to learn it is work
 * for no more answer. */
const MEDIAN_SAMPLE_MAX = 2048;

/**
 * The mesh's unique undirected edges, projected, for drawing its
 * structure. Cells share edges, so each is emitted once: an interior
 * edge drawn twice is drawn at double opacity, which reads as a seam
 * that is not there.
 */
export function surfaceEdgeData(
  geometry: SurfaceGeometry,
  project: (x: number, y: number) => [number, number] = (x, y) => [x, y],
): SurfaceEdgeData {
  const { nVertices, nCells, positions, triangles } = geometry;
  // Project once per vertex, not once per edge end: a vertex is shared
  // by ~6 edges, and proj4 is the expensive part of this build.
  const xy = new Float64Array(2 * nVertices);
  for (let vi = 0; vi < nVertices; vi++) {
    const [x, y] = project(positions[3 * vi], positions[3 * vi + 1]);
    xy[2 * vi] = x;
    xy[2 * vi + 1] = y;
  }

  const seen = new Set<number>();
  const src: number[] = [];
  const dst: number[] = [];
  for (let ci = 0; ci < nCells; ci++) {
    const a = triangles[3 * ci];
    const b = triangles[3 * ci + 1];
    const c = triangles[3 * ci + 2];
    for (const [u, v] of [
      [a, b],
      [b, c],
      [c, a],
    ]) {
      // Undirected: order the pair so the two cells sharing an edge
      // produce one key, not two.
      const lo = u < v ? u : v;
      const hi = u < v ? v : u;
      // Packed into one number so the Set holds primitives. 2^26 caps
      // the mesh at ~67M vertices, far past anything that renders.
      const key = lo * 67_108_864 + hi;
      if (seen.has(key)) continue;
      seen.add(key);
      src.push(lo);
      dst.push(hi);
    }
  }

  const n = src.length;
  const source = new Float64Array(2 * n);
  const target = new Float64Array(2 * n);
  for (let i = 0; i < n; i++) {
    source[2 * i] = xy[2 * src[i]];
    source[2 * i + 1] = xy[2 * src[i] + 1];
    target[2 * i] = xy[2 * dst[i]];
    target[2 * i + 1] = xy[2 * dst[i] + 1];
  }

  return {
    length: n,
    attributes: {
      getSourcePosition: { value: source, size: 2 },
      getTargetPosition: { value: target, size: 2 },
    },
    medianLength: medianEdgeLength(source, target),
  };
}

/** The median length of a sample of the edges, in projected units. */
function medianEdgeLength(source: Float64Array, target: Float64Array): number {
  const n = source.length / 2;
  if (n === 0) return 0;
  const step = Math.max(1, Math.floor(n / MEDIAN_SAMPLE_MAX));
  const lengths: number[] = [];
  for (let i = 0; i < n; i += step) {
    lengths.push(
      Math.hypot(
        target[2 * i] - source[2 * i],
        target[2 * i + 1] - source[2 * i + 1],
      ),
    );
  }
  lengths.sort((a, b) => a - b);
  return lengths[Math.floor(lengths.length / 2)];
}

/**
 * Pixels one projected unit occupies at this camera.
 *
 * The two views measure in different units — the map's projected space
 * is degrees of longitude, the orthographic view's is the model's own
 * plan units — so the conversion is per view, and everything downstream
 * can then think in pixels.
 */
export function pixelsPerUnit(
  view: "map" | "orthographic",
  zoom: number,
): number {
  // Web Mercator: the world is 512 pixels wide at zoom 0, spanning 360
  // degrees. Orthographic: one unit is 2^zoom pixels (see `grid.ts`).
  return view === "map" ? (512 * 2 ** zoom) / 360 : 2 ** zoom;
}

/** Whether this mesh's edges are worth drawing at this camera. */
export function meshEdgesLegible(
  medianLength: number,
  pixelsPerProjectedUnit: number,
): boolean {
  return medianLength * pixelsPerProjectedUnit >= MESH_EDGE_MIN_PIXELS;
}

/**
 * Whether to draw the mesh's own edges over the surface: legible at this
 * camera, and not blended.
 *
 * The blend exists to read the field as one continuous surface, and a
 * wireframe over it puts back the cell boundaries the blend is
 * dissolving — the picture would say both things at once. Flat mode
 * still draws them and is one click away, so each reading shows one
 * thing plainly: where the cells are, or what the field does between
 * their centres.
 */
export function meshEdgesShown(
  medianLength: number,
  pixelsPerProjectedUnit: number,
  blended: boolean,
): boolean {
  return !blended && meshEdgesLegible(medianLength, pixelsPerProjectedUnit);
}

/**
 * Each cell's bed elevation: the mean of its three vertices, which is
 * the same reading of a cell's ground the solver's own flat closure
 * takes (§15.3). Comes from the geometry, so it is available for any
 * mesh, run or not.
 */
export function surfaceGroundValues(geometry: SurfaceGeometry): Float32Array {
  const { nCells, positions, triangles } = geometry;
  const out = new Float32Array(nCells);
  for (let ci = 0; ci < nCells; ci++) {
    const a = triangles[3 * ci];
    const b = triangles[3 * ci + 1];
    const c = triangles[3 * ci + 2];
    out[ci] =
      (positions[3 * a + 2] + positions[3 * b + 2] + positions[3 * c + 2]) / 3;
  }
  return out;
}

// ── Shading a field held at the vertices ─────────────────────────────────────
//
// A cell is the solver's unit of state, so a *result* is painted one flat
// colour per cell and claims nothing between centres. There is no honest
// way to soften that from the outside: the values between cell centres
// are not ours to invent, and the engine publishes none (see
// `docs/plans/uds-2d-terrain-and-meshing.md`, gap 1). An earlier version
// did invent them — corners took the mean of every incident cell, dry
// ones included, and a cubic bubble carried each cell's own colour back
// to its centroid. That is the reconstruction the predecessor's own
// sources warn drags water surfaces up dry banks and down into thin
// films, and it is gone.
//
// The *ground* is a different question and keeps its smooth drawing. The
// mesh holds an elevation at every vertex, so a triangle's bed is the
// plane through three known numbers: colouring the vertices and letting
// the rasteriser interpolate draws exactly that plane, inventing nothing.
// Painting the cell flat is the version that misrepresents it, by
// showing three different elevations as their mean.

/**
 * The ground at each vertex, exactly as the mesh stores it — no
 * averaging, no invention.
 */
export function groundAtVertices(geometry: SurfaceGeometry): Float32Array {
  const { nVertices, positions } = geometry;
  const out = new Float32Array(nVertices);
  for (let v = 0; v < nVertices; v++) out[v] = positions[3 * v + 2];
  return out;
}

/**
 * Per-drawn-vertex RGBA from values the mesh holds at its vertices.
 *
 * One ramp evaluation per vertex, scattered to the cells that meet there
 * (a vertex is shared by about six). Handing these to the fill layer is
 * the whole of the smooth drawing: colours are a per-vertex attribute,
 * so the rasteriser interpolates them across each triangle, which for a
 * field that varies linearly over the triangle is exact.
 */
export function surfaceCornerColors(
  geometry: SurfaceGeometry,
  vertexValues: Float32Array,
  variable: GenericVariable,
): Uint8Array {
  const { nVertices, nCells, triangles } = geometry;
  const rgba = new Uint8Array(4 * nVertices);
  for (let vi = 0; vi < nVertices; vi++) {
    const [r, g, b, a] = genericRgba(
      vertexValues[vi],
      variable,
      SURFACE_ALPHA,
      "surface",
    );
    rgba[4 * vi] = r;
    rgba[4 * vi + 1] = g;
    rgba[4 * vi + 2] = b;
    rgba[4 * vi + 3] = a;
  }
  const out = new Uint8Array(12 * nCells);
  for (let ci = 0; ci < nCells; ci++) {
    for (let k = 0; k < 3; k++) {
      const vi = triangles[3 * ci + k];
      const at = 12 * ci + 4 * k;
      for (let j = 0; j < 4; j++) out[at + j] = rgba[4 * vi + j];
    }
  }
  return out;
}

/**
 * Per-drawn-vertex RGBA of each cell's own colour, repeated for its three
 * vertices — the flat fill, and the only honest way to draw a field the
 * solver holds per cell.
 */
export function surfaceCellColors(
  values: Float32Array,
  /** Per-cell water depth, masking cells that hold none — or `null` for
   * a variable that is not water. The ground under a dry cell is still
   * there, and hiding it would be a claim about the terrain rather than
   * about the flood. */
  depth: Float32Array | null,
  variable: GenericVariable,
): Uint8Array {
  const nCells = values.length;
  const out = new Uint8Array(12 * nCells);
  for (let ci = 0; ci < nCells; ci++) {
    const dry = depth != null && !(depth[ci] > SURFACE_DRY_DEPTH_M);
    const [r, g, b, a] = genericRgba(
      values[ci],
      variable,
      SURFACE_ALPHA,
      "surface",
    );
    // A dry cell keeps its colour and loses only its alpha: the hue is
    // still the reading, and black would be a different claim.
    for (let k = 0; k < 3; k++) {
      const at = 12 * ci + 4 * k;
      out[at] = r;
      out[at + 1] = g;
      out[at + 2] = b;
      out[at + 3] = dry ? 0 : a;
    }
  }
  return out;
}

/**
 * The value the smooth picture shows at a point inside a cell: linear
 * interpolation of the cell's three corner values, which for a field the
 * mesh holds at its vertices is the field itself.
 *
 * `x, y` are in the projected space the corners were built in. Weights
 * are clamped and renormalised so a point on an edge, or a hair outside
 * it from picking tolerance, still answers.
 */
export function valueAtPoint(
  geometry: SurfaceGeometry,
  corners: Float64Array,
  vertexValues: Float32Array,
  cellIndex: number,
  x: number,
  y: number,
): number | null {
  if (cellIndex < 0 || cellIndex >= geometry.nCells) return null;
  const at = 6 * cellIndex;
  const x0 = corners[at];
  const y0 = corners[at + 1];
  const x1 = corners[at + 2];
  const y1 = corners[at + 3];
  const x2 = corners[at + 4];
  const y2 = corners[at + 5];
  const det = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
  if (!Number.isFinite(det) || det === 0) return null;
  let w0 = ((y1 - y2) * (x - x2) + (x2 - x1) * (y - y2)) / det;
  let w1 = ((y2 - y0) * (x - x2) + (x0 - x2) * (y - y2)) / det;
  w0 = Math.min(1, Math.max(0, w0));
  w1 = Math.min(1, Math.max(0, w1));
  let w2 = 1 - w0 - w1;
  if (w2 < 0) {
    const s = w0 + w1;
    w0 /= s;
    w1 /= s;
    w2 = 0;
  }
  const t = geometry.triangles;
  return (
    w0 * vertexValues[t[3 * cellIndex]] +
    w1 * vertexValues[t[3 * cellIndex + 1]] +
    w2 * vertexValues[t[3 * cellIndex + 2]]
  );
}
