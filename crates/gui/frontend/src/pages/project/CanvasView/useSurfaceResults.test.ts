/**
 * @vitest-environment node
 *
 * What the mesh gets painted with.
 *
 * Two rules under test. A run's numbers are used only on the mesh they
 * came from — the model owns the geometry, the sidecar owns the values,
 * and where the two describe different meshes the values are refused
 * rather than misapplied. And whatever cannot be shown from a run falls
 * back to the ground, which the model always carries: a mesh nobody has
 * run still has terrain worth looking at.
 */

import { describe, expect, it } from "vitest";

import {
  SURFACE_DRY_DEPTH_M,
  SURFACE_FOOTPRINT_ALPHA,
  surfaceBlendedPolygonData,
  valueAtPoint,
} from "../../../canvas/surfaceMesh";
import type { GenericVariable } from "../../../hooks/results";
import { selectedVariable } from "../../../hooks/results";
import type {
  SurfaceGeometry,
  SurfaceMeta,
  SurfacePeriod,
} from "../../../hooks/surface";
import { shownSurface, surfaceVariableList } from "./useSurfaceResults";

const variable = (id: string): GenericVariable => ({
  id,
  label: id,
  ramp: { type: "sequential" },
  min: 0,
  max: 2,
});

const GROUND: GenericVariable[] = [{ ...variable("ground"), min: 10, max: 11 }];

/**
 * `nCells` separate triangles, each a unit right triangle of its own and
 * each vertex a step higher than the last. Kept non-degenerate: a
 * zero-area cell has no barycentric coordinates, and a reading inside
 * one is refused rather than invented.
 */
const geometry = (nCells: number): SurfaceGeometry => {
  const nVertices = 3 * nCells;
  const positions = new Float64Array(3 * nVertices);
  for (let v = 0; v < nVertices; v++) {
    const corner = v % 3;
    positions[3 * v] = 10 * Math.floor(v / 3) + (corner === 1 ? 1 : 0);
    positions[3 * v + 1] = corner === 2 ? 1 : 0;
    positions[3 * v + 2] = 10 + v / 10;
  }
  return {
    nVertices,
    nCells,
    positions,
    triangles: Uint32Array.from(
      Array.from({ length: 3 * nCells }, (_, i) => i),
    ),
  };
};

const meta = (nCells: number): SurfaceMeta => ({
  nVertices: 3 * nCells,
  nCells,
  periods: 4,
  reportStepS: 300,
  firstReportTS: 300,
  variables: [variable("depth"), variable("speed")],
});

/** One instant in which the first cell is wet and the rest are dry. */
const period = (nCells: number): SurfacePeriod => ({
  t: 300,
  depth: Float32Array.from(
    Array.from({ length: nCells }, (_, i) => (i === 0 ? 0.5 : 0)),
  ),
  elevation: Float32Array.from(Array.from({ length: nCells }, () => 10)),
  speed: Float32Array.from(Array.from({ length: nCells }, () => 0.25)),
});

/** Whether every cell was painted (nothing masked away). */
function allCellsPainted(colors: Uint8Array): boolean {
  for (let i = 3; i < colors.length; i += 4) {
    if (colors[i] === 0) return false;
  }
  return colors.length > 0;
}

describe("shownSurface", () => {
  it("paints a run's values onto the mesh they came from", () => {
    const shown = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "depth",
    );
    expect(shown.variable?.id).toBe("depth");
    expect(shown.values).not.toBeNull();
    // The dry cells are masked away; only the wet one is painted.
    expect(allCellsPainted(shown.colors)).toBe(false);
  });

  /**
   * The defect this exists to prevent: a run of a *different* mesh has
   * values for cells that are not these cells, and painting cell i with
   * cell i would be a confident wrong answer rather than a missing one.
   */
  it("refuses values from a run of a different mesh", () => {
    const shown = shownSurface(
      geometry(4),
      GROUND,
      meta(9),
      period(9),
      "depth",
    );
    expect(shown.variable?.id).toBe("ground");
    // One value per cell of *this* mesh, not the run's.
    expect(shown.values?.length).toBe(4);
    expect(shown.colors.length).toBe(12 * 4);
  });

  it("shows the ground of a mesh nobody has run", () => {
    const shown = shownSurface(geometry(4), GROUND, null, null, "");
    expect(shown.variable?.id).toBe("ground");
    // Each cell reads the mean bed of its three vertices.
    expect(shown.values?.[0]).toBeCloseTo(10.1, 6);
    expect(shown.values?.[1]).toBeCloseTo(10.4, 6);
  });

  it("shows the ground when asked for it, run or no run", () => {
    const shown = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "ground",
    );
    expect(shown.variable?.id).toBe("ground");
  });

  /**
   * The ground is not water: masking it by depth would hide the terrain
   * of every dry cell, which is a claim about the ground rather than
   * about the flood.
   */
  it("paints the ground of dry cells too", () => {
    const wet = shownSurface(geometry(4), GROUND, meta(4), period(4), "depth");
    const ground = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "ground",
    );
    expect(allCellsPainted(wet.colors)).toBe(false);
    expect(allCellsPainted(ground.colors)).toBe(true);
    // And the dry threshold is what did the masking above.
    expect(period(4).depth[1]).toBeLessThan(SURFACE_DRY_DEPTH_M);
  });

  it("falls back to a flat footprint only when there is nothing to show", () => {
    const shown = shownSurface(geometry(2), [], null, null, "");
    expect(shown.variable).toBeNull();
    expect(shown.values).toBeNull();
    expect(shown.colors[3]).toBe(SURFACE_FOOTPRINT_ALPHA);
  });

  it("falls back to the catalog's first variable for an id it does not carry", () => {
    const stale = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "elevation",
    );
    // The catalog here publishes depth and speed, and the properties
    // publish ground; "elevation" is a preference from another run.
    expect(stale.variable?.id).toBe("depth");
  });

  it("reads the selected variable's own column", () => {
    const shown = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "speed",
    );
    expect(shown.variable?.id).toBe("speed");
    expect(Array.from(shown.values ?? [])).toEqual([0.25, 0.25, 0.25, 0.25]);
  });
});

