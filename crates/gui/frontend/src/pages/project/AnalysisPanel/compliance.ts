/**
 * Pressure-compliance derivation from existing `ResultAnalytics` fields.
 *
 * Uses only what the DTO already provides: `pressureHistogram` (whose bucket
 * counts sum to the population of nodes with valid pressure data) and
 * `lowPressureCount` (junctions below the minimum-pressure threshold at peak
 * demand). No new data is invented — when the histogram is empty the
 * percentage is not derivable and `null` is returned.
 */

import type { ResultAnalytics } from "../../../hooks";

export type PressureComplianceInput = Pick<
  ResultAnalytics,
  "lowPressureCount" | "pressureHistogram"
>;

/**
 * Percentage of pressure-carrying nodes at or above the minimum-pressure
 * threshold: `(total - lowPressureCount) / total * 100`, clamped to [0, 100].
 * Returns `null` when analytics are absent or the histogram carries no
 * counts (nothing to derive from).
 */
export function pressureCompliancePct(
  analytics: PressureComplianceInput | null,
): number | null {
  if (!analytics) return null;
  let total = 0;
  for (const bucket of analytics.pressureHistogram) {
    if (Number.isFinite(bucket.count) && bucket.count > 0) {
      total += bucket.count;
    }
  }
  if (total <= 0) return null;
  const low = Number.isFinite(analytics.lowPressureCount)
    ? Math.min(total, Math.max(0, analytics.lowPressureCount))
    : 0;
  return ((total - low) / total) * 100;
}
