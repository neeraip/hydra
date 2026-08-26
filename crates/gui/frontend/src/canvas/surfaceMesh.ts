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

// ── Blended shading ──────────────────────────────────────────────────────────
//
// A cell is the solver's unit of state, so the plain fill paints one
// colour per cell and claims nothing between centres. Blending softens
// the cell boundaries without moving what a cell says about itself: each
// cell keeps its own colour at its centroid, corners take the mean of
// the cells that meet there, and the two are mixed by a weight that is
// one at the centroid and zero on every edge.
//
// The mixing happens per pixel, in `BlendedSurfaceLayer`. What is built
// here is the two colour arrays it mixes and the barycentric basis it
// reads them by — all per vertex of the same one-triangle-per-cell
// geometry the plain fill uses.
//
// Averaging cell values onto the corners *alone* — an earlier version of
// this — destroys peaks: a corner mixes up to six cells, so a local
// maximum is averaged away and appears nowhere. On the SWMM 2D example
// the deepest cell in the mesh, at 0.9983 m, painted its own centre at
// 0.5265 m. In flood work the peak is the number that matters, which is
// why the cell's own colour is kept at its middle.

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
 * A per-cell field carried to the vertices, each vertex taking the mean
 * of the cells that meet there.
 *
 * Used for the *corners* of the blend only. A cell's own value is never
 * replaced by this: see the note above.
 */
export function cellValuesAtVertices(
  geometry: SurfaceGeometry,
  cellValues: Float32Array,
): Float32Array {
  const { nVertices, nCells, triangles } = geometry;
  const sum = new Float64Array(nVertices);
  const count = new Uint32Array(nVertices);
  for (let ci = 0; ci < nCells; ci++) {
    const v = cellValues[ci];
    for (let k = 0; k < 3; k++) {
      const vi = triangles[3 * ci + k];
      sum[vi] += v;
      count[vi] += 1;
    }
  }
  const out = new Float32Array(nVertices);
  for (let vi = 0; vi < nVertices; vi++) {
    out[vi] = count[vi] > 0 ? sum[vi] / count[vi] : 0;
  }
  return out;
}

/**
 * The barycentric basis, per vertex of the drawn geometry: the three
 * vertices of every cell get (1,0,0), (0,1,0) and (0,0,1).
 *
 * Static for a mesh — the blend weight is a function of position within
 * a cell, not of anything a run produced — so this is built once and
 * never rebuilt while stepping the timeline.
 */
export function surfaceBarycentric(nCells: number): Float32Array {
  const out = new Float32Array(9 * nCells);
  for (let ci = 0; ci < nCells; ci++) {
    for (let k = 0; k < 3; k++) out[9 * ci + 3 * k + k] = 1;
  }
  return out;
}

/**
 * Per-vertex RGBA from values held at the *corners* — the colours the
 * blend interpolates between, and the whole picture on an edge.
 *
 * `depthAtVertices` masks as the flat path's depth does, but per corner,
 * which puts the waterline inside the cells it crosses rather than on
 * their boundaries. `null` for a field that is not water.
 */
export function surfaceCornerColors(
  geometry: SurfaceGeometry,
  vertexValues: Float32Array,
  depthAtVertices: Float32Array | null,
  variable: GenericVariable,
): Uint8Array {
  const { nVertices, nCells, triangles } = geometry;
  // One ramp evaluation per vertex, then scattered to the cells that
  // meet there: a vertex is shared by about six of them.
  const rgba = new Uint8Array(4 * nVertices);
  for (let vi = 0; vi < nVertices; vi++) {
    const dry =
      depthAtVertices != null && !(depthAtVertices[vi] > SURFACE_DRY_DEPTH_M);
    const [r, g, b, a] = genericRgba(
      vertexValues[vi],
      variable,
      SURFACE_ALPHA,
      "surface",
    );
    // A dry sample keeps its colour and loses only its alpha. The blend
    // mixes the colour channels too, so leaving a dry sample black drags
    // the hue toward black across the waterline instead of fading it —
    // invisible while the fill was flat, because nothing ever drew an
    // alpha-zero pixel, and plain to see once a neighbouring pixel is a
    // mix of the two.
    rgba[4 * vi] = r;
    rgba[4 * vi + 1] = g;
    rgba[4 * vi + 2] = b;
    rgba[4 * vi + 3] = dry ? 0 : a;
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
 * Per-vertex RGBA of each cell's own colour, repeated for its three
 * vertices — what the blend mixes toward at a cell's middle.
 *
 * Handed the same values as the corners, this is the plain fill: every
 * vertex of a cell carries the cell's colour and the mix has nothing to
 * do.
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
    // Colour kept, alpha dropped — see `surfaceCornerColors`.
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
 * The blended field at barycentric coordinates inside a cell.
 *
 * Linear between the corner values, lifted at the middle so the cell's
 * own value lands exactly at its centroid: `Σ w·A + (cell − mean A)·B`,
 * where `B = 27·w0·w1·w2` is zero on every edge and one at the centroid.
 *
 * This is the same weight the shader mixes colours by, so what the
 * pointer reads and what the picture shows agree at the centre and along
 * every edge. Between those they differ by whether the ramp is applied
 * before or after the mixing, which is a question the solver does not
 * answer either way: it holds one value per cell and says nothing about
 * the inside of one.
 */
/**
 * The blend's weight at the centroid of a cell, and the scale of the
 * bubble `w0·w1·w2` that carries it.
 *
 * Named because the weight is computed twice: here, for what the pointer
 * reads, and in `BlendedSurfaceLayer`'s fragment shader, for what the
 * picture shows. Those two must be the same function or the chip and the
 * map describe different surfaces, and a constant written out in GLSL
 * and again in TypeScript is one nobody would think to change in both.
 */
export const BLEND_BUBBLE_SCALE = 27;

export function blendedValue(
  w0: number,
  w1: number,
  w2: number,
  a0: number,
  a1: number,
  a2: number,
  cell: number | null,
): number {
  const linear = w0 * a0 + w1 * a1 + w2 * a2;
  if (cell == null) return linear;
  const bubble = BLEND_BUBBLE_SCALE * w0 * w1 * w2;
  return linear + (cell - (a0 + a1 + a2) / 3) * bubble;
}

/**
 * The value the blended picture shows at a point inside a cell: the
 * point's barycentric coordinates in the cell, through `blendedValue`.
 *
 * `cellValues` is `null` for a field held at the vertices (the ground),
 * where plain linear interpolation is already exact.
 *
 * `x, y` are in the projected space the corners were built in. Weights
 * are clamped and renormalised so a point on an edge, or a hair outside
 * it from picking tolerance, still answers.
 */
export function valueAtPoint(
  geometry: SurfaceGeometry,
  corners: Float64Array,
  vertexValues: Float32Array,
  cellValues: Float32Array | null,
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
  return blendedValue(
    w0,
    w1,
    w2,
    vertexValues[t[3 * cellIndex]],
    vertexValues[t[3 * cellIndex + 1]],
    vertexValues[t[3 * cellIndex + 2]],
    cellValues ? cellValues[cellIndex] : null,
  );
}
