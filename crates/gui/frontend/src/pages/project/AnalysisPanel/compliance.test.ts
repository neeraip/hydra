/**
 * Tests for the pressure-compliance derivation used by the Analysis page's
 * System Summary. The derivation must use only fields that exist on the
 * ResultAnalytics DTO and return null when nothing is derivable.
 */
import { describe, expect, it } from "vitest";
import { pressureCompliancePct } from "./compliance";

function histogram(counts: number[]) {
  return counts.map((count, i) => ({ lo: i * 10, hi: (i + 1) * 10, count }));
}

describe("pressureCompliancePct", () => {
  it("returns null for absent analytics", () => {
    expect(pressureCompliancePct(null)).toBeNull();
  });

  it("returns null when the histogram carries no counts", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 3, pressureHistogram: [] }),
    ).toBeNull();
    expect(
      pressureCompliancePct({
        lowPressureCount: 0,
        pressureHistogram: histogram([0, 0, 0]),
      }),
    ).toBeNull();
  });

  it("is 100% when no node is below the threshold", () => {
    expect(
      pressureCompliancePct({
        lowPressureCount: 0,
        pressureHistogram: histogram([10, 20, 30]),
      }),
    ).toBe(100);
  });

  it("derives (total - low) / total from the histogram population", () => {
    // 60 nodes with pressure data, 15 below threshold → 75%.
    expect(
      pressureCompliancePct({
        lowPressureCount: 15,
        pressureHistogram: histogram([10, 20, 30]),
      }),
    ).toBeCloseTo(75);
  });

  it("clamps a lowPressureCount larger than the histogram population to 0%", () => {
    expect(
      pressureCompliancePct({
        lowPressureCount: 999,
        pressureHistogram: histogram([5, 5]),
      }),
    ).toBe(0);
  });

  it("ignores negative or non-finite bucket counts", () => {
    expect(
      pressureCompliancePct({
        lowPressureCount: 0,
        pressureHistogram: [
          { lo: 0, hi: 10, count: -5 },
          { lo: 10, hi: 20, count: Number.NaN },
          { lo: 20, hi: 30, count: 10 },
        ],
      }),
    ).toBe(100);
  });

  it("treats a negative lowPressureCount as 0 (never > 100%)", () => {
    expect(
      pressureCompliancePct({
        lowPressureCount: -3,
        pressureHistogram: histogram([10]),
      }),
    ).toBe(100);
  });
});
