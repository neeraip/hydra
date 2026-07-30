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
