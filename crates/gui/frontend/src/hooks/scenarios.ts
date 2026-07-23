/**
 * Scenario hooks + persistence commands (list/create/rename/delete, folders).
 */

import { useEffect, useState } from "react";
import { tryInvoke, tryInvokeOr } from "./ipc";

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
