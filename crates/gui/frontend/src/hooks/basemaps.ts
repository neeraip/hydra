/**
 * Offline-basemap commands + events (region store listing/deletion, download
 * planning, background downloads with `basemap:download` progress events,
 * viewport coverage checks) plus the pure formatting / visibility helpers
 * shared by the basemap UI.
 */

import { listen } from "@tauri-apps/api/event";
import { invoke, tryInvoke } from "./ipc";

// ── Types ──────────────────────────────────────────────────────────────────

/** Geographic bounding box as `[minLon, minLat, maxLon, maxLat]` (WGS84). */
export type BasemapBbox = [number, number, number, number];

/** Mirrors the backend `RegionInfo` DTO (list_basemap_regions). */
export interface BasemapRegionInfo {
  id: string;
  name: string;
  minLon: number;
  minLat: number;
  maxLon: number;
  maxLat: number;
  minZoom: number;
  maxZoom: number;
  /** Unix seconds at creation. */
  createdAt: number;
  /** Identifier of the planet build the tiles came from, when known. */
  planetBuild?: string | null;
  /** Bytes of tile data only this region claims. */
  uniqueBytes: number;
  /** Bytes of tile data shared with at least one other region. */
  sharedBytes: number;
  tileCount: number;
  /** Project IDs referencing this region. */
  projectIds: string[];
}

/** Mirrors the backend `BasemapStorageDto`. */
export interface BasemapStorage {
  regions: BasemapRegionInfo[];
  /** Actual bytes of the store on disk (db + WAL). */
  dataBytes: number;
  diskBytes: number;
  /** Regions no project references — safe to remove. */
  unusedRegionIds: string[];
}

/** Mirrors the backend `DownloadPlanDto` (plan_basemap_download). */
export interface BasemapDownloadPlan {
  /** Exact bytes a download would fetch. */
  newBytes: number;
  /** Bytes already on disk the region would share. */
  sharedBytes: number;
  missingTiles: number;
  presentTiles: number;
  /** In-bbox tiles the planet archive has no data for (open water). */
  absentTiles: number;
}

/** Mirrors the backend `CoverageDto` (basemap_coverage). */
export interface BasemapCoverage {
  presentTiles: number;
  totalTiles: number;
  covered: boolean;
}

export type BasemapDownloadPhase =
  | "planning"
  | "downloading"
  | "complete"
  | "cancelled"
  | "failed";

/** Payload of `basemap:download` events. */
export interface BasemapDownloadEvent {
  regionName: string;
  phase: BasemapDownloadPhase;
  doneBytes: number;
  totalBytes: number;
  /** Set on the `complete` event. */
  regionId?: string | null;
  /** Set on the `failed` event. */
  error?: string | null;
}

export const BASEMAP_DOWNLOAD_EVENT = "basemap:download";

// ── Commands ───────────────────────────────────────────────────────────────

/** Fetch the region store contents. Resolves `null` outside a Tauri shell
 *  (or on backend errors, which are reported via the shared IPC handler). */
export async function listBasemapRegions(): Promise<BasemapStorage | null> {
  return tryInvoke<BasemapStorage>("list_basemap_regions");
}

/** Resolve the exact download cost of a bbox without fetching tile data.
 *  SLOW — makes network range-requests against the planet archive. Rejects
 *  on backend errors so the modal can surface the failure. */
export async function planBasemapDownload(
  bbox: BasemapBbox,
  archiveUrl?: string,
): Promise<BasemapDownloadPlan> {
  return invoke<BasemapDownloadPlan>("plan_basemap_download", {
    bbox,
    archiveUrl: archiveUrl ?? null,
  });
}

/** Start a background region download. Progress, completion, and failure
 *  all arrive as `basemap:download` events; only one download runs at a
 *  time (a second call rejects). */
export async function downloadBasemapRegion(args: {
  name: string;
  bbox: BasemapBbox;
  projectId?: string | null;
  archiveUrl?: string | null;
}): Promise<void> {
  return invoke<void>("download_basemap_region", {
    name: args.name,
    bbox: args.bbox,
    projectId: args.projectId ?? null,
    archiveUrl: args.archiveUrl ?? null,
  });
}

/** Request cooperative cancellation of the in-flight download (if any).
 *  Confirmation arrives as a `cancelled` `basemap:download` event. */
export async function cancelBasemapDownload(): Promise<void> {
  await tryInvoke<void>("cancel_basemap_download");
}

/** Is the viewport bbox covered by stored tiles at (floored) `zoom`?
 *  Resolves `null` outside a Tauri shell. */
