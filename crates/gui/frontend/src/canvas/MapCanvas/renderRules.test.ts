import { describe, expect, it } from "vitest";
import type { CanvasTool } from "../types";
import {
  classifyViewTransition,
  linksPickableFor,
  sizeUnitsFor,
} from "./renderRules";

/**
 * Each of these is one line where it is used, which is why none of them had
 * a name. A one-line rule buried in an argument list is exactly as
 * unreachable as a long one.
 */

describe("which tools can pick a link", () => {
  it("lets select and edit pick", () => {
    expect(linksPickableFor("select")).toBe(true);
    expect(linksPickableFor("edit")).toBe(true);
  });

  /** Snapping to a link needs picking, even though measure's own
   *  interaction goes through the map rather than the layer. */
  it("lets measure pick, so snapping works", () => {
    expect(linksPickableFor("measure")).toBe(true);
  });

  /** Skipping the pick pass halves the per-mousemove GPU cost. */
  it("does not, for the tools that cannot use it", () => {
    expect(linksPickableFor("add-node")).toBe(false);
    expect(linksPickableFor("add-link")).toBe(false);
  });

  it("has an answer for every tool", () => {
    const tools: CanvasTool[] = [
      "select",
      "measure",
      "edit",
      "add-node",
      "add-link",
    ];
    for (const t of tools) expect(typeof linksPickableFor(t)).toBe("boolean");
  });
});

describe("what a size is measured in", () => {
  /**
   * A geographic view has metres, so a size given in them means the same
   * thing at every zoom. A schematic has no metres — its coordinates are
   * the layout's own.
   */
  it("is metres on a map and common units in a schematic", () => {
    expect(sizeUnitsFor(false)).toBe("meters");
    expect(sizeUnitsFor(true)).toBe("common");
  });
});

describe("classifying a view-mode change", () => {
  it("knows arrival at the map", () => {
    expect(classifyViewTransition("schematic", "map")).toBe("entering-map");
  });

  it("knows departure from it", () => {
    expect(classifyViewTransition("map", "schematic")).toBe("leaving-map");
  });

  /**
   * The two conditions are near-mirrors and easy to get subtly different.
   * Being wrong either way means a camera not kept or a camera not
   * restored, both of which read as the canvas re-framing itself for no
   * reason.
   */
  it("knows when neither happened", () => {
    expect(classifyViewTransition("map", "map")).toBe("staying");
    expect(classifyViewTransition("schematic", "schematic")).toBe("staying");
  });

  /** The first render has no previous mode. Arriving at the map then is
   *  still an arrival: its camera has never been put anywhere. */
  it("treats a first render into the map as an arrival", () => {
    expect(classifyViewTransition(null, "map")).toBe("entering-map");
  });

  it("treats a first render into the schematic as neither", () => {
    expect(classifyViewTransition(null, "schematic")).toBe("staying");
  });
});
