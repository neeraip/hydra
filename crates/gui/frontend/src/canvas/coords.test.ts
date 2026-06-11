import { describe, expect, it } from "vitest";
import type { Node } from "../types";
import {
  COMMON_CRS,
  formatMeters,
  pixelDistance,
  reprojectNodes,
  sniffCoordCrs,
} from "./coords";

// ── helpers ────────────────────────────────────────────────────────────────────

function node(id: string, x: number, y: number): Node {
  return { id, type: "junction", x, y, pressure: null, demand: null };
}

// ── pixelDistance ─────────────────────────────────────────────────────────────

describe("pixelDistance", () => {
  it("returns 0 for identical points", () => {
    expect(pixelDistance(1, 2, 1, 2)).toBe(0);
  });

  it("scales by factor 4 (1 canvas unit ≈ 4 m)", () => {
    // 3-4-5 right triangle → Euclidean = 5 → scaled = 20
    expect(pixelDistance(0, 0, 3, 4)).toBe(20);
  });

  it("is symmetric", () => {
    const ab = pixelDistance(10, 20, 30, 40);
    const ba = pixelDistance(30, 40, 10, 20);
    expect(ab).toBe(ba);
  });

  it("is always non-negative", () => {
    expect(pixelDistance(-5, -5, 5, 5)).toBeGreaterThan(0);
  });
});

// ── formatMeters ──────────────────────────────────────────────────────────────

describe("formatMeters", () => {
  it("formats values < 1000 as metres", () => {
    expect(formatMeters(0)).toBe("0 m");
    expect(formatMeters(999)).toBe("999 m");
    expect(formatMeters(123.6)).toBe("124 m");
  });

  it("formats values ≥ 1000 as km with 2 decimal places", () => {
    expect(formatMeters(1000)).toBe("1.00 km");
    expect(formatMeters(1500)).toBe("1.50 km");
    expect(formatMeters(12345)).toBe("12.35 km");
  });
});

// ── sniffCoordCrs ─────────────────────────────────────────────────────────────

describe("sniffCoordCrs", () => {
  it("returns 'wgs84' for an empty node array", () => {
    expect(sniffCoordCrs([])).toBe("wgs84");
  });

  it("returns 'wgs84' for typical WGS84 coordinates", () => {
    const nodes = [
      node("n1", 151.2, -33.8), // Sydney
      node("n2", -0.1, 51.5), // London
    ];
    expect(sniffCoordCrs(nodes)).toBe("wgs84");
  });

  it("returns 'projected' when x is outside [-180, 180]", () => {
    expect(sniffCoordCrs([node("n1", 500_000, 6_000_000)])).toBe("projected");
  });

  it("returns 'projected' when y is outside [-90, 90]", () => {
    expect(sniffCoordCrs([node("n1", 10, 200)])).toBe("projected");
  });

  it("returns 'projected' on the first non-WGS84 node even if others are fine", () => {
    const nodes = [
      node("n1", 10.0, 20.0),
      node("n2", 400_000, 600_000), // UTM-like
    ];
    expect(sniffCoordCrs(nodes)).toBe("projected");
  });
});

// ── reprojectNodes ────────────────────────────────────────────────────────────

describe("reprojectNodes", () => {
  it("is a no-op for EPSG:4326", () => {
    const nodes = [node("n1", 151.2, -33.8)];
    const result = reprojectNodes(nodes, "EPSG:4326");
    // Should return the same array reference (identity short-circuit).
    expect(result).toBe(nodes);
  });

  it("reprojects UTM Zone 56S (EPSG:32756) to WGS84 with ~1° accuracy", () => {
    // Sydney lies in UTM Zone 56 (150–156°E). Central meridian = 153°E.
    // Easting ~334 000 m is ~166 000 m west of CM → ~151°E, ~33.9°S.
    const easting = 334_000;
    const northing = 6_252_000;
    const result = reprojectNodes(
      [node("syd", easting, northing)],
      "EPSG:32756",
    );
    expect(result[0].x).toBeCloseTo(151.0, 0); // longitude ≈ 151°E
    expect(result[0].y).toBeCloseTo(-33.9, 0); // latitude ≈ 33.9°S
  });

  it("preserves all non-coordinate fields unchanged", () => {
    const original: Node = {
      id: "n1",
      type: "junction",
      x: 334_000,
      y: 6_252_000,
      pressure: 32,
      demand: 1.5,
      elevation: 10,
    };
    const [result] = reprojectNodes([original], "EPSG:32756");
    expect(result.id).toBe("n1");
    expect(result.type).toBe("junction");
    expect(result.pressure).toBe(32);
    expect(result.demand).toBe(1.5);
    expect(result.elevation).toBe(10);
  });

  it("throws for an unknown non-UTM EPSG code", () => {
    expect(() => reprojectNodes([node("n1", 0, 0)], "EPSG:99999")).toThrow();
  });
});

// ── COMMON_CRS ─────────────────────────────────────────────────────────────────

describe("COMMON_CRS", () => {
  it("is a non-empty array", () => {
    expect(COMMON_CRS.length).toBeGreaterThan(0);
  });

  it("every entry has a non-empty label and epsg", () => {
    for (const entry of COMMON_CRS) {
      expect(entry.label.length).toBeGreaterThan(0);
      expect(entry.epsg.length).toBeGreaterThan(0);
    }
  });

  it("includes EPSG:4326 (WGS84)", () => {
    expect(COMMON_CRS.some((c) => c.epsg === "EPSG:4326")).toBe(true);
  });

  it("includes EPSG:3857 (Web Mercator)", () => {
    expect(COMMON_CRS.some((c) => c.epsg === "EPSG:3857")).toBe(true);
  });
});
