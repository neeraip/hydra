/**
 * Pure helpers for the pipe initial-status ("Status") column.
 *
 * Kept free of React so they can be unit-tested in a plain Node environment.
 */

import type { PipeInitialStatus } from "../../../hooks";

/**
 * Options for the Status select, in display order. `label` doubles as the
 * backend patch value ("Open" | "Closed" | "CV" — the patch arm parses
 * case-insensitively, capitalized is the canonical spelling).
 */
export const PIPE_STATUS_OPTIONS: ReadonlyArray<{
  value: PipeInitialStatus;
  label: "Open" | "Closed" | "CV";
}> = [
  { value: "open", label: "Open" },
  { value: "closed", label: "Closed" },
  { value: "cv", label: "CV" },
];

/** Backend patch value for a row's initial status. */
export function pipeStatusPatchValue(
  s: PipeInitialStatus,
): "Open" | "Closed" | "CV" {
  return PIPE_STATUS_OPTIONS.find((o) => o.value === s)?.label ?? "Open";
}
