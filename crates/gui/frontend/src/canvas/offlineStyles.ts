import { type Flavor, layers, namedFlavor } from "@protomaps/basemaps";
import type * as maplibregl from "maplibre-gl";

/** Basemap entries rendered from locally downloaded Protomaps tiles. */
export type OfflineBasemapStyle =
  | "offline-streets"
  | "offline-light"
  | "offline-dark";

// Flavor per entry. "light" is the fullest-colour Protomaps flavor and so the
// closest match to the online Liberty streets style; "white" is the minimal
// light theme (Positron analog); "dark" mirrors the online dark theme. The
// assets repo only ships "light" and "dark" sprite sheets, so "white" borrows
// the light sheet (its icons are neutral greys that suit both).
const FLAVOR_CONFIG: Record<
  OfflineBasemapStyle,
  { flavor: string; sprite: "light" | "dark" }
> = {
  "offline-streets": { flavor: "light", sprite: "light" },
  "offline-light": { flavor: "white", sprite: "light" },
  "offline-dark": { flavor: "dark", sprite: "dark" },
};

/** Highest zoom present in downloaded region tiles. */
const DETAIL_MAXZOOM = 15;
/** Highest zoom of the world-overview region (z0–6). */
const OVERVIEW_MAXZOOM = 6;

/**
 * Tile URL template for the local `basemap` custom protocol. Tauri custom
 * protocols surface as `<scheme>://localhost/…` on macOS/Linux (WKWebView /
 * WebKitGTK) but as `http://<scheme>.localhost/…` on Windows (WebView2), so
 * the template must be chosen at runtime.
 */
export function offlineTileUrlTemplate(): string {
  const isWindows =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
  return isWindows
    ? "http://basemap.localhost/tiles/{z}/{x}/{y}.mvt"
    : "basemap://localhost/tiles/{z}/{x}/{y}.mvt";
}

const ATTRIBUTION =
  '<a href="https://github.com/protomaps/basemaps">Protomaps</a> © <a href="https://osm.org/copyright">OpenStreetMap</a>';

/** Prefix every layer id so the overview and detail copies never collide. */
function prefixIds(
  prefix: string,
  layerSpecs: ReturnType<typeof layers>,
): ReturnType<typeof layers> {
  return layerSpecs.map((l) => ({ ...l, id: `${prefix}-${l.id}` }));
}

/**
 * Build a complete offline MapLibre style for one flavor.
 *
 * The same tile endpoint backs TWO vector sources: `offline-overview`
 * (maxzoom 6, no minzoom) and `offline-detail` (z7–15). Region downloads
 * cover z7–15 only, so past z15 MapLibre overzooms detail tiles, and wherever
 * a detail tile is missing (the store answers 204) the overview source's
 * overzoomed z0–6 data still renders underneath — a blurry world instead of
 * blank void. The flavor's layer set is generated once per source, overview
 * copy first so detail draws on top.
 *
 * Glyphs and sprites are served from the app bundle (root-absolute paths
 * resolve against the webview origin). Only Latin glyph ranges are bundled —
 * see public/basemaps/README.md.
 */
export function buildOfflineStyle(
  name: OfflineBasemapStyle,
): maplibregl.StyleSpecification {
  const { flavor: flavorName, sprite } = FLAVOR_CONFIG[name];
  const flavor: Flavor = namedFlavor(flavorName);
  const template = offlineTileUrlTemplate();

  const overviewLayers = prefixIds(
    "ov",
    layers("offline-overview", flavor, { lang: "en" }),
  );
  // The background layer is source-less; the overview copy already paints it,
  // so drop the duplicate from the detail copy.
  const detailLayers = prefixIds(
    "dt",
    layers("offline-detail", flavor, { lang: "en" }).filter(
      (l) => l.type !== "background",
    ),
  );

  return {
    version: 8,
    glyphs: "/basemaps/fonts/{fontstack}/{range}.pbf",
    sprite: `/basemaps/sprites/${sprite}`,
    sources: {
      "offline-overview": {
        type: "vector",
        tiles: [template],
        maxzoom: OVERVIEW_MAXZOOM,
        attribution: ATTRIBUTION,
      },
      "offline-detail": {
        type: "vector",
        tiles: [template],
        minzoom: OVERVIEW_MAXZOOM + 1,
        maxzoom: DETAIL_MAXZOOM,
      },
    },
    layers: [...overviewLayers, ...detailLayers],
  };
}
