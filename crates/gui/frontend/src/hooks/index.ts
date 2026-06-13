/**
 * Data-source seam.
 *
 * Every UI consumer reads project / network / scenario / task / time-series
 * data through this module. Hooks call real Tauri backend commands via
 * `tryInvoke` and return empty arrays/null when running outside a Tauri shell.
 *
 * Rules for callers:
 *   - Never import from `../types` or `../engines` directly — always go
 *     through this module so the seam stays in one place.
 *   - Treat returned arrays as referentially stable across renders for the
 *     same input (see the `useMemo` wrappers below).
 */

import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { invoke, isTauri, tryInvoke } from "./ipc";
import { useNetworkVersion } from "./NetworkVersionContext";

// Re-export so callers only need to import from the data seam.
export { useNetworkVersion } from "./NetworkVersionContext";

import {
  deltaColor,
  type Link,
  type Node,
  type Pattern,
  PRESSURE_MAX,
  PRESSURE_MIN,
  PRESSURE_THRESHOLD,
  pressureColor,
} from "../types";

// ── Project types ────────────────────────────────────────────────────────────
//
// Defined here to match the backend's `commands::Project` DTO exactly.
// `useProjects` calls `list_projects` and returns live DB rows.

export type ProjectState =
  | "draft"
  | "ready"
  | "simulated"
  | "running"
  | "failed"
  | "stale";

export type ProjectInsights = {
  minPressure: number;
  minPressureNode: string;
  maxVelocity: number;
  pumpEnergy: number;
  warningCount: number;
};

export interface Project {
  id: string;
  name: string;
  scenarioCount: number;
  state: ProjectState;
  modifiedLabel: string;
  /** Relative label for the last completed simulation. Absent when never run. */
  lastRunLabel?: string | null;
  nodeCount: number;
  linkCount: number;
  /** EPSG code for the INP [COORDINATES] CRS. Defaults to "EPSG:4326". */
  sourceCrs: string;
  insights: ProjectInsights | null;
  /** `true` when the DB row exists but the on-disk bundle folder is absent. */
  folderMissing: boolean;
}

// ── Public type surface ────────────────────────────────────────────────────

export type { ProjectView } from "../projectConfig";
// Engine constants and view config.
export { ACCENT, LABEL, PILL, PROJECT_VIEWS } from "../projectConfig";
export type {
  Command,
  CommandCategory,
  Link,
  LinkType,
  Node,
  NodeType,
  Pattern,
  Scenario,
  ScenarioState,
  SectionGroup,
  Task,
  TaskStatus,
} from "../types";
// Re-export pure helpers and constants — no hook needed.
export {
  deltaColor,
  PRESSURE_MAX,
  PRESSURE_MIN,
  PRESSURE_THRESHOLD,
  pressureColor,
};

import type { ProjectView } from "../projectConfig";

