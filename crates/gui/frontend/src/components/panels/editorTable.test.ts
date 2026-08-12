import { describe, expect, it } from "vitest";
import { TEXT_SCALES } from "../../textScale";
import { editorRowHeight } from "./editorTable";

describe("editorRowHeight", () => {
  it("is unchanged at the default scale", () => {
    // The tables were built around 30px rows; the text-scale work must not
    // move them for users who never touch the setting.
    expect(editorRowHeight(1)).toBe(30);
  });

  it("tracks the scale in the right direction", () => {
    expect(editorRowHeight(0.9)).toBeLessThan(30);
    expect(editorRowHeight(1.25)).toBeGreaterThan(30);
  });

  it("grows by less than the scale factor, because cell padding is fixed", () => {
    // The row is 14px of literal padding plus a line box that scales. Treating
    // the whole row as scalable would over-estimate, and the error accumulates
    // once per row across tens of thousands of rows.
    expect(editorRowHeight(1.25)).toBeLessThan(30 * 1.25);
  });

  it("returns whole pixels for every offered scale", () => {
    // Fractional row heights would drift against the browser's own rounding
    // of each rendered row.
    for (const { value } of TEXT_SCALES) {
      expect(Number.isInteger(editorRowHeight(value))).toBe(true);
    }
  });
});
