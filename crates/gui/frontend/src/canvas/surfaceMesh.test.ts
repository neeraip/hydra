/**
 * @vitest-environment node
 */

import { describe, expect, it } from "vitest";

import type { GenericVariable } from "../hooks/results";
import type { SurfaceGeometry } from "../hooks/surface";
import { seqRgb } from "./MapCanvas/colorUtils";
import {
  groundAtVertices,
  MESH_EDGE_MIN_PIXELS,
  meshEdgesLegible,
  meshEdgesShown,
  pixelsPerUnit,
  SURFACE_ALPHA,
  SURFACE_DRY_DEPTH_M,
  SURFACE_FOOTPRINT_ALPHA,
  surfaceCellColors,
  surfaceCornerColors,
  surfaceEdgeData,
  surfaceFootprintColors,
  surfaceGroundValues,
  surfacePolygonData,
  valueAtPoint,
} from "./surfaceMesh";

const geometry: SurfaceGeometry = {
  nVertices: 4,
  nCells: 2,
  positions: new Float64Array([0, 0, 10, 1, 0, 10.2, 1, 1, 10.4, 0, 1, 10.6]),
  triangles: new Uint32Array([0, 1, 2, 0, 2, 3]),
};

const depthVar: GenericVariable = {
  id: "depth",
  label: "Depth",
  ramp: { type: "sequential" },
  min: 0,
  max: 1,
};

describe("surfacePolygonData", () => {
  it("duplicates vertices per cell so colours can stay flat", () => {
    const d = surfacePolygonData(geometry);
    expect(d.length).toBe(2);
    expect(Array.from(d.startIndices)).toEqual([0, 3]);
    // Cell 0 = vertices 0,1,2; cell 1 = vertices 0,2,3. Vertex 0 and 2
    // appear in both cells — duplicated, not shared.
    expect(Array.from(d.attributes.getPolygon.value)).toEqual([
      0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0, 1,
    ]);
    expect(d.attributes.getPolygon.size).toBe(2);
  });

  it("runs every position through the projection", () => {
    const d = surfacePolygonData(geometry, (x, y) => [x + 100, y - 50]);
    expect(d.attributes.getPolygon.value[0]).toBe(100);
    expect(d.attributes.getPolygon.value[1]).toBe(-50);
  });
});

describe("surfaceFootprintColors", () => {
  // What a mesh looks like before anyone runs it: present and plainly
  // silent about depth, rather than absent.
  it("tints every cell alike, faintly, in the surface family", () => {
    const colors = surfaceFootprintColors(2);
    expect(colors.length).toBe(24);
    for (let i = 0; i < 6; i++) {
      expect(colors[4 * i + 3]).toBe(SURFACE_FOOTPRINT_ALPHA);
    }
    expect(Array.from(colors.slice(0, 3))).toEqual(seqRgb(0.5, "surface"));
    // Every vertex of every cell carries the same colour: a footprint
    // makes no claim that varies across the mesh.
    expect(Array.from(colors.slice(0, 4))).toEqual(
      Array.from(colors.slice(20, 24)),
    );
  });

  it("is fainter than a cell carrying a value", () => {
    expect(SURFACE_FOOTPRINT_ALPHA).toBeLessThan(SURFACE_ALPHA);
  });
});

describe("surfaceCellColors", () => {
  it("colours wet cells through the ramp and hides dry ones", () => {
    const colors = surfaceCellColors(
      new Float32Array([0.8, 0.2]),
      // Cell 1 sits below the display drying depth: dry. (Not exactly at
      // it — 0.001 rounds *up* through f32, which is the storage type.)
      new Float32Array([0.8, SURFACE_DRY_DEPTH_M / 2]),
      depthVar,
    );
    expect(colors.length).toBe(24);
    // Wet cell: three vertices, same colour, the wet alpha.
    expect(colors[3]).toBe(SURFACE_ALPHA);
    expect(Array.from(colors.slice(0, 4))).toEqual(
      Array.from(colors.slice(4, 8)),
    );
    expect(Array.from(colors.slice(0, 4))).toEqual(
      Array.from(colors.slice(8, 12)),
    );
    // Dry cell: every vertex transparent, whatever its value. Its hue
    // is kept rather than zeroed — see the note below on the blend.
    for (let k = 0; k < 3; k++) expect(colors[12 + 4 * k + 3]).toBe(0);
  });

  it("masks by depth even when another variable is on show", () => {
    // Speed on show: the dry cell has a (zero) speed, but renders
    // transparent because it holds no water.
    const speedVar: GenericVariable = { ...depthVar, id: "speed", max: 2 };
    const colors = surfaceCellColors(
      new Float32Array([1.5, 0]),
      new Float32Array([0.5, 0]),
      speedVar,
    );
    expect(colors[3]).toBe(SURFACE_ALPHA);
    expect(colors[15]).toBe(0);
  });

  // The legend samples the ramp by class key; the map must paint from
  // the same family or the swatch describes colours that are not on
  // screen. Pinned against seqRgb("surface") itself, not a copy.
  it("colours through the surface family the legend samples", () => {
    const colors = surfaceCellColors(
      new Float32Array([1]),
      new Float32Array([1]),
      depthVar,
    );
    expect(Array.from(colors.slice(0, 3))).toEqual(seqRgb(1, "surface"));
    expect(Array.from(colors.slice(0, 3))).not.toEqual(seqRgb(1, "region"));
    expect(Array.from(colors.slice(0, 3))).not.toEqual(seqRgb(1, "point"));
  });

  it("distinct values take distinct ramp colours", () => {
    const colors = surfaceCellColors(
      new Float32Array([0.05, 0.95]),
      new Float32Array([1, 1]),
      depthVar,
    );
    expect(Array.from(colors.slice(0, 3))).not.toEqual(
      Array.from(colors.slice(12, 15)),
    );
  });
});

