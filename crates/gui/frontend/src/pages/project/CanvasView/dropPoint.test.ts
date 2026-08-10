import { describe, expect, it, vi } from "vitest";
import { sourceCoordinate } from "./dropPoint";

/**
 * Which drop points get projected, and which must not be.
 *
 * The store holds source-CRS values. A basemap reports WGS84 and needs
 * converting; a plan reports the model's own coordinates and needs
 * nothing. Getting that backwards does not fail — it writes a plausible
 * number into the model, which is the worst kind of wrong.
 */

const project = vi.fn(([lng, lat]: [number, number]): [number, number] => [
  lng * 1000,
  lat * 1000,
]);

describe("sourceCoordinate", () => {
  it("passes a plan's own coordinates straight through", () => {
    project.mockClear();
    expect(
      sourceCoordinate({ space: "source", x: 4890, y: 52370 }, project),
    ).toEqual([4890, 52370]);
    // Not merely equal by luck — the projection was never consulted.
    expect(project).not.toHaveBeenCalled();
  });

  it("inverse-projects a point dropped on the basemap", () => {
    expect(
      sourceCoordinate({ space: "wgs84", x: 4.89, y: 52.37 }, project),
    ).toEqual([4890, 52370]);
  });

  it("lets an unconvertible point throw rather than storing something", () => {
    // The callers turn this into a refusal the user sees. Swallowing it
    // here would commit whatever the fallback happened to be.
    const failing = () => {
      throw new Error("outside the CRS' area of use");
    };
    expect(() =>
      sourceCoordinate({ space: "wgs84", x: 999, y: 999 }, failing),
    ).toThrow("area of use");
  });

  it("never throws for a plan, whatever the projection would have done", () => {
    // A local grid has no georeference at all, so there is no projection
    // that could succeed — which is exactly why it must not be asked.
    const failing = () => {
      throw new Error("no projection");
    };
    expect(sourceCoordinate({ space: "source", x: 1, y: 2 }, failing)).toEqual([
      1, 2,
    ]);
  });
});
