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
} from "../../../canvas/surfaceMesh";
import type { GenericVariable } from "../../../hooks/results";
import type {
  SurfaceGeometry,
  SurfaceMeta,
  SurfacePeriod,
} from "../../../hooks/surface";
import { shownSurface } from "./useSurfaceResults";

const variable = (id: string): GenericVariable => ({
  id,
  label: id,
  ramp: { type: "sequential" },
  min: 0,
  max: 2,
});

const GROUND: GenericVariable[] = [{ ...variable("ground"), min: 10, max: 11 }];

/** `nCells` triangles, each vertex a step higher than the last. */
const geometry = (nCells: number): SurfaceGeometry => {
  const nVertices = 3 * nCells;
  const positions = new Float64Array(3 * nVertices);
  for (let v = 0; v < nVertices; v++) {
    positions[3 * v] = v;
    positions[3 * v + 1] = v;
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
