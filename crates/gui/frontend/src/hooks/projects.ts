/**
 * Project hooks + persistence commands (list/create/rename/delete/save),
 * CRS catalog access, DB/filesystem reconciliation, and app versions.
 */

import { useEffect, useMemo, useState } from "react";
import { tryInvoke, tryInvokeOr } from "./ipc";

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
  insights: ProjectInsights | null;
  /** `true` when the DB row exists but the on-disk bundle folder is absent. */
  folderMissing: boolean;
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
  return tryInvokeOr<ReconcileReport>("reconcile_projects", undefined, {
    recovered: 0,
    folderMissing: [],
  });
}

// ── App versions ──────────────────────────────────────────────────────────

export interface Versions {
  hydra: string;
  app: string;
}

export async function getVersions(): Promise<Versions> {
  return tryInvokeOr<Versions>("get_versions", undefined, {
    hydra: "0.0.0",
    app: "0.0.0",
  });
}