/**
 * The legend names the surface variable; the canvas paints it. They
 * resolve it separately, so they must resolve it the *same* — and once
 * did not: the legend read a merged list whose first entry was the
 * ground while the canvas read the run's list whose first was depth, so
 * the legend said Ground over a map painted by depth.
 */
describe("the legend and the canvas agree on what is shown", () => {
  const cases: Array<[string, SurfaceMeta | null, SurfacePeriod | null]> = [
    ["a run that corresponds", meta(4), period(4)],
    ["a run of a different mesh", meta(9), period(9)],
    ["no run at all", null, null],
  ];

  for (const [name, m, p] of cases) {
    for (const id of ["", "depth", "ground", "speed", "elevation"]) {
      it(`${name}, asked for "${id}"`, () => {
        const g = geometry(4);
        // What the legend would name…
        const named = selectedVariable(surfaceVariableList(g, GROUND, m), id);
        // …and what the canvas paints.
        const painted = shownSurface(g, GROUND, m, p, id);
        expect(painted.variable?.id).toBe(named?.id);
      });
    }
  }

  it("offers a run's variables only where they can be shown", () => {
    const g = geometry(4);
    const ids = (m: SurfaceMeta | null) =>
      surfaceVariableList(g, GROUND, m).map((v) => v.id);
    // A corresponding run leads with its results; the ground follows.
    expect(ids(meta(4))).toEqual(["depth", "speed", "ground"]);
    // A run of a different mesh offers none of them: picking one could
    // only ever show something other than what it names.
    expect(ids(meta(9))).toEqual(["ground"]);
    expect(ids(null)).toEqual(["ground"]);
  });
});

/**
 * Smoothing says something different about a property than about a
 * result, and the difference is worth keeping straight: the mesh holds
 * the ground at its vertices, so smoothing shows it as stored, while a
 * result lives per cell and smoothing it is an interpolation the solver
 * never made.
 */
describe("the smooth surface", () => {
  it("takes the ground from the vertices, not from cell means", () => {
    const g = geometry(4);
    const shown = shownSurface(g, GROUND, null, null, "ground", true);
    expect(shown.vertexValues?.length).toBe(g.nVertices);
    // Vertex 0's own z, untouched by any averaging.
    expect(shown.vertexValues?.[0]).toBeCloseTo(g.positions[2], 6);
    // Held at the vertices, so a reading needs no centre term.
    expect(shown.centreValues).toBeNull();
  });

  /**
   * The peak case, end to end: a run's values reach the picture with
   * the cell's own number kept at its centre. Corner averaging alone
   * put the SWMM 2D example's deepest cell below the mesh average.
   */
  it("keeps a cell's own value at its centre", () => {
    const g = geometry(4);
    const shown = shownSurface(g, GROUND, meta(4), period(4), "depth", true);
    expect(shown.centreValues).toBe(shown.values);
    const deepest = shown.values?.[0];
    expect(deepest).toBeCloseTo(0.5, 6);
    // Read at the centroid of that cell, through the same function the
    // pointer uses.
    const corners = surfaceBlendedPolygonData(g).corners;
    const cx = (corners[0] + corners[2] + corners[4]) / 3;
    const cy = (corners[1] + corners[3] + corners[5]) / 3;
    const at = valueAtPoint(
      g,
      corners,
      shown.vertexValues as Float32Array,
      shown.centreValues,
      0,
      cx,
      cy,
    );
    expect(at).toBeCloseTo(deepest as number, 6);
  });

  it("carries a run's values to the corners as a stated mean", () => {
    const shown = shownSurface(
      geometry(4),
      GROUND,
      meta(4),
      period(4),
      "speed",
      true,
    );
    expect(shown.variable?.id).toBe("speed");
    expect(shown.vertexValues?.length).toBe(geometry(4).nVertices);
    // Every cell reads 0.25, so every vertex does too.
    expect(Array.from(shown.vertexValues ?? []).every((v) => v === 0.25)).toBe(
      true,
    );
  });

  it("holds no vertex values while the surface is drawn flat", () => {
    const flat = shownSurface(geometry(4), GROUND, null, null, "ground", false);
    expect(flat.vertexValues).toBeNull();
    // The per-cell field is there either way: it is what the flat
    // picture paints and what a flat hover reads.
    expect(flat.values?.length).toBe(4);
  });

  it("still names what the legend names", () => {
    for (const id of ["", "ground", "depth"]) {
      const g = geometry(4);
      const flat = shownSurface(g, GROUND, meta(4), period(4), id, false);
      const smooth = shownSurface(g, GROUND, meta(4), period(4), id, true);
      expect(smooth.variable?.id).toBe(flat.variable?.id);
    }
  });
});
