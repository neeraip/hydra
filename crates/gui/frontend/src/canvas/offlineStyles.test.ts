import { afterEach, describe, expect, it, vi } from "vitest";
import {
  buildOfflineStyle,
  type OfflineBasemapStyle,
  offlineTileUrlTemplate,
} from "./offlineStyles";

const STYLES: OfflineBasemapStyle[] = [
  "offline-streets",
  "offline-light",
  "offline-dark",
];

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("offlineTileUrlTemplate", () => {
  it("uses the custom scheme form on non-Windows platforms", () => {
    vi.stubGlobal("navigator", {
      userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    } as Navigator);
    expect(offlineTileUrlTemplate()).toBe(
      "basemap://localhost/tiles/{z}/{x}/{y}.mvt",
    );
  });

  it("uses the http subdomain form on Windows (WebView2)", () => {
    vi.stubGlobal("navigator", {
      userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    } as Navigator);
    expect(offlineTileUrlTemplate()).toBe(
      "http://basemap.localhost/tiles/{z}/{x}/{y}.mvt",
    );
  });
});

describe("buildOfflineStyle", () => {
  it.each(STYLES)("%s has overview and detail sources", (name) => {
    const style = buildOfflineStyle(name);
    const overview = style.sources["offline-overview"];
    const detail = style.sources["offline-detail"];
    expect(overview).toMatchObject({ type: "vector", maxzoom: 6 });
    expect(overview).not.toHaveProperty("minzoom");
    expect(detail).toMatchObject({ type: "vector", minzoom: 7, maxzoom: 15 });
    const template = offlineTileUrlTemplate();
    for (const src of [overview, detail]) {
      expect(src).toMatchObject({ tiles: [template] });
    }
  });

  it.each(STYLES)("%s has unique layer ids", (name) => {
    const { layers } = buildOfflineStyle(name);
    const ids = layers.map((l) => l.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(layers.length).toBeGreaterThan(0);
  });

  it.each(STYLES)("%s layers reference only declared sources", (name) => {
    const style = buildOfflineStyle(name);
    for (const layer of style.layers) {
      if (layer.type === "background") continue;
      expect("source" in layer && layer.source).toMatch(
        /^offline-(overview|detail)$/,
      );
    }
  });

  it.each(STYLES)("%s draws overview layers beneath detail layers", (name) => {
    const { layers } = buildOfflineStyle(name);
    let lastOverview = -1;
    for (let i = 0; i < layers.length; i++) {
      if (layers[i].id.startsWith("ov-")) lastOverview = i;
    }
    const firstDetail = layers.findIndex((l) => l.id.startsWith("dt-"));
    expect(firstDetail).toBeGreaterThan(lastOverview);
    expect(lastOverview).toBeGreaterThanOrEqual(0);
  });

  it.each(STYLES)("%s serves glyphs and sprites from the bundle", (name) => {
    const style = buildOfflineStyle(name);
    expect(style.glyphs).toBe("/basemaps/fonts/{fontstack}/{range}.pbf");
    expect(style.sprite).toMatch(/^\/basemaps\/sprites\/(light|dark)$/);
  });
});
