/**
 * Issue types + counting helpers for the Issues panel.
 */

import type { ProjectView } from "../projectConfig";

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
