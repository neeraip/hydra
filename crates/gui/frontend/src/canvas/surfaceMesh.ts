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

/**
 * Per-triangle-vertex RGBA for one instant: the selected variable's
 * value through the engine's ramp, dry cells transparent. `depth` masks
 * whatever variable is on show — a dry cell has no water surface or
 * speed worth colouring either.
 */
export function surfaceFillColors(
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
    if (depth != null && !(depth[ci] > SURFACE_DRY_DEPTH_M)) continue; // alpha stays 0
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
// the cell boundaries without moving what a cell says about itself.
//
// The construction: split every cell into three sub-triangles meeting at
// its centroid. The centroid carries the cell's own value; the corners
// carry the mean of the cells that meet there. So the middle of a cell
// reads exactly what the solver computed, two cells sharing an edge
// interpolate between the same pair of corner values and therefore agree
// along it, and all the blending happens in the band near the
// boundaries.
//
// Averaging cell values onto the corners *alone* — the obvious approach,
// and the one this replaced — destroys peaks: a corner mixes up to six
// cells, so a local maximum is averaged away and appears nowhere. On the
// SWMM 2D example the deepest cell in the mesh, at 0.9983 m, painted its
// own centre at 0.5265 m, below the mesh average. In flood work the peak
// is the number that matters.
//
// For a field that genuinely lives at the vertices — the ground — this
// same construction is exact rather than a compromise: the centroid's
// value is the mean of the three corners, which is what linear
// interpolation already gives there, so subdividing changes nothing.

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
 * Used for the *corners* of the blended construction only. A cell's own
 * value is never replaced by this: see the note above.
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

/** Binary polygon data for the blended surface: a grid of sub-triangles
 * per cell, fine enough that the blend reads as smooth. */
export interface SurfaceBlendedData extends SurfacePolygonData {
  /** The parent cells' projected corners, six numbers per cell — what a
   * reading at a point needs, since the polygon buffer above holds the
   * sub-triangles rather than the cells. */
  corners: Float64Array;
  /** Segments per cell edge: the grid this was built at. */
  segments: number;
}

/**
 * How finely a cell is subdivided for blending.
 *
 * The blend is a smooth curve sampled on a grid and drawn straight
 * between the samples, so too coarse a grid shows its own facets: the
 * first version of this drew three sub-triangles meeting at the
 * centroid, and the seams from each corner to the middle of every cell
 * were plainly visible. Finer is smoother and costs triangles, so the
 * grid follows the mesh: a small mesh is drawn generously because its
 * cells are large on screen, and a large one is drawn coarsely because
 * its cells are not.
 *
 * Multiples of three only: the centroid must be a sample, or the cell's
 * own value never appears.
 *
 * Zero above the ceiling, meaning "do not blend this mesh". The cost is
 * the reason: the grid multiplies the drawn polygons by its square, and
 * at a million polygons the geometry alone runs to tens of megabytes and
 * is rebuilt whenever the colours change. A mesh that large draws cells
 * smaller than a pixel, where blending changes nothing anyone can see,
 * so the ceiling costs no picture and averts a stall.
 */
export const BLEND_CELL_CEILING = 50_000;

export function blendSegments(nCells: number): number {
  if (nCells <= 20_000) return 6; // 36 sub-triangles per cell
  if (nCells <= BLEND_CELL_CEILING) return 3; // 9 per cell
  return 0;
}

/** The barycentric grid for `n` segments: (w0, w1, w2) per sample. */
function blendGrid(n: number): Float64Array {
  const out: number[] = [];
  for (let i = 0; i <= n; i++) {
    for (let j = 0; j <= n - i; j++) {
      out.push(i / n, j / n, (n - i - j) / n);
    }
  }
  return Float64Array.from(out);
}

/** Sample index of grid point (i, j) for `n` segments. */
function gridIndex(n: number, i: number, j: number): number {
  // Rows are laid out by i, each of length (n - i + 1).
  return ((2 * n + 3 - i) * i) / 2 + j;
}

/**
 * The blended surface's geometry: each cell as an `n × n` grid of
 * sub-triangles, in cell order.
 */
export function surfaceBlendedPolygonData(
  geometry: SurfaceGeometry,
  project: (x: number, y: number) => [number, number] = (x, y) => [x, y],
  segments: number = blendSegments(geometry.nCells),
): SurfaceBlendedData {
  const { nCells, positions, triangles } = geometry;
  const n = segments;
  const grid = blendGrid(n);
  const samples = grid.length / 3;
  const perCell = n * n;
  const corners = new Float64Array(6 * nCells);
  const value = new Float64Array(6 * perCell * nCells);
  const startIndices = new Uint32Array(perCell * nCells);
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  const px = new Float64Array(samples);
  const py = new Float64Array(samples);

  for (let ci = 0; ci < nCells; ci++) {
    // The cell's own corners, projected once.
    for (let k = 0; k < 3; k++) {
      const vi = triangles[3 * ci + k];
      const [x, y] = project(positions[3 * vi], positions[3 * vi + 1]);
      corners[6 * ci + 2 * k] = x;
      corners[6 * ci + 2 * k + 1] = y;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
    const x0 = corners[6 * ci];
    const y0 = corners[6 * ci + 1];
    const x1 = corners[6 * ci + 2];
    const y1 = corners[6 * ci + 3];
    const x2 = corners[6 * ci + 4];
    const y2 = corners[6 * ci + 5];
    // The grid's positions are linear in the corners, so they are taken
    // in projected space rather than reprojected per sample.
    for (let s = 0; s < samples; s++) {
      const w0 = grid[3 * s];
      const w1 = grid[3 * s + 1];
      const w2 = grid[3 * s + 2];
      px[s] = w0 * x0 + w1 * x1 + w2 * x2;
      py[s] = w0 * y0 + w1 * y1 + w2 * y2;
    }
    let tri = 0;
    const emit = (a: number, b: number, c: number) => {
      const sub = perCell * ci + tri;
      startIndices[sub] = 3 * sub;
      const at = 6 * sub;
      value[at] = px[a];
      value[at + 1] = py[a];
      value[at + 2] = px[b];
      value[at + 3] = py[b];
      value[at + 4] = px[c];
      value[at + 5] = py[c];
      tri += 1;
    };
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n - i; j++) {
        emit(
          gridIndex(n, i, j),
          gridIndex(n, i + 1, j),
          gridIndex(n, i, j + 1),
        );
        if (j < n - i - 1) {
          emit(
            gridIndex(n, i + 1, j),
            gridIndex(n, i + 1, j + 1),
            gridIndex(n, i, j + 1),
          );
        }
      }
    }
  }
  return {
    length: perCell * nCells,
    startIndices,
    attributes: { getPolygon: { value, size: 2 } },
    bounds: nCells > 0 ? [minX, minY, maxX, maxY] : null,
    corners,
    segments: n,
  };
}

