import { describe, expect, it } from "vitest";
import { overrideOptionLabel, sourceOptionLabel } from "./UnitSystemPicker";

describe("the units menu's labels", () => {
  /**
   * `Source` is the only option that indirects, so it is the only one that
   * has to say what it resolves to — otherwise the menu offers a choice
   * whose effect you can only learn by making it.
   */
  it("says what Source resolves to", () => {
    expect(sourceOptionLabel("us")).toBe("Source (US customary)");
    expect(sourceOptionLabel("si")).toBe("Source (SI (metric))");
  });

  /**
   * Before the model answers there is nothing to resolve to, and inventing
   * one would name a system the project may not use.
   */
  it("says only Source before the model has answered", () => {
    expect(sourceOptionLabel(null)).toBe("Source");
  });

  /** The two explicit systems are their own answer and take no annotation
   * — repeating it would read as a second, different claim. */
  it("leaves the explicit systems unannotated", () => {
    expect(overrideOptionLabel("si", "us")).toBe("SI (metric)");
    expect(overrideOptionLabel("us", "si")).toBe("US customary");
  });

  it("annotates Source wherever it appears", () => {
    expect(overrideOptionLabel("source", "us")).toBe(sourceOptionLabel("us"));
  });
});
