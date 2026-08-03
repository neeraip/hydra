import { describe, expect, it } from "vitest";
import { type ElementKindInfo, elementClassHeading } from "./engines";

describe("elementClassHeading", () => {
  const uds: ElementKindInfo[] = [
    {
      id: "junction",
      label: "Junction",
      labelPlural: "Junctions",
      class: "point",
      badge: "J",
    },
    {
      id: "outfall",
      label: "Outfall",
      labelPlural: "Outfalls",
      class: "point",
      badge: "Of",
    },
    {
      id: "conduit",
      label: "Conduit",
      labelPlural: "Conduits",
      class: "polyline",
      badge: "C",
    },
    {
      id: "weir",
      label: "Weir",
      labelPlural: "Weirs",
      class: "polyline",
      badge: "W",
    },
    {
      id: "subcatchment",
      label: "Subcatchment",
      labelPlural: "Subcatchments",
      class: "region",
      badge: "Sc",
    },
  ];

  it("names a single-kind class after that kind", () => {
    // The whole point: the engine's "Subcatchments" beats the contract's
    // internal "Regions" when the engine models exactly one areal kind.
    expect(elementClassHeading(uds, "region", "Regions")).toBe("Subcatchments");
  });

  it("names a multi-kind class generically", () => {
    expect(elementClassHeading(uds, "point", "Nodes")).toBe("Nodes");
    expect(elementClassHeading(uds, "polyline", "Links")).toBe("Links");
  });

  it("falls back when the class is absent or the catalog is empty", () => {
    expect(elementClassHeading(uds, "collection", "Collections")).toBe(
      "Collections",
    );
    // Before the catalog resolves, the generic name is the honest answer.
    expect(elementClassHeading([], "region", "Regions")).toBe("Regions");
  });
});
