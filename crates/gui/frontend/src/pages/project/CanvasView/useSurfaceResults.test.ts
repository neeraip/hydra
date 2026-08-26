/**
 * @vitest-environment node
 *
 * What the mesh gets painted with.
 *
 * The rule under test is the one that keeps a run's numbers off a mesh
 * they did not come from: the model owns the geometry, the sidecar owns
 * the values, and the values are used only where the two describe the
 * same mesh. Everything else falls back to the footprint, which is also
 * what a model nobody has run yet shows.
 */

import { describe, expect, it } from "vitest";

import { SURFACE_FOOTPRINT_ALPHA } from "../../../canvas/surfaceMesh";
import type {
  SurfaceGeometry,
  SurfaceMeta,
  SurfacePeriod,
} from "../../../hooks/surface";
import { shownSurface } from "./useSurfaceResults";

const geometry = (nCells: number): SurfaceGeometry => ({
  nVertices: nCells + 2,
  nCells,
  positions: new Float64Array(3 * (nCells + 2)),
  triangles: new Uint32Array(3 * nCells),
});

const variable = (id: string) => ({
  id,
  label: id,
  ramp: { type: "sequential" } as const,
  min: 0,
  max: 2,
});

const meta = (nCells: number): SurfaceMeta => ({
  nVertices: nCells + 2,
  nCells,
  periods: 4,
  reportStepS: 300,
  firstReportTS: 300,
  variables: [variable("depth"), variable("speed")],
});

const period = (nCells: number): SurfacePeriod => ({
  t: 300,
  depth: Float32Array.from(Array.from({ length: nCells }, () => 0.5)),
  elevation: Float32Array.from(Array.from({ length: nCells }, () => 10)),
  speed: Float32Array.from(Array.from({ length: nCells }, () => 0.25)),
});

/** Whether every cell got the flat footprint tint. */
function isFootprint(colors: Uint8Array): boolean {
  for (let i = 0; i < colors.length; i += 4) {
    if (colors[i + 3] !== SURFACE_FOOTPRINT_ALPHA) return false;
  }
  return colors.length > 0;
}

describe("shownSurface", () => {
  it("paints a run's values onto the mesh they came from", () => {
    const shown = shownSurface(geometry(4), meta(4), period(4), "depth");
    expect(shown.variable?.id).toBe("depth");
    expect(shown.values).not.toBeNull();
    expect(isFootprint(shown.colors)).toBe(false);
  });

  /**
   * The defect this exists to prevent: a run of a *different* mesh has
   * values for cells that are not these cells, and painting cell i with
   * cell i would be a confident wrong answer rather than a missing one.
   */
  it("refuses values from a run of a different mesh", () => {
    const shown = shownSurface(geometry(4), meta(9), period(9), "depth");
    expect(shown.variable).toBeNull();
    expect(shown.values).toBeNull();
    expect(isFootprint(shown.colors)).toBe(true);
    // One triple of vertices per cell of *this* mesh, not the run's.
    expect(shown.colors.length).toBe(12 * 4);
  });

  it("draws a footprint for a mesh nobody has run", () => {
    const shown = shownSurface(geometry(4), null, null, "depth");
    expect(shown.variable).toBeNull();
    expect(isFootprint(shown.colors)).toBe(true);
  });

  it("draws a footprint when the run has meta but no instant loaded", () => {
    const shown = shownSurface(geometry(4), meta(4), null, "depth");
    expect(isFootprint(shown.colors)).toBe(true);
  });

  it("falls back to the catalog's first variable for an id it does not carry", () => {
    const stale = shownSurface(geometry(4), meta(4), period(4), "elevation");
    // The catalog here publishes depth and speed; "elevation" is a
    // preference from another run, so the first variable serves.
    expect(stale.variable?.id).toBe("depth");
    expect(stale.values).not.toBeNull();
  });

  it("reads the selected variable's own column", () => {
    const shown = shownSurface(geometry(4), meta(4), period(4), "speed");
    expect(shown.variable?.id).toBe("speed");
    expect(Array.from(shown.values ?? [])).toEqual([0.25, 0.25, 0.25, 0.25]);
  });
});
