import { describe, expect, it } from "vitest";
import type { Node } from "../types/network";
import {
  betterUtmZone,
  plausibleLatLon,
  readCoordinates,
  utmEpsgFor,
  utmZoneForLongitude,
  utmZoneOf,
} from "./crsInference";

function node(x: number, y: number): Node {
  return {
    id: `${x},${y}`,
    type: "junction",
    x,
    y,
    pressure: null,
    demand: null,
  };
}

describe("readCoordinates", () => {
  it("reports the extent and centre", () => {
    const r = readCoordinates([node(10, 20), node(30, 60)]);
    expect(r).not.toBeNull();
    expect(r?.minX).toBe(10);
    expect(r?.maxY).toBe(60);
    expect(r?.centre).toEqual([20, 40]);
    expect(r?.count).toBe(2);
  });

  it("ignores placeholder coordinates", () => {
    // The importer writes (0,0) for a node with no coordinate; counting
    // those drags every extent towards null island.
    const r = readCoordinates([node(523150, 178400), node(0, 0)]);
    expect(r?.count).toBe(1);
    expect(r?.minX).toBe(523150);
  });

  it("calls out-of-range coordinates projected", () => {
    expect(readCoordinates([node(523150, 178400)])?.projected).toBe(true);
    expect(readCoordinates([node(-0.1278, 51.5074)])?.projected).toBe(false);
  });

  it("returns null when there is nothing to read", () => {
    expect(readCoordinates([])).toBeNull();
    expect(readCoordinates([node(0, 0)])).toBeNull();
  });
});

describe("utmZoneForLongitude", () => {
  it("places well-known longitudes in their zone", () => {
    expect(utmZoneForLongitude(-0.1278)).toBe(30); // London
    expect(utmZoneForLongitude(151.2093)).toBe(56); // Sydney
    expect(utmZoneForLongitude(-122.4194)).toBe(10); // San Francisco
  });

  it("holds at the zone edges and the antimeridian", () => {
    expect(utmZoneForLongitude(-180)).toBe(1);
    expect(utmZoneForLongitude(-174)).toBe(2);
    expect(utmZoneForLongitude(180)).toBe(60);
    expect(utmZoneForLongitude(179.9)).toBe(60);
  });

  it("wraps rather than falling off the end", () => {
    expect(utmZoneForLongitude(190)).toBe(utmZoneForLongitude(-170));
  });
});

describe("utmZoneOf / utmEpsgFor", () => {
  it("round-trips a northern and a southern zone", () => {
    expect(utmZoneOf("EPSG:32630")).toEqual({ zone: 30, north: true });
    expect(utmZoneOf("EPSG:32756")).toEqual({ zone: 56, north: false });
    expect(utmEpsgFor(30, true)).toBe("EPSG:32630");
    expect(utmEpsgFor(6, false)).toBe("EPSG:32706");
  });

  it("rejects codes that only look like UTM", () => {
    expect(utmZoneOf("EPSG:27700")).toBeNull(); // British National Grid
    expect(utmZoneOf("EPSG:4326")).toBeNull();
    expect(utmZoneOf("EPSG:32661")).toBeNull(); // zone 61 does not exist
    expect(utmZoneOf("LOCAL")).toBeNull();
  });
});

describe("plausibleLatLon", () => {
  it("accepts a real position", () => {
    expect(plausibleLatLon(51.5074, -0.1278)).toBe(true);
  });

  it("rejects the impossible and the tell-tale", () => {
    expect(plausibleLatLon(95, 0)).toBe(false);
    expect(plausibleLatLon(0, 200)).toBe(false);
    expect(plausibleLatLon(Number.NaN, 0)).toBe(false);
    // A failed or identity transform lands on null island far more often
    // than a water network does.
    expect(plausibleLatLon(0, 0)).toBe(false);
  });
});

describe("betterUtmZone", () => {
  it("names the right zone when a neighbouring one was chosen", () => {
    // Picked zone 31N, but the network came out over London — zone 30.
    const better = betterUtmZone("EPSG:32631", -0.1278);
    expect(better).toEqual({ epsg: "EPSG:32630", zone: 30 });
  });

  it("keeps the hemisphere it was given", () => {
    expect(betterUtmZone("EPSG:32731", -0.1278)?.epsg).toBe("EPSG:32730");
  });

  it("says nothing when the zone is already right", () => {
    expect(betterUtmZone("EPSG:32630", -0.1278)).toBeNull();
  });

  it("says nothing about systems that are not UTM", () => {
    expect(betterUtmZone("EPSG:27700", -0.1278)).toBeNull();
  });
});