export async function getBasemapCoverage(
  bbox: BasemapBbox,
  zoom: number,
): Promise<BasemapCoverage | null> {
  return tryInvoke<BasemapCoverage>("basemap_coverage", { bbox, zoom });
}

/** Delete a stored region. Resolves the bytes actually freed (shared tiles
 *  survive). Rejects on backend errors so the caller can surface them. */
export async function deleteBasemapRegion(
  regionId: string,
): Promise<{ freedBytes: number }> {
  return invoke<{ freedBytes: number }>("delete_basemap_region", { regionId });
}

/** Reference `regionId` from `projectId` so it stops counting as unused. */
export async function linkProjectBasemapRegion(
  projectId: string,
  regionId: string,
): Promise<void> {
  await tryInvoke<void>("link_project_basemap_region", {
    projectId,
    regionId,
  });
}

/** Subscribe to `basemap:download` progress events from the backend.
 *  Returns the unlisten function — call it to unsubscribe. */
export function listenBasemapDownload(
  cb: (e: BasemapDownloadEvent) => void,
): Promise<() => void> {
  return listen<BasemapDownloadEvent>(BASEMAP_DOWNLOAD_EVENT, (ev) =>
    cb(ev.payload),
  );
}

// ── Pure helpers (unit-tested in basemaps.test.ts) ─────────────────────────

/** Human-readable byte size: "0 B", "512 B", "1.5 KB", "12.3 MB", "1.2 GB".
 *  Binary (1024) steps; one decimal, with a trailing ".0" trimmed. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = "B";
  for (const u of units) {
    if (value < 1024) break;
    value /= 1024;
    unit = u;
  }
  const rounded = value.toFixed(1).replace(/\.0$/, "");
  return `${rounded} ${unit}`;
}

/** Region size summary: `"12.3 MB unique · 4.1 MB shared"`. */
export function regionSizeLabel(r: {
  uniqueBytes: number;
  sharedBytes: number;
}): string {
  return `${formatBytes(r.uniqueBytes)} unique · ${formatBytes(r.sharedBytes)} shared`;
}

/** Web-Mercator latitude limit — tiles don't exist beyond it. */
const MAX_MERCATOR_LAT = 85.051129;

/** Grow `bbox` by `fraction` of its span on every side, clamped to the
 *  world (lon ±180, lat ±85.05 Web-Mercator limit). */
export function padBbox(bbox: BasemapBbox, fraction: number): BasemapBbox {
  const [minLon, minLat, maxLon, maxLat] = bbox;
  const lonPad = (maxLon - minLon) * fraction;
  const latPad = (maxLat - minLat) * fraction;
  return [
    Math.max(-180, minLon - lonPad),
    Math.max(-MAX_MERCATOR_LAT, minLat - latPad),
    Math.min(180, maxLon + lonPad),
    Math.min(MAX_MERCATOR_LAT, maxLat + latPad),
  ];
}

/** Parse the download modal's four bbox fields. Returns `null` unless all
 *  four parse as finite numbers within world bounds with min < max. */
export function bboxFromStrings(
  minLon: string,
  minLat: string,
  maxLon: string,
  maxLat: string,
): BasemapBbox | null {
  const nums = [minLon, minLat, maxLon, maxLat].map((s) =>
    Number.parseFloat(s.trim()),
  );
  if (nums.some((n) => !Number.isFinite(n))) return null;
  const [w, s, e, n] = nums;
  if (w < -180 || e > 180 || s < -90 || n > 90) return null;
  if (w >= e || s >= n) return null;
  return [w, s, e, n];
}

/** True for the locally-served basemap styles ("offline-*"). */
export function isOfflineBasemap(basemap: string): boolean {
  return basemap.startsWith("offline-");
}

/** Region downloads only store street-detail zooms ≥ 7; below that the
 *  coverage chip is meaningless (the world overview covers 0–6). */
export const COVERAGE_MIN_ZOOM = 7;

/** Should the "No offline detail here" chip render? Pure so the visibility
 *  matrix is unit-testable. */
export function shouldShowCoverageChip(args: {
  /** Selected basemap style value — chip is offline-only. */
  basemap: string;
  /** Current canvas view mode — chip is map-mode only. */
  viewMode: string;
  /** Current map zoom, or null before the first move-end report. */
  zoom: number | null;
  /** Coverage of the current viewport, or null while unknown/stale. */
  covered: boolean | null;
  /** True while a region download is in flight. */
  downloadActive: boolean;
}): boolean {
  return (
    isOfflineBasemap(args.basemap) &&
    args.viewMode === "map" &&
    args.zoom != null &&
    args.zoom >= COVERAGE_MIN_ZOOM &&
    args.covered === false &&
    !args.downloadActive
  );
}
