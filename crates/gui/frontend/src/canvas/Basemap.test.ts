import { describe, expect, it } from "vitest";
import type { BasemapProvider } from "../hooks/basemapProviders";
import {
  type BasemapVisibility,
  basemapDisplayLabel,
  basemapIdForCatalogStyle,
  basemapPickerGroups,
  buildProviderRasterStyle,
  clampBasemapOpacity,
  isBasemapStyleHidden,
  isLegacyBasemapId,
  isValidBasemapId,
  parseProviderBasemapId,
  providerBasemapId,
  providerTileUrlTemplate,
} from "./Basemap";

/** Visibility overrides shorthand. */
function vis(hidden: string[] = [], shown: string[] = []): BasemapVisibility {
  return {
    hiddenLegacyIds: new Set(hidden),
    shownProviderIds: new Set(shown),
  };
}

const MAC_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
const WIN_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120.0";

function provider(overrides: Partial<BasemapProvider>): BasemapProvider {
  return {
    id: "mapbox",
    displayName: "Mapbox",
    kind: "paid",
    builtin: false,
    tokenLabel: "Access token",
    signupUrl: "https://example.com",
    attribution: "© Mapbox",
    connected: false,
    styles: [
      { id: "satellite", displayName: "Satellite", tileSize: 512, maxZoom: 22 },
    ],
    ...overrides,
  };
}

describe("basemap id model", () => {
  it("round-trips provider ids", () => {
    const id = providerBasemapId("mapbox", "satellite");
    expect(id).toBe("provider:mapbox:satellite");
    expect(parseProviderBasemapId(id)).toEqual({
      providerId: "mapbox",
      styleId: "satellite",
    });
  });

  it("rejects legacy and malformed provider ids", () => {
    for (const bad of [
      "streets",
      "none",
      "provider:",
      "provider:mapbox",
      "provider:mapbox:",
      "provider::satellite",
      "provider:mapbox:satellite:extra",
      "prov:mapbox:satellite",
    ]) {
      expect(parseProviderBasemapId(bad)).toBeNull();
    }
  });

  it("keeps the four legacy ids valid (pref compatibility)", () => {
    for (const id of ["streets", "light", "dark", "none"]) {
      expect(isLegacyBasemapId(id)).toBe(true);
      expect(isValidBasemapId(id)).toBe(true);
    }
  });

  it("validates provider ids structurally, rejects junk", () => {
    expect(isValidBasemapId("provider:esri:world-imagery")).toBe(true);
    expect(isValidBasemapId("liberty")).toBe(false);
    expect(isValidBasemapId(42)).toBe(false);
    expect(isValidBasemapId(null)).toBe(false);
  });

  it("maps OpenFreeMap catalog styles onto the legacy ids", () => {
    expect(basemapIdForCatalogStyle("openfreemap", "streets")).toBe("streets");
    expect(basemapIdForCatalogStyle("openfreemap", "dark")).toBe("dark");
    expect(basemapIdForCatalogStyle("esri", "world-imagery")).toBe(
      "provider:esri:world-imagery",
    );
  });
});

describe("clampBasemapOpacity", () => {
  it("passes through in-range values and clamps out-of-range ones", () => {
    expect(clampBasemapOpacity(0)).toBe(0);
    expect(clampBasemapOpacity(0.35)).toBe(0.35);
    expect(clampBasemapOpacity(1)).toBe(1);
    expect(clampBasemapOpacity(-0.5)).toBe(0);
    expect(clampBasemapOpacity(3)).toBe(1);
  });

  it("defaults missing/corrupt values to fully opaque", () => {
    expect(clampBasemapOpacity(undefined)).toBe(1);
    expect(clampBasemapOpacity(null)).toBe(1);
    expect(clampBasemapOpacity("0.5")).toBe(1);
    expect(clampBasemapOpacity(Number.NaN)).toBe(1);
    expect(clampBasemapOpacity(Number.POSITIVE_INFINITY)).toBe(1);
  });
});

describe("providerTileUrlTemplate", () => {
  it("uses the basemap: custom scheme by default", () => {
    expect(providerTileUrlTemplate("esri", "world-imagery", MAC_UA)).toBe(
      "basemap://localhost/provider/esri/world-imagery/{z}/{x}/{y}",
    );
  });

  it("uses http://basemap.localhost on Windows webviews", () => {
    expect(providerTileUrlTemplate("mapbox", "satellite", WIN_UA)).toBe(
      "http://basemap.localhost/provider/mapbox/satellite/{z}/{x}/{y}",
    );
  });
});

