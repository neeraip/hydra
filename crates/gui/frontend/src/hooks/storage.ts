/**
 * What Hydra has written to disk, for the Settings Data section.
 *
 * Results are the only thing offered for deletion here: they are derived,
 * reproducible by running again, and by far the largest thing a run
 * writes. Models, scenarios and reports are not.
 */

import { invoke, tryInvoke } from "./ipc";

export interface DataUsage {
  /** Everything under the data folder, results included. */
  totalBytes: number;
  /** The part of it that is simulation results. */
  resultsBytes: number;
  /** How many projects that is spread across. */
  projectCount: number;
}

export interface ClearedResults {
  removed: number;
  /** Projects left alone because a simulation was writing to one. */
  skipped: number;
}

export async function getDataUsage(): Promise<DataUsage | null> {
  return tryInvoke<DataUsage>("get_data_usage");
}

export async function clearAllResults(): Promise<ClearedResults | null> {
  return tryInvoke<ClearedResults>("clear_all_results");
}

/**
 * Reveal today's diagnostic log in the file manager.
 *
 * Rejects rather than resolving quietly when this run has no log file:
 * "here are your logs" and "logging is not working" must not look the
 * same to someone collecting them for a bug report.
 */
export async function openLogFolder(): Promise<void> {
  await invoke("open_log_folder");
}

/**
 * Bytes as a person reads them.
 *
 * Binary units, because this describes a file on disk and every file
 * manager the reader has open beside it says GB for 2^30. One decimal
 * place past a kilobyte and none below it: "1.4 GB" is the useful
 * precision for deciding whether to clear something, and "1433.6 kB" is
 * not.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 bytes";
  const units = ["bytes", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  if (unit === 0) return `${Math.round(value)} bytes`;
  return `${value.toFixed(1)} ${units[unit]}`;
}

/**
 * What the Data section says about the folder.
 *
 * A sentence rather than a figure because two numbers on their own invite
 * the wrong subtraction — the total includes the results, and a reader
 * seeing "4.2 GB" beside "3.9 GB" cannot tell whether one is part of the
 * other.
 */
export function describeUsage(usage: DataUsage | null): string {
  if (usage === null) return "Measuring…";
  if (usage.projectCount === 0) return "Nothing stored yet.";
  const projects =
    usage.projectCount === 1 ? "1 project" : `${usage.projectCount} projects`;
  return `${formatBytes(usage.totalBytes)} across ${projects}, of which ${formatBytes(
    usage.resultsBytes,
  )} is simulation results.`;
}

/** What to say once a clear has run. */
export function describeCleared(cleared: ClearedResults): string {
  const removed =
    cleared.removed === 1
      ? "1 result cleared"
      : `${cleared.removed} results cleared`;
  if (cleared.skipped === 0) return `${removed}.`;
  const projects =
    cleared.skipped === 1
      ? "1 project was"
      : `${cleared.skipped} projects were`;
  return `${removed}. ${projects} left alone — a simulation is running.`;
}
