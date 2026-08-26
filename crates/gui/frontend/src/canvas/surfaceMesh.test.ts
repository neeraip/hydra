/**
 * @vitest-environment node
 */

import { describe, expect, it } from "vitest";

import type { GenericVariable } from "../hooks/results";
import type { SurfaceGeometry } from "../hooks/surface";
import {
  SURFACE_ALPHA,
  SURFACE_DRY_DEPTH_M,
  surfaceFillColors,
  surfacePolygonData,
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

describe("surfaceFillColors", () => {
  it("colours wet cells through the ramp and hides dry ones", () => {
    const colors = surfaceFillColors(
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
    // Dry cell: fully transparent, whatever its value.
    expect(Array.from(colors.slice(12, 24))).toEqual(new Array(12).fill(0));
  });

  it("masks by depth even when another variable is on show", () => {
    // Speed on show: the dry cell has a (zero) speed, but renders
    // transparent because it holds no water.
    const speedVar: GenericVariable = { ...depthVar, id: "speed", max: 2 };
    const colors = surfaceFillColors(
      new Float32Array([1.5, 0]),
      new Float32Array([0.5, 0]),
      speedVar,
    );
    expect(colors[3]).toBe(SURFACE_ALPHA);
    expect(colors[15]).toBe(0);
  });

  it("distinct values take distinct ramp colours", () => {
    const colors = surfaceFillColors(
      new Float32Array([0.05, 0.95]),
      new Float32Array([1, 1]),
      depthVar,
    );
    expect(Array.from(colors.slice(0, 3))).not.toEqual(
      Array.from(colors.slice(12, 15)),
    );
  });
});
