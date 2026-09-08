/**
 * @vitest-environment node
 *
 * The decoders' side of the surface payload contracts. Byte offsets
 * mirror `uds_surface.rs`, whose tests pin the same layouts against the
 * sidecar reader — a drift on either side fails one of the two suites.
 */

import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import type { GenericVariable } from "./results";
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

function periodPayload(
  version = SURFACE_PERIOD_VERSION,
  nColumns = 3,
): ArrayBuffer {
  const values = [
    0.5,
    0,
    10.5,
    10.4,
    0.25,
    0,
    ...Array(2 * (nColumns - 3)).fill(0),
  ];
  const buf = new ArrayBuffer(16 + 4 * 2 * nColumns);
  const dv = new DataView(buf);
  dv.setUint32(0, version, true);
  dv.setUint32(4, 2, true);
  dv.setFloat64(8, 300, true);
  values.forEach((v, i) => {
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

describe("getMeshGeometry", () => {
  // The shared layout says "no mesh" with zero counts, which is a
  // sound payload rather than a missing one. The caller is asking about
  // presence, so the empty answer must read as absent, not as a mesh of
  // no cells that something downstream then tries to draw.
  it("reads the empty-counts payload as no mesh at all", async () => {
    const { getMeshGeometry } = await import("./surface");
    const ipc = await import("./ipc");
    const empty = new ArrayBuffer(12);
    new DataView(empty).setUint32(0, SURFACE_GEOMETRY_VERSION, true);
    const spy = vi.spyOn(ipc, "tryInvoke").mockResolvedValue(empty);
    await expect(getMeshGeometry("p")).resolves.toBeNull();
    spy.mockRestore();
  });

  it("decodes a real mesh", async () => {
    const { getMeshGeometry } = await import("./surface");
    const ipc = await import("./ipc");
    const spy = vi.spyOn(ipc, "tryInvoke").mockResolvedValue(geometryPayload());
    const g = await getMeshGeometry("p");
    expect(g?.nCells).toBe(2);
    spy.mockRestore();
  });
});

/** A catalog of the given ids, which is all the addressing needs. */
const catalog = (...ids: string[]): GenericVariable[] =>
  ids.map((id) => ({
    id,
    label: id,
    ramp: { type: "sequential" },
    min: 0,
    max: 1,
  }));

const FIXED = catalog("depth", "elevation", "speed");

describe("decodeSurfacePeriod", () => {
  it("serves the instant and one column per variable", () => {
    const p = decodeSurfacePeriod(periodPayload());
    expect(p.t).toBe(300);
    expect(p.columns.length).toBe(3);
    expect(Array.from(p.columns[0])).toEqual([0.5, 0]);
    expect(Array.from(p.columns[1])).toEqual([10.5, 10.399999618530273]);
    expect(Array.from(p.columns[2])).toEqual([0.25, 0]);
  });

  it("reads as many columns as the payload carries", () => {
    // A model declaring a pollutant publishes a fourth series, and the
    // count is the model's answer rather than this decoder's.
    const p = decodeSurfacePeriod(periodPayload(SURFACE_PERIOD_VERSION, 4));
    expect(p.columns.length).toBe(4);
    expect(Array.from(p.columns[3])).toEqual([0, 0]);
  });

  it("refuses a version it does not serve", () => {
    expect(() => decodeSurfacePeriod(periodPayload(9))).toThrow(/version 9/);
  });

  it("refuses a payload that is not whole columns", () => {
    const whole = periodPayload();
    expect(() =>
      decodeSurfacePeriod(whole.slice(0, whole.byteLength - 4)),
    ).toThrow(/whole columns/);
  });

  // The encoder is Rust and this decoder is TypeScript, and neither
  // language can check the other's idea of the layout; only a file both
  // read fails when they diverge. Encoded by `commands/uds_surface.rs`
  // from the same record its own fixture test uses; regenerate with
  // `UPDATE_SNAPSHOT_FIXTURE=1 cargo test -p hydra-gui fixture`.
  const fixture = () => {
    const bytes = readFileSync(
      new URL("../../../tests/fixtures/surface-period.bin", import.meta.url),
    );
    return bytes.buffer.slice(
      bytes.byteOffset,
      bytes.byteOffset + bytes.byteLength,
    ) as ArrayBuffer;
  };

  it("reads the encoder's own bytes back at the values it put in", () => {
    const p = decodeSurfacePeriod(fixture());
    expect(p.t).toBe(300);
    // Three fixed columns and the one pollutant series the record holds.
    expect(p.columns.length).toBe(4);
    expect(Array.from(p.columns[0])).toEqual([0.5, 0]);
    expect(Array.from(p.columns[1])).toEqual([10.5, 10]);
    expect(Array.from(p.columns[2])).toEqual([0.5, 0]);
    expect(Array.from(p.columns[3])).toEqual([2, 0]);

    // Addressed through a catalog of the shape the backend publishes.
    const cat = catalog("depth", "elevation", "speed", "pollutant:TSS");
    expect(Array.from(surfaceColumn(p, "pollutant:TSS", cat) ?? [])).toEqual([
      2, 0,
    ]);
  });

  it("selects a column by its place in the catalog", () => {
    const p = decodeSurfacePeriod(periodPayload());
    expect(surfaceColumn(p, "depth", FIXED)).toBe(p.columns[0]);
    expect(surfaceColumn(p, "elevation", FIXED)).toBe(p.columns[1]);
    expect(surfaceColumn(p, "speed", FIXED)).toBe(p.columns[2]);
    expect(surfaceColumn(p, "volume", FIXED)).toBeNull();
  });

  it("addresses a pollutant series by catalog position, not by name", () => {
    const p = decodeSurfacePeriod(periodPayload(SURFACE_PERIOD_VERSION, 4));
    const withTss = catalog("depth", "elevation", "speed", "pollutant:TSS");
    expect(surfaceColumn(p, "pollutant:TSS", withTss)).toBe(p.columns[3]);

    // An instant written before the model declared the pollutant has no
    // column for it, and that reads as no value rather than as the last
    // column of something else.
    const older = decodeSurfacePeriod(periodPayload());
    expect(surfaceColumn(older, "pollutant:TSS", withTss)).toBeNull();
  });
});
