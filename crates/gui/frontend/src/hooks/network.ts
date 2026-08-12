import type { OptionKind } from "./reports";
import type { GenericQuantity } from "./results";
/**
 * Network model hooks and mutation commands: nodes/links/patterns/curves,
 * element patching, controls & rules, and the diff-preview seam.
 */

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { Link, LinkType, Node, NodeType, Pattern } from "../types";
import { invoke, isTauri, tryInvoke, tryInvokeOr } from "./ipc";
import type { ValidationFinding } from "./issues";
import type { NetworkSummary } from "./NetworkDataContext";
import { useNetworkData } from "./NetworkDataContext";
import { useNetworkVersion } from "./NetworkVersionContext";
import type { SidecarRef } from "./projects";

// ── Binary network snapshot decoding ───────────────────────────────────────
//
// `get_network_snapshot` and `load_project_network` return the full-network
// snapshot as a compact little-endian columnar binary payload instead of
// ~15 MB of JSON (~5 MB binary at 46k nodes + 46k links, and no JSON parse
// on the webview main thread). The layout is produced by the backend's
// `encode_network_snapshot` (commands.rs) — see its doc comment for the
// authoritative byte map:
//
//   u32 version | u32 flags (bit 0 = present) | u32 nNodes | u32 nLinks |
//   u32 totalVerts | u32×3 reserved |
//   f64×nNodes x | y |
//   f64×totalVerts vertexX | f64×totalVerts vertexY   (concatenated in link order) |
//   f32×nNodes elevation | baseDemand | pressure | demand |
//              tankMinLevel | tankMaxLevel | tankInitialLevel | tankDiameter |
//   f32×nLinks velocity | diameter | length | roughness |
//              pumpPowerKw | pumpSpeed | valveSetting |
//   u8×nNodes nodeKind | u8×nLinks linkKind |
//   u8×nLinks initialStatus   (0 = open, 1 = closed, 2 = cv; non-pipes 0) |
//   u32×nLinks vertexCount   (LE, possibly unaligned) |
//   9 string columns (u32 byteLen + newline-joined UTF-8):
//     node id | tankVolumeCurve | headPattern |
//     link id | fromId | toId | pumpCurve | valveType | valveCurve
//
// Optional numeric columns use NaN for "absent" (preserving null vs 0),
// optional string columns use the empty string.

const SNAPSHOT_HEADER_BYTES = 32;
/**
 * The leading `u32` of a snapshot answers two questions at once — which of
 * the two layouts this is, and which version of it — because the decoder
 * picks the layout by reading it. So these two numbers share one namespace
 * and must never meet: bumping this to 4 for a new column would silently
 * hand every water-distribution snapshot to the generic decoder.
 *
 * Kept in step with `NETWORK_SNAPSHOT_VERSION` in `commands/binary_codec.rs`
 * and `GENERIC_SNAPSHOT_VERSION` in `commands/uds_view.rs`; asserted distinct
 * on both sides, since neither compiler can see the other's constant.
 */
export const SNAPSHOT_VERSION = 3;
const SNAPSHOT_FLAG_PRESENT = 1;

// Canvas-facing fields carried on Link beyond the backend DTO baseline:
// `vertices` is decoded from the v2 snapshot (intermediate polyline points in
// the source CRS, exclusive of the endpoints); `headloss` is merged per
// reporting period by the canvas from PeriodResults.linkHeadloss. Both are
// optional, so every existing Link consumer keeps compiling untouched.
import type { Region } from "../types/network";

declare module "../types/network" {
  interface Link {
    /** Intermediate polyline vertices [x, y] in source-CRS coordinates
     * (endpoints excluded). Omitted for straight links. */
    vertices?: Array<[number, number]>;
    /** Head loss for the current reporting period (per unit length for
     * pipes). `null`/absent when no simulation has run. */
    headloss?: number | null;
  }
}

/** Index ↔ kind code mapping; must match the backend's `encode_network_snapshot`. */
const SNAPSHOT_NODE_TYPES: readonly NodeType[] = [
  "junction",
  "tank",
  "reservoir",
];
const SNAPSHOT_LINK_TYPES: readonly LinkType[] = ["pipe", "pump", "valve"];

/** v3 `initialStatus` code → `Link.initialStatus` value (pipes only). */
const SNAPSHOT_LINK_STATUSES = ["open", "closed", "cv"] as const;

// ── Generic (engine-neutral) snapshot, layout v4 ─────────────────────────────
// Emitted for engines the GUI views through the element-class contract
// (hydra-common §4.1): classed points, polylines, and regions with a kind
// string table. Produced by `commands/uds_view.rs`; layout documented there.

/** See [`SNAPSHOT_VERSION`] — one namespace, two layouts. */
export const GENERIC_SNAPSHOT_VERSION = 4;
const GENERIC_HEADER_BYTES = 48;

function decodeGenericSnapshot(
  buf: ArrayBuffer,
  view: DataView,
): { nodes: Node[]; links: Link[]; regions: Region[] } | null {
  const flags = view.getUint32(4, true);
  if ((flags & SNAPSHOT_FLAG_PRESENT) === 0) return null;
  const nPoints = view.getUint32(8, true);
  const nPolylines = view.getUint32(12, true);
  const nRegions = view.getUint32(16, true);
  const nKinds = view.getUint32(20, true);
  const totalBends = view.getUint32(24, true);
  const totalRing = view.getUint32(28, true);

  let offset = GENERIC_HEADER_BYTES;
  const takeF64 = (len: number): Float64Array => {
    const arr = new Float64Array(buf, offset, len);
    offset += 8 * len;
    return arr;
  };
  const takeI32 = (len: number): Int32Array => {
    // May be unaligned after odd f64 totals never occur (all blocks are
    // 8-byte multiples before this point), but copy defensively anyway.
    const bytes = new Uint8Array(buf, offset, 4 * len);
    offset += 4 * len;
    return new Int32Array(bytes.slice().buffer);
  };
  const takeU8 = (len: number): Uint8Array => {
    const arr = new Uint8Array(buf, offset, len);
    offset += len;
    return arr;
  };
  const takeStrings = (count: number): string[] => {
    const byteLen = new DataView(buf, offset, 4).getUint32(0, true);
    offset += 4;
    const text = new TextDecoder().decode(new Uint8Array(buf, offset, byteLen));
    offset += byteLen;
    if (count === 0) return [];
    const parts = text.split("\n");
    if (parts.length !== count) {
      throw snapshotError(
        `string column has ${parts.length} entries, expected ${count}`,
      );
    }
    return parts;
  };

  const px = takeF64(nPoints);
  const py = takeF64(nPoints);
  const bx = takeF64(totalBends);
  const by = takeF64(totalBends);
  const rx = takeF64(totalRing);
  const ry = takeF64(totalRing);
  const from = takeI32(nPolylines);
  const to = takeI32(nPolylines);
  const outlet = takeI32(nRegions);
  const bendCount = takeI32(nPolylines);
  const ringCount = takeI32(nRegions);
  const pointKind = takeU8(nPoints);
  const polylineKind = takeU8(nPolylines);
  const regionKind = takeU8(nRegions);
  const kinds = takeStrings(nKinds);
  const pointIds = takeStrings(nPoints);
  const polylineIds = takeStrings(nPolylines);
  const regionIds = takeStrings(nRegions);

  const kindOf = (arr: Uint8Array, i: number): string =>
    kinds[arr[i]] ?? "unknown";

  const nodes: Node[] = [];
  for (let i = 0; i < nPoints; i += 1) {
    // No attribute fields: the v4 snapshot is geometry + identity only.
    // Fabricating zeros here made every consumer print "Elevation 0.00 m"
    // as if it were model data.
    nodes.push({
      id: pointIds[i],
      type: kindOf(pointKind, i),
      x: px[i],
      y: py[i],
      pressure: null,
      demand: null,
    });
  }

  const links: Link[] = [];
  let bendAt = 0;
  for (let i = 0; i < nPolylines; i += 1) {
    const n = bendCount[i];
    const vertices: Array<[number, number]> = [];
    for (let k = 0; k < n; k += 1) {
      vertices.push([bx[bendAt + k], by[bendAt + k]]);
    }
    bendAt += n;
    const link: Link = {
      id: polylineIds[i],
      type: kindOf(polylineKind, i),
      fromId: from[i] >= 0 ? pointIds[from[i]] : "",
      toId: to[i] >= 0 ? pointIds[to[i]] : "",
    };
    if (vertices.length > 0) link.vertices = vertices;
    links.push(link);
  }

  const regions: Region[] = [];
  let ringAt = 0;
  for (let i = 0; i < nRegions; i += 1) {
    const n = ringCount[i];
    const ring: Array<[number, number]> = [];
    for (let k = 0; k < n; k += 1) {
      ring.push([rx[ringAt + k], ry[ringAt + k]]);
    }
    ringAt += n;
    regions.push({
      id: regionIds[i],
      type: kindOf(regionKind, i),
      ring,
      outletId: outlet[i] >= 0 ? pointIds[outlet[i]] : null,
    });
  }

  return { nodes, links, regions };
}

