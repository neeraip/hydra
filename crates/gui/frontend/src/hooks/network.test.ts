/**
 * Tests for the binary network-snapshot decoder in network.ts.
 *
 * The buffers are hand-constructed to mirror the backend's
 * `encode_network_snapshot` layout exactly (see commands.rs and its
 * `encode_network_snapshot_layout_roundtrips` test):
 *
 *   u32 version | u32 flags (bit 0 = present) | u32 nNodes | u32 nLinks |
 *   f64×nNodes x | y |
 *   f32×nNodes elevation | baseDemand | pressure | demand |
 *              tankMinLevel | tankMaxLevel | tankInitialLevel | tankDiameter |
 *   f32×nLinks velocity | diameter | length | roughness |
 *              pumpPowerKw | pumpSpeed | valveSetting |
 *   u8×nNodes nodeKind | u8×nLinks linkKind |
 *   9 string columns (u32 byteLen + newline-joined UTF-8):
 *     node id | tankVolumeCurve | headPattern |
 *     link id | fromId | toId | pumpCurve | valveType | valveCurve
 *
 * All values little-endian. NaN = absent for optional f32 columns; empty
 * string = absent for optional string columns.
 */
import { describe, expect, it } from "vitest";
import { normalizeNodes } from "./NetworkDataContext";
import { decodeNetworkSnapshot } from "./network";

const VERSION = 1;
const FLAG_PRESENT = 1;

/** Incremental little-endian byte writer for hand-building test payloads. */
class ByteWriter {
  private bytes: number[] = [];
  private scratch = new DataView(new ArrayBuffer(8));

  u8(v: number): this {
    this.bytes.push(v & 0xff);
    return this;
  }
  u32(v: number): this {
    this.scratch.setUint32(0, v, true);
    for (let i = 0; i < 4; i += 1) this.bytes.push(this.scratch.getUint8(i));
    return this;
  }
  f32(v: number): this {
    this.scratch.setFloat32(0, v, true);
    for (let i = 0; i < 4; i += 1) this.bytes.push(this.scratch.getUint8(i));
    return this;
  }
  f64(v: number): this {
    this.scratch.setFloat64(0, v, true);
    for (let i = 0; i < 8; i += 1) this.bytes.push(this.scratch.getUint8(i));
    return this;
  }
  f32s(vs: number[]): this {
    for (const v of vs) this.f32(v);
    return this;
  }
  f64s(vs: number[]): this {
    for (const v of vs) this.f64(v);
    return this;
  }
  u8s(vs: number[]): this {
    for (const v of vs) this.u8(v);
    return this;
  }
  /** One string column: u32 byte length + newline-joined UTF-8 values. */
  strCol(values: string[]): this {
    const joined = values.join("\n");
    const encoded = new TextEncoder().encode(joined);
    this.u32(encoded.byteLength);
    for (const b of encoded) this.bytes.push(b);
    return this;
  }
  build(): ArrayBuffer {
    return new Uint8Array(this.bytes).buffer;
  }
}

/**
 * Mirrors the Rust `snapshot_test_dto` fixture: junction J1, tank T1,
 * reservoir R1; pipe P1, pump PU1, valve V1. All numeric values are exactly
 * representable in f32 so assertions can compare without rounding.
 */