/**
 * The blended field at barycentric coordinates inside a cell.
 *
 * Linear between the corner values, lifted at the middle so the cell's
 * own value lands exactly at its centroid: `Σ w·A + (cell − mean A)·B`,
 * where `B = 27·w0·w1·w2` is zero on every edge and one at the centroid.
 *
 * Being zero on the edges is what keeps neighbouring cells agreeing
 * along them; being smooth everywhere inside is what keeps the picture
 * free of the creases a piecewise-linear blend showed.
 */
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
  const bubble = 27 * w0 * w1 * w2;
  return linear + (cell - (a0 + a1 + a2) / 3) * bubble;
}

/**
 * Per-sub-vertex RGBA for the blended surface, sampled on the same grid
 * `surfaceBlendedPolygonData` built.
 *
 * `depthAtVertices` and `cellDepth` mask as the flat path's depth does,
 * through the same blend, so the waterline crosses the inside of a cell
 * rather than snapping to its boundary. Both are `null` for a field
 * that is not water.
 */
export function surfaceBlendedColors(
  geometry: SurfaceGeometry,
  vertexValues: Float32Array,
  cellValues: Float32Array | null,
  depthAtVertices: Float32Array | null,
  cellDepth: Float32Array | null,
  variable: GenericVariable,
  segments: number = blendSegments(geometry.nCells),
): Uint8Array {
  const { nCells, triangles } = geometry;
  const n = segments;
  const grid = blendGrid(n);
  const samples = grid.length / 3;
  const perCell = n * n;
  const out = new Uint8Array(12 * perCell * nCells);
  const rgba = new Uint8Array(4 * samples);

  for (let ci = 0; ci < nCells; ci++) {
    const v0 = triangles[3 * ci];
    const v1 = triangles[3 * ci + 1];
    const v2 = triangles[3 * ci + 2];
    const a0 = vertexValues[v0];
    const a1 = vertexValues[v1];
    const a2 = vertexValues[v2];
    const cell = cellValues ? cellValues[ci] : null;
    const d0 = depthAtVertices ? depthAtVertices[v0] : null;
    const cellD = cellDepth ? cellDepth[ci] : null;
    for (let s = 0; s < samples; s++) {
      const w0 = grid[3 * s];
      const w1 = grid[3 * s + 1];
      const w2 = grid[3 * s + 2];
      if (depthAtVertices && d0 != null) {
        const depth = blendedValue(
          w0,
          w1,
          w2,
          d0,
          depthAtVertices[v1],
          depthAtVertices[v2],
          cellD,
        );
        if (!(depth > SURFACE_DRY_DEPTH_M)) {
          rgba[4 * s + 3] = 0;
          continue;
        }
      }
      const [r, g, b, a] = genericRgba(
        blendedValue(w0, w1, w2, a0, a1, a2, cell),
        variable,
        SURFACE_ALPHA,
        "surface",
      );
      rgba[4 * s] = r;
      rgba[4 * s + 1] = g;
      rgba[4 * s + 2] = b;
      rgba[4 * s + 3] = a;
    }
    let tri = 0;
    const emit = (a: number, b: number, c: number) => {
      const at = 12 * (perCell * ci + tri);
      for (let k = 0; k < 4; k++) {
        out[at + k] = rgba[4 * a + k];
        out[at + 4 + k] = rgba[4 * b + k];
        out[at + 8 + k] = rgba[4 * c + k];
      }
      tri += 1;
    };
    for (let i = 0; i < n; i++) {
      for (let j = 0; j < n - i; j++) {
        emit(
          gridIndex(n, i, j),
          gridIndex(n, i + 1, j),
          gridIndex(n, i, j + 1),
        );
        if (j < n - i - 1) {
          emit(
            gridIndex(n, i + 1, j),
            gridIndex(n, i + 1, j + 1),
            gridIndex(n, i, j + 1),
          );
        }
      }
    }
  }
  return out;
}

/**
 * Which cell a picked polygon belongs to.
 *
 * The blended surface draws many sub-triangles per cell, so what deck
 * hands back from a pick indexes the *polygons*, not the cells. Reading
 * one as the other names a cell that may not exist and reads its
 * geometry from beyond the end of the array: the chip said "Cell 173"
 * of an eight-cell mesh, and its value came out as nothing at all.
 */
export function pickedCell(polygonIndex: number, subsPerCell: number): number {
  if (polygonIndex < 0 || subsPerCell < 1) return -1;
  return Math.floor(polygonIndex / subsPerCell);
}

/**
 * The value the blended picture shows at a point inside a cell.
 *
 * With barycentric coordinates `w` in the cell and `m` the smallest of
 * them, the piecewise-linear field over the three sub-triangles is
 * `Σ(w_i − w_m)·A_i + 3·w_m·cell`, where `A` are the corner values. It
 * returns the cell's own value at the centre, a corner's average at a
 * corner, and the same number from either side of a shared edge.
 *
 * `cellValues` is `null` for a field held at the vertices (the ground),
 * where the plain linear interpolation is already exact.
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
