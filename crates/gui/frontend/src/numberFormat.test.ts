import { describe, expect, it } from "vitest";
import {
  formatDecimal,
  formatSignificant,
  rangeDecimals,
  significantDecimals,
} from "./numberFormat";

describe("significantDecimals", () => {
  it("keeps a small value readable instead of rounding it away", () => {
    // The reported case: two fixed decimals rendered this as "0.00".
    expect(formatSignificant(0.000471, 2)).toBe("0.000471");
  });

  it("shortens a large value rather than trailing digits", () => {
    expect(significantDecimals(1513.3612)).toBe(0);
    expect(significantDecimals(12.3456)).toBe(2);
    expect(significantDecimals(1.23456)).toBe(3);
  });

  it("treats the declared decimals as a floor, never as the answer", () => {
    // The engine asks for two; the magnitude needs six. It gets six.
    expect(significantDecimals(0.000471, 2)).toBe(6);
    // The engine asks for two; the magnitude needs none. It still gets two.
    expect(significantDecimals(1513.3612, 2)).toBe(2);
  });

  it("falls back to the floor with no magnitude to read", () => {
    expect(significantDecimals(0, 2)).toBe(2);
    expect(significantDecimals(Number.NaN, 3)).toBe(3);
  });

  it("caps the decimals so nothing renders as a run of zeros", () => {
    expect(significantDecimals(1e-12, 0)).toBeLessThanOrEqual(6);
  });

  it("is symmetric about zero", () => {
    expect(significantDecimals(-0.000471, 2)).toBe(
      significantDecimals(0.000471, 2),
    );
  });
});

describe("rangeDecimals", () => {
  it("resolves a narrow span at high magnitude", () => {
    // The reported case: a trend running 1513.36 → 1514.00 showed only
    // "1513" and "1514", so the line read as flat.
    expect(rangeDecimals(1513.3612, 1514.0021)).toBe(2);
  });

  it("does not pad a wide span with decimals it does not need", () => {
    expect(rangeDecimals(0, 100)).toBe(0);
    expect(rangeDecimals(0, 5000)).toBe(0);
  });

  it("derives from the span, not the values", () => {
    // Same span, magnitudes three orders apart — same answer.
    expect(rangeDecimals(1, 1.5)).toBe(rangeDecimals(1001, 1001.5));
  });

  it("shows a constant series to its own precision", () => {
    expect(rangeDecimals(0.000471, 0.000471, 2)).toBe(6);
  });

  it("honours the floor", () => {
    expect(rangeDecimals(0, 100, 2)).toBe(2);
  });

  it("survives a reversed or non-finite range", () => {
    expect(rangeDecimals(1514, 1513.36)).toBe(2);
    expect(rangeDecimals(Number.NaN, 5, 1)).toBe(1);
  });
});

describe("formatDecimal", () => {
  it("no longer rounds away variation above 1000", () => {
    // The bug: `>= 1000 → Math.round` collapsed a whole trend to integers.
    expect(formatDecimal(1513.3612, 2)).toBe("1513.36");
    expect(formatDecimal(1514.0021, 2)).toBe("1514.00");
  });

  it("groups only where grouping helps", () => {
    // Four digits read fine unseparated and keep a column narrow; five do
    // not, so grouping starts at ten thousand.
    expect(formatDecimal(1513.36, 2)).toBe("1513.36");
    expect(formatDecimal(12345.6, 1)).toBe("12,345.6");
    expect(formatDecimal(999.5, 1)).toBe("999.5");
  });

  it("escapes to exponential at the extremes", () => {
    expect(formatDecimal(0.0000047, 6)).toBe("4.70e-6");
    expect(formatDecimal(1.2e9, 2)).toBe("1.20e+9");
  });

  it("keeps zero and non-finite values sane", () => {
    expect(formatDecimal(0, 2)).toBe("0.00");
    expect(formatDecimal(Number.NaN, 2)).toBe("—");
    expect(formatDecimal(Number.POSITIVE_INFINITY, 2)).toBe("—");
  });
});
