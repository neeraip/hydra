import { beforeEach, describe, expect, it } from "vitest";
import {
  clearElementBadges,
  elementTypeBadge,
  registerElementBadges,
} from "./elementTypes";

// The badge registry is module-global, so a test inherits whatever the
// previous one registered unless it is cleared.
beforeEach(clearElementBadges);

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
    // Deliberately not a real kind: `aquifer` stood here until drainage
    // declared it, at which point this test was asserting the opposite of
    // what it meant.
    expect(elementTypeBadge("thingummy").label).toBe("T");
    expect(elementTypeBadge("").label).toBe("?");
  });
});

describe("engine-declared badges", () => {
  // The static map is a copy of something the engines publish, and it had
  // already fallen behind: six drainage kinds shipped with no entry and
  // fell through to their initial, so `landuse` and `lidcontrol` both
  // rendered as a grey "L". Registering the catalog is what stops a new
  // kind depending on someone remembering this file.
  it("prefers the engine's letters over the static map", () => {
    registerElementBadges([{ id: "junction", badge: "JX" }]);
    expect(elementTypeBadge("junction").label).toBe("JX");
    // ...while keeping this layer's colour, which no engine declares.
    expect(elementTypeBadge("junction").color).toBe(
      elementTypeBadge("outfall").color,
    );
  });

  it("badges a kind the static map has never heard of", () => {
    expect(elementTypeBadge("hypothetical").label).toBe("H");
    registerElementBadges([{ id: "hypothetical", badge: "Hy" }]);
    expect(elementTypeBadge("hypothetical").label).toBe("Hy");
  });

  it("ignores a kind that declares no badge", () => {
    registerElementBadges([{ id: "pipe", badge: "" }]);
    expect(elementTypeBadge("pipe").label).toBe("P");
  });

  // Distinct kinds must stay distinguishable; the initial-letter fallback
  // could not keep `landuse` and `lidcontrol` apart.
  it("keeps every declared drainage kind distinct", () => {
    const ids = [
      "landuse",
      "aquifer",
      "snowpack",
      "hydrograph",
      "lidcontrol",
      "transect",
    ];
    const labels = ids.map((id) => elementTypeBadge(id).label);
    expect(new Set(labels).size).toBe(ids.length);
  });
});
