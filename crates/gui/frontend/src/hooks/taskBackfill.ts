/**
 * Backfilling task rows from the run queue.
 *
 * A `simulation_progress` event names only a run id, so when progress
 * arrives before `run_queue_update` has created the task entry, the tray
 * synthesises a placeholder with neither names nor identity. The queue
 * update then has to fill both in.
 *
 * Identity is the half that matters beyond cosmetics: a settled row's "View
 * results" needs the project and target to navigate to, and a row missing
 * them is a button that does nothing.
 */

import type { Task } from "../types/task";

/** The fields a run-queue item contributes to a task row. */
export interface QueueItemFacts {
  projectId: string;
  /** `null` = base model. */
  targetId: string | null;
  projectName: string;
  targetName: string | null;
}

const PLACEHOLDER = "…";

/**
 * Whether `task` is still missing anything the queue can supply.
 *
 * Deliberately checks identity separately from names. Keying the backfill on
 * the names alone meant a task whose names had already been patched was
 * never revisited, so a placeholder that gained a name but never a
 * `projectId` kept a dead "View results" for the rest of the session.
 */
export function taskNeedsBackfill(task: Task): boolean {
  return (
    task.projectId === undefined ||
    task.projectName === PLACEHOLDER ||
    task.scenarioName === PLACEHOLDER
  );
}

/**
 * Fill in whatever `task` is missing from `facts`, leaving everything it
 * already has untouched — a live row's real values must never be clobbered
 * by a later queue snapshot.
 */
export function backfillTask(task: Task, facts: QueueItemFacts): Task {
  if (!taskNeedsBackfill(task)) return task;
  return {
    ...task,
    projectId: task.projectId ?? facts.projectId,
    // `null` is a meaningful scenarioId (the base model), so only `undefined`
    // counts as absent here.
    scenarioId:
      task.scenarioId === undefined ? facts.targetId : task.scenarioId,
    projectName:
      task.projectName === PLACEHOLDER ? facts.projectName : task.projectName,
    scenarioName:
      task.scenarioName === PLACEHOLDER
        ? (facts.targetName ?? "Base Model")
        : task.scenarioName,
  };
}
