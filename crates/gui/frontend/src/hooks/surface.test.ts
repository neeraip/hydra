/**
 * @vitest-environment node
 *
 * The decoders' side of the surface payload contracts. Byte offsets
 * mirror `uds_surface.rs`, whose tests pin the same layouts against the
 * sidecar reader — a drift on either side fails one of the two suites.
 */

import { describe, expect, it, vi } from "vitest";

import {
  decodeSurfaceGeometry,
  decodeSurfacePeriod,
  SURFACE_GEOMETRY_VERSION,
  SURFACE_PERIOD_VERSION,
  surfaceColumn,
} from "./surface";

function geometryPayload(version = SURFACE_GEOMETRY_VERSION): ArrayBuffer {
  // Two cells over four vertices: the unit square split on a diagonal.
  const verts = [0, 0, 10, 1, 0, 10.2, 1, 1, 10.4, 0, 1, 10.6];
  const tris = [0, 1, 2, 0, 2, 3];
  const buf = new ArrayBuffer(12 + 8 * verts.length + 4 * tris.length);
  const dv = new DataView(buf);
  dv.setUint32(0, version, true);
  dv.setUint32(4, 4, true);
  dv.setUint32(8, 2, true);
  verts.forEach((v, i) => {
    dv.setFloat64(12 + 8 * i, v, true);
  });
  tris.forEach((t, i) => {
    dv.setUint32(12 + 8 * verts.length + 4 * i, t, true);
  });
  return buf;
}

function periodPayload(version = SURFACE_PERIOD_VERSION): ArrayBuffer {
  const depth = [0.5, 0];
  const elevation = [10.5, 10.4];
  const speed = [0.25, 0];
  const buf = new ArrayBuffer(16 + 4 * 6);
  const dv = new DataView(buf);
  dv.setUint32(0, version, true);
  dv.setUint32(4, 2, true);
  dv.setFloat64(8, 300, true);
  [...depth, ...elevation, ...speed].forEach((v, i) => {
    dv.setFloat32(16 + 4 * i, v, true);
  });
  return buf;
}

describe("decodeSurfaceGeometry", () => {
  it("round-trips the mesh", () => {
    const g = decodeSurfaceGeometry(geometryPayload());
    expect(g.nVertices).toBe(4);
    expect(g.nCells).toBe(2);
    expect(Array.from(g.positions.slice(0, 3))).toEqual([0, 0, 10]);
    expect(Array.from(g.triangles)).toEqual([0, 1, 2, 0, 2, 3]);
  });

  it("refuses a version it does not serve", () => {
    expect(() => decodeSurfaceGeometry(geometryPayload(2))).toThrow(
      /version 2/,
    );
  });

  it("refuses a payload whose counts do not tile its bytes", () => {
    expect(() => decodeSurfaceGeometry(geometryPayload().slice(0, 40))).toThrow(
      /expected/,
    );
  });
});

describe("the transport guard", () => {
  // The defect this pins: a backend command returning bare Vec<u8>
  // arrives as a JSON number array, and the fetch used to fold that
  // into a silent null — the surface simply never appeared. The guard
  // names the command and the type instead.
  it("refuses a payload that is not an ArrayBuffer, loudly", async () => {
    const { getSurfaceGeometry } = await import("./surface");
    const ipc = await import("./ipc");
    const spy = vi
      .spyOn(ipc, "tryInvoke")
      .mockResolvedValue([1, 0, 0, 0] as unknown as ArrayBuffer);
    await expect(getSurfaceGeometry("p")).rejects.toThrow(
      /unexpected payload type object/,
    );
    spy.mockRestore();
  });
});

describe("decodeSurfacePeriod", () => {
  it("serves the three columns and the instant", () => {
    const p = decodeSurfacePeriod(periodPayload());
    expect(p.t).toBe(300);
    expect(Array.from(p.depth)).toEqual([0.5, 0]);
    expect(Array.from(p.elevation)).toEqual([10.5, 10.399999618530273]);
    expect(Array.from(p.speed)).toEqual([0.25, 0]);
  });

  it("refuses a version it does not serve", () => {
    expect(() => decodeSurfacePeriod(periodPayload(9))).toThrow(/version 9/);
  });

  it("selects a column by catalog id, and only catalog ids", () => {
    const p = decodeSurfacePeriod(periodPayload());
    expect(surfaceColumn(p, "depth")).toBe(p.depth);
    expect(surfaceColumn(p, "elevation")).toBe(p.elevation);
    expect(surfaceColumn(p, "speed")).toBe(p.speed);
    expect(surfaceColumn(p, "volume")).toBeNull();
  });
});
