/**
 * Pressure-compliance derivation from existing `ResultAnalytics` fields.
 *
 * The denominator is `junctionCount` — the population the backend actually
 * evaluated — not a sum of histogram bucket counts. Summing buckets was only
 * ever a proxy for that population, and it was a broken one: the first bin
 * used to start at 0, so every junction with a negative worst-case pressure
 * fell outside every bucket. On a network with a real pressure deficit that
 * shrank the denominator far below the true population while
 * `lowPressureCount` kept counting those same junctions, and compliance came
 * out several times too low.
 */

import type { ResultAnalytics } from "../../../hooks";

export type PressureComplianceInput = Pick<
  ResultAnalytics,
  "lowPressureCount" | "junctionCount"
>;

/**
 * Percentage of evaluated junctions at or above the minimum-pressure
 * threshold: `(junctionCount - lowPressureCount) / junctionCount * 100`,
 * clamped to [0, 100]. Returns `null` when analytics are absent or no
 * junction carried pressure data (nothing to derive from).
 */
export function pressureCompliancePct(
  analytics: PressureComplianceInput | null,
): number | null {
  if (!analytics) return null;
  const total = analytics.junctionCount;
  if (!Number.isFinite(total) || total <= 0) return null;
  const low = Number.isFinite(analytics.lowPressureCount)
    ? Math.min(total, Math.max(0, analytics.lowPressureCount))
    : 0;
  return ((total - low) / total) * 100;
}