function snapshotError(detail: string): Error {
  return new Error(`network snapshot decode failed: ${detail}`);
}

/**
 * Decode the binary network snapshot into the exact node/link object shape
 * the JSON path produced (plain objects; optional fields explicitly `null`
 * when absent, so `normalizeNodes` finds nothing left to fill in).
 *
 * Returns `null` when the payload's "present" flag is clear (the binary
 * equivalent of `load_project_network`'s old `null` — target INP missing).
 * Throws on a malformed, truncated, or version-mismatched buffer so callers
 * surface the error instead of rendering a silently empty network.
 *
 * Exported for tests — production callers go through
 * `fetchNetworkSnapshot` / `loadProjectNetwork`.
 */
export function decodeNetworkSnapshot(
  buf: ArrayBuffer,
): { nodes: Node[]; links: Link[]; regions: Region[] } | null {
  if (buf.byteLength < SNAPSHOT_HEADER_BYTES) {
    throw snapshotError(`buffer too short (${buf.byteLength} bytes)`);
  }
  const view = new DataView(buf);
  const version = view.getUint32(0, true);
  if (version === GENERIC_SNAPSHOT_VERSION) {
    return decodeGenericSnapshot(buf, view);
  }
  if (version !== SNAPSHOT_VERSION) {
    throw snapshotError(`unsupported version ${version}`);
  }
  const flags = view.getUint32(4, true);
  if ((flags & SNAPSHOT_FLAG_PRESENT) === 0) return null;
  const nNodes = view.getUint32(8, true);
  const nLinks = view.getUint32(12, true);
  const totalVerts = view.getUint32(16, true);
  // Bytes 20..32 are reserved.

  // Fixed-width section: 16B coords + 32B f32s + 1B kind per node,
  // 28B f32s + 1B kind + 1B initialStatus + 4B vertexCount per link,
  // 16B per link vertex.
  const fixedBytes =
    SNAPSHOT_HEADER_BYTES + 49 * nNodes + 34 * nLinks + 16 * totalVerts;
  if (buf.byteLength < fixedBytes) {
    throw snapshotError(
      `truncated buffer (${buf.byteLength} bytes for ${nNodes} nodes + ${nLinks} links + ${totalVerts} vertices)`,
    );
  }

  let offset = SNAPSHOT_HEADER_BYTES;
  const takeF64 = (len: number): Float64Array => {
    const arr = new Float64Array(buf, offset, len);
    offset += 8 * len;
    return arr;
  };
  const takeF32 = (len: number): Float32Array => {
    const arr = new Float32Array(buf, offset, len);
    offset += 4 * len;
    return arr;
  };
  const takeU8 = (len: number): Uint8Array => {
    const arr = new Uint8Array(buf, offset, len);
    offset += len;
    return arr;
  };
  const utf8 = new TextDecoder();
  const takeStrings = (count: number, label: string): string[] => {
    if (offset + 4 > buf.byteLength) {
      throw snapshotError(`truncated ${label} column header`);
    }
    const byteLen = view.getUint32(offset, true);
    offset += 4;
    if (offset + byteLen > buf.byteLength) {
      throw snapshotError(`truncated ${label} column`);
    }
    const joined = utf8.decode(new Uint8Array(buf, offset, byteLen));
    offset += byteLen;
    if (count === 0) return [];
    // Splitting one big string is fast in JS; empty string = absent.
    const parts = joined.split("\n");
    if (parts.length !== count) {
      throw snapshotError(
        `${label} column has ${parts.length} values, expected ${count}`,
      );
    }
    return parts;
  };

  const nodeX = takeF64(nNodes);
  const nodeY = takeF64(nNodes);
  const vertexX = takeF64(totalVerts);
  const vertexY = takeF64(totalVerts);
  const nodeElevation = takeF32(nNodes);
  const nodeBaseDemand = takeF32(nNodes);
  const nodePressure = takeF32(nNodes);
  const nodeDemand = takeF32(nNodes);
  const tankMinLevel = takeF32(nNodes);
  const tankMaxLevel = takeF32(nNodes);
  const tankInitialLevel = takeF32(nNodes);
  const tankDiameter = takeF32(nNodes);
  const linkVelocity = takeF32(nLinks);
  const linkDiameter = takeF32(nLinks);
  const linkLength = takeF32(nLinks);
  const linkRoughness = takeF32(nLinks);
  const pumpPowerKw = takeF32(nLinks);
  const pumpSpeed = takeF32(nLinks);
  const valveSetting = takeF32(nLinks);
  const nodeKind = takeU8(nNodes);
  const linkKind = takeU8(nLinks);
  const linkInitialStatus = takeU8(nLinks);
  // Per-link vertex counts. This column follows the u8 kind columns so its
  // start offset is not necessarily 4-byte aligned — a Uint32Array view would
  // throw, so read each value through the DataView instead.
  const vertexCount = new Uint32Array(nLinks);
  for (let i = 0; i < nLinks; i += 1) {
    vertexCount[i] = view.getUint32(offset + 4 * i, true);
  }
  offset += 4 * nLinks;
  const nodeIds = takeStrings(nNodes, "node id");
  const tankVolumeCurve = takeStrings(nNodes, "tankVolumeCurve");
  const headPattern = takeStrings(nNodes, "headPattern");
  const linkIds = takeStrings(nLinks, "link id");
  const fromIds = takeStrings(nLinks, "fromId");
  const toIds = takeStrings(nLinks, "toId");
  const pumpCurve = takeStrings(nLinks, "pumpCurve");
  const valveType = takeStrings(nLinks, "valveType");
  const valveCurve = takeStrings(nLinks, "valveCurve");

  const optNum = (v: number): number | null => (Number.isNaN(v) ? null : v);
  const optStr = (s: string): string | null => (s.length === 0 ? null : s);

  const nodes: Node[] = new Array(nNodes);
  for (let i = 0; i < nNodes; i += 1) {
    const type = SNAPSHOT_NODE_TYPES[nodeKind[i]];
    if (type === undefined) {
      throw snapshotError(`unknown node kind code ${nodeKind[i]}`);
    }
    nodes[i] = {
      id: nodeIds[i],
      type,
      x: nodeX[i],
      y: nodeY[i],
      elevation: nodeElevation[i],
      baseDemand: nodeBaseDemand[i],
      pressure: optNum(nodePressure[i]),
      demand: optNum(nodeDemand[i]),
      tankMinLevel: optNum(tankMinLevel[i]),
      tankMaxLevel: optNum(tankMaxLevel[i]),
      tankInitialLevel: optNum(tankInitialLevel[i]),
      tankDiameter: optNum(tankDiameter[i]),
      tankVolumeCurve: optStr(tankVolumeCurve[i]),
      headPattern: optStr(headPattern[i]),
    };
  }

  const links: Link[] = new Array(nLinks);
  let vertCursor = 0;
  for (let i = 0; i < nLinks; i += 1) {
    const type = SNAPSHOT_LINK_TYPES[linkKind[i]];
    if (type === undefined) {
      throw snapshotError(`unknown link kind code ${linkKind[i]}`);
    }
    // Slice this link's vertex run out of the concatenated columns. Links
    // without vertices omit the field entirely so existing Link consumers
    // (and object-shape assertions) see the exact pre-v2 shape.
    const nVerts = vertexCount[i];
    let vertices: Array<[number, number]> | undefined;
    if (nVerts > 0) {
      if (vertCursor + nVerts > totalVerts) {
        throw snapshotError(
          `vertexCount sum exceeds totalVerts (${totalVerts})`,
        );
      }
      vertices = new Array(nVerts);
      for (let v = 0; v < nVerts; v += 1) {
        vertices[v] = [vertexX[vertCursor + v], vertexY[vertCursor + v]];
      }
      vertCursor += nVerts;
    }
    // Initial [STATUS] is only meaningful for pipes; pumps/valves always
    // carry code 0 and omit the field so their object shape is unchanged.
    const initialStatus = SNAPSHOT_LINK_STATUSES[linkInitialStatus[i]];
    if (initialStatus === undefined) {
      throw snapshotError(
        `unknown link initialStatus code ${linkInitialStatus[i]}`,
      );
    }
    links[i] = {
      ...(vertices !== undefined ? { vertices } : null),
      ...(type === "pipe" ? { initialStatus } : null),
      id: linkIds[i],
      type,
      fromId: fromIds[i],
      toId: toIds[i],
      velocity: linkVelocity[i],
      diameter: linkDiameter[i],
      length: linkLength[i],
      roughness: linkRoughness[i],
      pumpCurve: optStr(pumpCurve[i]),
      pumpPowerKw: optNum(pumpPowerKw[i]),
      pumpSpeed: optNum(pumpSpeed[i]),
      valveType: optStr(valveType[i]),
      valveSetting: optNum(valveSetting[i]),
      valveCurve: optStr(valveCurve[i]),
    };
  }
  if (vertCursor !== totalVerts) {
    throw snapshotError(
      `vertexCount sum ${vertCursor} does not match totalVerts ${totalVerts}`,
    );
  }

  return { nodes, links, regions: [] };
}

