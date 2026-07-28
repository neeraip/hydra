/**
 * Tests for the pressure-compliance derivation used by the Analysis page's
 * System Summary. The derivation must use the evaluated-junction population
 * the backend reports and return null when nothing is derivable.
 */
import { describe, expect, it } from "vitest";
import { pressureCompliancePct } from "./compliance";

describe("pressureCompliancePct", () => {
  it("returns null for absent analytics", () => {
    expect(pressureCompliancePct(null)).toBeNull();
  });

  it("returns null when no junction carried pressure data", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 3, junctionCount: 0 }),
    ).toBeNull();
    expect(
      pressureCompliancePct({
        lowPressureCount: 0,
        junctionCount: Number.NaN,
      }),
    ).toBeNull();
  });

  it("is 100% when no junction is below the threshold", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 0, junctionCount: 60 }),
    ).toBe(100);
  });

  it("derives (total - low) / total from the evaluated population", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 15, junctionCount: 60 }),
    ).toBeCloseTo(75);
  });

  /**
   * The regression: negative-pressure junctions used to fall outside every
   * histogram bucket, so summing buckets gave a denominator far below the
   * real population while lowPressureCount still counted them. With 46190
   * junctions of which 23012 are low, compliance is ~50% — the bucket-sum
   * denominator (26366) produced 12.7%.
   */
  it("is unaffected by how the histogram buckets the population", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 23012, junctionCount: 46190 }),
    ).toBeCloseTo(50.18, 1);
  });

  it("clamps a lowPressureCount larger than the population to 0%", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: 999, junctionCount: 10 }),
    ).toBe(0);
  });

  it("treats a negative lowPressureCount as 0 (never > 100%)", () => {
    expect(
      pressureCompliancePct({ lowPressureCount: -3, junctionCount: 10 }),
    ).toBe(100);
  });
});
