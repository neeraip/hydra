/**
 * Run-queue commands + events (enqueue, fetch, cancel).
 */

import { listen } from "@tauri-apps/api/event";
import { invoke, tryInvokeOr } from "./ipc";

// ── Tasks ──────────────────────────────────────────────────────────────────
// useTasks() lives in ../AppContext (SimulationProvider). Import from there
// directly.

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
  /** This item continues an interrupted run rather than starting one. */
  resume: boolean;
  /** This target has an interrupted run that can be continued. */
  resumable: boolean;
  queuedAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  error: string | null;
}

const RUN_QUEUE_UPDATE_EVENT = "run_queue_update";

/** Enqueue simulation runs for `projectId`.
 *  `targets` is a list where `null` = base model and a UUID string = scenario.
 *  Returns the updated queue for `projectId`. Rejects on backend errors (and
 *  outside a Tauri shell) so callers can surface the failure to the user. */
export async function enqueueRuns(
  projectId: string,
  targets: (string | null)[],
): Promise<RunQueueItem[]> {
  return invoke<RunQueueItem[]>("enqueue_runs", { projectId, targets });
}

/** Fetch the current run queue for `projectId`. */
export async function getRunQueue(projectId: string): Promise<RunQueueItem[]> {
  return tryInvokeOr<RunQueueItem[]>("get_run_queue", { projectId }, []);
}

/** Cancel all queued items and request cancellation for any currently running
 *  queue item for `projectId`. Returns number of affected items. Rejects on
 *  backend errors so callers can surface the failure to the user. */
export async function cancelRunQueue(projectId: string): Promise<number> {
  return invoke<number>("cancel_run_queue", { projectId });
}

/** Cancel a single queue run item by its run ID.
 *  Queued items are cancelled immediately; running items are cancelled cooperatively.
 *  Returns `true` when the item was queued or running and accepted cancellation. */
export async function cancelRunItem(runId: string): Promise<boolean> {
  return tryInvokeOr<boolean>("cancel_run_item", { runId }, false);
}

/** Queue a run that continues this target's interrupted run rather than
 *  starting the model again. Rejects when there is nothing to continue, so
 *  the caller can say so instead of silently starting an ordinary run. */
export async function resumeRun(
  projectId: string,
  targetId: string | null,
): Promise<RunQueueItem[]> {
  return invoke<RunQueueItem[]>("resume_run", { projectId, targetId });
}

/** Which of a project's targets have an interrupted run to continue.
 *
 *  Derived from the queue rather than asked for separately, because a
 *  checkpoint only exists where a run of this session was cancelled, and
 *  the queue is where that is recorded. `null` in the returned set is the
 *  base model, matching `targetId`.
 *
 *  A target that was cancelled and then run again to completion is not
 *  resumable: the later item carries `resumable: false`, and the newest
 *  item for a target is the one that decides. */
export function resumableTargets(items: RunQueueItem[]): Set<string | null> {
  const newest = new Map<string | null, RunQueueItem>();
  for (const item of items) {
    const seen = newest.get(item.targetId);
    if (!seen || item.queuedAt >= seen.queuedAt) {
      newest.set(item.targetId, item);
    }
  }
  const out = new Set<string | null>();
  for (const [targetId, item] of newest) {
    if (item.resumable) out.add(targetId);
  }
  return out;
}

/** Subscribe to `run_queue_update` events from the backend.
 *  The payload is the `project_id` whose queue changed.
 *  Returns the unlisten function. */
export function listenRunQueueUpdate(
  cb: (projectId: string) => void,
): Promise<() => void> {
  return listen<string>(RUN_QUEUE_UPDATE_EVENT, (ev) => cb(ev.payload));
}