/**
 * Fetch the full nodes+links snapshot of the loaded network as a binary
 * payload and decode it. Returns `null` outside Tauri (or when the command
 * itself fails — reported via `onIpcError` like every `tryInvoke`); throws
 * when the payload cannot be decoded (frontend/backend layout mismatch).
 */
export async function fetchNetworkSnapshot(): Promise<{
  nodes: Node[];
  links: Link[];
  regions: Region[];
} | null> {
  const buf = await tryInvoke<ArrayBuffer>("get_network_snapshot");
  // `null` = outside Tauri or the command failed (reported via onIpcError).
  if (buf === null) return null;
  // Any other non-ArrayBuffer payload is a frontend/backend contract break —
  // throw instead of conflating it with "no data".
  if (!(buf instanceof ArrayBuffer)) {
    throw snapshotError(
      `get_network_snapshot returned unexpected payload type ${typeof buf} (expected ArrayBuffer)`,
    );
  }
  // `get_network_snapshot` always sets the "present" flag, so decode only
  // returns null in the (never-hit) flag-clear case.
  return decodeNetworkSnapshot(buf);
}

// ── Network model hooks (nodes, links) ─────────────────────────────────────

/**
 * Open the native file picker filtered to `engine`'s source-model formats,
 * parse the chosen file with that engine, and hold it in backend state.
 *
 * `engine` is required rather than defaulting: the picker filter and the
 * parser both depend on it, and every `.inp` looks alike from here.
 */
export async function openAndLoadNetwork(
  engine: string,
): Promise<ImportedModel | null> {
  if (!isTauri()) return null;
  return await invoke<ImportedModel | null>("open_and_load_network", {
    engine,
  });
}

/**
 * Open a model file of any format the app supports and let the file say
 * which engine owns it (hydra-common §2.5.1 recognition).
 *
 * The inverse of [`openAndLoadNetwork`], for the reader who has a `.inp`
 * and should not have to know whether EPANET or SWMM wrote it. Rejects —
 * rather than guesses — when no engine claims the file definitively, so
 * the caller must be ready to report why. `null` means the dialog was
 * cancelled.
 */
export async function openAndRecogniseNetwork(): Promise<ImportedModel | null> {
  if (!isTauri()) return null;
  return await invoke<ImportedModel | null>("open_and_recognise_network", {});
}

/**
 * A model just read by the import dialog.
 *
 * `findings` is empty when the model is ready to run. Non-empty means it was
 * read but is not yet simulable, and these are the very findings the Issues
 * panel will list once the project opens — the wizard reports the count rather
 * than importing silently.
 */
export interface ImportedModel {
  network: {
    nodes: Node[];
    links: Link[];
    fileStem: string;
  };
  /** Element counts of the imported model. Authoritative over the array
   * lengths above: engines whose element data arrives via the viewer
   * snapshot return an empty `network` here, but never empty counts. */
  nodeCount: number;
  linkCount: number;
  findings: ValidationFinding[];
  /** Repairs applied during import (one message per line the importer
   * commented out); empty when the file imported as written. Callers must
   * surface these — the repair contract forbids applying them silently. */
  repairs: string[];
  /** Whether the model's coordinates rule out longitude and latitude.
   * Answered by the importer, not here: a drainage import returns no
   * elements at all, so the frontend has nothing to read them from. */
  coordinatesProjected: boolean;
  /** The engine that owns this model — recognised from the file when the
   * caller did not name one. */
  engine: string;
  /** Auxiliary files the model references: carried when the import has
   * their bytes (found beside the model or attached), missing otherwise. */
  sidecars: SidecarRef[];
}

