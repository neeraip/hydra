/**
 * Issue types + counting helpers for the Issues panel, plus the
 * `validate_network` fetcher and its finding→Issue mapper.
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
  dismissed: boolean;
}
export interface IssueCounts {
  error: number;
  warn: number;
  info: number;
  total: number;
}
export function countIssues(
  issues: Issue[],
  includeDismissed = false,
): IssueCounts {
  const list = includeDismissed ? issues : issues.filter((i) => !i.dismissed);
  return {
    error: list.filter((i) => i.severity === "error").length,
    warn: list.filter((i) => i.severity === "warn").length,
    info: list.filter((i) => i.severity === "info").length,
    total: list.length,
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
 * stable across refetches — the panel's dismissed flags key on issue id.
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
    link: { view: "canvas", label: "Open canvas" },
    firstSeen,
    dismissed: false,
  };
}

export function validationFindingsToIssues(
  findings: ValidationFinding[],
  firstSeen: string,
): Issue[] {
  return findings.map((f) => validationFindingToIssue(f, firstSeen));
}
