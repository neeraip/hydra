/**
 * Basemap-provider catalog IPC + two small module-level stores (the units.ts
 * useSyncExternalStore pattern — no context/provider needed):
 *
 * - the provider catalog with per-provider connection status, shared by the
 *   canvas basemap picker, MapCanvas's style resolver, and the providers
 *   modal, refreshed after connect/disconnect;
 * - the per-machine style-visibility pref (Copilot-style: hiding removes a
 *   style from the picker, never from the active map), persisted globally in
 *   localStorage — deliberately NOT per project. OpenFreeMap styles default
 *   to visible, provider styles to hidden, so the pref stores explicit
 *   overrides in both directions (see BasemapVisibility).
 */

import { useEffect, useSyncExternalStore } from "react";
import {
  type BasemapVisibility,
  basemapStyleDefaultsVisible,
  parseProviderBasemapId,
} from "../canvas/Basemap";
import { invoke, tryInvoke } from "./ipc";

// ── DTO types (match commands::basemap_providers serde camelCase) ────────────

/**
 * Whether this provider has anything to connect to.
 *
 * `connected` answers two questions at once: for a provider that needs a
 * credential it means one is stored, and for a free one it is hard-coded
 * true because there was never anything to store. Rendering it either way
 * put a "Connected" badge on OpenFreeMap and Esri, which had connected to
 * nothing — nothing was asked of the reader and nothing is being reported.
 *
 * The credential label is the honest test: a provider that needs one has a
 * connection state worth showing, and one that does not has none.
 */
export function needsCredential(provider: {
  tokenLabel?: string | null;
}): boolean {
  return provider.tokenLabel != null;
}

export interface BasemapProviderStyle {
  id: string;
  displayName: string;
  /** Logical tile size in CSS pixels (256 or 512). */
  tileSize: number;
  maxZoom: number;
}

export interface BasemapProvider {
  id: string;
  displayName: string;
  kind: "free" | "paid";
  /** Built-in providers (OpenFreeMap) are rendered directly by the frontend
   * and never proxied; listed only for the management UI. */
  builtin: boolean;
  /** UI label for the credential ("Access token", "API key"); absent when the
   * provider needs no credential. */
  tokenLabel?: string | null;
  signupUrl: string;
  attribution: string;
  /** `true` for free/built-in providers, or when a token is stored. */
  connected: boolean;
  /** Redacted stored token (`abcd…wxyz`); absent when disconnected or free. */
  tokenPreview?: string | null;
  styles: BasemapProviderStyle[];
}

// ── Provider catalog store ───────────────────────────────────────────────────

let providers: BasemapProvider[] = [];
let providersFetched = false;
const providerListeners = new Set<() => void>();

function emitProviders(): void {
  for (const l of providerListeners) l();
}

function subscribeProviders(cb: () => void): () => void {
  providerListeners.add(cb);
  return () => providerListeners.delete(cb);
}

function getProviders(): BasemapProvider[] {
  return providers;
}

/** Re-fetch the catalog + connection status. Outside a Tauri shell this
 * resolves silently and the catalog stays empty (the picker falls back to the
 * hardcoded OpenFreeMap entries). */
export async function refreshBasemapProviders(): Promise<void> {
  const rows = await tryInvoke<BasemapProvider[]>("list_basemap_providers");
  if (rows === null) return;
  providers = rows;
  providersFetched = true;
  emitProviders();
}

/** Current provider catalog; re-renders the caller when it changes. Triggers
 * the initial fetch on first use. */
export function useBasemapProviders(): BasemapProvider[] {
  useEffect(() => {
    if (!providersFetched) void refreshBasemapProviders();
  }, []);
  return useSyncExternalStore(subscribeProviders, getProviders, getProviders);
}

/**
 * Validate + store a paid provider's token. The backend performs a live tile
 * fetch (slow — show a spinner) and throws with a human-readable message on
 * an invalid token. On success the shared catalog store is updated in place.
 */
export async function connectBasemapProvider(
  providerId: string,
  token: string,
): Promise<BasemapProvider> {
  const updated = await invoke<BasemapProvider>("connect_basemap_provider", {
    providerId,
    token,
  });
  providers = providers.map((p) => (p.id === updated.id ? updated : p));
  emitProviders();
  return updated;
}

/** Delete a provider's stored token and mark it disconnected in the store. */
export async function disconnectBasemapProvider(
  providerId: string,
): Promise<void> {
  await invoke<void>("disconnect_basemap_provider", { providerId });
  providers = providers.map((p) =>
    p.id === providerId
      ? { ...p, connected: p.tokenLabel == null, tokenPreview: null }
      : p,
  );
  emitProviders();
}

