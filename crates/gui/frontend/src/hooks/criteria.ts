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

import { useCallback, useEffect, useSyncExternalStore } from "react";
import { invoke, tryInvokeResult } from "./ipc";

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

/**
 * What a project has saved: the criteria, `null` for never having had any,
 * or `undefined` when the question could not be answered.
 *
 * Three states rather than two, because two of them are acted on in
 * opposite ways and the third had been collapsed into one of them. "None
 * saved" is what makes the canvas seed bands from the simulation options
 * and write them; "could not read" must do nothing at all. Through a plain
 * `tryInvokeOr(…, null)` both arrived as `null`, so a failed read seeded
 * defaults over criteria that were sitting on disk perfectly intact.
 */
export type CriteriaFetch = ProjectCriteria | null | undefined;

/** The project's saved criteria. See {@link CriteriaFetch} for the states. */
export async function getProjectCriteria(
  projectId: string,
): Promise<CriteriaFetch> {
  // `tryInvokeResult` rather than `tryInvokeOr`, so a command that answered
  // `null` stays distinct from one that could not be reached. Outside a
  // Tauri shell it reports unreachable too, which is the same "cannot say"
  // — seeding a project's criteria from a bare dev server would be just as
  // wrong as seeding them from a failed read.
  const read = await tryInvokeResult<ProjectCriteria | null>(
    "get_project_criteria",
    { projectId },
  );
  return read.ok ? read.value : undefined;
}

export async function updateProjectCriteria(
  projectId: string,
  criteria: ProjectCriteria,
): Promise<void> {
  await invoke("update_project_criteria", { projectId, criteria });
}

/**
 * One criteria store per project, shared by everyone reading it.
 *
 * Every project view is mounted at once — hidden with `display: none`
 * rather than unmounted — so the Analysis page and the canvas are both live
 * whichever one you are looking at. Holding this in component state gave
 * them a copy each: two fetches of the same file on open, and, once both
 * could edit, a change on one side that the other never saw. The canvas
 * went on colouring by the bands it had loaded with.
 *
 * That is the same shape as a value stored twice, and the fix is the same:
 * one store, many readers. The unit preference is kept this way already.
 */
const cache = new Map<string, ProjectCriteria>();
const savedFlags = new Map<string, boolean>();
const inFlight = new Set<string>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/**
 * Fetch once per project, however many readers ask.
 *
 * `inFlight` is what makes it once: two components mounting in the same
 * frame both find nothing cached, and without this both would ask.
 */
function ensureLoaded(projectId: string): void {
  if (cache.has(projectId) || inFlight.has(projectId)) return;
  inFlight.add(projectId);
  void getProjectCriteria(projectId).then((read) => {
    inFlight.delete(projectId);
    if (read === undefined) {
      // Unreadable. Readers fall back to the defaults as they always did,
      // but `saved` stays unknown so nothing treats this as "never had
      // any" and writes defaults over what is on disk. Not cached either,
      // so the next reader retries instead of inheriting the failure.
      notify();
      return;
    }
    cache.set(projectId, read ?? DEFAULT_CRITERIA);
    savedFlags.set(projectId, read !== null);
    notify();
  });
}

/**
 * The active project's criteria, with a setter that persists.
 *
 * Readers of the same project share one value, so an edit on one screen is
 * the same edit on every other.
 */
export function useProjectCriteria(projectId: string | null): {
  criteria: ProjectCriteria;
  setCriteria: (next: ProjectCriteria) => void;
  /** Whether this project has criteria saved on disk, or `null` for not
   * known — the fetch still in flight, or a fetch that failed. Callers that
   * seed defaults must wait on `null` rather than read it as "none saved":
   * seeding *writes*, so guessing here overwrites real criteria. */
  saved: boolean | null;
} {
  useEffect(() => {
    if (projectId) ensureLoaded(projectId);
  }, [projectId]);

  // Identity-stable while nothing changes, which is what lets a subscriber
  // skip re-rendering — the defaults are one shared object rather than a
  // fresh literal per call.
  const snapshot = useCallback(
    () =>
      projectId ? (cache.get(projectId) ?? DEFAULT_CRITERIA) : DEFAULT_CRITERIA,
    [projectId],
  );
  const criteria = useSyncExternalStore(subscribe, snapshot, snapshot);

  const savedSnapshot = useCallback(
    () => (projectId ? (savedFlags.get(projectId) ?? null) : null),
    [projectId],
  );
  const saved = useSyncExternalStore(subscribe, savedSnapshot, savedSnapshot);

  const setCriteria = useCallback(
    (next: ProjectCriteria) => {
      if (!projectId) return;
      // Applied locally first so the canvas repaints on the same frame as the
      // edit; the write is fire-and-forget because a failure loses a
      // preference, not model data.
      cache.set(projectId, next);
      savedFlags.set(projectId, true);
      notify();
      void updateProjectCriteria(projectId, next);
    },
    [projectId],
  );

  return { criteria, setCriteria, saved };
}
