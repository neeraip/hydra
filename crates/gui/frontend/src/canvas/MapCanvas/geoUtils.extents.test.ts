/**
 * @vitest-environment node
 *
 * What the camera frames.
 *
 * The defect: both fits read node positions and nothing else, so they
 * framed the pipe network rather than the picture. An overland mesh
 * routinely covers ground the network does not — that is what it is for
 * — and its edges fell outside the very view meant to show everything.
 */

import { describe, expect, it } from "vitest";

import type { Node } from "../../types";
import { geoBounds, orthoCenterFromMap } from "./geoUtils";

const node = (id: string, x: number, y: number) =>
  ({ id, x, y, type: "junction" }) as unknown as Node;

/** The shape of the SWMM 2D example: a mesh over 0–20, pipes over 10–22. */
const NODES = [node("J1", 10, 10), node("ST1", 15, 15), node("O1", 22, 0)];
const MESH: [number, number, number, number] = [0, 0, 20, 20];

describe("geoBounds", () => {
  it("frames the mesh as well as the network", () => {
    const withMesh = geoBounds(NODES, MESH);
    expect(withMesh).toEqual([
      [0, 0],
      [22, 20],
    ]);
    // Without it, the mesh's west and north fall outside the frame.
    const nodesOnly = geoBounds(NODES);
    expect(nodesOnly).toEqual([
      [10, 0],
      [22, 15],
    ]);
  });

  it("frames a mesh whose model places no nodes at all", () => {
    expect(geoBounds([], MESH)).toEqual([
      [0, 0],
      [20, 20],
    ]);
  });

  it("is unchanged for a model with no mesh", () => {
    expect(geoBounds(NODES, null)).toEqual(geoBounds(NODES));
    expect(geoBounds([])).toBeNull();
  });

  /**
   * Unplaced nodes are the backend's (0, 0) sentinel and are skipped —
   * but a mesh's corner at the origin is a real place, so the two must
   * not be conflated.
   */
  it("keeps a mesh that genuinely reaches the origin", () => {
    const placed = [node("J1", 10, 10), node("unplaced", 0, 0)];
    expect(geoBounds(placed, [0, 0, 5, 5])).toEqual([
      [0, 0],
      [10, 10],
    ]);
  });
});

describe("orthoCenterFromMap", () => {
  const coords = new Map<string, [number, number]>([
    ["J1", [10, 10]],
    ["O1", [22, 0]],
  ]);

  it("centres on the network and its mesh together", () => {
    const { target } = orthoCenterFromMap(coords, MESH);
    // x spans 0..22, y spans 0..20.
    expect(target[0]).toBe(11);
    expect(target[1]).toBe(10);
  });

  it("zooms out far enough to hold the wider extent", () => {
    const withMesh = orthoCenterFromMap(coords, MESH);
    const nodesOnly = orthoCenterFromMap(coords);
    expect(withMesh.zoom).toBeLessThan(nodesOnly.zoom);
  });

  it("frames a mesh with no placed nodes", () => {
    const { target } = orthoCenterFromMap(new Map(), MESH);
    expect(target[0]).toBe(10);
    expect(target[1]).toBe(10);
  });

  it("is unchanged for a model with no mesh", () => {
    expect(orthoCenterFromMap(coords, null)).toEqual(
      orthoCenterFromMap(coords),
    );
  });
});