// ── Project hooks ──────────────────────────────────────────────────────────
//
// `useProjects` is the first hook to hit the real Tauri backend
export function useProjects(_version: number = 0): Project[] {
  const [projects, setProjects] = useState<Project[]>([]);

  useEffect(() => {
    let cancelled = false;
    tryInvoke<Project[]>("list_projects").then((rows) => {
      if (!cancelled && rows !== null) setProjects(rows);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return projects;
}

export function useProject(
  id: string | null | undefined,
  version: number = 0,
): Project | null {
  const projects = useProjects(version);
  return useMemo(
    () => projects.find((p) => p.id === id) ?? null,
    [id, projects],
  );
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

/** Open a native file-open dialog filtered to CSV/Excel. Returns the picked
 *  filename + a generated id, or null if the user cancelled. */
export async function pickCsvFile(): Promise<{
  id: string;
  filename: string;
} | null> {
  const result = await tryInvoke<{ id: string; filename: string } | null>(
    "pick_csv_file",
  );
  return result ?? null;
}

/**
 * Persist a new project bundle on disk via the Tauri backend.
 *
 * The backend captures whatever network is currently loaded in managed state
 * (from a prior `openAndLoadNetwork()`) and writes it into the bundle as the
 * project's canonical INP. Returns the persisted manifest as a `Project`,
 * or `null` when running outside a Tauri shell so the caller can fall back to
 * a purely in-memory project.
 */
export async function createProjectOnDisk(args: {
  id: string;
  name: string;
}): Promise<Project | null> {
  return (await tryInvoke<Project | null>("create_project", args)) ?? null;
}

/**
 * Result returned by `loadProject`: the manifest, plus the parsed network if
 * the bundle had one. The backend has also populated managed state with the
 * parsed network, so subsequent `useNodes()` / `useLinks()` / `runSimulation()`
 * calls operate on it.
 */
export interface LoadedProject {
  project: Project;
  network: { nodes: Node[]; links: Link[] } | null;
}

/**
 * Open a persisted project bundle. Returns `null` when the project is not on
 * disk (e.g. in-memory-only projects, or running outside a Tauri shell) so
 * the caller can fall back to in-memory project metadata.
 */
export async function loadProject(id: string): Promise<LoadedProject | null> {
  return (
    (await tryInvoke<LoadedProject | null>("load_project", { id })) ?? null
  );
}

/**
 * Permanently delete a project bundle from disk. Returns `true` when a bundle
 * was removed, `false` when the project wasn't persisted (in-memory or
 * non-Tauri).
 */
export async function deleteProjectOnDisk(id: string): Promise<boolean> {
  return (await tryInvoke<boolean>("delete_project", { id })) ?? false;
}

/**
 * Rename a persisted project. Returns the updated manifest, or `null` when
 * the project isn't on disk.
 */
export async function renameProjectOnDisk(
  id: string,
  name: string,
): Promise<Project | null> {
  return (
    (await tryInvoke<Project | null>("rename_project", { id, name })) ?? null
  );
}

/**
 * Persist a CRS selection for a project. Returns `true` when written.
 */
export async function updateProjectCrs(
  id: string,
  crs: string,
): Promise<boolean> {
  return (await tryInvoke<boolean>("update_project_crs", { id, crs })) ?? false;
}

/**
 * Persist the in-memory network (INP bytes held in `NetworkState`) back into
 * the project bundle on disk. Returns `true` when written, `false` when there
 * is no loaded network (draft project with no INP attached yet).
 */
export async function saveProjectOnDisk(
  id: string,
  scenarioId?: string | null,
): Promise<boolean> {
  return (
    (await tryInvoke<boolean>("save_project", {
      id,
      scenarioId: scenarioId ?? null,
    })) ?? false
  );
}

// ── Scenario hooks ─────────────────────────────────────────────────────────

/** Flat DTO returned by `list_scenarios` / `create_scenario`. */
export interface ScenarioDto {
  id: string;
  projectId: string;
  parentScenarioId: string | null;
  name: string;
  /** "not-run" | "simulated" | "stale" | "running" | "failed" | "queued" */
  state: string;
}

/**
 * Fetch scenarios for `projectId` from the backend (flat list). Returns `[]`
 * when `projectId` is null, running outside Tauri, or the list is empty.
 */
export function useScenarios(
  projectId: string | null,
  _version: number = 0,
): ScenarioDto[] {
  const [scenarios, setScenarios] = useState<ScenarioDto[]>([]);

  useEffect(() => {
    if (!projectId) {
      setScenarios([]);
      return;
    }
    let cancelled = false;
    tryInvoke<ScenarioDto[]>("list_scenarios", { projectId }).then((rows) => {
      if (!cancelled) setScenarios(rows ?? []);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  return scenarios;
}

/**
 * Create a new scenario on disk. `parentScenarioId` is `null` to branch from
 * the base model. Returns the new `ScenarioDto`, or `null` outside Tauri.
 */
export async function createScenarioOnDisk(args: {
  projectId: string;
  name: string;
  parentScenarioId?: string | null;
}): Promise<ScenarioDto | null> {
  return (
    (await tryInvoke<ScenarioDto>("create_scenario", {
      projectId: args.projectId,
      name: args.name,
      parentScenarioId: args.parentScenarioId ?? null,
    })) ?? null
  );
}

/**
 * Open the base model directory for `projectId` in the system file manager
 * (Finder on macOS, Explorer on Windows). No-op outside Tauri.
 */
export async function openBaseFolder(projectId: string): Promise<void> {
  await tryInvoke<void>("open_base_folder", { projectId });
}

/**
 * Open the directory for `scenarioId` in the system file manager.
 * No-op outside Tauri.
 */
export async function openScenarioFolder(
  projectId: string,
  scenarioId: string,
): Promise<void> {
  await tryInvoke<void>("open_scenario_folder", { projectId, scenarioId });
}

export async function deleteScenario(
  projectId: string,
  scenarioId: string,
): Promise<boolean> {
  return (
    (await tryInvoke<boolean>("delete_scenario", { projectId, scenarioId })) ??
    false
  );
}

export async function renameScenario(
  projectId: string,
  scenarioId: string,
  name: string,
): Promise<boolean> {
  return (
    (await tryInvoke<boolean>("rename_scenario", {
      projectId,
      scenarioId,
      name,
    })) ?? false
  );
}

/**
 * Load the INP for the base model (`scenarioId = null`) or a named scenario
 * into the backend `NetworkState`, then return the parsed network so callers
 * can bump `networkVersion` to trigger a `useNodes` / `useLinks` refetch.
 *
 * Returns `null` when the target INP does not exist on disk yet (draft project).
 */
export async function loadProjectNetwork(
  projectId: string,
  scenarioId: string | null,
): Promise<{ nodes: Node[]; links: Link[] } | null> {
  return (
    (await tryInvoke<{ nodes: Node[]; links: Link[] } | null>(
      "load_project_network",
      { projectId, scenarioId },
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

/** Create a new time pattern with 24 flat hourly multipliers (all 1.0). */
export async function createPattern(id: string): Promise<void> {
  await invoke<void>("create_pattern", { id });
}

export interface Versions {
  hydra: string;
  app: string;
}

export async function getVersions(): Promise<Versions> {
  return (
    (await tryInvoke<Versions>("get_versions")) ?? {
      hydra: "0.0.0",
      app: "0.0.0",
    }
  );
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

// ── DB / filesystem reconciliation ────────────────────────────────────────

export interface ReconcileReport {
  /** Number of orphaned on-disk folders recovered into the DB. */
  recovered: number;
  /** Project IDs in the DB whose on-disk folder is missing. */
  folderMissing: string[];
}

/**
 * Scan `<app_data>/projects/` for orphaned folders and recover them into the
 * DB. Also returns the IDs of DB rows whose folder no longer exists on disk.
 */
export async function reconcileProjects(): Promise<ReconcileReport> {
  return (
    (await tryInvoke<ReconcileReport>("reconcile_projects")) ?? {
      recovered: 0,
      folderMissing: [],
    }
  );
}

// ── Simulation results ────────────────────────────────────────────────

/** Per-pump energy accounting for the full simulation. */
export interface PumpEnergyRecord {
  id: string;
  pctOnline: number;
  avgEfficiency: number;
  avgKwhPerFlow: number;
  avgKw: number;
  peakKw: number;
}

/** Returned by `run_simulation`. Contains only pump energy. */
export interface SimulationResult {
  pumpEnergy: PumpEnergyRecord[];
}

export async function runSimulation(opts?: {
  projectId?: string;
  scenarioId?: string;
  qualityMode?: string;
  traceNode?: string;
}): Promise<SimulationResult | null> {
  return await invoke<SimulationResult | null>("run_simulation", {
    projectId: opts?.projectId ?? null,
    scenarioId: opts?.scenarioId ?? null,
    qualityMode: opts?.qualityMode ?? null,
    traceNode: opts?.traceNode ?? null,
  });
}

// ── Analytics DTOs ──────────────────────────────────────────────────────────

export interface MassBalance {
  inflowM3: number;
  outflowM3: number;
  balancePct: number;
  series: number[];
}

export interface HistogramBucket {
  lo: number;
  hi: number;
  count: number;
}

export interface TopPipe {
  id: string;
  fromId: string;
  toId: string;
  diameterMm: number;
  maxVelocityMs: number;
}

export interface TankHeadSeries {
  nodeId: string;
  head: number[];
}

export interface ResultAnalytics {
  periodCount: number;
  nodeCount: number;
  linkCount: number;
  massBalance: MassBalance;
  minPressureNodeId: string;
  minPressureM: number;
  lowPressureCount: number;
  maxVelocityLinkId: string;
  maxVelocityMs: number;
  pressureHistogram: HistogramBucket[];
  velocityHistogram: HistogramBucket[];
  topPipes: TopPipe[];
  tankSeries: TankHeadSeries[];
}

export async function getPumpEnergy(
  projectId: string,
  scenarioId?: string | null,
): Promise<PumpEnergyRecord[]> {
  return (
    (await tryInvoke<PumpEnergyRecord[]>("get_pump_energy", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? []
  );
}

export async function getResultAnalytics(
  projectId: string,
  scenarioId?: string | null,
): Promise<ResultAnalytics | null> {
  return (
    (await tryInvoke<ResultAnalytics | null>("get_result_analytics", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? null
  );
}

// ── Simulation progress events ─────────────────────────────────────────────

export const NETWORK_CHANGED_EVENT = "network-changed";

/** Subscribe to network mutation events from the backend.
 *  Fires whenever `patch_element`, `patch_node_position`, or `delete_element`
 *  succeeds.  Returns the unlisten function — call it to unsubscribe. */
export function listenNetworkChanged(cb: () => void): Promise<() => void> {
  return listen(NETWORK_CHANGED_EVENT, () => cb());
}

export const SIMULATION_PROGRESS_EVENT = "simulation_progress";

export interface SimulationProgressEvent {
  /** The run-queue item UUID; `null` for direct (non-queued) runs. */
  runId: string | null;
  /** "hydraulics" or "quality" */
  phase: string;
  simulatedSeconds: number;
  durationSeconds: number;
  percent: number;
  done: boolean;
  failed: boolean;
  message: string | null;
  /** Whether water-quality is enabled for this simulation run. */
  runQuality: boolean;
}

/** Subscribe to live simulation progress events from the backend.
 *  Returns the unlisten function — call it to unsubscribe. */
export function listenSimulationProgress(
  cb: (e: SimulationProgressEvent) => void,
): Promise<() => void> {
  return listen<SimulationProgressEvent>(SIMULATION_PROGRESS_EVENT, (ev) =>
    cb(ev.payload),
  );
}

// ── Result metadata + period results ──────────────────────────────────────────
//
// These map to the `load_result_meta` / `get_period_results` Tauri commands.
// `loadResultMeta` reads only the tiny 72-byte header + epilog from `results.out`
// and returns snapshot times with global min/max ranges — fast on any size file.
// `getPeriodResults` seeks directly to one period and returns flat SI arrays.

export interface ResultRanges {
  pressureMin: number;
  pressureMax: number;
  headMin: number;
  headMax: number;
  demandMin: number;
  demandMax: number;
  flowMin: number;
  flowMax: number;
  velocityMin: number;
  velocityMax: number;
  /** Present only when quality simulation was run. */
  qualityMin?: number;
  qualityMax?: number;
}

export interface ResultMeta {
  /** Snapshot times in seconds from the start of the simulation. */
  times: number[];
  ranges: ResultRanges;
  /** Quality mode used: `"none"` | `"chemical"` | `"age"` | `"trace"`. */
  qualityMode: string;
}

export interface PeriodResults {
  /** Node demand (L/s), one entry per node in network order. */
  nodeDemand: number[];
  /** Node hydraulic head (m), one entry per node in network order. */
  nodeHead: number[];
  /** Node gauge pressure (m), one entry per node in network order. */
  nodePressure: number[];
  /** Link flow (L/s), one entry per link in network order. */
  linkFlow: number[];
  /** Link mean velocity (m/s), one entry per link in network order. */
  linkVelocity: number[];
  /** Link head loss per unit length (or total for pumps/valves). */
  linkHeadloss: number[];
  /** Link status (0 = closed, 1 = open, etc.) */
  linkStatus: number[];
  /** Per-node quality values. Present only when quality simulation was run. */
  nodeQuality?: number[];
  /** Per-link quality values. Present only when quality simulation was run. */
  linkQuality?: number[];
}

/**
 * Return snapshot times and global result ranges for a project or scenario.
 * Reads only the header + epilog of `results.out` — never the full file.
 * Returns `null` when running outside Tauri or when no results exist yet.
 */
export async function loadResultMeta(
  projectId: string,
  scenarioId?: string | null,
): Promise<ResultMeta | null> {
  return (
    (await tryInvoke<ResultMeta | null>("load_result_meta", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? null
  );
}

/**
 * Return flat result arrays for a single reporting period.
 * Values are in SI units (L/s, m, m/s). Returns `null` outside Tauri.
 */
export async function getPeriodResults(
  projectId: string,
  period: number,
  scenarioId?: string | null,
): Promise<PeriodResults | null> {
  return (
    (await tryInvoke<PeriodResults | null>("get_period_results", {
      projectId,
      period,
      scenarioId: scenarioId ?? null,
    })) ?? null
  );
}

export function useNodes(_version = 0): Node[] {
  const { version: ctxVersion } = useNetworkVersion();
  const [nodes, setNodes] = useState<Node[]>([]);
  useEffect(() => {
    void ctxVersion;
    void _version;
    let cancelled = false;
    tryInvoke<Node[]>("get_nodes").then((rows) => {
      if (!cancelled && rows !== null) setNodes(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, _version]);
  return nodes;
}

export function useLinks(_version = 0): Link[] {
  const { version: ctxVersion } = useNetworkVersion();
  const [links, setLinks] = useState<Link[]>([]);
  useEffect(() => {
    void ctxVersion;
    void _version;
    let cancelled = false;
    tryInvoke<Link[]>("get_links").then((rows) => {
      if (!cancelled && rows !== null) setLinks(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [ctxVersion, _version]);
  return links;
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

// ── Tasks ──────────────────────────────────────────────────────────────────
// useTasks() lives in ../state (SimulationProvider). Import from there directly.

// ── Run queue ──────────────────────────────────────────────────────────────

/** Mirrors the `RunQueueItemDto` returned by the `get_run_queue` command. */
export interface RunQueueItem {
  id: string;
  projectId: string;
  /** `null` = base model; UUID string = scenario. */
  targetId: string | null;
  /** Human-readable scenario name, or `null` for the base model. */
  targetName: string | null;
  /** "queued" | "running" | "done" | "failed" | "cancelled" */
  status: string;
  queuedAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  error: string | null;
}

export const RUN_QUEUE_UPDATE_EVENT = "run_queue_update";

/** Enqueue simulation runs for `projectId`.
 *  `targets` is a list where `null` = base model and a UUID string = scenario.
 *  Returns the updated queue for `projectId`. */
export async function enqueueRuns(
  projectId: string,
  targets: (string | null)[],
): Promise<RunQueueItem[]> {
  return (
    (await tryInvoke<RunQueueItem[]>("enqueue_runs", { projectId, targets })) ??
    []
  );
}

/** Fetch the current run queue for `projectId`. */
export async function getRunQueue(projectId: string): Promise<RunQueueItem[]> {
  return (
    (await tryInvoke<RunQueueItem[]>("get_run_queue", { projectId })) ?? []
  );
}

/** Cancel all queued items and request cancellation for any currently running
 *  queue item for `projectId`. Returns number of affected items. */
export async function cancelRunQueue(projectId: string): Promise<number> {
  return (await tryInvoke<number>("cancel_run_queue", { projectId })) ?? 0;
}

/** Cancel a single queue run item by its run ID.
 *  Queued items are cancelled immediately; running items are cancelled cooperatively.
 *  Returns `true` when the item was queued or running and accepted cancellation. */
export async function cancelRunItem(runId: string): Promise<boolean> {
  return (await tryInvoke<boolean>("cancel_run_item", { runId })) ?? false;
}

// ── Simulation parameters (TIMES + OPTIONS, INP-canonical) ────────────────

/** Mirrors the backend `SimParamsDto`. PDA pressure values are in metres (SI). */
export interface SimParams {
  duration: number;
  hydStep: number;
  qualStep: number;
  patternStep: number;
  reportStep: number;
  startClocktime: number;
  statistic: "series" | "average" | "minimum" | "maximum" | "range";

  headLossFormula: "H-W" | "D-W" | "C-M";
  demandModel: "DDA" | "PDA";
  demandMultiplier: number;
  pdaMinPressure: number;
  pdaRequiredPressure: number;
  pdaPressureExponent: number;

  qualityMode: "none" | "chemical" | "age" | "trace";
  traceNode: string | null;
  chemName: string;
  chemUnits: string;

  maxIter: number;
  flowTol: number;
  headTol: number;
  dampLimit: number;
  checkFreq: number;
  maxCheck: number;
  viscosity: number;
  specificGravity: number;
}

/** Read [TIMES] + [OPTIONS] from `base/model.inp`. `null` if no base INP. */
export async function getSimParams(
  projectId: string,
): Promise<SimParams | null> {
  return (
    (await tryInvoke<SimParams | null>("get_sim_params", { projectId })) ?? null
  );
}

/** Persist new sim params: rewrites base + every scenario INP and marks every
 *  existing result stale. Returns `true` on success. */
export async function updateSimParams(
  projectId: string,
  params: SimParams,
): Promise<boolean> {
  try {
    await invoke("update_sim_params", { projectId, params });
    return true;
  } catch {
    return false;
  }
}

/**
 * React hook that tracks simulation parameters for `projectId`, re-fetching
 * whenever `networkVersion` bumps (i.e. a new network was loaded). Returns
 * `null` until the first fetch resolves or when `projectId` is absent.
 */
export function useSimParams(
  projectId: string | null | undefined,
): SimParams | null {
  const { version: networkVersion } = useNetworkVersion();
  const [params, setParams] = useState<SimParams | null>(null);
  useEffect(() => {
    void networkVersion;
    if (!projectId) {
      setParams(null);
      return;
    }
    let cancelled = false;
    getSimParams(projectId).then((p) => {
      if (!cancelled) setParams(p);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, networkVersion]);
  return params;
}

/** Subscribe to `run_queue_update` events from the backend.
 *  The payload is the `project_id` whose queue changed.
 *  Returns the unlisten function. */
export function listenRunQueueUpdate(
  cb: (projectId: string) => void,
): Promise<() => void> {
  return listen<string>(RUN_QUEUE_UPDATE_EVENT, (ev) => cb(ev.payload));
}

// ── Editor rows ────────────────────────────────────────────────────────────

export interface JunctionRow {
  id: string;
  /** Elevation in metres. */
  elevation: number;
  /** Base demand in L/s. */
  baseDemand: number;
  demand: number;
  /** Gauge pressure in metres, or null when no simulation results are available. */
  pressure: number | null;
  x: number;
  y: number;
  /** True only when a simulation result exists and pressure is below threshold. */
  belowThreshold: boolean;
}

export interface PipeRow {
  id: string;
  from: string;
  to: string;
  length: number;
  diameter: number;
  roughness: number;
  velocity: number;
  highVelocity: boolean;
}

export interface PumpRow {
  id: string;
  from: string;
  to: string;
  /** Head-flow curve ID; undefined for constant-power pumps. */
  curve: string | null;
  /** Rated power in kW; defined only for constant-power pumps. */
  powerKw: number | null;
  /** Initial relative speed (1.0 = rated). */
  speed: number;
  /** Post-simulation velocity in m/s; 0 before first run. */
  velocity: number;
}

export interface TankRow {
  id: string;
  elevation: number;
  minLevel: number;
  maxLevel: number;
  initialLevel: number;
  /** Tank diameter in m; null for volume-curve tanks. */
  diameter: number | null;
  /** Volume curve ID; null for cylindrical tanks. */
  volumeCurve: string | null;
  x: number;
  y: number;
}

export interface ReservoirRow {
  id: string;
  /** Fixed hydraulic head in m (= elevation for a simple reservoir). */
  head: number;
  /** Head pattern ID; null when unset. */
  pattern: string | null;
  x: number;
  y: number;
}

export interface ValveRow {
  id: string;
  from: string;
  to: string;
  /** "PRV" | "PSV" | "FCV" | "TCV" | "GPV" | "PBV" | "PCV" */
  valveType: string;
  /** Nominal diameter in mm. */
  diameter: number;
  /**
   * Setting in display units: m for PRV/PSV/PBV, L/s for FCV, dimensionless
   * for TCV.  `null` for GPV/PCV (curve-based).
   */
  setting: number | null;
  /** Curve ID for GPV/PCV valve types; null otherwise. */
  curve: string | null;
  /** Post-simulation velocity in m/s; 0 before first run. */
  velocity: number;
}

export function useJunctionRows(): JunctionRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "junction")
        .map((n) => ({
          id: n.id,
          elevation: Math.round((n.elevation ?? 0) * 100) / 100,
          baseDemand: Math.round((n.baseDemand ?? 0) * 100) / 100,
          demand: n.demand ?? 0,
          pressure:
            n.pressure !== null ? Math.round(n.pressure * 10) / 10 : null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
          belowThreshold:
            n.pressure !== null && n.pressure < PRESSURE_THRESHOLD,
        })),
    [nodes],
  );
}

export function usePipeRows(): PipeRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "pipe")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          length: Math.round((l.length ?? 0) * 10) / 10,
          diameter: l.diameter,
          roughness: l.roughness ?? 0,
          velocity: l.velocity,
          highVelocity: l.velocity > 1.0,
        })),
    [links],
  );
}

export function usePumpRows(): PumpRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "pump")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          curve: l.pumpCurve ?? null,
          powerKw: l.pumpPowerKw ?? null,
          speed: l.pumpSpeed ?? 1.0,
          velocity: l.velocity,
        })),
    [links],
  );
}

export function useTankRows(): TankRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "tank")
        .map((n) => ({
          id: n.id,
          elevation: Math.round((n.elevation ?? 0) * 100) / 100,
          minLevel: Math.round((n.tankMinLevel ?? 0) * 100) / 100,
          maxLevel: Math.round((n.tankMaxLevel ?? 0) * 100) / 100,
          initialLevel: Math.round((n.tankInitialLevel ?? 0) * 100) / 100,
          diameter:
            n.tankDiameter != null
              ? Math.round(n.tankDiameter * 100) / 100
              : null,
          volumeCurve: n.tankVolumeCurve ?? null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
        })),
    [nodes],
  );
}

export function useReservoirRows(): ReservoirRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "reservoir")
        .map((n) => ({
          id: n.id,
          head: Math.round((n.elevation ?? 0) * 100) / 100,
          pattern: n.headPattern ?? null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
        })),
    [nodes],
  );
}

export function useValveRows(): ValveRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "valve")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          valveType: l.valveType ?? "PRV",
          diameter: Math.round(l.diameter * 10) / 10,
          setting:
            l.valveSetting != null
              ? Math.round(l.valveSetting * 1000) / 1000
              : null,
          curve: l.valveCurve ?? null,
          velocity: l.velocity,
        })),
    [links],
  );
}

export function useEditorSections() {
  // Intentionally empty — EditorView derives its own sections inline.
  return [] as never[];
}

// ── Editor types ────────────────────────────────────────────────────────────

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
export type ControlMode = "simple" | "rule";
export interface SimpleControl {
  id: string;
  mode: "simple";
  text: string;
  enabled: boolean;
}
export interface RuleControl {
  id: string;
  mode: "rule";
  name: string;
  priority: number;
  ifClause: string;
  thenClause: string;
  elseClause?: string;
  enabled: boolean;
}
export type ControlEntry = SimpleControl | RuleControl;
export interface XSStation {
  station: number;
  elev: number;
  manning?: number;
}
export interface CrossSection {
  id: string;
  reach: string;
  riverStation: number;
  description: string;
  bankLeft: number;
  bankRight: number;
  manningChannel: number;
  manningOverbankL: number;
  manningOverbankR: number;
  ineffective?: { left: number; right: number; elevation: number };
  points: XSStation[];
}
export interface Subcatchment {
  id: string;
  area: number;
  imperv: number;
  width: number;
  slope: number;
  manningImp: number;
  manningPerv: number;
  outletNode: string;
  rainGage: string;
  peakRunoff: number;
}

// ── Issue types ─────────────────────────────────────────────────────────────

export type IssueSeverity = "error" | "warn" | "info";
export type IssueSource = "preflight" | "runtime" | "quality" | "data";
export interface Issue {
  id: string;
  severity: IssueSeverity;
  source: IssueSource;
  title: string;
  detail: string;
  code?: string;
  link?: { view: ProjectView; assetId?: string; label?: string };
  firstSeen: string;
  dismissed: boolean;
}
export interface IssueCounts {
  error: number;
  warn: number;
  info: number;
  total: number;
}
export function countIssues(
  issues: Issue[],
  includeDismissed = false,
): IssueCounts {
  const list = includeDismissed ? issues : issues.filter((i) => !i.dismissed);
  return {
    error: list.filter((i) => i.severity === "error").length,
    warn: list.filter((i) => i.severity === "warn").length,
    info: list.filter((i) => i.severity === "info").length,
    total: list.length,
  };
}

// ── Exhibit types (static UI config) ────────────────────────────────────────

export type ExhibitTheme =
  | "pressure"
  | "velocity"
  | "pipe-age"
  | "fire-flow"
  | "calibration-rmse"
  | "demand";
export type ExhibitStyle = "choropleth" | "graduated" | "dot" | "heatmap";
export type ExhibitScope = "whole" | "selection" | "south-side" | "north-feed";

export interface ThemeSpec {
  id: ExhibitTheme;
  label: string;
  unit: string;
  stops: { v: number; color: string; label?: string }[];
  defaultNode: number;
  nodeValue: (n: Node) => number | null;
  linkValue: (l: Link) => number | null;
  narrative: string;
}
export interface ExhibitSpec {
  id: string;
  title: string;
  caption: string;
  theme: ExhibitTheme;
  style: ExhibitStyle;
  scope: ExhibitScope;
  showLegend: boolean;
  showScale: boolean;
  showNorth: boolean;
  callouts: { id: string; nodeId: string; text: string }[];
  sectionId: string | null;
}

export const STYLE_SPECS: { id: ExhibitStyle; label: string; desc: string }[] =
  [
    {
      id: "choropleth",
      label: "Choropleth",
      desc: "Solid color fill on nodes or pipes by class.",
    },
    {
      id: "graduated",
      label: "Graduated",
      desc: "Line/circle thickness scaled to value.",
    },
    {
      id: "dot",
      label: "Dot density",
      desc: "Proportional dots, best for demand and counts.",
    },
    {
      id: "heatmap",
      label: "Heatmap",
      desc: "Smooth radial blend, best for coverage themes.",
    },
  ];
export const SCOPE_SPECS: { id: ExhibitScope; label: string; desc: string }[] =
  [
    { id: "whole", label: "Whole network", desc: "All junctions and pipes." },
    {
      id: "selection",
      label: "Current selection",
      desc: "Items currently selected on the canvas.",
    },
    {
      id: "south-side",
      label: "South Side district",
      desc: "DMA covering rows 2 and 3.",
    },
    {
      id: "north-feed",
      label: "North feed",
      desc: "Pumped supply from reservoir into tank.",
    },
  ];

function _h(id: string, salt = 0): number {
  let x = salt;
  for (let i = 0; i < id.length; i++) x = ((x << 5) - x + id.charCodeAt(i)) | 0;
  return Math.abs(x);
}

export const THEMES: Record<ExhibitTheme, ThemeSpec> = {
  pressure: {
    id: "pressure",
    label: "Pressure",
    unit: "m",
    stops: [
      { v: 20, color: "#c94040", label: "< 24" },
      { v: 30, color: "#d4a017", label: "24–35" },
      { v: 40, color: "#3daf75", label: "35–45" },
      { v: 50, color: "#4a90d9", label: "> 45" },
    ],
    defaultNode: 35,
    nodeValue: (n) => n.pressure,
    linkValue: () => null,
    narrative: "Junction pressure at peak demand hour (HH 08:00).",
  },
  velocity: {
    id: "velocity",
    label: "Velocity",
    unit: "m/s",
    stops: [
      { v: 0.0, color: "#666", label: "stagnant" },
      { v: 0.3, color: "#4a90d9", label: "0.3" },
      { v: 1.0, color: "#3daf75", label: "1.0" },
      { v: 1.8, color: "#d4a017", label: "1.8" },
      { v: 2.5, color: "#c94040", label: "> 2.5" },
    ],
    defaultNode: 0,
    nodeValue: () => null,
    linkValue: (l) => l.velocity,
    narrative:
      "Pipe velocity at peak demand. Stagnant pipes flagged for flushing.",
  },
  "pipe-age": {
    id: "pipe-age",
    label: "Pipe age",
    unit: "yrs",
    stops: [
      { v: 0, color: "#3daf75", label: "< 20" },
      { v: 30, color: "#d4a017", label: "20–50" },
      { v: 60, color: "#c94040", label: "> 60" },
    ],
    defaultNode: 0,
    nodeValue: () => null,
    linkValue: (l) => (_h(l.id, 17) % 80) + 5,
    narrative:
      "Estimated pipe age from asset register. Highlights candidates for renewal.",
  },
  "fire-flow": {
    id: "fire-flow",
    label: "Fire-flow availability",
    unit: "L/s",
    stops: [
      { v: 0, color: "#c94040", label: "< 15" },
      { v: 25, color: "#d4a017", label: "15–30" },
      { v: 35, color: "#3daf75", label: "> 30" },
    ],
    defaultNode: 25,
    nodeValue: (n) => 8 + (_h(n.id, 91) % 35),
    linkValue: () => null,
    narrative: "Available fire-flow at 14 m residual pressure per AS 2419.1.",
  },
  "calibration-rmse": {
    id: "calibration-rmse",
    label: "Calibration RMSE",
    unit: "m",
    stops: [
      { v: 0, color: "#3daf75", label: "< 1" },
      { v: 2, color: "#d4a017", label: "1–3" },
      { v: 4, color: "#c94040", label: "> 3" },
    ],
    defaultNode: 0,
    nodeValue: (n) => (_h(n.id, 7) % 50) / 10,
    linkValue: () => null,
    narrative: "Per-node RMSE between simulated and observed pressure traces.",
  },
  demand: {
    id: "demand",
    label: "Demand",
    unit: "L/s",
    stops: [
      { v: 0, color: "#1a3a5c", label: "0" },
      { v: 3, color: "#4a90d9", label: "3" },
      { v: 6, color: "#9bc8ec", label: "6+" },
    ],
    defaultNode: 0,
    nodeValue: (n) => n.demand ?? 0,
    linkValue: () => null,
    narrative:
      "Junction base demand. Larger circles = higher demand allocation.",
  },
};

export function defaultExhibit(theme: ExhibitTheme): ExhibitSpec {
  const t = THEMES[theme];
  return {
    id: `EX-${Date.now().toString(36)}`,
    title: `${t.label}: Peak Hour`,
    caption: t.narrative,
    theme,
    style:
      theme === "demand"
        ? "dot"
        : theme === "velocity" || theme === "pipe-age"
          ? "graduated"
          : "choropleth",
    scope: "whole",
    showLegend: true,
    showScale: true,
    showNorth: true,
    callouts: [],
    sectionId: null,
  };
}