function buildFullSnapshot(): ArrayBuffer {
  return new ByteWriter()
    .u32(VERSION)
    .u32(FLAG_PRESENT)
    .u32(3) // nNodes
    .u32(3) // nLinks
    .f64s([1.5, 3.0, -1.0]) // x
    .f64s([2.5, 4.0, 0.0]) // y
    .f32s([10.5, 50.0, 100.0]) // elevation
    .f32s([5.25, 0.0, 0.0]) // baseDemand
    .f32s([NaN, NaN, NaN]) // pressure (all absent)
    .f32s([0.0, NaN, NaN]) // demand (explicit 0 on J1)
    .f32s([NaN, 1.5, NaN]) // tankMinLevel
    .f32s([NaN, 6.5, NaN]) // tankMaxLevel
    .f32s([NaN, 2.25, NaN]) // tankInitialLevel
    .f32s([NaN, 20.0, NaN]) // tankDiameter
    .f32s([0.5, 0.0, 0.0]) // velocity
    .f32s([300.0, 0.0, 0.0]) // diameter
    .f32s([1200.0, 0.0, 0.0]) // length
    .f32s([100.0, 0.0, 0.0]) // roughness
    .f32s([NaN, 15.5, NaN]) // pumpPowerKw
    .f32s([NaN, 1.0, NaN]) // pumpSpeed
    .f32s([NaN, NaN, 35.5]) // valveSetting
    .u8s([0, 1, 2]) // node kinds: junction, tank, reservoir
    .u8s([0, 1, 2]) // link kinds: pipe, pump, valve
    .strCol(["J1", "T1", "R1"])
    .strCol(["", "VC1", ""]) // tankVolumeCurve
    .strCol(["", "", "PAT7"]) // headPattern
    .strCol(["P1", "PU1", "V1"])
    .strCol(["J1", "R1", "T1"]) // fromId
    .strCol(["T1", "J1", "J1"]) // toId
    .strCol(["", "C1", ""]) // pumpCurve
    .strCol(["", "", "PRV"]) // valveType
    .strCol(["", "", ""]) // valveCurve
    .build();
}

/** Present-but-empty snapshot: header + nine zero-length string columns. */
function buildEmptySnapshot(): ArrayBuffer {
  const w = new ByteWriter().u32(VERSION).u32(FLAG_PRESENT).u32(0).u32(0);
  for (let i = 0; i < 9; i += 1) w.strCol([]);
  return w.build();
}

