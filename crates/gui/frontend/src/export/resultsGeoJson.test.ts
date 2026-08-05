import { describe, expect, it } from "vitest";
import type { Link, Node, Region } from "../types/network";
import { buildResultsGeoJson } from "./resultsGeoJson";

function node(over: Partial<Node> = {}): Node {
  return {
    id: "N1",
    type: "junction",
    x: 1,
    y: 2,
    pressure: null,
    demand: null,
    ...over,
  } as Node;
}

function link(over: Partial<Link> = {}): Link {
  return {
    id: "L1",
    type: "pipe",
    fromId: "N1",
    toId: "N2",
    flow: null,
    ...over,
  } as Link;
}

function region(over: Partial<Region> = {}): Region {
  return {
    id: "S1",
    type: "subcatchment",
    ring: [
      [0, 0],
      [1, 0],
      [1, 1],
    ],
    outletId: "N1",
    ...over,
  };
}

describe("buildResultsGeoJson", () => {
  /**
   * The rule the export exists to keep. Every class carried `resultValues`
   * except areal elements, which were built from the unmerged array and
   * spread nothing — so one file answered "does this include results" two
   * different ways depending on which feature you read.
   */
  it("carries result values for every element class, not just nodes and links", () => {
    const fc = buildResultsGeoJson(
      [node({ resultValues: { head: 12 } })],
      [link({ resultValues: { flow: 3 } })],
      [region({ resultValues: { runoff: 0.4 } })],
    );
    const props = fc.features.map((f) => f.properties);
    expect(props[0]).toMatchObject({ id: "N1", head: 12 });
    expect(props[1]).toMatchObject({ id: "L1", flow: 3 });
    expect(props[2]).toMatchObject({ id: "S1", runoff: 0.4 });
  });

  it("emits one feature per element, in node/link/region order", () => {
    const fc = buildResultsGeoJson([node()], [link()], [region()]);
    expect(fc.type).toBe("FeatureCollection");
    expect(fc.features.map((f) => f.geometry.type)).toEqual([
      "Point",
      "LineString",
      "Polygon",
    ]);
  });

  it("omits absent attributes rather than exporting them as zero", () => {
    const [n] = buildResultsGeoJson(
      [node({ elevation: undefined })],
      [],
      [],
    ).features;
    expect(n.properties).not.toHaveProperty("elevation");
    const [l] = buildResultsGeoJson([], [link({ diameter: 0 })], []).features;
    expect(l.properties).not.toHaveProperty("diameter");
  });

  it("routes a link through its intermediate vertices", () => {
    const fc = buildResultsGeoJson(
      [node({ id: "A", x: 0, y: 0 }), node({ id: "B", x: 4, y: 0 })],
      [link({ fromId: "A", toId: "B", vertices: [[2, 1]] })],
      [],
    );
    const line = fc.features[2];
    expect(line.geometry.coordinates).toEqual([
      [0, 0],
      [2, 1],
      [4, 0],
    ]);
  });

  it("closes a region ring and drops a ring too small to be a polygon", () => {
    const fc = buildResultsGeoJson(
      [],
      [],
      [
        region(),
        region({
          id: "S2",
          ring: [
            [0, 0],
            [1, 1],
          ],
        }),
      ],
    );
    expect(fc.features).toHaveLength(1);
    expect(fc.features[0].geometry.coordinates).toEqual([
      [
        [0, 0],
        [1, 0],
        [1, 1],
        [0, 0],
      ],
    ]);
  });
});
