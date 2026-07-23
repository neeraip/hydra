/**
 * Tests for the binary period-results decoder in results.ts.
 *
 * The buffers are hand-constructed to mirror the backend's
 * `encode_period_results` layout exactly (see commands.rs and its
 * `encode_period_results_layout_roundtrips` test):
 *
 *   u32 nNodes | u32 nLinks | u32 flags |
 *   f32×nNodes nodeDemand | nodeHead | nodePressure |
 *   f32×nLinks linkFlow | linkVelocity | linkHeadloss | linkStatus |
 *   [f32×nNodes nodeQuality | f32×nLinks linkQuality]   (flags bit 0)
 *
 * All values little-endian.
 */
import { describe, expect, it } from "vitest";
import { decodePeriodResults } from "./results";

const HEADER_BYTES = 12;
const FLAG_QUALITY = 1;

/** Build an encoded buffer from the header fields + f32 arrays in order. */
function buildBuffer(
  nNodes: number,
  nLinks: number,
  flags: number,
  arrays: number[][],
): ArrayBuffer {
  const totalFloats = arrays.reduce((sum, a) => sum + a.length, 0);
  const buf = new ArrayBuffer(HEADER_BYTES + 4 * totalFloats);
  const view = new DataView(buf);
  view.setUint32(0, nNodes, true);
  view.setUint32(4, nLinks, true);
  view.setUint32(8, flags, true);
  let offset = HEADER_BYTES;
  for (const arr of arrays) {
    for (const v of arr) {
      view.setFloat32(offset, v, true);
      offset += 4;
    }
  }
  return buf;
}

// Mirrors the Rust round-trip test fixture: 2 nodes, 3 links.
const nodeDemand = [1, 2];
const nodeHead = [3, 4];
const nodePressure = [5, 6];
const linkFlow = [9, 10, 11];
const linkVelocity = [12, 13, 14];
const linkHeadloss = [15, 16, 17];
const linkStatus = [1, 0, 1];
const nodeQuality = [7, 8];
const linkQuality = [18, 19, 20];

describe("decodePeriodResults", () => {
  it("decodes a no-quality payload into the seven arrays", () => {
    const buf = buildBuffer(2, 3, 0, [
      nodeDemand,
      nodeHead,
      nodePressure,
      linkFlow,
      linkVelocity,
      linkHeadloss,
      linkStatus,
    ]);
    expect(buf.byteLength).toBe(12 + 4 * (3 * 2 + 4 * 3));

    const res = decodePeriodResults(buf);
    expect(res).not.toBeNull();
    if (!res) throw new Error("unreachable");
    expect(Array.from(res.nodeDemand)).toEqual(nodeDemand);
    expect(Array.from(res.nodeHead)).toEqual(nodeHead);
    expect(Array.from(res.nodePressure)).toEqual(nodePressure);
    expect(Array.from(res.linkFlow)).toEqual(linkFlow);
    expect(Array.from(res.linkVelocity)).toEqual(linkVelocity);
    expect(Array.from(res.linkHeadloss)).toEqual(linkHeadloss);
    expect(Array.from(res.linkStatus)).toEqual(linkStatus);
    expect(res.nodeQuality).toBeUndefined();
    expect(res.linkQuality).toBeUndefined();
  });

  it("decodes quality arrays when flags bit 0 is set", () => {
    const buf = buildBuffer(2, 3, FLAG_QUALITY, [
      nodeDemand,
      nodeHead,
      nodePressure,
      linkFlow,
      linkVelocity,
      linkHeadloss,
      linkStatus,
      nodeQuality,
      linkQuality,
    ]);
    expect(buf.byteLength).toBe(12 + 4 * (3 * 2 + 4 * 3) + 4 * (2 + 3));

    const res = decodePeriodResults(buf);
    expect(res).not.toBeNull();
    if (!res) throw new Error("unreachable");
    expect(Array.from(res.nodeQuality ?? [])).toEqual(nodeQuality);
    expect(Array.from(res.linkQuality ?? [])).toEqual(linkQuality);
    // Quality arrays must come after the seven base arrays, not interleaved.
    expect(Array.from(res.linkStatus)).toEqual(linkStatus);
  });

  it("returns zero-copy views over the input buffer (perf contract)", () => {
    const buf = buildBuffer(2, 3, 0, [
      nodeDemand,
      nodeHead,
      nodePressure,
      linkFlow,
      linkVelocity,
      linkHeadloss,
      linkStatus,
    ]);
    const res = decodePeriodResults(buf);
    if (!res) throw new Error("expected decode to succeed");
    for (const arr of [
      res.nodeDemand,
      res.nodeHead,
      res.nodePressure,
      res.linkFlow,
      res.linkVelocity,
      res.linkHeadloss,
      res.linkStatus,
    ]) {
      expect(arr.buffer).toBe(buf); // view, not copy
    }
    // Views are laid out contiguously after the header.
    expect(res.nodeDemand.byteOffset).toBe(12);
    expect(res.linkStatus.byteOffset).toBe(12 + 4 * (3 * 2 + 3 * 3));
  });

  it("handles zero nodes and zero links (header-only buffer)", () => {
    const res = decodePeriodResults(buildBuffer(0, 0, 0, []));
    expect(res).not.toBeNull();
    if (!res) throw new Error("unreachable");
    expect(res.nodeDemand.length).toBe(0);
    expect(res.linkStatus.length).toBe(0);
    expect(res.nodeQuality).toBeUndefined();

    // Same with the quality flag set: empty quality views, still non-null.
    const resQ = decodePeriodResults(buildBuffer(0, 0, FLAG_QUALITY, []));
    expect(resQ).not.toBeNull();
    expect(resQ?.nodeQuality?.length).toBe(0);
    expect(resQ?.linkQuality?.length).toBe(0);
  });

  it("returns null for a buffer shorter than the 12-byte header", () => {
    expect(decodePeriodResults(new ArrayBuffer(0))).toBeNull();
    expect(decodePeriodResults(new ArrayBuffer(11))).toBeNull();
  });

  it("returns null when the buffer is truncated vs the declared counts", () => {
    const full = buildBuffer(2, 3, 0, [
      nodeDemand,
      nodeHead,
      nodePressure,
      linkFlow,
      linkVelocity,
      linkHeadloss,
      linkStatus,
    ]);
    const truncated = full.slice(0, full.byteLength - 4);
    expect(decodePeriodResults(truncated)).toBeNull();

    // Quality flag set but quality arrays missing → also truncated.
    const noQuality = buildBuffer(2, 3, FLAG_QUALITY, [
      nodeDemand,
      nodeHead,
      nodePressure,
      linkFlow,
      linkVelocity,
      linkHeadloss,
      linkStatus,
    ]);
    expect(decodePeriodResults(noQuality)).toBeNull();
  });

  it("tolerates trailing bytes beyond the expected payload", () => {
    const base = buildBuffer(1, 1, 0, [[1], [2], [3], [4], [5], [6], [7]]);
    const padded = new ArrayBuffer(base.byteLength + 8);
    new Uint8Array(padded).set(new Uint8Array(base));
    const res = decodePeriodResults(padded);
    expect(res).not.toBeNull();
    expect(Array.from(res?.linkStatus ?? [])).toEqual([7]);
  });
});
