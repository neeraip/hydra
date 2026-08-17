/**
 * Issue types + counting helpers for the Issues panel, plus the
 * `validate_network` / `get_run_warnings` fetchers and their finding→Issue
 * mappers.
 */

import type { ProjectView } from "../projectConfig";
import { tryInvokeOr } from "./ipc";

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
}
/**
 * Fold a freshly derived issue list onto the one already on screen,
 * keeping each surviving issue's original `firstSeen`.
 *
 * The list is re-derived from scratch whenever anything it watches
 * changes — a run finishes, the model is edited, validation reruns — and
 * every derivation stamps `firstSeen` as *now*. Without this merge an
 * issue that has been present for an hour would report itself as new on
 * every refresh, which makes "how long has this been wrong" meaningless
 * and hides a persistent fault among transient ones.
 *
 * Identity is the issue's id, which is why those ids are built from
 * stable code + element keys rather than anything positional.
 */
export function mergeIssues(previous: Issue[], next: Issue[]): Issue[] {
  const seenBefore = new Map(previous.map((i) => [i.id, i.firstSeen]));
  // Ordered and filtered by `next`: an issue that no longer derives has
  // been resolved and must leave, however long it was present.
  return next.map((issue) => {
    const firstSeen = seenBefore.get(issue.id);
    return firstSeen ? { ...issue, firstSeen } : issue;
  });
}

export interface IssueCounts {
  error: number;
  warn: number;
  info: number;
  total: number;
}
export function countIssues(issues: Issue[]): IssueCounts {
  return {
    error: issues.filter((i) => i.severity === "error").length,
    warn: issues.filter((i) => i.severity === "warn").length,
    info: issues.filter((i) => i.severity === "info").length,
    total: issues.length,
  };
}

// ── Network validation (validate_network command) ──────────────────────────

/** One finding from the backend `validate_network` command. */
export interface ValidationFinding {
  severity: "error" | "warning";
  code: string;
  message: string;
  elementId?: string | null;
  elementKind?: string | null;
}

/**
 * Run backend network validation for a project/scenario. Resolves `[]` when
 * the command is unavailable (older backend, non-Tauri shell) or fails, so
 * the Issues panel simply shows no validation findings.
 */
export async function fetchValidationFindings(
  projectId: string,
  scenarioId: string | null,
): Promise<ValidationFinding[]> {
  const findings = await tryInvokeOr<ValidationFinding[]>(
    "validate_network",
    { projectId, scenarioId },
    [],
  );
  return Array.isArray(findings) ? findings : [];
}

/**
 * Map one validation finding to an Issues-panel `Issue`.
 *
 * The id is derived from `code` + `elementId` (not array position) so it is
 * stable across refetches — its first-seen timestamp survives re-derivation.
 * Severity maps error→error, warning→warn; source reuses the existing
 * "data" category so the panel's filter list needs no changes.
 */
export function validationFindingToIssue(
  f: ValidationFinding,
  firstSeen: string,
): Issue {
  return {
    id: `validation-${f.code}-${f.elementId ?? "network"}`,
    severity: f.severity === "error" ? "error" : "warn",
    source: "data",
    code: f.code,
    title: f.message,
    detail: f.elementId
      ? `${f.message} (${f.elementKind ?? "element"} ${f.elementId})`
      : f.message,
    link: {
      view: "canvas",
      assetId: f.elementId ?? undefined,
      label: f.elementId ? `Show ${f.elementId}` : "Open canvas",
    },
    firstSeen,
  };
}

export function validationFindingsToIssues(
  findings: ValidationFinding[],
  firstSeen: string,
): Issue[] {
  return findings.map((f) => validationFindingToIssue(f, firstSeen));
}

// ── Simulation run warnings (get_run_warnings command) ─────────────────────

/** One solver warning from the backend `get_run_warnings` command. */
export interface RunWarning {
  code: string;
  message: string;
  elementId: string | null;
}

/**
 * Fetch the last run's solver warnings for a project/scenario. Resolves `[]`
 * when the command is unavailable (older backend, non-Tauri shell), fails,
 * or no run/warnings exist — the Issues panel then shows nothing extra.
 */
export async function fetchRunWarnings(
  projectId: string,
  scenarioId: string | null,
): Promise<RunWarning[]> {
  const warnings = await tryInvokeOr<RunWarning[]>(
    "get_run_warnings",
    { projectId, scenarioId },
    [],
  );
  return Array.isArray(warnings) ? warnings : [];
}

/**
 * Map one run warning to an Issues-panel `Issue`.
 *
 * The id is derived from `code` + `elementId` (not array position) so it is
 * stable across refetches — duplicates collapse and its first-seen timestamp
 * is merged forward by the AppContext issues derivation. Severity is always
 * "warn" and source "runtime"; the solver message is passed through as the
 * title (the detail additionally names the element when one is present).
 */
export function runWarningToIssue(w: RunWarning, firstSeen: string): Issue {
  return {
    id: `simwarn-${w.code}-${w.elementId ?? "network"}`,
    severity: "warn",
    source: "runtime",
    code: w.code,
    title: w.message,
    detail: w.elementId ? `${w.message} (element ${w.elementId})` : w.message,
    link: {
      view: "canvas",
      assetId: w.elementId ?? undefined,
      label: w.elementId ? `Show ${w.elementId}` : "Open canvas",
    },
    firstSeen,
  };
}

/**
 * Map a run's warnings to Issues, collapsing duplicates.
 *
 * The engine emits some warnings once per affected timestep — a pump running
 * outside its curve or unbalanced hydraulics can repeat hundreds of times in
 * one EPS run — and every repeat maps to the SAME issue id
 * (`simwarn-<code>-<elementId|network>`). Passing duplicates through broke
 * the panel: duplicate React keys and inflated counts. Collapse to the first
 * occurrence (earliest sim time in its message) and record the repeat count
 * in the detail instead.
 */
export function runWarningsToIssues(
  warnings: RunWarning[],
  firstSeen: string,
): Issue[] {
  const byId = new Map<string, { issue: Issue; occurrences: number }>();
  for (const w of warnings) {
    const issue = runWarningToIssue(w, firstSeen);
    const existing = byId.get(issue.id);
    if (existing) existing.occurrences += 1;
    else byId.set(issue.id, { issue, occurrences: 1 });
  }
  return [...byId.values()].map(({ issue, occurrences }) =>
    occurrences === 1
      ? issue
      : {
          ...issue,
          detail: `${issue.detail} (repeated at ${occurrences} report times)`,
        },
  );
}