// ── Style visibility pref ────────────────────────────────────────────────────
//
// Defaults differ by style origin (OpenFreeMap visible, provider styles
// hidden — even when connected), so the pref stores explicit overrides in
// both directions; see BasemapVisibility in canvas/Basemap.

const VISIBILITY_KEY = "hydra2-basemap-visibility";
/** Pre-two-list hide-list key; read once as a migration source, never
 * written. (Provider ids it contains were *hidden*, which is now the
 * default, so only its legacy-id entries carry information.) */
const LEGACY_HIDDEN_STYLES_KEY = "hydra2-basemap-hidden-styles";

function stringSet(value: unknown): ReadonlySet<string> {
  return new Set(
    Array.isArray(value)
      ? value.filter((v): v is string => typeof v === "string")
      : [],
  );
}

function readVisibility(): BasemapVisibility {
  try {
    if (typeof localStorage !== "undefined") {
      const raw = localStorage.getItem(VISIBILITY_KEY);
      if (raw) {
        const parsed: unknown = JSON.parse(raw);
        if (typeof parsed === "object" && parsed !== null) {
          const o = parsed as Record<string, unknown>;
          return {
            hiddenLegacyIds: stringSet(o.hiddenLegacyIds),
            shownProviderIds: stringSet(o.shownProviderIds),
          };
        }
      }
      // Migrate the old single hide-list: its legacy-id entries become
      // hidden overrides; provider entries are dropped (hidden is now the
      // default). The stale key is left in place and simply ignored once
      // the new key exists.
      const oldRaw = localStorage.getItem(LEGACY_HIDDEN_STYLES_KEY);
      if (oldRaw) {
        const old = stringSet(JSON.parse(oldRaw));
        return {
          hiddenLegacyIds: new Set(
            [...old].filter((id) => parseProviderBasemapId(id) === null),
          ),
          shownProviderIds: new Set(),
        };
      }
    }
  } catch {
    // Corrupt/unavailable storage — fall through to the defaults.
  }
  return { hiddenLegacyIds: new Set(), shownProviderIds: new Set() };
}

let visibility: BasemapVisibility = readVisibility();
const visibilityListeners = new Set<() => void>();

function subscribeVisibility(cb: () => void): () => void {
  visibilityListeners.add(cb);
  return () => visibilityListeners.delete(cb);
}

export function getBasemapVisibility(): BasemapVisibility {
  return visibility;
}

function sameSet(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  return a.size === b.size && [...a].every((id) => b.has(id));
}

/** Hide or unhide a set of picker style ids (legacy ids for OpenFreeMap,
 * `provider:{providerId}:{styleId}` otherwise). Each id is routed to the
 * override list matching its default: hiding a legacy id records it in
 * `hiddenLegacyIds`; showing a provider id records it in `shownProviderIds`.
 * Persisted best-effort. */
export function setBasemapStylesHidden(ids: string[], hide: boolean): void {
  const hiddenLegacyIds = new Set(visibility.hiddenLegacyIds);
  const shownProviderIds = new Set(visibility.shownProviderIds);
  for (const id of ids) {
    // Routed by the id's default rather than by whether it is a provider
    // style: one provider style now starts visible, so recording it in the
    // "explicitly shown" set would mean hiding it did nothing.
    if (basemapStyleDefaultsVisible(id)) {
      if (hide) hiddenLegacyIds.add(id);
      else hiddenLegacyIds.delete(id);
    } else if (hide) {
      shownProviderIds.delete(id);
    } else {
      shownProviderIds.add(id);
    }
  }
  if (
    sameSet(hiddenLegacyIds, visibility.hiddenLegacyIds) &&
    sameSet(shownProviderIds, visibility.shownProviderIds)
  ) {
    return;
  }
  visibility = { hiddenLegacyIds, shownProviderIds };
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(
        VISIBILITY_KEY,
        JSON.stringify({
          hiddenLegacyIds: [...hiddenLegacyIds],
          shownProviderIds: [...shownProviderIds],
        }),
      );
    }
  } catch {
    // Persistence is best-effort.
  }
  for (const l of visibilityListeners) l();
}

/** Current visibility overrides; re-renders the caller on change. */
export function useBasemapVisibility(): BasemapVisibility {
  return useSyncExternalStore(
    subscribeVisibility,
    getBasemapVisibility,
    getBasemapVisibility,
  );
}
