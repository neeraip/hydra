/**
 * Project hooks + persistence commands (list/create/rename/delete/save),
 * CRS catalog access, and app versions.
 */

import { useEffect, useMemo, useState } from "react";
import type { UnitSystem } from "../units";
import { invoke, tryInvoke, tryInvokeOr } from "./ipc";

// ── Project types ────────────────────────────────────────────────────────────
//
// Defined here to match the backend's `commands::Project` DTO exactly.
// `useProjects` calls `list_projects` and returns live DB rows.

/**
 * The unit system a target's own model declares — what the `source`
 * preference resolves to.
 *
 * `null` until it resolves, for a project with no network yet, or for an
 * engine that declares none. Callers treat that as "fall back to SI"
 * (`resolveUnitSystem`), never as "assume US".
 */
export function useModelUnitSystem(
  projectId: string | null | undefined,
  scenarioId: string | null | undefined,
): UnitSystem | null {
  const [system, setSystem] = useState<UnitSystem | null>(null);
  useEffect(() => {
    if (!projectId) {
      setSystem(null);
      return;
    }
    let cancelled = false;
    void tryInvoke<string | null>("get_model_unit_system", {
      projectId,
      scenarioId: scenarioId ?? null,
    }).then((v) => {
      if (cancelled) return;
      setSystem(v === "si" || v === "us" ? v : null);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, scenarioId]);
  return system;
}

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
  /** Engine key from the registry (`"wds"`, …). Resolve presentation via
   * `engineByKey`; an unresolvable key renders as unsupported, never as a
   * default engine. */
  engine: string;
  scenarioCount: number;
  state: ProjectState;
  modifiedLabel: string;
  /** Last-modified time in epoch milliseconds. Absent/null on older backends. */
  modifiedAtMs?: number | null;
  /** Relative label for the last completed simulation. Absent when never run. */
  lastRunLabel?: string | null;
  /** Last completed run time in epoch milliseconds. Absent/null when never run. */
  lastRunAtMs?: number | null;
  nodeCount: number;
  linkCount: number;
  /** EPSG code for the INP [COORDINATES] CRS. Defaults to "EPSG:4326". */
  sourceCrs: string;
  /** Per-project display-unit override; absent = follow the app default. */
  unitSystem?: "source" | "si" | "us";
  insights: ProjectInsights | null;
  /** `true` when the DB row exists but the on-disk bundle folder is absent. */
  folderMissing: boolean;
}

/**
 * Whether the project has a network at all.
 *
 * A project created without importing a source model has no `model.inp` on
 * disk — the wizard's "start with an empty network" path produces exactly
 * that, and it is a normal resting state, not a broken one. Nearly
 * everything downstream (running, scenarios, settings, export, reports)
 * needs a network to act on, so each of those has to ask this question
 * before offering itself.
 *
 * Element counts are the signal because a model that parses always has at
 * least one node: parse-time validation rejects a network with no source
 * (`NoReservoir`). So "zero elements" and "no model file" coincide, and the
 * counts are already on every `Project` — no extra round-trip.
 *
 * This inference lives here, once, rather than as scattered
 * `nodeCount === 0` checks whose intent would have to be re-derived at every
 * call site.
 */
export function projectHasNetwork(project: Project | null): boolean {
  if (project === null) return false;
  return project.nodeCount > 0 || project.linkCount > 0;
}

export interface CustomCrsDef {
  label: string;
  epsg: string;
  proj4: string;
}

export interface CrsCatalogEntry {
  label: string;
  epsg: string;
  proj4: string;
  custom: boolean;
}

export interface CrsCatalogPage {
  items: CrsCatalogEntry[];
  total: number;
  page: number;
  pageSize: number;
  hasMore: boolean;
}

// ── Project hooks ──────────────────────────────────────────────────────────

// Module-level dedup for `list_projects` (a full directory scan on the Rust
// side): all `useProjects` instances mounting in the same render burst share
// one in-flight invoke, and the last resolved rows seed newly mounted hooks.
let projectsInFlight: Promise<Project[] | null> | null = null;
let lastProjects: Project[] = [];

/** Shared `list_projects` fetch — concurrent callers reuse one in-flight
 *  invoke. Exported for `useProjects` and tests; prefer the hook in UI code. */
export function fetchProjectsShared(): Promise<Project[] | null> {
  if (!projectsInFlight) {
    projectsInFlight = tryInvoke<Project[]>("list_projects")
      .then((rows) => {
        if (rows !== null) lastProjects = rows;
        return rows;
      })
      .finally(() => {
        projectsInFlight = null;
      });
  }
  return projectsInFlight;
}

// `useProjects` is the first hook to hit the real Tauri backend
export function useProjects(_version: number = 0): Project[] {
  const [projects, setProjects] = useState<Project[]>(lastProjects);

  useEffect(() => {
    // `_version` is a caller-controlled refetch counter.
    void _version;
    let cancelled = false;
    fetchProjectsShared().then((rows) => {
      if (!cancelled && rows !== null) setProjects(rows);
    });
    return () => {
      cancelled = true;
    };
  }, [_version]);

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

// ── Archive import ───────────────────────────────────────────────────────────

/**
 * One archive entry that looked like a model, described by the backend scan.
 * Mirrors `commands::ArchiveModelEntry` exactly.
 *
 * `engine` is the single definite recognition claim; `candidates` the
 * GUI-openable possibilities when no engine was definite (the user chooses);
 * `error` the reason an entry that looked importable is not.
 */
export interface ArchiveModelEntry {
  path: string;
  stem: string;
  engine: string | null;
  candidates: string[];
  nodeCount: number;
  linkCount: number;
  findingCount: number;
  /** §14.10 repairs the import will apply, one message each — surfaced
   * before the user commits; the repair contract forbids silence. */
  repairs: string[];
  /** External files the model references (rain, climate, interface):
   * carried into the project when the archive holds them, warned about
   * when it does not. */
  sidecars: SidecarRef[];
  error: string | null;
}

/** One referenced auxiliary file. Mirrors `commands::SidecarRef`. */
export interface SidecarRef {
  /** The name as the model wrote it. */
  file: string;
  /** Human label naming its role, e.g. `rain file "rain.dat"`. */
  label: string;
  /** Whether the archive holds it (matched by trailing file name). */
  carried: boolean;
  /** Whether a run can consume supplied bytes: rain, climate, hotstart,
   * and routing-inflow records can; interface-file formats and external
   * data series are declared but not served yet — named, never promised. */
  supported: boolean;
}

/** What a backend archive scan found. Mirrors `commands::ArchiveScan`. */
export interface ArchiveScan {
  archivePath: string;
  models: ArchiveModelEntry[];
  /** Every non-model entry — the likely sidecars — listed so the review can
   * say what will not be imported. */
  others: string[];
}

/** The fate of one selection. Mirrors `commands::ArchiveImportOutcome`. */
export interface ArchiveImportOutcome {
  path: string;
  name: string;
  project: Project | null;
  error: string | null;
}

/**
 * Open a native file-picker for a `.zip` of models and scan it: every entry
 * is recognised and trial-parsed exactly as a single-file import would be.
 * Returns `null` when the dialog is cancelled. Throws on an unreadable
 * archive — the caller owns the toast.
 */
export async function openAndScanArchive(): Promise<ArchiveScan | null> {
  return invoke<ArchiveScan | null>("open_and_scan_archive", {});
}

/**
 * Create one project per selected archive entry. Resolves whenever the
 * archive itself was readable; each selection's own failure comes back in
 * its outcome — partial success is reported, never rolled back.
 */
/**
 * Pick an auxiliary file on disk and attach it to the drainage model held
 * for import; `create_project` then writes it into the bundle. Returns the
 * refreshed reference list, or `null` when the dialog is cancelled. Throws
 * when the picked file is one the model never references.
 */
export async function attachAuxFile(): Promise<SidecarRef[] | null> {
  return invoke<SidecarRef[] | null>("attach_aux_file", {});
}

export async function createProjectsFromArchive(
  archivePath: string,
  selections: { path: string; name: string; engine: string }[],
): Promise<ArchiveImportOutcome[]> {
  return invoke<ArchiveImportOutcome[]>("create_projects_from_archive", {
    archivePath,
    selections,
  });
}

/**
 * Persist a new project bundle on disk via the Tauri backend.
 *
 * `importLoadedNetwork` states the caller's intent explicitly. Pass `true`
 * only when the user imported a model in this flow (which left it in managed
 * state via `openAndLoadNetwork()`); the backend then writes those bytes into
 * the bundle as the project's canonical INP. Pass `false` for an empty
 * project and the backend writes its starter model instead.
 *
 * The flag is not optional by design: managed state holds whichever network
 * was last opened and is not cleared by leaving a project, so letting the
 * backend infer the intent wrote the previous project's model into a project
 * the user asked to be empty.
 *
 * Returns the persisted manifest as a `Project`, or `null` when running
 * outside a Tauri shell so the caller can fall back to a purely in-memory
 * project.
 *
 * `engine` is persisted into the bundle and never rewritten; the backend
 * rejects a key it cannot run, so a project can never be created for an
 * engine that has no implementation.
 */
export async function createProjectOnDisk(args: {
  id: string;
  name: string;
  engine: string;
  importLoadedNetwork: boolean;
}): Promise<Project | null> {
  return tryInvokeOr<Project | null>("create_project", args, null);
}

/**
 * Permanently delete a project bundle from disk. Returns `true` when a bundle
 * was removed, `false` when the project wasn't persisted (in-memory or
 * non-Tauri).
 */
export async function deleteProjectOnDisk(id: string): Promise<boolean> {
  return tryInvokeOr<boolean>("delete_project", { id }, false);
}

/**
 * Rename a persisted project. Returns the updated manifest, or `null` when
 * the project isn't on disk.
 */
export async function renameProjectOnDisk(
  id: string,
  name: string,
): Promise<Project | null> {
  return tryInvokeOr<Project | null>("rename_project", { id, name }, null);
}

/**
 * Persist a CRS selection for a project. Returns `true` when written.
 */
export async function updateProjectCrs(
  id: string,
  crs: string,
): Promise<boolean> {
  return tryInvokeOr<boolean>("update_project_crs", { id, crs }, false);
}

/**
 * Set or clear a project's display-unit override.
 *
 * `null` clears it back to following the app-wide default — deliberately
 * distinct from pinning the value that default currently holds.
 */
export async function updateProjectUnits(
  id: string,
  unitSystem: "source" | "si" | "us" | null,
): Promise<boolean> {
  return tryInvokeOr<boolean>(
    "update_project_units",
    { id, unitSystem },
    false,
  );
}

export async function listCustomCrsDefs(): Promise<CustomCrsDef[]> {
  return tryInvokeOr<CustomCrsDef[]>("list_custom_crs", undefined, []);
}

export async function listCrsCatalogPage(params: {
  query?: string;
  page?: number;
  pageSize?: number;
}): Promise<CrsCatalogPage> {
  const payload = {
    query: params.query,
    page: params.page,
    page_size: params.pageSize,
  };
  return tryInvokeOr<CrsCatalogPage>("list_crs_catalog_page", payload, {
    items: [],
    total: 0,
    page: params.page ?? 0,
    pageSize: params.pageSize ?? 100,
    hasMore: false,
  });
}

export async function upsertCustomCrsDef(input: {
  label: string;
  epsg: string;
  proj4: string;
}): Promise<CustomCrsDef[] | null> {
  return await tryInvoke<CustomCrsDef[]>("upsert_custom_crs", input);
}

export async function deleteCustomCrsDef(
  epsg: string,
): Promise<CustomCrsDef[] | null> {
  return await tryInvoke<CustomCrsDef[]>("delete_custom_crs", { epsg });
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
  return tryInvokeOr<boolean>(
    "save_project",
    { id, scenarioId: scenarioId ?? null },
    false,
  );
}

// ── App versions ──────────────────────────────────────────────────────────

export interface Versions {
  hydra: string;
  app: string;
  /** The platform the binary was built for, as `os/arch`. Not the same
   *  question as the webview's user agent, which names the system's
   *  browser engine rather than this build. */
  platform: string;
}

export async function getVersions(): Promise<Versions> {
  return tryInvokeOr<Versions>("get_versions", undefined, {
    hydra: "0.0.0",
    app: "0.0.0",
    platform: "unknown",
  });
}
