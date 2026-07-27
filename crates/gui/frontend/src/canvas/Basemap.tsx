/**
 * Basemap identity model + provider raster-style helpers.
 *
 * A basemap id is either one of the four legacy ids ("streets" | "light" |
 * "dark" | "none" — the three built-in OpenFreeMap vector styles plus the
 * tile-free blank background, persisted as-is in per-project canvas prefs) or
 * a provider style id of the form `provider:{providerId}:{styleId}` served
 * through the backend's `basemap:` tile proxy.
 */

import type { StyleSpecification } from "maplibre-gl";
import type { BasemapProvider } from "../hooks/basemapProviders";

/** Open string type: a legacy id or `provider:{providerId}:{styleId}`. */
export type BasemapId = string;

export const LEGACY_BASEMAP_IDS = ["streets", "light", "dark", "none"] as const;
export type LegacyBasemapId = (typeof LEGACY_BASEMAP_IDS)[number];

export function isLegacyBasemapId(id: string): id is LegacyBasemapId {
  return (LEGACY_BASEMAP_IDS as readonly string[]).includes(id);
}

const PROVIDER_ID_PREFIX = "provider:";

/** Build the basemap id for a non-builtin provider style. */
export function providerBasemapId(
  providerId: string,
  styleId: string,
): BasemapId {
  return `${PROVIDER_ID_PREFIX}${providerId}:${styleId}`;
}

/** Parse a `provider:{providerId}:{styleId}` id. Returns null for legacy ids
 * and anything malformed (missing/empty/extra segments). */
export function parseProviderBasemapId(
  id: string,
): { providerId: string; styleId: string } | null {
  if (!id.startsWith(PROVIDER_ID_PREFIX)) return null;
  const parts = id.slice(PROVIDER_ID_PREFIX.length).split(":");
  if (parts.length !== 2) return null;
  const [providerId, styleId] = parts;
  if (!providerId || !styleId) return null;
  return { providerId, styleId };
}

/** Validate a persisted pref value: legacy id or well-formed provider id.
 * (Existence in the live catalog is NOT checked here — a stale-but-well-formed
 * id is allowed through and falls back to "streets" at render time.) */
export function isValidBasemapId(id: unknown): id is BasemapId {
  return (
    typeof id === "string" &&
    (isLegacyBasemapId(id) || parseProviderBasemapId(id) !== null)
  );
}

/**
 * The basemap id used by the picker/visibility model for a catalog style.
 * OpenFreeMap's styles keep their legacy ids ("streets"/"light"/"dark") so
 * existing per-project prefs stay valid; every other provider's styles use
 * the `provider:{providerId}:{styleId}` form.
 */
export function basemapIdForCatalogStyle(
  providerId: string,
  styleId: string,
): BasemapId {
  return providerId === "openfreemap"
    ? styleId
    : providerBasemapId(providerId, styleId);
}

// ── Basemap opacity ──────────────────────────────────────────────────────────

/**
 * Clamp a persisted basemap opacity to [0, 1]. Missing/corrupt values
 * (non-numbers, NaN, ±Infinity) fall back to fully opaque — the pref's
 * default — so stale per-project prefs can never blank the basemap.
 */
export function clampBasemapOpacity(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 1;
  return Math.min(1, Math.max(0, value));
}

// ── Tile URL template ────────────────────────────────────────────────────────

/**
 * `{z}`/`{x}`/`{y}` tile URL template for the backend `basemap:` proxy.
 *
 * Windows webviews cannot fetch custom-scheme URLs directly — Tauri maps the
 * scheme to `http://basemap.localhost` there, so the template is built
 * per-platform from the user agent.
 */
export function providerTileUrlTemplate(
  providerId: string,
  styleId: string,
  userAgent: string = typeof navigator !== "undefined"
    ? navigator.userAgent
    : "",
): string {
  const origin = /windows/i.test(userAgent)
    ? "http://basemap.localhost"
    : "basemap://localhost";
  return `${origin}/provider/${providerId}/${styleId}/{z}/{x}/{y}`;
}

// ── Provider raster style ────────────────────────────────────────────────────

/** Catalog facts needed to render one provider style as a raster basemap. */
export interface ProviderStyleParams {
  providerId: string;
  styleId: string;
  /** Logical tile size in CSS pixels (256 or 512). */
  tileSize: number;
  maxZoom: number;
  attribution: string;
}

/**
 * Build a self-contained MapLibre style for a proxied provider raster style:
 * one raster source (tiles via the `basemap:` proxy, carrying the provider's
 * attribution) under a solid background layer, mirroring BLANK_STYLE's colour
 * so missing tiles degrade to the familiar blank canvas.
 */