describe("surfaceEdgeData", () => {
  // The unit square split on its diagonal: 5 distinct edges, not 6 —
  // the diagonal belongs to both cells and is one edge, drawn once.
  it("emits each shared edge once", () => {
    const e = surfaceEdgeData(geometry);
    expect(e.length).toBe(5);
    const drawn = new Set<string>();
    for (let i = 0; i < e.length; i++) {
      const s = e.attributes.getSourcePosition.value;
      const t = e.attributes.getTargetPosition.value;
      const a = `${s[2 * i]},${s[2 * i + 1]}`;
      const b = `${t[2 * i]},${t[2 * i + 1]}`;
      // Undirected: an edge and its reverse are the same edge.
      drawn.add([a, b].sort().join("|"));
    }
    expect(drawn.size).toBe(5);
  });

  it("runs every endpoint through the projection", () => {
    const e = surfaceEdgeData(geometry, (x, y) => [x + 100, y - 50]);
    const s = e.attributes.getSourcePosition.value;
    for (let i = 0; i < e.length; i++) {
      expect(s[2 * i]).toBeGreaterThanOrEqual(100);
      expect(s[2 * i + 1]).toBeLessThanOrEqual(-49);
    }
  });

  it("measures the mesh by its median edge, in projected units", () => {
    // Unit-square cells: four sides of 1 and a diagonal of √2, so the
    // median is 1. The projection scales it with everything else.
    expect(surfaceEdgeData(geometry).medianLength).toBe(1);
    expect(
      surfaceEdgeData(geometry, (x, y) => [10 * x, 10 * y]).medianLength,
    ).toBe(10);
  });
});

describe("the edge legibility decision", () => {
  // The two views measure in different units; a decision in pixels has
  // to come through their own conversions or it means nothing.
  it("converts each view's units to pixels", () => {
    // Orthographic: one unit is 2^zoom pixels (grid.ts' own convention).
    expect(pixelsPerUnit("orthographic", 0)).toBe(1);
    expect(pixelsPerUnit("orthographic", 4)).toBe(16);
    // Web Mercator: 512 px spans 360° at zoom 0.
    expect(pixelsPerUnit("map", 0)).toBeCloseTo(512 / 360);
    expect(pixelsPerUnit("map", 10)).toBeCloseTo((512 * 1024) / 360);
  });

  it("draws edges only once a cell spans enough pixels to read", () => {
    expect(meshEdgesLegible(1, MESH_EDGE_MIN_PIXELS)).toBe(true);
    expect(meshEdgesLegible(1, MESH_EDGE_MIN_PIXELS - 1)).toBe(false);
  });

  /**
   * The case the gate exists for: a 120k-cell mesh viewed whole. Its
   * cells are sub-pixel, and drawing 180k edges there paints the domain
   * a solid wash — worse than the plain footprint it covers.
   */
  it("refuses a fine mesh viewed whole, and allows it zoomed in", () => {
    const cellMetres = 2; // a 2 m cell on a large mesh
    const whole = pixelsPerUnit("orthographic", -4); // zoomed far out
    const close = pixelsPerUnit("orthographic", 4);
    expect(meshEdgesLegible(cellMetres, whole)).toBe(false);
    expect(meshEdgesLegible(cellMetres, close)).toBe(true);
  });
});