describe("buildProviderRasterStyle", () => {
  it("builds one raster source + background + raster layer", () => {
    const style = buildProviderRasterStyle(
      {
        providerId: "mapbox",
        styleId: "satellite",
        tileSize: 512,
        maxZoom: 22,
        attribution: "© Mapbox",
      },
      MAC_UA,
    );
    expect(style.version).toBe(8);
    expect(style.sources).toEqual({
      "provider-tiles": {
        type: "raster",
        tiles: ["basemap://localhost/provider/mapbox/satellite/{z}/{x}/{y}"],
        tileSize: 512,
        maxzoom: 22,
        attribution: "© Mapbox",
      },
    });
    expect(style.layers.map((l) => l.type)).toEqual(["background", "raster"]);
    const raster = style.layers[1];
    expect(raster).toMatchObject({ source: "provider-tiles" });
  });
});

describe("isBasemapStyleHidden", () => {
  it("legacy ids default to visible; hide-list overrides", () => {
    expect(isBasemapStyleHidden("streets", vis())).toBe(false);
    expect(isBasemapStyleHidden("light", vis(["light"]))).toBe(true);
  });

  it("provider ids default to hidden; shown-list overrides", () => {
    expect(isBasemapStyleHidden("provider:esri:world-imagery", vis())).toBe(
      true,
    );
    expect(
      isBasemapStyleHidden(
        "provider:esri:world-imagery",
        vis([], ["provider:esri:world-imagery"]),
      ),
    ).toBe(false);
  });
});

describe("basemapPickerGroups", () => {
  const esri = provider({
    id: "esri",
    displayName: "Esri",
    kind: "free",
    tokenLabel: null,
    connected: true,
    styles: [
      {
        id: "world-imagery",
        displayName: "World Imagery",
        tileSize: 256,
        maxZoom: 19,
      },
    ],
  });

  it("defaults to the labeled OpenFreeMap group even with no catalog", () => {
    const groups = basemapPickerGroups([], vis());
    expect(groups).toHaveLength(1);
    expect(groups[0].label).toBe("OpenFreeMap");
    expect(groups[0].entries.map((e) => e.id)).toEqual([
      "streets",
      "light",
      "dark",
    ]);
  });

  it("hides connected providers' styles by default (until explicitly shown)", () => {
    const groups = basemapPickerGroups([esri], vis());
    expect(groups.map((g) => g.providerId)).toEqual(["openfreemap"]);
  });

  it("lists explicitly shown styles of connected providers, skipping disconnected + builtin ones", () => {
    const providers = [
      provider({
        id: "openfreemap",
        displayName: "OpenFreeMap",
        kind: "free",
        builtin: true,
        tokenLabel: null,
        connected: true,
        styles: [
          { id: "streets", displayName: "Streets", tileSize: 512, maxZoom: 14 },
        ],
      }),
      esri,
      provider({ id: "mapbox", connected: false }),
    ];
    const shown = [
      "provider:esri:world-imagery",
      // Shown but disconnected — must still be excluded.
      "provider:mapbox:satellite",
    ];
    const groups = basemapPickerGroups(providers, vis([], shown));
    expect(groups.map((g) => g.providerId)).toEqual(["openfreemap", "esri"]);
    expect(groups[1].label).toBe("Esri");
    expect(groups[1].entries).toEqual([
      { id: "provider:esri:world-imagery", label: "World Imagery" },
    ]);
  });

  it("filters hidden legacy styles and drops emptied groups", () => {
    const groups = basemapPickerGroups([esri], vis(["light"]));
    expect(groups).toHaveLength(1);
    expect(groups[0].entries.map((e) => e.id)).toEqual(["streets", "dark"]);
  });

  it("returns no groups when everything is hidden", () => {
    expect(
      basemapPickerGroups([esri], vis(["streets", "light", "dark"])),
    ).toEqual([]);
  });
});

describe("basemapDisplayLabel", () => {
  const providers = [provider({ connected: true })];

  it("labels legacy ids and none", () => {
    expect(basemapDisplayLabel("none", providers)).toBe("No basemap");
    expect(basemapDisplayLabel("streets", providers)).toBe("Streets");
  });

  it("labels provider styles by display name", () => {
    expect(basemapDisplayLabel("provider:mapbox:satellite", providers)).toBe(
      "Satellite",
    );
  });

  it("falls back to Streets for unresolvable ids (what actually renders)", () => {
    expect(basemapDisplayLabel("provider:gone:x", providers)).toBe("Streets");
    expect(basemapDisplayLabel("garbage", providers)).toBe("Streets");
  });
});
