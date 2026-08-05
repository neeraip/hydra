import { describe, expect, it } from "vitest";
import {
  type ClearableView,
  clearableCountOf,
  readGenericSelection,
} from "./CanvasView";

const clear: ClearableView = {
  rail: false,
  selection: false,
  legend: false,
  basemapMenu: false,
  tool: false,
  measurements: false,
};

describe("clearableCountOf", () => {
  // Zero disables the button: offering an action with no visible effect
  // teaches the user it does nothing.
  it("is zero when nothing covers the map", () => {
    expect(clearableCountOf(clear)).toBe(0);
  });

  it("counts each covering thing once", () => {
    expect(clearableCountOf({ ...clear, rail: true })).toBe(1);
    expect(clearableCountOf({ ...clear, rail: true, legend: true })).toBe(2);
  });

  // Counts whatever categories exist rather than a hard-coded total, so a
  // category added later is covered without editing this test.
  it("counts every category it knows about", () => {
    const all = Object.fromEntries(
      Object.keys(clear).map((k) => [k, true]),
    ) as unknown as ClearableView;
    expect(clearableCountOf(all)).toBe(Object.keys(clear).length);
  });
});

describe("readGenericSelection", () => {
  it("round-trips a saved selection", () => {
    expect(
      readGenericSelection({
        genericSelection: {
          point: "depth",
          polyline: "flow",
          region: "runoff",
        },
      }),
    ).toEqual({ point: "depth", polyline: "flow", region: "runoff" });
  });

  // An id from another engine's catalog is not corruption — it is a
  // selection made against a different run. The legend falls back to the
  // catalog's first variable, so it must survive the read unchanged.
  it("passes through ids it cannot validate here", () => {
    expect(
      readGenericSelection({ genericSelection: { point: "pressure" } }).point,
    ).toBe("pressure");
  });

  it("falls back to empty for missing or corrupt prefs", () => {
    for (const raw of [
      null,
      undefined,
      {},
      "nonsense",
      { genericSelection: 7 },
    ]) {
      expect(readGenericSelection(raw)).toEqual({
        point: "",
        polyline: "",
        region: "",
      });
    }
  });

  it("ignores non-string entries rather than storing them", () => {
    expect(
      readGenericSelection({ genericSelection: { point: 3, polyline: null } }),
    ).toEqual({ point: "", polyline: "", region: "" });
  });
});