/**
 * The wireframe and the blend answer different questions — where the
 * cells are, and what the field does between their centres — and the
 * second is spoiled by drawing the first over it. Kept as one decision
 * because the canvas asks it twice: once when it builds the layers, and
 * again on every pan to see whether the answer has changed. Those two
 * disagreeing would rebuild the whole layer list on every gesture of a
 * blended mesh.
 */
describe("whether the mesh's edges are drawn", () => {
  const legible = MESH_EDGE_MIN_PIXELS; // one unit spans exactly the minimum

  it("draws them flat, once the cells are big enough to tell apart", () => {
    expect(meshEdgesShown(1, legible, false)).toBe(true);
    expect(meshEdgesShown(1, legible / 2, false)).toBe(false);
  });

  it("draws none while the surface is blended, at any zoom", () => {
    expect(meshEdgesShown(1, legible, true)).toBe(false);
    expect(meshEdgesShown(1, legible * 100, true)).toBe(false);
  });
});

describe("surfaceGroundValues", () => {
  // The solver reads a cell's bed as the mean of its vertices (spec
  // 15.3's flat closure); the canvas must read it the same way, or the
  // terrain on screen is not the terrain the run was over.
  it("reads each cell's bed as the mean of its vertices", () => {
    const g: SurfaceGeometry = {
      nVertices: 4,
      nCells: 2,
      positions: new Float64Array([0, 0, 10, 1, 0, 11, 1, 1, 12, 0, 1, 13]),
      triangles: new Uint32Array([0, 1, 2, 0, 2, 3]),
    };
    const z = surfaceGroundValues(g);
    expect(z[0]).toBeCloseTo(11, 6); // (10 + 11 + 12) / 3
    expect(z[1]).toBeCloseTo(35 / 3, 6); // (10 + 12 + 13) / 3
  });
});

describe("the water mask", () => {
  const wet = new Float32Array([1, 0]);
  const v: GenericVariable = {
    id: "ground",
    label: "Ground",
    ramp: { type: "sequential" },
    min: 0,
    max: 2,
  };

  it("hides dry cells for a water variable", () => {
    const colors = surfaceCellColors(new Float32Array([1, 1]), wet, v);
    expect(colors[3]).toBe(SURFACE_ALPHA);
    expect(colors[15]).toBe(0);
  });

  /**
   * A dry sample keeps its colour and loses only its alpha. The blend
   * mixes the colour channels as well as the alpha, so a dry sample left
   * black drags the hue toward black across the waterline rather than
   * fading it. Invisible while the fill was flat — nothing draws an
   * alpha-zero pixel — and plain to see once a neighbouring pixel is a
   * mix of the two.
   */
  it("keeps a dry cell's hue, dropping only its alpha", () => {
    const colors = surfaceCellColors(new Float32Array([1, 1]), wet, v);
    const dryRgb = Array.from(colors.slice(12, 15));
    const wetRgb = Array.from(colors.slice(0, 3));
    expect(dryRgb).toEqual(wetRgb);
    expect(dryRgb).not.toEqual([0, 0, 0]);
  });

  // The ground under a dry cell is still ground: masking it would be a
  // claim about the terrain rather than about the flood.
  it("paints every cell when there is no water to mask by", () => {
    const colors = surfaceCellColors(new Float32Array([1, 1]), null, v);
    expect(colors[3]).toBe(SURFACE_ALPHA);
    expect(colors[15]).toBe(SURFACE_ALPHA);
  });
});

describe("valueAtPoint", () => {
  const g: SurfaceGeometry = {
    nVertices: 3,
    nCells: 1,
    positions: new Float64Array([0, 0, 10, 12, 0, 20, 0, 12, 30]),
    triangles: new Uint32Array([0, 1, 2]),
  };
  const corners = surfacePolygonData(g).attributes.getPolygon.value;
  const z = groundAtVertices(g);

  /**
   * Only a field the mesh holds at its vertices is ever read this way,
   * and for one of those the reading is the field: a plane through three
   * known elevations, sampled. A run's values are held per cell and are
   * drawn and read flat — there is nothing between cell centres for the
   * pointer to report.
   */
  it("reads each corner as its own value, and interpolates between", () => {
    expect(valueAtPoint(g, corners, z, 0, 0, 0)).toBeCloseTo(10, 6);
    expect(valueAtPoint(g, corners, z, 0, 12, 0)).toBeCloseTo(20, 6);
    expect(valueAtPoint(g, corners, z, 0, 6, 0)).toBeCloseTo(15, 6);
    expect(valueAtPoint(g, corners, z, 0, 4, 4)).toBeCloseTo(20, 6);
  });

  it("never leaves the range of the corners it reads between", () => {
    for (let i = 0; i <= 60; i++) {
      for (let j = 0; i + j <= 60; j++) {
        const w1 = i / 60;
        const w2 = j / 60;
        const v = valueAtPoint(g, corners, z, 0, w1 * 12, w2 * 12) as number;
        expect(v).toBeGreaterThanOrEqual(10 - 1e-9);
        expect(v).toBeLessThanOrEqual(30 + 1e-9);
      }
    }
  });

  it("answers just outside the triangle rather than refusing", () => {
    const v = valueAtPoint(g, corners, z, 0, 6.2, -0.2);
    expect(v).not.toBeNull();
    expect(v as number).toBeGreaterThanOrEqual(10);
    expect(v as number).toBeLessThanOrEqual(30);
  });

  it("refuses a cell that is not there", () => {
    expect(valueAtPoint(g, corners, z, 1, 0, 0)).toBeNull();
    expect(valueAtPoint(g, corners, z, -1, 0, 0)).toBeNull();
  });
});

