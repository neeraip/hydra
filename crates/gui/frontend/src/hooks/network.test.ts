/**
 * Tests for the binary network-snapshot decoder in network.ts.
 *
 * The buffers are hand-constructed to mirror the backend's
 * `encode_network_snapshot` layout exactly (see commands.rs and its
 * `encode_network_snapshot_layout_roundtrips` test):
 *
 *   u32 version | u32 flags (bit 0 = present) | u32 nNodes | u32 nLinks |
 *   u32 totalVerts | u32×3 reserved |
 *   f64×nNodes x | y |
 *   f64×totalVerts vertexX | f64×totalVerts vertexY (link order) |
 *   f32×nNodes elevation | baseDemand | pressure | demand |
 *              tankMinLevel | tankMaxLevel | tankInitialLevel | tankDiameter |
 *   f32×nLinks velocity | diameter | length | roughness |
 *              pumpPowerKw | pumpSpeed | valveSetting |
 *   u8×nNodes nodeKind | u8×nLinks linkKind |
 *   u8×nLinks initialStatus (0 = open, 1 = closed, 2 = cv; non-pipes 0) |
 *   u32×nLinks vertexCount (possibly unaligned) |
 *   9 string columns (u32 byteLen + newline-joined UTF-8):
 *     node id | tankVolumeCurve | headPattern |
 *     link id | fromId | toId | pumpCurve | valveType | valveCurve
 *
 * All values little-endian. NaN = absent for optional f32 columns; empty
 * string = absent for optional string columns.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri IPC seam so `loadProjectNetwork` / `fetchNetworkSnapshot`
// can be exercised with controlled payloads. Established before importing
// ./network (which pulls ./ipc).
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { normalizeNodes } from "./NetworkDataContext";
import {
  decodeNetworkSnapshot,
  fetchNetworkSnapshot,
  isStructuralNetworkChange,
  loadProjectNetwork,
} from "./network";

const mockInvoke = vi.mocked(invoke);

/** Make `isTauri()` return true for the current test. */
function stubTauriShell() {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

beforeEach(() => {
  mockInvoke.mockReset();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const VERSION = 3;
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

/** 32-byte v3 header: version, flags, counts, totalVerts, 3 reserved words. */
function header(
  w: ByteWriter,
  nNodes: number,
  nLinks: number,
  totalVerts = 0,
  flags = FLAG_PRESENT,
): ByteWriter {
  return w
    .u32(VERSION)
    .u32(flags)
    .u32(nNodes)
    .u32(nLinks)
    .u32(totalVerts)
    .u32(0)
    .u32(0)
    .u32(0);
}

/**
 * Mirrors the Rust `snapshot_test_dto` fixture: junction J1, tank T1,
 * reservoir R1; pipe P1, pump PU1, valve V1. All numeric values are exactly
 * representable in f32 so assertions can compare without rounding.
 * Pipe P1 carries two intermediate polyline vertices; PU1/V1 carry none.
 * P1's v3 initialStatus is "closed" (code 1); non-pipes carry code 0.
 */
function buildFullSnapshot(): ArrayBuffer {
  return header(new ByteWriter(), 3, 3, 2)
    .f64s([1.5, 3.0, -1.0]) // x
    .f64s([2.5, 4.0, 0.0]) // y
    .f64s([1.75, 2.0]) // vertexX (P1's run)
    .f64s([2.75, 3.5]) // vertexY
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
    .u8s([1, 0, 0]) // initialStatus: P1 closed; non-pipes always 0
    .u32(2) // vertexCount: P1 has 2 vertices
    .u32(0) // PU1
    .u32(0) // V1
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
  const w = header(new ByteWriter(), 0, 0);
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
        initialStatus: "closed",
        vertices: [
          [1.75, 2.75],
          [2.0, 3.5],
        ],
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
    // Links without vertices omit the field entirely (not `vertices: []` /
    // `undefined`) so pre-v2 consumers see the exact same object shape.
    expect("vertices" in res.links[1]).toBe(false);
    expect("vertices" in res.links[2]).toBe(false);
    // v3 initialStatus is pipe-only: pumps/valves omit the field entirely.
    expect("initialStatus" in res.links[0]).toBe(true);
    expect("initialStatus" in res.links[1]).toBe(false);
    expect("initialStatus" in res.links[2]).toBe(false);
  });

  it("decodes the full initialStatus code range on pipes", () => {
    // Three pipes carrying codes 0/1/2 → open/closed/cv.
    const w = header(new ByteWriter(), 2, 3)
      .f64s([0, 1]) // x
      .f64s([0, 1]) // y
      .f32s([0, 0]) // elevation
      .f32s([0, 0]) // baseDemand
      .f32s([NaN, NaN]) // pressure
      .f32s([NaN, NaN]) // demand
      .f32s([NaN, NaN]) // tankMinLevel
      .f32s([NaN, NaN]) // tankMaxLevel
      .f32s([NaN, NaN]) // tankInitialLevel
      .f32s([NaN, NaN]) // tankDiameter
      .f32s([0, 0, 0]) // velocity
      .f32s([100, 100, 100]) // diameter
      .f32s([10, 10, 10]) // length
      .f32s([100, 100, 100]) // roughness
      .f32s([NaN, NaN, NaN]) // pumpPowerKw
      .f32s([NaN, NaN, NaN]) // pumpSpeed
      .f32s([NaN, NaN, NaN]) // valveSetting
      .u8s([0, 0]) // node kinds
      .u8s([0, 0, 0]) // link kinds: all pipes
      .u8s([0, 1, 2]) // initialStatus: open, closed, cv
      .u32(0)
      .u32(0)
      .u32(0)
      .strCol(["N1", "N2"])
      .strCol(["", ""])
      .strCol(["", ""])
      .strCol(["PA", "PB", "PC"])
      .strCol(["N1", "N1", "N1"])
      .strCol(["N2", "N2", "N2"])
      .strCol(["", "", ""])
      .strCol(["", "", ""])
      .strCol(["", "", ""]);
    const res = decodeNetworkSnapshot(w.build());
    if (!res) throw new Error("expected decode to succeed");
    expect(res.links.map((l) => l.initialStatus)).toEqual([
      "open",
      "closed",
      "cv",
    ]);
  });

  it("throws on an unknown initialStatus code", () => {
    const full = new Uint8Array(buildFullSnapshot().slice(0));
    // initialStatus column offset: 32 header + 48 x/y + 32 verts +
    // 96 node f32 + 84 link f32 + 3 nodeKind + 3 linkKind = 298.
    full[298] = 9;
    expect(() => decodeNetworkSnapshot(full.buffer)).toThrow(
      /unknown link initialStatus code 9/,
    );
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
    const buf = header(new ByteWriter(), 0, 0, 0, 0).build();
    expect(decodeNetworkSnapshot(buf)).toBeNull();
  });

  it("throws on a buffer shorter than the 32-byte header", () => {
    expect(() => decodeNetworkSnapshot(new ArrayBuffer(0))).toThrow(
      /too short/,
    );
    expect(() => decodeNetworkSnapshot(new ArrayBuffer(31))).toThrow(
      /too short/,
    );
  });

  it("throws on unsupported versions (v1 and v2 payloads are rejected)", () => {
    for (const version of [1, 2]) {
      const buf = new ByteWriter()
        .u32(version)
        .u32(FLAG_PRESENT)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .build();
      expect(() => decodeNetworkSnapshot(buf)).toThrow(
        new RegExp(`unsupported version ${version}`),
      );
    }
  });

  it("throws when the fixed-width section is truncated", () => {
    const full = buildFullSnapshot();
    const truncated = full.slice(0, 56); // header + part of the x column
    expect(() => decodeNetworkSnapshot(truncated)).toThrow(/truncated/);
  });

  it("throws when vertexCount does not sum to totalVerts", () => {
    // Rebuild the full snapshot but claim only 1 vertex on P1 (of 2 encoded).
    const full = new Uint8Array(buildFullSnapshot().slice(0));
    // vertexCount column offset: 32 header + 48 x/y + 32 verts + 96 node f32
    // + 84 link f32 + 6 kinds + 3 initialStatus = 301.
    const view = new DataView(full.buffer);
    expect(view.getUint32(301, true)).toBe(2); // sanity: P1's count
    view.setUint32(301, 1, true);
    expect(() => decodeNetworkSnapshot(full.buffer)).toThrow(
      /vertexCount sum 1 does not match totalVerts 2/,
    );
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
    const w = header(new ByteWriter(), 3, 0)
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
    const w = header(new ByteWriter(), 1, 0)
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

describe("loadProjectNetwork", () => {
  it("returns null outside a Tauri shell (tryInvoke null)", async () => {
    // `window` is undefined in the Node test env → isTauri() false.
    await expect(loadProjectNetwork("p1", null)).resolves.toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("returns null when the command fails (tryInvoke swallows + reports)", async () => {
    stubTauriShell();
    mockInvoke.mockRejectedValueOnce("boom");
    await expect(loadProjectNetwork("p1", null)).resolves.toBeNull();
  });

  it("decodes an ArrayBuffer payload into nodes+links", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(buildFullSnapshot());
    const res = await loadProjectNetwork("p1", "s1");
    expect(res?.nodes).toHaveLength(3);
    expect(res?.links).toHaveLength(3);
  });

  it("returns null for a present-flag-clear payload (target INP missing)", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(
      header(new ByteWriter(), 0, 0, 0, 0).build(),
    );
    await expect(loadProjectNetwork("p1", "s1")).resolves.toBeNull();
  });

  it("throws (not null) on an unexpected payload type", async () => {
    // Distinguishes a frontend/backend contract break from the legitimate
    // "INP missing" null — conflating them hid decode-path regressions.
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce({ nodes: [], links: [] });
    await expect(loadProjectNetwork("p1", null)).rejects.toThrow(
      /unexpected payload type/,
    );
  });

  it("propagates decode failures for malformed buffers", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce(new ArrayBuffer(4));
    await expect(loadProjectNetwork("p1", null)).rejects.toThrow(/too short/);
  });
});

describe("fetchNetworkSnapshot", () => {
  it("returns null outside a Tauri shell", async () => {
    await expect(fetchNetworkSnapshot()).resolves.toBeNull();
  });

  it("throws on an unexpected payload type", async () => {
    stubTauriShell();
    mockInvoke.mockResolvedValueOnce("nope");
    await expect(fetchNetworkSnapshot()).rejects.toThrow(
      /unexpected payload type/,
    );
  });
});

describe("isStructuralNetworkChange", () => {
  it("is true for a null payload (create/delete/pattern/curve/control)", () => {
    expect(isStructuralNetworkChange(null)).toBe(true);
  });

  it("is true for an empty elements list", () => {
    expect(isStructuralNetworkChange({ elements: [] })).toBe(true);
  });

  it("is false for an element-scoped delta", () => {
    expect(
      isStructuralNetworkChange({
        elements: [{ node: { id: "J1" } as never }],
      }),
    ).toBe(false);
  });
});
