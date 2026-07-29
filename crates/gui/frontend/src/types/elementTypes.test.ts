import { describe, expect, it } from "vitest";
import { elementTypeBadge } from "./elementTypes";

describe("elementTypeBadge", () => {
  it("gives every element type a letter and a colour", () => {
    for (const type of [
      "junction",
      "reservoir",
      "tank",
      "pipe",
      "pump",
      "valve",
    ]) {
      const badge = elementTypeBadge(type);
      expect(badge.label.length).toBeGreaterThan(0);
      expect(badge.color).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it("never gives two types the same letter", () => {
    // Colour alone must not be the only differentiator — pipe owns "P", so
    // pump has to be "Pu".
    const labels = [
      "junction",
      "reservoir",
      "tank",
      "pipe",
      "pump",
      "valve",
    ].map((t) => elementTypeBadge(t).label);
    expect(new Set(labels).size).toBe(labels.length);
  });

  it("falls back to an initial for an unknown type", () => {
    expect(elementTypeBadge("subcatchment").label).toBe("S");
    expect(elementTypeBadge("").label).toBe("?");
  });
});
