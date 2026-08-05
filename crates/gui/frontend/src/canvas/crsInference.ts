/**
 * What a model's own coordinates can tell you about the system they are in.
 *
 * Choosing a coordinate system is guesswork in most tools: you are shown
 * eight thousand codes and asked to know which one. But the model is not
 * silent — its numbers rule most answers out, and once a candidate is
 * applied, *where the network lands* confirms or destroys it instantly. An
 * engineer who cannot recall an EPSG code knows perfectly well whether
 * their network belongs in Leeds or in the Gulf of Guinea.
 *
 * Everything here is arithmetic on the coordinates. Applying a candidate
 * needs proj4 and lives with the picker; this module stays pure so the
 * reasoning can be tested without a projection library.
 */

import type { Node } from "../types/network";

export interface CoordinateReading {
  /** Nodes carrying real coordinates — (0,0) placeholders are excluded. */
  count: number;
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
  /** Mid-point of the extent: the probe applied to a candidate system. */
  centre: [number, number];
  /**
   * Coordinates fall outside longitude/latitude range, so whatever they
   * are, they are not degrees. The strongest single fact available before
   * any system is chosen.
   */
  projected: boolean;
}

/** Extent and character of a network's coordinates, or `null` when it has
 * none to speak of. */
export function readCoordinates(nodes: Node[]): CoordinateReading | null {
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  let count = 0;
  for (const n of nodes) {
    // The importer writes (0,0) for a node with no coordinate at all;
    // counting those would drag every extent towards null island.
    if (n.x === 0 && n.y === 0) continue;
    if (!Number.isFinite(n.x) || !Number.isFinite(n.y)) continue;
    count += 1;
    if (n.x < minX) minX = n.x;
    if (n.x > maxX) maxX = n.x;
    if (n.y < minY) minY = n.y;
    if (n.y > maxY) maxY = n.y;
  }
  if (count === 0) return null;
  return {
    count,
    minX,
    maxX,
    minY,
    maxY,
    centre: [(minX + maxX) / 2, (minY + maxY) / 2],
    projected: minX < -180 || maxX > 180 || minY < -90 || maxY > 90,
  };
}

/** The UTM zone covering a longitude. Zones are 6° wide from the
 * antimeridian, so this is exact rather than a lookup. */
export function utmZoneForLongitude(lon: number): number {
  // Wrap only what is genuinely out of range: folding +180 to -180 would
  // move the antimeridian from the end of zone 60 to the start of zone 1,
  // an edge the clamp below already resolves the conventional way.
  const wrapped =
    lon > 180 || lon < -180 ? ((((lon + 180) % 360) + 360) % 360) - 180 : lon;
  return Math.min(60, Math.max(1, Math.floor((wrapped + 180) / 6) + 1));
}

/** Zone and hemisphere of a WGS 84 UTM code, or `null` for anything else.
 * EPSG numbers 326xx are northern, 327xx southern. */
export function utmZoneOf(
  epsg: string,
): { zone: number; north: boolean } | null {
  const m = /^EPSG:(326|327)(\d{2})$/.exec(epsg.trim().toUpperCase());
  if (!m) return null;
  const zone = Number(m[2]);
  if (zone < 1 || zone > 60) return null;
  return { zone, north: m[1] === "326" };
}

/** The WGS 84 UTM code for a zone and hemisphere. */
export function utmEpsgFor(zone: number, north: boolean): string {
  return `EPSG:${north ? 326 : 327}${String(zone).padStart(2, "0")}`;
}

/**
 * Whether a transformed position could be somewhere on earth a network is.
 *
 * Deliberately weak — it rejects the impossible, not the unlikely. Without
 * an area-of-use for each system the only honest test is arithmetic, and
 * the reader's own knowledge of where their network is does the rest.
 */
export function plausibleLatLon(lat: number, lon: number): boolean {
  if (!Number.isFinite(lat) || !Number.isFinite(lon)) return false;
  if (lat < -90 || lat > 90 || lon < -180 || lon > 180) return false;
  // Exactly null island is what a failed or identity transform produces far
  // more often than a real network sits there.
  if (Math.abs(lat) < 1e-9 && Math.abs(lon) < 1e-9) return false;
  return true;
}

/**
 * A better UTM zone for a position, when the chosen one is not the right
 * one for the longitude it produced.
 *
 * Picking the neighbouring zone is the commonest CRS mistake there is, and
 * it is self-diagnosing: the network lands in the sea a few hundred
 * kilometres east or west of where it belongs. `null` when the code is not
 * UTM, or is already the right zone.
 */
export function betterUtmZone(
  epsg: string,
  lon: number,
): { epsg: string; zone: number } | null {
  const current = utmZoneOf(epsg);
  if (!current) return null;
  const wanted = utmZoneForLongitude(lon);
  if (wanted === current.zone) return null;
  return { epsg: utmEpsgFor(wanted, current.north), zone: wanted };
}
