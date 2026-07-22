/**
 * Network model hooks and mutation commands: nodes/links/patterns/curves,
 * element patching, controls & rules, and the diff-preview seam.
 */

import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { Link, Node, Pattern } from "../types";
import { invoke, isTauri, tryInvoke } from "./ipc";
import type { NetworkSummary } from "./NetworkDataContext";
import { useNetworkData } from "./NetworkDataContext";
import { useNetworkVersion } from "./NetworkVersionContext";

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
 * Returns a nodes+links snapshot when loaded, or `null` when the target INP
 * does not exist yet.
 */
export async function loadProjectNetwork(
  projectId: string,
  scenarioId: string | null,
): Promise<{ nodes: Node[]; links: Link[] } | null> {
  return (
    (await tryInvoke<{ nodes: Node[]; links: Link[] } | null>(
      "load_project_network",
      {
        projectId,
        scenarioId,
      },
    )) ?? null
  );
}

/**
 * Apply a single field change to the in-memory network, re-serialise the INP,
 * and update `NetworkState`.  Returns the updated node/link arrays on success,
 * or `null` when running outside a Tauri shell.
 *
 * `kind`  — `"junction"` | `"reservoir"` | `"tank"` | `"pipe"` | `"pump"`
 * `id`    — element ID as shown in the editor table
 * `field` — camelCase field name (e.g. `"elevation"`, `"diameter"`, `"speed"`)
 * `value` — new value in display units (metres, L/s, mm, or a status string)
 */
export async function patchElement(
  kind: string,
  id: string,
  field: string,
  value: number | string,
): Promise<{ nodes: Node[]; links: Link[] }> {
  return invoke<{ nodes: Node[]; links: Link[] }>("patch_element", {
    kind,
    id,
    field,
    value,
  });
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

/**
 * Return the on-disk INP text for the base model of a project.
 * Used by the diff preview dialog.
 */
export async function getProjectInp(projectId: string): Promise<string | null> {
  return (await tryInvoke<string>("get_project_inp", { projectId })) ?? null;
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
  return (await tryInvoke<string>("preview_patches", { patches })) ?? null;
}

// ── Network change events ──────────────────────────────────────────────────

export const NETWORK_CHANGED_EVENT = "network-changed";

/** Subscribe to network mutation events from the backend.
 *  Fires whenever `patch_element`, `patch_node_position`, or `delete_element`
 *  succeeds.  Returns the unlisten function — call it to unsubscribe. */
export function listenNetworkChanged(cb: () => void): Promise<() => void> {
  return listen(NETWORK_CHANGED_EVENT, () => cb());
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

export function usePatterns(_version = 0): Pattern[] {
  const { version: ctxVersion } = useNetworkVersion();
  const [patterns, setPatterns] = useState<Pattern[]>([]);
  useEffect(() => {
    void ctxVersion;
    void _version;
    let cancelled = false;
    tryInvoke<Pattern[]>("get_patterns").then((rows) => {
      if (!cancelled && rows !== null) setPatterns(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, _version]);
  return patterns;
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
  const { version: ctxVersion } = useNetworkVersion();
  const [dtos, setDtos] = useState<NetworkCurveDto[]>([]);
  const links = useLinks(version);
  useEffect(() => {
    void ctxVersion;
    void version;
    let cancelled = false;
    tryInvoke<NetworkCurveDto[]>("get_curves").then((rows) => {
      if (!cancelled && rows !== null) setDtos(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, version]);

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

export function useNode(id: string | null | undefined) {
  const nodes = useNodes();
  return useMemo(() => nodes.find((n) => n.id === id) ?? null, [id, nodes]);
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
  const { version: ctxVersion } = useNetworkVersion();
  const [controls, setControls] = useState<SimpleControlDto[]>([]);
  useEffect(() => {
    void ctxVersion;
    void version;
    let cancelled = false;
    tryInvoke<SimpleControlDto[]>("get_controls").then((rows) => {
      if (!cancelled && rows !== null) setControls(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, version]);
  return controls;
}

export function useRules(version = 0): RuleDto[] {
  const { version: ctxVersion } = useNetworkVersion();
  const [rules, setRules] = useState<RuleDto[]>([]);
  useEffect(() => {
    void ctxVersion;
    void version;
    let cancelled = false;
    tryInvoke<RuleDto[]>("get_rules").then((rows) => {
      if (!cancelled && rows !== null) setRules(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, version]);
  return rules;
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
