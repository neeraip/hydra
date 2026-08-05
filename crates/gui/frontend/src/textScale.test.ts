import { describe, expect, it } from "vitest";
import { DEFAULT_TEXT_SCALE, parseTextScale, TEXT_SCALES } from "./textScale";

describe("parseTextScale", () => {
  it("accepts every offered scale, round-tripping through storage", () => {
    for (const option of TEXT_SCALES) {
      expect(parseTextScale(String(option.value))).toBe(option.value);
    }
  });

  it("falls back to the default for unusable input", () => {
    // A stored value the app can't honour must never be applied: there is no
    // way back to Settings from a layout rendered at an absurd size.
    for (const raw of [null, "", "abc", "NaN", "Infinity", "-1", "0", "12"]) {
      expect(parseTextScale(raw)).toBe(DEFAULT_TEXT_SCALE);
    }
  });

  it("offers the default as one of its options", () => {
    expect(TEXT_SCALES.some((o) => o.value === DEFAULT_TEXT_SCALE)).toBe(true);
  });
});

describe("TEXT_SCALES", () => {
  /**
   * The option a user reads as "Default" has to *be* the default. These
   * are two declarations of one fact — a label and a constant — and the
   * ladder has already been renumbered once, which is exactly when they
   * come apart.
   */
  it("makes the step labelled Default the default", () => {
    const labelled = TEXT_SCALES.find((o) => o.label === "Default");
    expect(labelled?.value).toBe(DEFAULT_TEXT_SCALE);
  });

  /** A dropdown of sizes is read as a ladder; an unsorted one would offer
   * "Large" above "Small" and mean nothing. */
  it("ascends", () => {
    const values = TEXT_SCALES.map((o) => o.value);
    expect(values).toEqual([...values].sort((a, b) => a - b));
  });

  it("has no repeated label or value", () => {
    expect(new Set(TEXT_SCALES.map((o) => o.label)).size).toBe(
      TEXT_SCALES.length,
    );
    expect(new Set(TEXT_SCALES.map((o) => o.value)).size).toBe(
      TEXT_SCALES.length,
    );
  });

  /** Every step has to keep the app usable — the module says so, and the
   * range is where that claim is bounded. */
  it("stays within a range each step is verified at", () => {
    for (const o of TEXT_SCALES) {
      expect(o.value).toBeGreaterThanOrEqual(0.9);
      expect(o.value).toBeLessThanOrEqual(1.4);
    }
  });
});
