/**
 * @vitest-environment jsdom
 */
import type { Map as MapLibreMap, PaddingOptions } from "maplibre-gl";
import { beforeEach, describe, expect, it } from "vitest";
import { visibleMapPadding } from "./geoUtils";

/** A map stand-in: `visibleMapPadding` only asks for the canvas size. */
const fakeMap = (width: number, height: number) =>
  ({
    getCanvas: () => ({ clientWidth: width, clientHeight: height }),
  }) as unknown as MapLibreMap;

/** MapLibre types every edge as optional; this function always sets all
 * four, and a missing one would itself be the bug. */
function edges(p: PaddingOptions) {
  const n = (v: number | undefined, name: string): number => {
    if (typeof v !== "number") throw new Error(`padding.${name} is not set`);
    return v;
  };
  return {
    top: n(p.top, "top"),
    bottom: n(p.bottom, "bottom"),
    left: n(p.left, "left"),
    right: n(p.right, "right"),
  };
}

function setVars(vars: Record<string, string>) {
  for (const [k, v] of Object.entries(vars)) {
    document.documentElement.style.setProperty(k, v);
  }
}

beforeEach(() => {
  document.documentElement.style.cssText = "";
  setVars({
    "--tool-btn-size": "30px",
    "--rail-effective-w": "0px",
    "--inspector-effective-w": "0px",
  });
});

describe("visibleMapPadding", () => {
  // The regression: the viewport controls are a tall, narrow column in one
  // corner, but their *height* was charged to the bottom edge. MapLibre
  // padding is a frame, so that withheld ~164px across the whole width —
  // a quarter of the container — and the network was fitted into the top
  // two-thirds with a wide empty band beneath it.
  it("does not charge a corner cluster's height to the bottom edge", () => {
    const p = edges(visibleMapPadding(fakeMap(1224, 698)));
    // Whatever the exact constants, the bottom must not cost more than the
    // top: both edges carry one bar, and neither carries the cluster.
    expect(p.bottom).toBeLessThan(200);
    expect(p.bottom - p.top).toBeLessThan(80);
  });

  it("reserves the cluster's width on the edge it hugs", () => {
    const p = edges(visibleMapPadding(fakeMap(1224, 698)));
    // Nothing is then placed in the cluster's column, so nothing can be
    // drawn behind it — the guarantee the height-charging never gave.
    expect(p.right).toBeGreaterThan(p.left);
  });

  // The whole point: enough vertical room that a network actually fills the
  // view it was fitted into.
  it("leaves most of the container's height available", () => {
    const height = 698;
    const p = edges(visibleMapPadding(fakeMap(1224, height)));
    const free = height - p.top - p.bottom;
    expect(free / height).toBeGreaterThan(0.6);
  });

  it("grows the side padding with the panels that publish their width", () => {
    const bare = edges(visibleMapPadding(fakeMap(1224, 698)));
    setVars({ "--rail-effective-w": "300px" });
    const railed = edges(visibleMapPadding(fakeMap(1224, 698)));
    expect(railed.left).toBeGreaterThan(bare.left + 250);
  });

  it("falls back to an even margin when padding would exceed the container", () => {
    expect(visibleMapPadding(fakeMap(80, 60))).toEqual({
      top: 8,
      bottom: 8,
      left: 8,
      right: 8,
    });
  });

  it("keeps every edge positive and finite", () => {
    const p = edges(visibleMapPadding(fakeMap(1224, 698)));
    for (const v of [p.top, p.bottom, p.left, p.right]) {
      expect(Number.isFinite(v)).toBe(true);
      expect(v).toBeGreaterThan(0);
    }
  });
});
