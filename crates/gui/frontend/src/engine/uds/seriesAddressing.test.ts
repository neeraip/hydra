import { describe, expect, it } from "vitest";
import type { GenericResultMeta, GenericVariable } from "../../hooks/results";
import { seriesIndex, seriesVariables } from "./seriesAddressing";

const v = (id: string): GenericVariable =>
  ({ id, label: id }) as GenericVariable;

const META: GenericResultMeta = {
  pointVars: [v("depth")],
  polylineVars: [v("flow")],
  regionVars: [v("runoff")],
};

describe("seriesVariables", () => {
  /**
   * The three classes must each reach their own catalog. This was a
   * two-branch ternary keyed on a two-value union; widening the union to
   * include areal elements would have silently charted a subcatchment
   * against the conduit catalog, because the `else` arm meant "link".
   */
  it("gives each class its own catalog, with no class falling through to another", () => {
    expect(seriesVariables(META, "node")).toEqual(META.pointVars);
    expect(seriesVariables(META, "link")).toEqual(META.polylineVars);
    expect(seriesVariables(META, "region")).toEqual(META.regionVars);
  });

  it("has no variables before the engine's metadata arrives", () => {
    expect(seriesVariables(undefined, "region")).toEqual([]);
    expect(seriesVariables(null, "node")).toEqual([]);
  });
});

describe("seriesIndex", () => {
  const arrays = {
    nodes: [{ id: "J1" }, { id: "J2" }],
    links: [{ id: "C1" }],
    regions: [{ id: "S1" }, { id: "S2" }, { id: "S3" }],
  };

  it("indexes within the element's own class", () => {
    expect(seriesIndex(arrays, "node", "J2")).toBe(1);
    expect(seriesIndex(arrays, "link", "C1")).toBe(0);
    expect(seriesIndex(arrays, "region", "S3")).toBe(2);
  });

  /** An id that exists in a different class is not this class's element —
   * returning its position there would fetch the wrong series. */
  it("does not find an id that belongs to another class", () => {
    expect(seriesIndex(arrays, "region", "J1")).toBe(-1);
    expect(seriesIndex(arrays, "node", "S1")).toBe(-1);
  });
});
