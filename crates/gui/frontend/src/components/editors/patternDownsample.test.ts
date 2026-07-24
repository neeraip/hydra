import { describe, expect, it } from "vitest";
import { downsampleMinMax, envelopePath } from "./patternDownsample";

describe("downsampleMinMax", () => {
  it("returns empty output for empty input or non-positive bucket counts", () => {
    expect(downsampleMinMax([], 10)).toEqual([]);
    expect(downsampleMinMax([1, 2, 3], 0)).toEqual([]);
    expect(downsampleMinMax([1, 2, 3], -5)).toEqual([]);
  });

  it("is lossless when values fit within maxBuckets", () => {
    const values = [1, 0.5, 2, 1.25];
    expect(downsampleMinMax(values, 4)).toEqual(
      values.map((v) => ({ min: v, max: v })),
    );
    expect(downsampleMinMax(values, 100)).toHaveLength(4);
  });

  it("caps output length at maxBuckets", () => {
    const values = Array.from({ length: 8760 }, (_, i) => Math.sin(i));
    expect(downsampleMinMax(values, 200)).toHaveLength(200);
  });

  it("keeps each bucket's exact min and max", () => {
    // 8 values → 2 buckets of 4.
    const values = [1, 9, 3, 4, 5, 0, 7, 2];
    expect(downsampleMinMax(values, 2)).toEqual([
      { min: 1, max: 9 },
      { min: 0, max: 7 },
    ]);
  });

  it("covers every index exactly once when n is not divisible by buckets", () => {
    // Use a distinct spike per position: for each bucket layout, the union of
    // all buckets must contain the global max and min, and per-bucket
    // extremes must come from disjoint contiguous slices.
    const n = 10;
    const values = Array.from({ length: n }, (_, i) => i);
    for (const buckets of [3, 4, 6, 7, 9]) {
      const out = downsampleMinMax(values, buckets);
      expect(out).toHaveLength(buckets);
      // Contiguous coverage: each bucket's min follows the previous bucket's
      // max, starting at 0 and ending at n-1.
      expect(out[0].min).toBe(0);
      expect(out[out.length - 1].max).toBe(n - 1);
      for (let i = 1; i < out.length; i++) {
        expect(out[i].min).toBe(out[i - 1].max + 1);
      }
    }
  });

  it("never loses a spike, however aggressive the compression", () => {
    const values = new Array(8760).fill(1);
    values[4321] = 42; // lone spike
    values[1234] = -7; // lone trough
    const out = downsampleMinMax(values, 50);
    expect(Math.max(...out.map((b) => b.max))).toBe(42);
    expect(Math.min(...out.map((b) => b.min))).toBe(-7);
  });
});

describe("envelopePath", () => {
  it("returns empty string for degenerate inputs", () => {
    expect(envelopePath([], 100, 50, 2)).toBe("");
    expect(envelopePath([{ min: 0, max: 1 }], 0, 50, 2)).toBe("");
    expect(envelopePath([{ min: 0, max: 1 }], 100, 0, 2)).toBe("");
    expect(envelopePath([{ min: 0, max: 1 }], 100, 50, 0)).toBe("");
  });

  it("produces a closed path spanning the full width", () => {
    const path = envelopePath(
      [
        { min: 0, max: 2 },
        { min: 1, max: 1 },
        { min: 0.5, max: 1.5 },
      ],
      300,
      100,
      2,
    );
    expect(path.startsWith("M ")).toBe(true);
    expect(path.endsWith(" Z")).toBe(true);
    // First bucket max = 2 at yMax 2 → y = 0, x = 0.
    expect(path).toContain("M 0.00,0.00");
    // Last bucket x = width.
    expect(path).toContain("300.00,");
  });

  it("clamps values outside [0, yMax] into the viewBox", () => {
    const path = envelopePath([{ min: -5, max: 99 }], 100, 40, 2);
    // max clamps to yMax → y 0; min clamps to 0 → y = height.
    expect(path).toContain("50.00,0.00");
    expect(path).toContain("50.00,40.00");
  });
});
