/**
 * Scenario hooks + persistence commands (list/create/rename/delete, folders).
 */

import { useEffect, useState } from "react";
import { invoke, tryInvoke, tryInvokeOr } from "./ipc";

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
  version: number = 0,
): ScenarioDto[] {
  const [scenarios, setScenarios] = useState<ScenarioDto[]>([]);

  useEffect(() => {
    // `version` is a caller-controlled refetch counter.
    void version;
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
  }, [projectId, version]);

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
  return tryInvokeOr<ScenarioDto | null>(
    "create_scenario",
    {
      projectId: args.projectId,
      name: args.name,
      parentScenarioId: args.parentScenarioId ?? null,
    },
    null,
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
  return tryInvokeOr<boolean>(
    "delete_scenario",
    { projectId, scenarioId },
    false,
  );
}

export async function renameScenario(
  projectId: string,
  scenarioId: string,
  name: string,
): Promise<boolean> {
  return tryInvokeOr<boolean>(
    "rename_scenario",
    { projectId, scenarioId, name },
    false,
  );
}

/**
 * Delete a target's simulation results, returning it to "not-run".
 *
 * Resolves `true` when results were removed, `false` when the target had
 * none. Rejects with the backend message on failure — most usefully when a
 * simulation is currently writing to this target, which the backend refuses
 * rather than pulling the file out from under it.
 *
 * `scenarioId: null` addresses the base model, matching every other
 * target-addressed command.
 */
export async function deleteSimulation(
  projectId: string,
  scenarioId: string | null,
): Promise<boolean> {
  return await invoke<boolean>("delete_simulation", { projectId, scenarioId });
}

/**
 * Delete every simulation result in a project — base model and all
 * scenarios. Resolves to the number of results files removed.
 *
 * All-or-nothing: the backend takes every target's run lock before touching
 * a file, so a project with a simulation in flight rejects the whole call
 * rather than clearing some targets and stopping at the busy one.
 */
export async function deleteAllSimulations(projectId: string): Promise<number> {
  return await invoke<number>("delete_all_simulations", { projectId });
}

/** Run-artifact sizes for every target in a project, in bytes. */
export interface ProjectResultsSizes {
  base: number;
  /** Scenario id → bytes. */
  scenarios: Record<string, number>;
  /** Base plus every scenario. */
  total: number;
}

/**
 * Sizes for all of a project's targets in one call.
 *
 * Batched because the scenarios panel labels a clear action per row; the work
 * is two `stat` calls per target, so this is metadata only — a 650 MB result
 * costs the same to size as an empty one.
 */
export async function projectResultsSizes(
  projectId: string,
): Promise<ProjectResultsSizes> {
  return tryInvokeOr<ProjectResultsSizes>(
    "project_results_sizes",
    { projectId },
    { base: 0, scenarios: {}, total: 0 },
  );
}

/** Bytes a clear across several projects would reclaim — one call for the
 * whole selection, which can be arbitrarily large. */
export async function projectsResultsSize(
  projectIds: string[],
): Promise<number> {
  return tryInvokeOr<number>("projects_results_size", { projectIds }, 0);
}