describe("decodeNetworkSnapshot", () => {
  it("decodes nodes into the exact JSON-path object shape", () => {
    const res = decodeNetworkSnapshot(buildFullSnapshot());
    expect(res).not.toBeNull();
    if (!res) throw new Error("unreachable");
    expect(res.nodes).toEqual([
      {
        id: "J1",
        type: "junction",
        x: 1.5,
        y: 2.5,
        elevation: 10.5,
        baseDemand: 5.25,
        pressure: null,
        demand: 0, // explicit zero survives — distinct from absent (null)
        tankMinLevel: null,
        tankMaxLevel: null,
        tankInitialLevel: null,
        tankDiameter: null,
        tankVolumeCurve: null,
        headPattern: null,
      },
      {
        id: "T1",
        type: "tank",
        x: 3,
        y: 4,
        elevation: 50,
        baseDemand: 0,
        pressure: null,
        demand: null,
        tankMinLevel: 1.5,
        tankMaxLevel: 6.5,
        tankInitialLevel: 2.25,
        tankDiameter: 20,
        tankVolumeCurve: "VC1",
        headPattern: null,
      },
      {
        id: "R1",
        type: "reservoir",
        x: -1,
        y: 0,
        elevation: 100,
        baseDemand: 0,
        pressure: null,
        demand: null,
        tankMinLevel: null,
        tankMaxLevel: null,
        tankInitialLevel: null,
        tankDiameter: null,
        tankVolumeCurve: null,
        headPattern: "PAT7",
      },
    ]);
  });

  it("decodes links into the exact JSON-path object shape", () => {
    const res = decodeNetworkSnapshot(buildFullSnapshot());
    if (!res) throw new Error("expected decode to succeed");
    expect(res.links).toEqual([
      {
        id: "P1",
        type: "pipe",
        fromId: "J1",
        toId: "T1",
        velocity: 0.5,
        diameter: 300,
        length: 1200,
        roughness: 100,
        pumpCurve: null,
        pumpPowerKw: null,
        pumpSpeed: null,
        valveType: null,
        valveSetting: null,
        valveCurve: null,
      },
      {
        id: "PU1",
        type: "pump",
        fromId: "R1",
        toId: "J1",
        velocity: 0,
        diameter: 0,
        length: 0,
        roughness: 0,
        pumpCurve: "C1",
        pumpPowerKw: 15.5,
        pumpSpeed: 1,
        valveType: null,
        valveSetting: null,
        valveCurve: null,
      },
      {
        id: "V1",
        type: "valve",
        fromId: "T1",
        toId: "J1",
        velocity: 0,
        diameter: 0,
        length: 0,
        roughness: 0,
        pumpCurve: null,
        pumpPowerKw: null,
        pumpSpeed: null,
        valveType: "PRV",
        valveSetting: 35.5,
        valveCurve: null,
      },
    ]);
  });

  it("composes with normalizeNodes as a no-op (nulls already explicit)", () => {
    const res = decodeNetworkSnapshot(buildFullSnapshot());
    if (!res) throw new Error("expected decode to succeed");
    const before = res.nodes.map((n) => ({ ...n }));
    const out = normalizeNodes(res.nodes);
    expect(out).toBe(res.nodes);
    expect(out).toEqual(before);
  });

  it("decodes a present-but-empty snapshot to empty arrays", () => {
    const res = decodeNetworkSnapshot(buildEmptySnapshot());
    expect(res).toEqual({ nodes: [], links: [] });
  });

  it("returns null when the present flag is clear (no network loaded)", () => {
    const buf = new ByteWriter().u32(VERSION).u32(0).u32(0).u32(0).build();
    expect(decodeNetworkSnapshot(buf)).toBeNull();
  });

  it("throws on a buffer shorter than the 16-byte header", () => {
    expect(() => decodeNetworkSnapshot(new ArrayBuffer(0))).toThrow(
      /too short/,
    );
    expect(() => decodeNetworkSnapshot(new ArrayBuffer(15))).toThrow(
      /too short/,
    );
  });

  it("throws on an unsupported version", () => {
    const buf = new ByteWriter().u32(2).u32(FLAG_PRESENT).u32(0).u32(0).build();
    expect(() => decodeNetworkSnapshot(buf)).toThrow(/unsupported version 2/);
  });

  it("throws when the fixed-width section is truncated", () => {
    const full = buildFullSnapshot();
    const truncated = full.slice(0, 40); // header + part of the x column
    expect(() => decodeNetworkSnapshot(truncated)).toThrow(/truncated/);
  });

  it("throws when a string column is truncated", () => {
    const full = buildFullSnapshot();
    const truncated = full.slice(0, full.byteLength - 1);
    expect(() => decodeNetworkSnapshot(truncated)).toThrow(
      /truncated .* column/,
    );
  });

  it("throws when a string column has the wrong value count", () => {
    // Rebuild with a node-id column holding 2 values instead of 3.
    const w = new ByteWriter()
      .u32(VERSION)
      .u32(FLAG_PRESENT)
      .u32(3)
      .u32(0)
      .f64s([0, 0, 0])
      .f64s([0, 0, 0])
      .f32s([0, 0, 0])
      .f32s([0, 0, 0])
      .f32s([NaN, NaN, NaN])
      .f32s([NaN, NaN, NaN])
      .f32s([NaN, NaN, NaN])
      .f32s([NaN, NaN, NaN])
      .f32s([NaN, NaN, NaN])
      .f32s([NaN, NaN, NaN])
      .u8s([0, 0, 0])
      .strCol(["J1", "J2"]); // wrong count
    expect(() => decodeNetworkSnapshot(w.build())).toThrow(
      /node id column has 2 values, expected 3/,
    );
  });

  it("throws on an unknown kind code", () => {
    const w = new ByteWriter()
      .u32(VERSION)
      .u32(FLAG_PRESENT)
      .u32(1)
      .u32(0)
      .f64(0)
      .f64(0)
      .f32s([0, 0, NaN, NaN, NaN, NaN, NaN, NaN])
      .u8(7) // invalid node kind
      .strCol(["J1"])
      .strCol([""])
      .strCol([""]);
    for (let i = 0; i < 6; i += 1) w.strCol([]);
    expect(() => decodeNetworkSnapshot(w.build())).toThrow(
      /unknown node kind code 7/,
    );
  });
});
