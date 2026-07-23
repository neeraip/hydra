/**
 * Network model hooks and mutation commands: nodes/links/patterns/curves,
 * element patching, controls & rules, and the diff-preview seam.
 */

import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { Link, LinkType, Node, NodeType, Pattern } from "../types";
import { invoke, isTauri, tryInvoke, tryInvokeOr } from "./ipc";
import type { NetworkSummary } from "./NetworkDataContext";
import { useNetworkData } from "./NetworkDataContext";
import { useNetworkVersion } from "./NetworkVersionContext";

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
const SNAPSHOT_VERSION = 3;
const SNAPSHOT_FLAG_PRESENT = 1;

// Canvas-facing fields carried on Link beyond the backend DTO baseline:
// `vertices` is decoded from the v2 snapshot (intermediate polyline points in
// the source CRS, exclusive of the endpoints); `headloss` is merged per
// reporting period by the canvas from PeriodResults.linkHeadloss. Both are
// optional, so every existing Link consumer keeps compiling untouched.
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
): { nodes: Node[]; links: Link[] } | null {
  if (buf.byteLength < SNAPSHOT_HEADER_BYTES) {
    throw snapshotError(`buffer too short (${buf.byteLength} bytes)`);
  }
  const view = new DataView(buf);
  const version = view.getUint32(0, true);
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

  return { nodes, links };
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

export async function openAndLoadNetwork(): Promise<{
  nodes: Node[];
  links: Link[];
  fileStem: string;
} | null> {
  if (!isTauri()) return null;
  return await invoke<{
    nodes: Node[];
    links: Link[];
    fileStem: string;
  } | null>("open_and_load_network");
}

/** Convert backend/Tauri import errors into concise toast-safe text. */
export function formatInpImportError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err ?? "");
  const normalized = raw.replace(/\r\n/g, "\n").trim();
  if (!normalized) return "Could not import INP file.";

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

  if (!detail) return "Could not import INP file.";

  const maxLen = 220;
  if (detail.length > maxLen) {
    detail = `${detail.slice(0, maxLen - 1).trimEnd()}...`;
  }

  return `Could not import INP file: ${detail}`;
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
): Promise<{ nodes: Node[]; links: Link[] } | null> {
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

/**
 * Delete a node or link from the in-memory network.
 * `kind` must be one of: "junction", "reservoir", "tank", "pipe", "pump", "valve".
 * Deleting a node also removes all links that referenced it.
 */
export async function deleteElement(kind: string, id: string): Promise<void> {
  await tryInvoke<void>("delete_element", { kind, id });
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

export interface CurvePoint {
  flow: number; // L/s
  head: number; // m
}
export interface PumpCurve {
  id: string;
  pumpId: string;
  curveType: "single-point" | "three-point" | "multi-point";
  points: CurvePoint[];
  bep?: number;
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
        flow: x,
        head: d.y[i] ?? 0,
      }));
      const curveType: PumpCurve["curveType"] =
        points.length === 1
          ? "single-point"
          : points.length === 3
            ? "three-point"
            : "multi-point";
      return {
        id: d.id,
        pumpId: pumpByCurveId.get(d.id) ?? "",
        curveType,
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