/** Convert backend/Tauri import errors into concise toast-safe text. */
export function formatInpImportError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err ?? "");
  const normalized = raw.replace(/\r\n/g, "\n").trim();
  if (!normalized) return "Could not import the model file.";

  const firstUsefulLine = normalized
    .split("\n")
    .map((line) => line.trim())
    .find(
      (line) =>
        line.length > 0 &&
        !/^at\s+/i.test(line) &&
        !/^stack backtrace:/i.test(line),
    );

  let detail = (firstUsefulLine ?? normalized)
    .replace(/^error invoking [`'"]?[^`'"]+[`'"]?:\s*/i, "")
    .replace(/^command [`'"]?[^`'"]+[`'"]? failed:?\s*/i, "")
    .replace(/^error:\s*/i, "")
    .trim();

  const causedByIdx = detail.toLowerCase().indexOf("caused by:");
  if (causedByIdx >= 0) {
    detail = detail.slice(0, causedByIdx).trim();
  }

  if (!detail) return "Could not import the model file.";

  const maxLen = 220;
  if (detail.length > maxLen) {
    detail = `${detail.slice(0, maxLen - 1).trimEnd()}...`;
  }

  return `Could not import the model file: ${detail}`;
}

/**
 * Load the INP for the base model (`scenarioId = null`) or a named scenario
 * into the backend `NetworkState` so callers can bump `networkVersion` to
 * trigger a `useNodes` / `useLinks` refetch.
 *
 * The backend responds with the compact binary snapshot layout (see
 * `decodeNetworkSnapshot`). Returns a nodes+links snapshot when loaded, or
 * `null` when the target INP does not exist yet (encoded as a payload with
 * the "present" flag clear) or when running outside Tauri / the command
 * failed (reported via onIpcError). Decode failures and unexpected payload
 * types throw.
 */
export async function loadProjectNetwork(
  projectId: string,
  scenarioId: string | null,
): Promise<{ nodes: Node[]; links: Link[]; regions: Region[] } | null> {
  const buf = await tryInvoke<ArrayBuffer>("load_project_network", {
    projectId,
    scenarioId,
  });
  // `null` = outside Tauri or the command failed (reported via onIpcError).
  if (buf === null) return null;
  // An unexpected payload type is a frontend/backend contract break, not
  // the "target INP missing" case — that one is a real ArrayBuffer with the
  // "present" flag clear, which decodeNetworkSnapshot maps to null.
  if (!(buf instanceof ArrayBuffer)) {
    throw snapshotError(
      `load_project_network returned unexpected payload type ${typeof buf} (expected ArrayBuffer)`,
    );
  }
  return decodeNetworkSnapshot(buf);
}

/**
 * A single updated element carried in the `network-changed` event's delta
 * payload (one entry per element updated by `patch_element` /
 * `patch_elements` / `patch_node_position`) — exactly one of `node` / `link`
 * is set.
 */
export interface PatchedElement {
  node?: Node;
  link?: Link;
}

/** Result of a bulk `patchElements` call. */
export interface PatchElementsResult {
  /** Number of patches applied successfully. */
  applied: number;
  /** Error strings for the patches that failed (batch continues past them). */
  errors: string[];
}

/**
 * Apply a batch of field changes in a single backend call: one IPC round
 * trip and one `network-changed` event for the whole batch, instead of one
 * command (and formerly one full INP re-serialisation) per field.
 */
export async function patchElements(
  patches: PatchItem[],
): Promise<PatchElementsResult> {
  return invoke<PatchElementsResult>("patch_elements", { patches });
}

/**
 * Move a node to a new [x, y] coordinate in one backend call.
 * More efficient than two `patchElement` calls (single INP re-serialisation).
 */
export async function patchNodePosition(
  id: string,
  x: number,
  y: number,
): Promise<void> {
  await tryInvoke<void>("patch_node_position", { id, x, y });
}

/** What an application supplies to create one element (§4.5.3). */
export interface NewElement {
  /** The engine's kind id, from its catalog. */
  kind: string;
  id: string;
  /** Where to put it, in the model's own coordinate system. Required for
   * a point or a region; a polyline is placed by its two ends instead. */
  position?: [number, number];
  fromId?: string;
  toId?: string;
  /** Values for the kind's editable attributes, by schema key. Anything
   * omitted keeps the engine's default. */
  fields?: Record<string, number | string>;
}

/**
 * Add one element to the loaded model, whichever engine holds it.
 *
 * Throws the engine's refusal — a kind that needs a relation curve says
 * what is missing, and a create that cannot finish leaves nothing
 * behind.
 */
export async function createElement(
  projectId: string,
  element: NewElement,
): Promise<void> {
  await invoke<void>("create_element", { projectId, element });
}

/** What a delete took with it besides the element itself. */
export interface Removed {
  /** The element asked for. */
  id: string;
  /** Links removed because an end of theirs went. */
  links: string[];
  /**
   * Other records removed because what they attached to went, already
   * phrased for a sentence ("2 inflows"). Empty for water distribution,
   * whose nodes carry no such records.
   */
  attachments: string[];
}

/**
 * Delete an element from the in-memory network.
 *
 * `kind` is the water-distribution kind for a wds model; a drainage
 * model finds the element by id and ignores it.
 *
 * Deleting a node also removes the links attached to it — and, in a
 * drainage model, the records that only described it. The answer says
 * which, so the caller can report it rather than leave it to be found.
 *
 * Throws on a refused delete rather than resolving. It used the silent
 * variant, which resolves `null` on a backend error — so a refusal
 * ("still attached to a control") looked exactly like a success to the
 * caller, which then reported nothing, pushed an undo entry for a
 * delete that had not happened, and saved. A delete that can be refused
 * has to be able to say so.
 */
export async function deleteElement(
  kind: string,
  id: string,
): Promise<Removed> {
  return invoke<Removed>("delete_element", { kind, id });
}

/** Create a new node (junction / tank / reservoir) at the given geographic coordinates. */
export async function createNode(
  kind: string,
  id: string,
  x: number,
  y: number,
  elevation = 0,
  minLevel?: number,
  maxLevel?: number,
  initialLevel?: number,
): Promise<void> {
  await invoke<void>("create_node", {
    kind,
    id,
    x,
    y,
    elevation,
    minLevel,
    maxLevel,
    initialLevel,
  });
}

/** Create a new link (pipe / pump) between two existing nodes. */
export async function createLink(
  kind: string,
  id: string,
  fromId: string,
  toId: string,
): Promise<void> {
  await invoke<void>("create_link", { kind, id, fromId, toId });
}

/** Create a new pump-head curve with default two-point data. */
export async function createCurve(id: string): Promise<void> {
  await invoke<void>("create_curve", { id });
}

/**
 * Replace all points of an existing curve. `xs`/`ys` must be in the same
 * display units returned by `useCurves()` (flow L/s and head m for pump-head
 * curves) and have equal length.
 */
export async function updateCurvePoints(
  id: string,
  xs: number[],
  ys: number[],
): Promise<void> {
  await invoke<void>("update_curve_points", { id, xs, ys });
}

/**
 * Delete a curve. Rejects if any pump, valve, or tank still references it —
 * the caller should surface the returned error and let the user detach it
 * first.
 */
export async function deleteCurve(id: string): Promise<void> {
  await invoke<void>("delete_curve", { id });
}

/** Create a new time pattern with 24 flat hourly multipliers (all 1.0). */
export async function createPattern(id: string): Promise<void> {
  await invoke<void>("create_pattern", { id });
}

/** Replace all multipliers of an existing time pattern. */
export async function updatePatternMultipliers(
  id: string,
  multipliers: number[],
): Promise<void> {
  await invoke<void>("update_pattern_multipliers", { id, multipliers });
}

/**
 * Rename a time pattern, cascading the new ID to every junction demand,
 * reservoir/tank head pattern, pump speed/price pattern, and the network's
 * global default/energy-price pattern that referenced it. Applied
 * immediately (not staged in the Network Editor draft) since it's a single
 * atomic, low-risk operation.
 */
export async function renamePattern(
  oldId: string,
  newId: string,
): Promise<void> {
  await invoke<void>("rename_pattern", { oldId, newId });
}

/**
 * Rename a node or link, cascading the new ID to its coordinates/vertices,
 * tags, and (for nodes) the quality trace node. `kind` is one of
 * `"junction"`/`"reservoir"`/`"tank"` or `"pipe"`/`"pump"`/`"valve"`. Applied
 * immediately (not staged in the editor draft); rejects with the backend
 * message if `newId` is empty, unsafe, or already used by another node/link.
 */
export async function renameElement(
  kind: string,
  oldId: string,
  newId: string,
): Promise<void> {
  await invoke<void>("rename_element", { kind, oldId, newId });
}

/**
 * Rename a curve, cascading the new ID to every pump head/efficiency curve,
 * GPV valve curve, and tank volume curve that referenced it. Applied
 * immediately; rejects if `newId` is empty, unsafe, or already a curve ID.
 */
export async function renameCurve(oldId: string, newId: string): Promise<void> {
  await invoke<void>("rename_curve", { oldId, newId });
}

/**
 * Delete a time pattern. Rejects if any junction demand, reservoir/tank head
 * pattern, pump speed/price pattern, or the global default/energy-price
 * pattern still references it — the caller should surface the returned
 * error and let the user detach it first.
 */
export async function deletePattern(id: string): Promise<void> {
  await invoke<void>("delete_pattern", { id });
}

export interface PatchItem {
  kind: string;
  id: string;
  field: string;
  value: number | string;
}

/**
 * Apply patches to a temporary clone of the in-memory network and return the
 * resulting INP text without mutating backend state.
 * Used by the diff preview dialog to show what the file would look like after saving.
 */
export async function previewPatches(
  patches: PatchItem[],
): Promise<string | null> {
  return tryInvokeOr<string | null>("preview_patches", { patches }, null);
}

// ── Network change events ──────────────────────────────────────────────────

export const NETWORK_CHANGED_EVENT = "network-changed";

/**
 * Delta payload of a `network-changed` event. Element-scoped edits
 * (`patch_element` / `patch_elements` / `patch_node_position`) list the
 * updated element DTOs; structural mutations (create/delete/pattern/curve/
 * control commands) emit a `null` payload, which consumers treat as
 * "refetch the full snapshot".
 */
export interface NetworkChangedPayload {
  elements: PatchedElement[];
}

/** Subscribe to network mutation events from the backend (fired whenever any
 *  mutating command succeeds), delivering the event's delta payload (`null`
 *  when the mutation requires a full snapshot refetch).
 *  Returns the unlisten function — call it to unsubscribe. */
export function listenNetworkChangedPayload(
  cb: (payload: NetworkChangedPayload | null) => void,
): Promise<() => void> {
  return listen<NetworkChangedPayload | null>(NETWORK_CHANGED_EVENT, (ev) =>
    cb(ev.payload ?? null),
  );
}

/**
 * True when a `network-changed` payload denotes a structural mutation
 * (create / delete / pattern / curve / control commands — no element DTOs):
 * consumers must refetch from the backend. Element-scoped deltas (non-empty
 * `elements`) are self-applied by NetworkDataContext's own listener and
 * carry everything the frontend needs, so they must NOT trigger the
 * version-keyed refetch machinery.
 */
export function isStructuralNetworkChange(
  payload: NetworkChangedPayload | null,
): boolean {
  return !payload || payload.elements.length === 0;
}

// ── Node / link / pattern / curve hooks ────────────────────────────────────

export function useNodes(_version = 0): Node[] {
  // `_version` is kept for API compatibility with existing callers.
  void _version;
  const { nodes } = useNetworkData();
  return nodes;
}

export function useLinks(_version = 0): Link[] {
  // `_version` is kept for API compatibility with existing callers.
  void _version;
  const { links } = useNetworkData();
  return links;
}

/** Areal elements (subcatchments); empty for engines without them. */
export function useRegions(): Region[] {
  const { regions } = useNetworkData();
  return regions;
}

export function useNetworkSummary(): NetworkSummary {
  const { summary } = useNetworkData();
  return summary;
}

/**
 * Shared fetch-effect for the version-keyed row hooks (patterns / curves /
 * controls / rules): re-fetch `cmd` whenever the network version from
 * `NetworkVersionContext` or the caller-supplied refetch counter bumps.
 * Keeps the previous rows when the fetch resolves `null` (outside Tauri or
 * command failure) and ignores results that land after unmount or a re-run.
 */
function useVersionedRows<T>(cmd: string, version: number): T[] {
  const { version: ctxVersion } = useNetworkVersion();
  const [rows, setRows] = useState<T[]>([]);
  useEffect(() => {
    // Both versions are pure refetch triggers.
    void ctxVersion;
    void version;
    let cancelled = false;
    tryInvoke<T[]>(cmd).then((next) => {
      if (!cancelled && next !== null) setRows(next);
    });
    return () => {
      cancelled = true;
    };
  }, [cmd, ctxVersion, version]);
  return rows;
}

export function usePatterns(_version = 0): Pattern[] {
  return useVersionedRows<Pattern>("get_patterns", _version);
}

// ── Curve / pattern editor types ───────────────────────────────────────────

/**
 * One sample on a curve, in the SI display units of its curve's axes.
 *
 * Named `x`/`y` because only the curve knows what they are: flow and head
 * on a pump curve, level and volume on a tank curve, a valve position and
 * a loss ratio on a PCV curve. They were `flow`/`head`, which made every
 * curve read as a pump curve and every axis label a lie for four of the
 * six kinds.
 */
export interface CurvePoint {
  x: number;
  y: number;
}

/** What one axis of a curve measures, and in what — engine-authored. */
export interface CurveAxis {
  label: string;
  /** §5 quantity for this axis's values; absent = unitless. */
  quantity?: GenericQuantity;
}

interface CurveKindAxesDto {
  kind: string;
  axes: [CurveAxis, CurveAxis];
}

/**
 * Axes for a curve whose kind is not (yet) known, or whose engine
 * publishes none: two bare magnitudes, converted by nothing.
 *
 * Deliberately not pump-head axes. A wrong unit is worse than no unit —
 * it invites the reader to trust a number that has not been converted,
 * and this is exactly what a value typed into a not-yet-created curve
 * used to be stored as.
 */
export const UNKNOWN_CURVE_AXES: [CurveAxis, CurveAxis] = [
  { label: "X" },
  { label: "Y" },
];

/**
 * The engine's curve axes by kind, keyed for lookup.
 *
 * Static per engine — a property of the domain, not of any model — so one
 * fetch serves every curve, saved or staged. Empty before it resolves, and
 * for engines whose curves this GUI does not edit.
 */
export function useCurveAxes(
  engineKey: string | null | undefined,
): Record<string, [CurveAxis, CurveAxis]> {
  const [byKind, setByKind] = useState<Record<string, [CurveAxis, CurveAxis]>>(
    {},
  );
  useEffect(() => {
    if (!engineKey) {
      setByKind({});
      return;
    }
    let cancelled = false;
    void tryInvokeOr<CurveKindAxesDto[]>(
      "list_curve_axes",
      { engine: engineKey },
      [],
    ).then((rows) => {
      if (cancelled) return;
      setByKind(Object.fromEntries(rows.map((r) => [r.kind, r.axes])));
    });
    return () => {
      cancelled = true;
    };
  }, [engineKey]);
  return byKind;
}
export interface PumpCurve {
  id: string;
  pumpId: string;
  /**
   * What the curve is for, as the engine classified it from the model's
   * own references: `pump-head`, `pump-efficiency`, `tank-volume`,
   * `gpv-headloss`, `pcv-loss-ratio`, or `generic` for one nothing
   * references.
   *
   * This replaced a `curveType` field that was not a type at all — it
   * held `single-point`/`three-point`/`multi-point`, a restatement of
   * `points.length` under a name that read as the curve's role, while the
   * engine's actual role travelled in the same payload and was dropped.
   */
  role: string;
  points: CurvePoint[];
  notes?: string;
}
export interface TimePattern {
  id: string;
  label: string;
  multipliers: number[];
  stepHours: number;
}

/** Raw curve DTO mirroring the Rust `CurveDto`. */
interface NetworkCurveDto {
  id: string;
  kind: string;
  x: number[];
  y: number[];
}

/**
 * Returns the curves of the loaded network as `PumpCurve[]`.
 * Derives `pumpId` by cross-referencing the link list (the pump that
 * references each curve by ID). Non-pump-head curves (tank-volume, etc.)
 * are included with `pumpId = ""`.
 */
export function useCurves(version = 0): PumpCurve[] {
  const dtos = useVersionedRows<NetworkCurveDto>("get_curves", version);
  const links = useLinks(version);

  return useMemo<PumpCurve[]>(() => {
    const pumpByCurveId = new Map<string, string>();
    for (const l of links) {
      if (l.pumpCurve) pumpByCurveId.set(l.pumpCurve, l.id);
    }
    return dtos.map((d) => {
      const points: CurvePoint[] = d.x.map((x, i) => ({
        x,
        y: d.y[i] ?? 0,
      }));
      return {
        id: d.id,
        pumpId: pumpByCurveId.get(d.id) ?? "",
        role: d.kind,
        points,
      };
    });
  }, [dtos, links]);
}

export function useLinksConnectedTo(nodeId: string | null | undefined) {
  const links = useLinks();
  return useMemo(
    () =>
      nodeId
        ? links.filter((l) => l.fromId === nodeId || l.toId === nodeId)
        : [],
    [nodeId, links],
  );
}

// ── Controls & rules ────────────────────────────────────────────────────────

/** Mirrors the Rust `ControlDto`. Addressed by array position — there is no
 *  natural ID for simple controls in the INP format. */
export interface SimpleControlDto {
  linkId: string;
  /** "open" | "closed"; `null` when only `actionSetting` is used. */
  actionStatus: "open" | "closed" | null;
  /** Display-unit setting value; `null` when only `actionStatus` is used. */
  actionSetting: number | null;
  triggerKind: "timer" | "clocktime" | "hiLevel" | "loLevel";
  /** Seconds — elapsed sim time for "timer", seconds-from-midnight for "clocktime". */
  triggerSeconds: number | null;
  /** Trigger node ID for "hiLevel"/"loLevel". */
  triggerNodeId: string | null;
  /** Display-unit threshold (m) for "hiLevel"/"loLevel". */
  triggerValue: number | null;
  enabled: boolean;
}

export type RulePremiseAttribute =
  | "head"
  | "pressure"
  | "demand"
  | "level"
  | "flow"
  | "status"
  | "setting"
  | "power"
  | "fillTime"
  | "drainTime"
  | "clockTime"
  | "time";
export type RulePremiseOperator = "eq" | "neq" | "lt" | "gt" | "le" | "ge";

/** Mirrors the Rust `RulePremiseDto`. */
export interface RulePremiseDto {
  object: "node" | "link" | "clock";
  nodeId: string | null;
  linkId: string | null;
  attribute: RulePremiseAttribute;
  operator: RulePremiseOperator;
  /** Display-unit threshold; ignored when `attribute === "status"`. */
  value: number;
  /** Only meaningful when `attribute === "status"`. */
  statusValue: "open" | "closed" | "active" | null;
  connective: "and" | "or" | null;
}

/** Mirrors the Rust `RuleActionDto`. */
export interface RuleActionDto {
  linkId: string;
  status: "open" | "closed" | null;
  setting: number | null;
}

/** Mirrors the Rust `RuleDto`. `name` is a display-only label ("R1", "R2", …)
 *  synthesised from array position — rule-based controls have no persisted
 *  name in the engine's data model. Addressed by array position. */
export interface RuleDto {
  name: string;
  priority: number;
  premises: RulePremiseDto[];
  thenActions: RuleActionDto[];
  elseActions: RuleActionDto[];
}

export function useControls(version = 0): SimpleControlDto[] {
  return useVersionedRows<SimpleControlDto>("get_controls", version);
}

export function useRules(version = 0): RuleDto[] {
  return useVersionedRows<RuleDto>("get_rules", version);
}

export async function createControl(control: SimpleControlDto): Promise<void> {
  await invoke<void>("create_control", { control });
}
export async function updateControl(
  index: number,
  control: SimpleControlDto,
): Promise<void> {
  await invoke<void>("update_control", { index, control });
}
export async function deleteControl(index: number): Promise<void> {
  await invoke<void>("delete_control", { index });
}

export async function createRule(rule: RuleDto): Promise<void> {
  await invoke<void>("create_rule", { rule });
}
export async function updateRule(index: number, rule: RuleDto): Promise<void> {
  await invoke<void>("update_rule", { index, rule });
}
export async function deleteRule(index: number): Promise<void> {
  await invoke<void>("delete_rule", { index });
}

/** Return the loaded network's `[TITLE]` lines (empty outside Tauri or when
 * no network/title exists). */
export async function getNetworkTitle(): Promise<string[]> {
  return tryInvokeOr<string[]>("get_network_title", undefined, []);
}

/** Replace the network's `[TITLE]` lines (≤3, line 1 = title). Throws on
 * validation failure so callers can surface the message. */
export async function updateNetworkTitle(lines: string[]): Promise<void> {
  await invoke<void>("update_network_title", { lines });
}

// ── Engine-generic element details ──────────────────────────────────────────

/** §5 quantity descriptor accompanying a numeric attribute — everything the
 * frontend needs to convert the SI value to the active display system. */
export interface ElementAttributeQuantity {
  key: string;
  siLabel: string;
  usLabel: string;
  siToUsScale: number;
  siToUsOffset: number;
  siDecimals: number;
  usDecimals: number;
}

/** One Properties row of the engine-generic element inspector: an
 * engine-authored label with either a numeric SI value or a display text. */
export interface ElementAttribute {
  /** The engine's schema key — what a write is addressed by. */
  key: string;
  /** Whether this row can be written. Decided by the backend from the
   * same table its setter consults, so an input is never offered for a
   * key that would refuse it. */
  editable: boolean;
  label: string;
  number?: number;
  text?: string;
  quantity?: ElementAttributeQuantity;
}

/** Display string for an attribute row in the given unit system. */
export function formatElementAttribute(
  attr: ElementAttribute,
  sys: "si" | "us",
): string {
  if (attr.text != null) return attr.text;
  if (attr.number == null) return "—";
  const q = attr.quantity;
  if (!q) {
    // Unitless: enough precision for roughness-scale values.
    return String(Number(attr.number.toFixed(4)));
  }
  const value =
    sys === "us" ? attr.number * q.siToUsScale + q.siToUsOffset : attr.number;
  const decimals = sys === "us" ? q.usDecimals : q.siDecimals;
  const unit = sys === "us" ? q.usLabel : q.siLabel;
  return `${value.toFixed(decimals)} ${unit}`;
}

/**
 * The number to offer for editing at one place a value is shown, or
 * `null` to show it read-only.
 *
 * Two different questions have to both say yes, and they are answered by
 * different things. `editable` is about the *key*: the backend decides
 * from the same table its setter consults whether that attribute can be
 * written at all. The value is about this *element*: a null cell means
 * the element does not carry the attribute, and a field offered there
 * would invite creating a value the model never had — the table serves a
 * column for every attribute the kind declares, including ones a given
 * element has none of.
 *
 * A text value is never editable here whatever the flag says. It states
 * a referent or a choice, and setting one of those is a different
 * operation from typing a number over it.
 */
export function editableNumberOf(
  editable: boolean,
  value: number | string | null | undefined,
): number | null {
  if (!editable) return null;
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/**
 * The text an input holds while a value is being edited: the number in
 * the displayed unit, without that unit and without trailing zeros.
 *
 * Not {@link formatElementAttribute} — that string carries its unit and
 * pads to the declared decimals, and a field that shows "12.50 m" and
 * expects "12.5" back punishes reading it. Not the raw number either: a
 * value converted to US units lands on 3.2808398950131235, and offering
 * that for editing is offering noise.
 *
 * It is the string {@link parseElementAttribute} inverts, so the two are
 * defined together — a field that displays through one and reads through
 * the other is only stable while they agree.
 */
export function editableNumberText(
  value: number,
  quantity: ElementAttributeQuantity | undefined,
  sys: "si" | "us",
): string {
  if (!quantity) return String(Number(value.toFixed(4)));
  const shown =
    sys === "us" ? value * quantity.siToUsScale + quantity.siToUsOffset : value;
  const decimals = sys === "us" ? quantity.usDecimals : quantity.siDecimals;
  return String(Number(shown.toFixed(decimals)));
}

/**
 * The number a display string stands for, in the unit the backend
 * serves and takes — the inverse of {@link formatElementAttribute}'s
 * conversion.
 *
 * Kept beside the formatter because the two have to agree: a scale
 * applied on the way out and not on the way in stores a value a hundred
 * or ten thousand times out, which is exactly the mistake the backend
 * made first. Returns `null` for anything that is not a number, so a
 * half-typed value leaves the model alone.
 */
export function parseElementAttribute(
  text: string,
  quantity: ElementAttributeQuantity | undefined,
  sys: "si" | "us",
): number | null {
  const entered = Number(text.trim());
  if (text.trim() === "" || !Number.isFinite(entered)) return null;
  if (!quantity || sys === "si") return entered;
  return (entered - quantity.siToUsOffset) / quantity.siToUsScale;
}

/**
 * Write one attribute back, whichever engine holds the model.
 *
 * Addressed by the schema key the read served, and taking the value in
 * the unit that read served it in — the attribute's declared quantity,
 * which is not always SI.
 *
 * `value` is not always a number: water distribution edits a demand
 * pattern and a valve type in its tables, so the contract cannot
 * restrict editing to numbers. An engine whose write takes only numbers
 * refuses anything else, which is why its schema marks only numbers
 * editable.
 */
export async function setElementAttribute(
  projectId: string,
  elementId: string,
  key: string,
  value: number | string,
): Promise<void> {
  await invoke<void>("set_element_attribute", {
    projectId,
    elementId,
    key,
    value,
  });
}

/**
 * Engine-described attribute rows for one element, from whichever engine
 * holds the model.
 *
 * `null` outside Tauri, for a project whose engine this build cannot
 * open, and for an id no element answers to. It used to be null for
 * water distribution too, which served the same values as typed columns
 * in the network snapshot under names the frontend chose — two roads to
 * one feature, and the reason a surface could show one engine's element
 * and not the other's.
 */
export async function getElementDetails(
  projectId: string,
  scenarioId: string | null | undefined,
  elementId: string,
): Promise<ElementAttribute[] | null> {
  return tryInvokeOr<ElementAttribute[] | null>(
    "get_element_details",
    { projectId, scenarioId: scenarioId ?? null, elementId },
    null,
  );
}

// ── Inlet couplings (dual drainage) ─────────────────────────────────────────

/**
 * A hydraulic connection that is **not a link**: a street conduit capturing
 * flow into a sewer node through an inlet. In a dual-drainage model the
 * surface network reaches the buried sewer only this way, so anything
 * reasoning about connectivity from links alone would wrongly call the
 * street network detached.
 */
export interface InletCoupling {
  /** Id of the street conduit carrying the inlet. */
  link: string;
  /** Id of the node receiving captured flow. */
  node: string;
  /** Id of the inlet design doing the capturing — an `inlet` registry
   * entry, listed in the Editor like any other collection. */
  design: string;
}

/** Inlet couplings for a target; empty for engines that have none. */
export async function getInletCouplings(
  projectId: string,
  scenarioId?: string | null,
): Promise<InletCoupling[]> {
  return tryInvokeOr<InletCoupling[]>(
    "get_inlet_couplings",
    { projectId, scenarioId: scenarioId ?? null },
    [],
  );
}

/** Inlet couplings for the given target, refetched when it changes. */
export function useInletCouplings(
  projectId: string | null | undefined,
  scenarioId: string | null | undefined,
): { couplings: InletCoupling[]; resolved: boolean } {
  // `resolved` is the point of the shape. An empty array cannot say whether
  // this model has no couplings or has not been asked yet, and a layout that
  // guesses "none" declares every street conduit detached from the sewer it
  // drains into — briefly, until the answer arrives.
  const [state, setState] = useState<{
    couplings: InletCoupling[];
    resolved: boolean;
  }>({ couplings: EMPTY_COUPLINGS, resolved: false });
  useEffect(() => {
    if (!projectId) {
      setState({ couplings: EMPTY_COUPLINGS, resolved: true });
      return;
    }
    let cancelled = false;
    setState({ couplings: EMPTY_COUPLINGS, resolved: false });
    void getInletCouplings(projectId, scenarioId)
      .then((c) => {
        if (!cancelled) setState({ couplings: c, resolved: true });
      })
      .catch(() => {
        // A failed read is still an answer: draw the network without
        // couplings rather than never drawing it.
        if (!cancelled) {
          setState({ couplings: EMPTY_COUPLINGS, resolved: true });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId]);
  return state;
}

/** Stable empty, so a coupling-less model does not hand the layout a fresh
 * array identity on every render and invalidate its cache. */
const EMPTY_COUPLINGS: InletCoupling[] = [];

// ── Per-kind element tables ────────────────────────────────────────────────

/** One column of a kind's property table: an engine-declared attribute
 * (§4.4) with every element's value, parallel to `ids`. */
export interface KindColumn {
  key: string;
  label: string;
  /** Whether this column's cells can be written. Decided by the backend
   * from the same table its setter consults — the per-column twin of
   * `ElementAttribute.editable`, true for the same attributes. It says
   * the key is writable, not that any given cell is: see
   * {@link editableNumberOf}. */
  editable: boolean;
  /** The value's shape and bounds. What lets one table render a select
   * for a valve type, a yes/no for a check valve and a number for a
   * diameter, without naming any of them. */
  kind: OptionKind;
  /** Present for numeric columns; values are SI and convert through it. */
  quantity?: ElementAttributeQuantity;
  /** Number, string, or null where the element lacks the attribute. */
  values: Array<number | string | null>;
}

/** Every element of one kind with its declared properties. */
export interface KindElements {
  /** Element ids in model order — the row order every column follows. */
  ids: string[];
  columns: KindColumn[];
  /**
   * Each element's position in the model's own coordinate system, or
   * `null` for one the model places nowhere. Parallel to `ids`, and
   * empty for a kind that is not anywhere.
   *
   * Not a column, because position is not an attribute: it is implied
   * by the element's class (hydra-common §4.5.2), which is what lets a
   * table show an X and a Y for a drainage junction — whose position is
   * a line in a section the engine preserves verbatim and which appears
   * in no attribute schema.
   */
  positions: Array<[number, number] | null>;
}

const EMPTY_KIND_ELEMENTS: KindElements = {
  ids: [],
  columns: [],
  positions: [],
};

export async function getKindElements(
  projectId: string,
  scenarioId: string | null | undefined,
  kind: string,
): Promise<KindElements> {
  return tryInvokeOr<KindElements>(
    "get_kind_elements",
    { projectId, scenarioId: scenarioId ?? null, kind },
    EMPTY_KIND_ELEMENTS,
  );
}

/**
 * The contents of one collection element — a curve's points, a pattern's
 * factors, a rule's clauses.
 *
 * One shape for every container: `rows` under `columns` when the content
 * is tabular, `lines` when it is language. A consumer renders whichever
 * is non-empty.
 */
export interface CollectionDetail {
  columns: string[];
  /** The §5 quantity each column carries; `null` where dimensionless.
   * Values are SI, so this is what makes them displayable. */
  quantities: (GenericQuantity | null)[];
  rows: number[][];
  lines: string[];
}

const EMPTY_DETAIL: CollectionDetail = {
  columns: [],
  quantities: [],
  rows: [],
  lines: [],
};

export async function getCollectionDetail(
  projectId: string,
  scenarioId: string | null | undefined,
  kind: string,
  id: string,
): Promise<CollectionDetail> {
  return tryInvokeOr<CollectionDetail>(
    "get_collection_detail",
    { projectId, scenarioId: scenarioId ?? null, kind, id },
    EMPTY_DETAIL,
  );
}

/** The contents of the selected container, or empty when none is chosen. */
export function useCollectionDetail(
  projectId: string | null | undefined,
  scenarioId: string | null | undefined,
  kind: string | null,
  id: string | null,
): CollectionDetail {
  const [detail, setDetail] = useState<CollectionDetail>(EMPTY_DETAIL);
  useEffect(() => {
    if (!projectId || !kind || !id) {
      setDetail(EMPTY_DETAIL);
      return;
    }
    let cancelled = false;
    void getCollectionDetail(projectId, scenarioId, kind, id).then((d) => {
      if (!cancelled) setDetail(d);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, kind, id]);
  return detail;
}

/** How many elements each declared kind holds, keyed by kind id. */
export async function getKindCounts(
  projectId: string,
  scenarioId: string | null | undefined,
): Promise<Record<string, number>> {
  return tryInvokeOr<Record<string, number>>(
    "get_kind_counts",
    { projectId, scenarioId: scenarioId ?? null },
    {},
  );
}

/**
 * Per-kind element counts for a target.
 *
 * The editor's rail needs every kind's size at once, which the per-kind
 * fetch cannot give without one call per kind — including the collections,
 * whose contents nothing else loads.
 */
export function useKindCounts(
  projectId: string | null | undefined,
  scenarioId: string | null | undefined,
): Record<string, number> {
  const [counts, setCounts] = useState<Record<string, number>>({});
  useEffect(() => {
    if (!projectId) {
      setCounts({});
      return;
    }
    let cancelled = false;
    void getKindCounts(projectId, scenarioId).then((c) => {
      if (!cancelled) setCounts(c);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId]);
  return counts;
}

/**
 * The elements of `kind` for a target, with a way to ask again.
 *
 * `refetch` rather than a counter the caller bumps: a value that exists
 * only to re-run an effect is a trigger wearing a dependency's clothes,
 * and the effect never reads it. After a write, the caller asks for the
 * table again and redraws from what the model actually holds — not from
 * what was typed, which may have been converted or clamped on the way
 * in.
 */
export function useKindElements(
  projectId: string | null | undefined,
  scenarioId: string | null | undefined,
  kind: string | null,
): { elements: KindElements; refetch: () => void } {
  const [elements, setElements] = useState<KindElements>(EMPTY_KIND_ELEMENTS);
  const load = useCallback(() => {
    if (!projectId || !kind) {
      setElements(EMPTY_KIND_ELEMENTS);
      return () => {};
    }
    let cancelled = false;
    void getKindElements(projectId, scenarioId, kind).then((e) => {
      if (!cancelled) setElements(e);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId, kind]);
  useEffect(load, [load]);
  return { elements, refetch: load };
}
