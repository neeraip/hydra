/**
 * Per-project analysis criteria — the minimum service pressure and the
 * pressure/velocity/flow threshold bands.
 *
 * These are analysis *inputs*, not facts needed to load a project, so they
 * live in `<project>/criteria.json` beside the manifest rather than inside
 * it: a project with no criteria opens on defaults, whereas one with no
 * manifest cannot be listed at all.
 *
 * Project-scoped, not per-scenario. Criteria are the ruler and scenarios are
 * what is measured with it — per-scenario criteria would make two scenarios'
 * compliance figures incomparable, which is the reason scenarios exist.
 *
 * Values are SI (metres, m/s, L/s), matching what the canvas and the
 * analytics command already exchange.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke, tryInvokeOr } from "./ipc";

export interface RequiredBand {
  low: number;
  required: number;
  high: number;
}

export interface TargetBand {
  low: number;
  target: number;
  high: number;
}

export interface ProjectCriteria {
  version: number;
  minPressureM: number;
  pressure: RequiredBand;
  velocity: TargetBand;
  flow: TargetBand;
}

/** EPANET/AWWA-typical minimum service pressure, ~20 psi. */
export const DEFAULT_MIN_PRESSURE_M = 14;

/** Mirrors the backend defaults; used before the fetch resolves and outside
 * a Tauri shell. */
export const DEFAULT_CRITERIA: ProjectCriteria = {
  version: 1,
  minPressureM: DEFAULT_MIN_PRESSURE_M,
  pressure: { low: 24, required: 35, high: 45 },
  velocity: { low: 0.1, target: 0.5, high: 1.5 },
  flow: { low: 0.1, target: 1.0, high: 10.0 },
};

/** The project's saved criteria, or `null` when it has never had any. */
export async function getProjectCriteria(
  projectId: string,
): Promise<ProjectCriteria | null> {
  return tryInvokeOr<ProjectCriteria | null>(
    "get_project_criteria",
    { projectId },
    null,
  );
}

export async function updateProjectCriteria(
  projectId: string,
  criteria: ProjectCriteria,
): Promise<void> {
  await invoke("update_project_criteria", { projectId, criteria });
}

/**
 * The active project's criteria, with a setter that persists.
 *
 * Resets to the defaults while a new project's file is in flight, so one
 * project's criteria are never briefly applied to another's results.
 */
export function useProjectCriteria(projectId: string | null): {
  criteria: ProjectCriteria;
  setCriteria: (next: ProjectCriteria) => void;
  /** Whether this project has criteria saved on disk. `null` while the fetch
   * is in flight — callers that seed defaults must wait rather than treat
   * "not yet known" as "none saved". */
  saved: boolean | null;
} {
  const [criteria, setLocal] = useState<ProjectCriteria>(DEFAULT_CRITERIA);
  const [saved, setSaved] = useState<boolean | null>(null);

  useEffect(() => {
    if (!projectId) {
      setLocal(DEFAULT_CRITERIA);
      setSaved(null);
      return;
    }
    let cancelled = false;
    setLocal(DEFAULT_CRITERIA);
    setSaved(null);
    void getProjectCriteria(projectId).then((c) => {
      if (cancelled) return;
      setLocal(c ?? DEFAULT_CRITERIA);
      setSaved(c !== null);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const setCriteria = useCallback(
    (next: ProjectCriteria) => {
      // Applied locally first so the canvas repaints on the same frame as the
      // edit; the write is fire-and-forget because a failure loses a
      // preference, not model data.
      setLocal(next);
      setSaved(true);
      if (projectId) void updateProjectCriteria(projectId, next);
    },
    [projectId],
  );

  return { criteria, setCriteria, saved };
}