export function buildProviderRasterStyle(
  { providerId, styleId, tileSize, maxZoom, attribution }: ProviderStyleParams,
  userAgent?: string,
): StyleSpecification {
  return {
    version: 8,
    sources: {
      "provider-tiles": {
        type: "raster",
        tiles: [providerTileUrlTemplate(providerId, styleId, userAgent)],
        tileSize,
        maxzoom: maxZoom,
        attribution,
      },
    },
    layers: [
      {
        id: "background",
        type: "background",
        paint: { "background-color": "#16181c" },
      },
      {
        id: "provider-tiles",
        type: "raster",
        source: "provider-tiles",
      },
    ],
  };
}

// ── Visibility model ─────────────────────────────────────────────────────────

/**
 * Per-machine picker-visibility overrides. Defaults differ by style origin,
 * so the pref stores explicit overrides per direction:
 *
 * - OpenFreeMap's three styles are visible by default → `hiddenLegacyIds`
 *   lists the ones the user explicitly hid;
 * - every provider style is hidden by default (even when its provider is
 *   connected) → `shownProviderIds` lists the `provider:{providerId}:{styleId}`
 *   ids the user explicitly unhid.
 */
export interface BasemapVisibility {
  hiddenLegacyIds: ReadonlySet<string>;
  shownProviderIds: ReadonlySet<string>;
}

/** Nothing overridden: OpenFreeMap visible, all provider styles hidden. */
export const DEFAULT_BASEMAP_VISIBILITY: BasemapVisibility = {
  hiddenLegacyIds: new Set(),
  shownProviderIds: new Set(),
};

/** Whether a picker style id is hidden under the given overrides. */
export function isBasemapStyleHidden(
  id: BasemapId,
  visibility: BasemapVisibility,
): boolean {
  return parseProviderBasemapId(id) !== null
    ? !visibility.shownProviderIds.has(id)
    : visibility.hiddenLegacyIds.has(id);
}

// ── Picker grouping ──────────────────────────────────────────────────────────

export interface BasemapPickerEntry {
  id: BasemapId;
  label: string;
}

export interface BasemapPickerGroup {
  providerId: string;
  /** Group heading shown above the entries. */
  label: string;
  entries: BasemapPickerEntry[];
}

/** The built-in OpenFreeMap styles, hardcoded so the picker keeps its familiar
 * entries even before (or without) the provider catalog IPC responding. */
const OPENFREEMAP_ENTRIES: readonly BasemapPickerEntry[] = [
  { id: "streets", label: "Streets" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
];

/**
 * Visible picker groups: the OpenFreeMap group first, then one group per
 * *connected*, non-builtin catalog provider — every group carries a heading
 * label. Hidden styles are filtered out (provider styles are hidden by
 * default — see {@link BasemapVisibility}); groups left empty are dropped
 * entirely. ("none" is not a style and is never listed here — the picker
 * renders it separately at the top level.)
 */
export function basemapPickerGroups(
  providers: readonly BasemapProvider[],
  visibility: BasemapVisibility,
): BasemapPickerGroup[] {
  const groups: BasemapPickerGroup[] = [
    {
      providerId: "openfreemap",
      label: "OpenFreeMap",
      entries: OPENFREEMAP_ENTRIES.filter(
        (e) => !isBasemapStyleHidden(e.id, visibility),
      ),
    },
  ];
  for (const p of providers) {
    if (p.builtin || !p.connected) continue;
    groups.push({
      providerId: p.id,
      label: p.displayName,
      entries: p.styles
        .map((s) => ({
          id: basemapIdForCatalogStyle(p.id, s.id),
          label: s.displayName,
        }))
        .filter((e) => !isBasemapStyleHidden(e.id, visibility)),
    });
  }
  return groups.filter((g) => g.entries.length > 0);
}

/** Display label for a basemap id (toolbar button + picker rows). Unresolvable
 * provider ids label as "Streets" — that is what actually renders. */
export function basemapDisplayLabel(
  id: BasemapId,
  providers: readonly BasemapProvider[],
): string {
  if (id === "none") return "No basemap";
  const legacy = OPENFREEMAP_ENTRIES.find((e) => e.id === id);
  if (legacy) return legacy.label;
  const parsed = parseProviderBasemapId(id);
  if (parsed) {
    const style = providers
      .find((p) => p.id === parsed.providerId)
      ?.styles.find((s) => s.id === parsed.styleId);
    if (style) return style.displayName;
  }
  return "Streets";
}