describe("shading", () => {
  // The unit square split on its diagonal.
  const g: SurfaceGeometry = {
    nVertices: 4,
    nCells: 2,
    positions: new Float64Array([0, 0, 10, 1, 0, 11, 1, 1, 14, 0, 1, 13]),
    triangles: new Uint32Array([0, 1, 2, 0, 2, 3]),
  };
  const v: GenericVariable = {
    id: "depth",
    label: "Depth",
    ramp: { type: "sequential" },
    min: 0,
    max: 10,
  };

  it("takes the ground from the vertices exactly", () => {
    expect(Array.from(groundAtVertices(g))).toEqual([10, 11, 14, 13]);
  });

  /**
   * A result is the solver's own number for a whole cell, so all three
   * of the cell's drawn vertices carry it and the triangle reads flat.
   * The version this replaced averaged neighbouring cells onto the
   * corners and mixed the cell's colour back in, which painted values
   * between cell centres that nothing had computed.
   */
  it("paints a cell's own colour on all three of its vertices", () => {
    const colors = surfaceCellColors(Float32Array.from([2, 8]), null, v);
    expect(colors.length).toBe(12 * g.nCells);
    const rgb = (i: number) => Array.from(colors.slice(4 * i, 4 * i + 3));
    expect(rgb(0)).toEqual(rgb(1));
    expect(rgb(1)).toEqual(rgb(2));
    expect(rgb(0)).toEqual(seqRgb(2 / 10, "surface"));
    expect(rgb(3)).toEqual(seqRgb(8 / 10, "surface"));
  });

  /**
   * The smooth drawing, and the reason it is honest: each drawn vertex
   * takes the colour of the value the mesh holds *at that vertex*, so a
   * corner is painted the same in every cell that meets there and the
   * rasteriser's interpolation between them is the plane the mesh
   * describes. A corner painted differently in two cells would seam
   * along their shared edge.
   */
  // Ranged over the mesh's own bed, as `mesh_info_of` ranges it: with a
  // variable whose range excludes the elevations every corner clamps to
  // the same colour and the test proves nothing.
  const ground: GenericVariable = { ...v, id: "ground", min: 10, max: 14 };

  it("paints a corner from its own value, alike in every cell", () => {
    const colors = surfaceCornerColors(g, groundAtVertices(g), ground);
    const rgb = (i: number) => Array.from(colors.slice(4 * i, 4 * i + 3));
    // Vertex 0 is the first vertex of both cells; vertex 2 is the third
    // of cell 0 and the second of cell 1.
    expect(rgb(0)).toEqual(rgb(3));
    expect(rgb(2)).toEqual(rgb(4));
    // And each is its own elevation, not a mean of anything.
    expect(rgb(0)).toEqual(seqRgb(0, "surface"));
    expect(rgb(1)).toEqual(seqRgb(0.25, "surface"));
  });

  it("gives a cell's three vertices different colours once smooth", () => {
    const colors = surfaceCornerColors(g, groundAtVertices(g), ground);
    const rgb = (i: number) => Array.from(colors.slice(4 * i, 4 * i + 3));
    expect(rgb(0)).not.toEqual(rgb(1));
  });

  it("hides dry cells but never the ground", () => {
    const cells = Float32Array.from([1, 1]);
    const dry = Float32Array.from([1, 0]);
    const masked = surfaceCellColors(cells, dry, v);
    expect(masked[3]).toBe(SURFACE_ALPHA);
    expect(masked[15]).toBe(0);
    // The ground passes no depth at all and is painted everywhere.
    expect(surfaceCellColors(cells, null, v)[15]).toBe(SURFACE_ALPHA);
  });
});
