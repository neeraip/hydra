/**
 * What a project's row says about the state of its work.
 *
 * The home page used to show a name and a date. Both are true and neither
 * answers the question a reader actually has on opening the app, which is
 * whether the results in a project still describe the model in it. A
 * project edited since its last run will open showing numbers computed
 * from a network that no longer exists, and until now nothing said so
 * until after it was opened.
 *
 * So the row says it in words rather than through a coloured dot. A dot
 * needs a legend, and there is no room for a legend on a row.
 */

import type { Project, ProjectState } from "../../hooks";

/** How much attention a status deserves. */
export type StatusTone =
  /** Nothing to act on. */
  | "quiet"
  /** Worth reading before opening, but not wrong. */
  | "attention"
  /** Something failed. */
  | "alarm"
  /** Work is happening now. */
  | "busy";

export interface ProjectStatus {
  label: string;
  tone: StatusTone;
}

const BY_STATE: Record<ProjectState, ProjectStatus> = {
  // Never run. Not a problem, just the honest state of a new project.
  draft: { label: "Never run", tone: "quiet" },
  ready: { label: "Never run", tone: "quiet" },
  // The case this whole function exists for.
  stale: { label: "Edited since last run", tone: "attention" },
  running: { label: "Running now", tone: "busy" },
  failed: { label: "Last run failed", tone: "alarm" },
  simulated: { label: "Results ready", tone: "quiet" },
};

/**
 * The status line for a project.
 *
 * `simulated` is the one state that carries a time, because "results
 * ready" invites the question "from when" and the answer is already on
 * the record. The others do not: a draft has no run to date, and a stale
 * project's last run is the misleading number, not the useful one.
 */
export function projectStatus(
  project: Pick<Project, "state" | "lastRunLabel">,
): ProjectStatus {
  const base = BY_STATE[project.state];
  if (project.state === "simulated" && project.lastRunLabel) {
    return { ...base, label: `Results from ${project.lastRunLabel}` };
  }
  return base;
}

/**
 * The size of a model, for recognising which one this is.
 *
 * Engineers know their models by scale before they know them by name,
 * and the counts are already on the record. Thousands are separated
 * because six digits unbroken are a number nobody reads.
 */
export function modelSize(
  project: Pick<Project, "nodeCount" | "linkCount">,
): string | null {
  const { nodeCount, linkCount } = project;
  if (!nodeCount && !linkCount) return null;
  const n = nodeCount.toLocaleString();
  const l = linkCount.toLocaleString();
  return `${n} ${nodeCount === 1 ? "node" : "nodes"}, ${l} ${
    linkCount === 1 ? "link" : "links"
  }`;
}
