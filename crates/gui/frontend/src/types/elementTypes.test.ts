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

  it("never gives two types the same letter within an engine", () => {
    // Colour alone must not be the only differentiator — pipe owns "P", so
    // pump has to be "Pu"; outfall/orifice/outlet get two-letter labels.
    const wds = ["junction", "reservoir", "tank", "pipe", "pump", "valve"];
    const uds = [
      "junction",
      "outfall",
      "storage",
      "divider",
      "conduit",
      "pump",
      "orifice",
      "weir",
      "outlet",
      "subcatchment",
      "raingage",
    ];
    for (const kinds of [wds, uds]) {
      const labels = kinds.map((t) => elementTypeBadge(t).label);
      expect(new Set(labels).size).toBe(labels.length);
    }
  });

  it("falls back to an initial for an unknown type", () => {
    expect(elementTypeBadge("aquifer").label).toBe("A");
    expect(elementTypeBadge("").label).toBe("?");
  });
});
