import { afterEach, describe, expect, it, vi } from "vitest";

// Mock the Tauri IPC seam so we can drive success/rejection without a shell.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { BasemapProvider } from "./basemapProviders";
import {
  connectBasemapProvider,
  getBasemapVisibility,
  setBasemapStylesHidden,
} from "./basemapProviders";

const mockInvoke = vi.mocked(tauriInvoke);

function stubTauriShell() {
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
}

/** In-memory localStorage stand-in for the Node test environment. */
function stubLocalStorage(seed: Record<string, string> = {}) {
  const store = new Map(Object.entries(seed));
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
  });
  return store;
}

afterEach(() => {
  vi.unstubAllGlobals();
  mockInvoke.mockReset();
  // Reset the visibility store between tests: drop both override lists.
  const v = getBasemapVisibility();
  setBasemapStylesHidden([...v.hiddenLegacyIds], false);
  setBasemapStylesHidden([...v.shownProviderIds], true);
});

describe("visibility store", () => {
  it("defaults to no overrides (OpenFreeMap visible, provider styles hidden)", () => {
    expect(getBasemapVisibility().hiddenLegacyIds.size).toBe(0);
    expect(getBasemapVisibility().shownProviderIds.size).toBe(0);
  });

  it("routes ids to the override list matching their default, persisting best-effort", () => {
    const store = stubLocalStorage();

    // Hide a legacy id (default visible) + show a provider id (default hidden).
    setBasemapStylesHidden(["light"], true);
    setBasemapStylesHidden(["provider:esri:world-imagery"], false);
    expect(getBasemapVisibility().hiddenLegacyIds.has("light")).toBe(true);
    expect(
      getBasemapVisibility().shownProviderIds.has(
        "provider:esri:world-imagery",
      ),
    ).toBe(true);
    expect(JSON.parse(store.get("hydra2-basemap-visibility") ?? "{}")).toEqual({
      hiddenLegacyIds: ["light"],
      shownProviderIds: ["provider:esri:world-imagery"],
    });

    // Flip both back to their defaults — the overrides clear.
    setBasemapStylesHidden(["light"], false);
    setBasemapStylesHidden(["provider:esri:world-imagery"], true);
    expect(getBasemapVisibility().hiddenLegacyIds.size).toBe(0);
    expect(getBasemapVisibility().shownProviderIds.size).toBe(0);
  });

  it("no-ops (keeps the same snapshot) when nothing changes", () => {
    const before = getBasemapVisibility();
    // Both calls match the defaults, so no override changes.
    setBasemapStylesHidden(["streets"], false);
    setBasemapStylesHidden(["provider:mapbox:satellite"], true);
    expect(getBasemapVisibility()).toBe(before);
  });

  it("migrates the old hide-list key: legacy ids carry over, provider ids drop", async () => {
    stubLocalStorage({
      "hydra2-basemap-hidden-styles": JSON.stringify([
        "dark",
        "provider:esri:world-imagery",
      ]),
    });
    vi.resetModules();
    const fresh = await import("./basemapProviders");
    const v = fresh.getBasemapVisibility();
    expect([...v.hiddenLegacyIds]).toEqual(["dark"]);
    expect(v.shownProviderIds.size).toBe(0);
  });

  it("ignores a corrupt visibility key without crashing", async () => {
    stubLocalStorage({ "hydra2-basemap-visibility": "not json{" });
    vi.resetModules();
    const fresh = await import("./basemapProviders");
    const v = fresh.getBasemapVisibility();
    expect(v.hiddenLegacyIds.size).toBe(0);
    expect(v.shownProviderIds.size).toBe(0);
  });
});

describe("connectBasemapProvider", () => {
  it("propagates backend validation errors to the caller", async () => {
    stubTauriShell();
    mockInvoke.mockRejectedValueOnce("Mapbox rejected the token");
    await expect(connectBasemapProvider("mapbox", "bad")).rejects.toBe(
      "Mapbox rejected the token",
    );
    expect(mockInvoke).toHaveBeenCalledWith("connect_basemap_provider", {
      providerId: "mapbox",
      token: "bad",
    });
  });

  it("resolves the updated provider row on success", async () => {
    stubTauriShell();
    const row = {
      id: "mapbox",
      displayName: "Mapbox",
      kind: "paid",
      builtin: false,
      tokenLabel: "Access token",
      signupUrl: "https://example.com",
      attribution: "© Mapbox",
      connected: true,
      tokenPreview: "pk.a…mnop",
      styles: [],
    } satisfies BasemapProvider;
    mockInvoke.mockResolvedValueOnce(row);
    await expect(connectBasemapProvider("mapbox", "pk.token")).resolves.toEqual(
      row,
    );
  });
});
