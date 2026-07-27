//! Static curated catalog of basemap providers and their tile styles.
//!
//! The catalog is compile-time data: providers are never added at runtime.
//! Templates use `{z}`/`{x}`/`{y}` placeholders (any order — Esri is
//! `{z}/{y}/{x}`) plus `{token}` for the credential on paid providers.
//! Built-in providers (`builtin: true`) are rendered directly by the
//! frontend and are never served by the `basemap:` proxy; they appear here
//! only so the management UI can list and show/hide their styles.

use serde::Serialize;

/// Whether a provider requires a paid account / credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Free,
    Paid,
}

/// One tile style offered by a provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStyle {
    pub id: &'static str,
    pub display_name: &'static str,
    /// `{z}`/`{x}`/`{y}`/`{token}` URL template. `None` for built-in styles
    /// the frontend renders directly (the proxy refuses them).
    pub tile_url_template: Option<&'static str>,
    /// Logical tile size in CSS pixels (256 or 512).
    pub tile_size: u16,
    pub max_zoom: u8,
    /// Content-Type fallback when the upstream response omits one.
    pub format: &'static str,
}

/// One provider in the curated catalog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProvider {
    pub id: &'static str,
    pub display_name: &'static str,
    pub kind: ProviderKind,
    /// UI label for the credential ("Access token", "API key");
    /// `None` when the provider needs no credential.
    pub token_label: Option<&'static str>,
    pub signup_url: &'static str,
    pub attribution: &'static str,
    /// Built-in provider: styles are rendered directly by the frontend,
    /// never proxied. Listed only for the management UI.
    pub builtin: bool,
    pub styles: &'static [CatalogStyle],
}

/// The curated provider catalog.
pub const CATALOG: &[CatalogProvider] = &[
    CatalogProvider {
        id: "openfreemap",
        display_name: "OpenFreeMap",
        kind: ProviderKind::Free,
        token_label: None,
        signup_url: "https://openfreemap.org",
        attribution: "OpenFreeMap © OpenMapTiles, data from © OpenStreetMap contributors",
        builtin: true,
        // Built-in vector styles: the frontend keeps loading these directly
        // from tiles.openfreemap.org (already in the CSP); the proxy never
        // serves them, hence no templates.
        styles: &[
            CatalogStyle {
                id: "streets",
                display_name: "Streets",
                tile_url_template: None,
                tile_size: 512,
                max_zoom: 14,
                format: "application/x-protobuf",
            },
            CatalogStyle {
                id: "light",
                display_name: "Light",
                tile_url_template: None,
                tile_size: 512,
                max_zoom: 14,
                format: "application/x-protobuf",
            },
            CatalogStyle {
                id: "dark",
                display_name: "Dark",
                tile_url_template: None,
                tile_size: 512,
                max_zoom: 14,
                format: "application/x-protobuf",
            },
        ],
    },
    CatalogProvider {
        id: "esri",
        display_name: "Esri",
        kind: ProviderKind::Free,
        token_label: None,
        signup_url: "https://www.esri.com/en-us/arcgis/products/arcgis-location-services",
        attribution:
            "Source: Esri, Maxar, Earthstar Geographics, and the GIS User Community",
        builtin: false,
        styles: &[CatalogStyle {
            id: "world-imagery",
            display_name: "World Imagery",
            // NOTE: ArcGIS tile services use {z}/{y}/{x} (row before column).
            tile_url_template: Some(
                "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            ),
            tile_size: 256,
            // World Imagery serves up to z23 in select regions; 19 is the
            // globally safe ceiling.
            max_zoom: 19,
            format: "image/jpeg",
        }],
    },
    CatalogProvider {
        id: "mapbox",
        display_name: "Mapbox",
        kind: ProviderKind::Paid,
        token_label: Some("Access token"),
        signup_url: "https://account.mapbox.com/auth/signup/",
        attribution: "© Mapbox © OpenStreetMap contributors © Maxar",
        builtin: false,
        styles: &[
            CatalogStyle {
                id: "satellite",
                display_name: "Satellite",
                // Raster Tiles API (v4). The tileset grid is 256px; `@2x`
                // returns 512px images, which we declare as 512 logical so
                // imagery renders sharp — labels-free imagery tolerates the
                // half-zoom offset this implies.
                tile_url_template: Some(
                    "https://api.mapbox.com/v4/mapbox.satellite/{z}/{x}/{y}@2x.jpg90?access_token={token}",
                ),
                tile_size: 512,
                max_zoom: 22,
                format: "image/jpeg",
            },
            CatalogStyle {
                id: "satellite-streets",
                display_name: "Satellite Streets",
                // Static Tiles API: /tiles/{tilesize}/{z}/{x}/{y}{@2x}.
                tile_url_template: Some(
                    "https://api.mapbox.com/styles/v1/mapbox/satellite-streets-v12/tiles/512/{z}/{x}/{y}@2x?access_token={token}",
                ),
                tile_size: 512,
                max_zoom: 22,
                format: "image/jpeg",
            },
        ],
    },
    CatalogProvider {
        id: "maptiler",
        display_name: "MapTiler",
        kind: ProviderKind::Paid,
        token_label: Some("API key"),
        signup_url: "https://cloud.maptiler.com/auth/widget?mode=signup",
        attribution: "© MapTiler © OpenStreetMap contributors",
        builtin: false,
        styles: &[
            CatalogStyle {
                id: "satellite",
                display_name: "Satellite",
                // satellite-v2 serves 512px tiles (MapTiler's standard tile
                // size). Docs advertise zoom 0–22, but resolution is 2 m
                // globally with higher detail only in select areas; 20 avoids
                // requesting tiles that may not exist everywhere.
                tile_url_template: Some(
                    "https://api.maptiler.com/tiles/satellite-v2/{z}/{x}/{y}.jpg?key={token}",
                ),
                tile_size: 512,
                max_zoom: 20,
                format: "image/jpeg",
            },
            CatalogStyle {
                id: "streets",
                display_name: "Streets",
                // Rasterized map endpoint; default rasterization is 512px.
                tile_url_template: Some(
                    "https://api.maptiler.com/maps/streets-v2/{z}/{x}/{y}.png?key={token}",
                ),
                tile_size: 512,
                max_zoom: 22,
                format: "image/png",
            },
        ],
    },
];

/// Look up a provider by id.
pub fn provider(id: &str) -> Option<&'static CatalogProvider> {
    CATALOG.iter().find(|p| p.id == id)
}

/// Look up a style within a provider.
pub fn style(
    provider_id: &str,
    style_id: &str,
) -> Option<(&'static CatalogProvider, &'static CatalogStyle)> {
    let p = provider(provider_id)?;
    let s = p.styles.iter().find(|s| s.id == style_id)?;
    Some((p, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for p in CATALOG {
            let mut style_ids: Vec<&str> = p.styles.iter().map(|s| s.id).collect();
            style_ids.sort_unstable();
            style_ids.dedup();
            assert_eq!(
                style_ids.len(),
                p.styles.len(),
                "duplicate style in {}",
                p.id
            );
        }
    }

    #[test]
    fn catalog_invariants() {
        for p in CATALOG {
            match p.kind {
                ProviderKind::Paid => {
                    assert!(p.token_label.is_some(), "{} paid without token label", p.id)
                }
                ProviderKind::Free => {
                    assert!(p.token_label.is_none(), "{} free with token label", p.id)
                }
            }
            for s in p.styles {
                assert!(s.tile_size == 256 || s.tile_size == 512);
                if p.builtin {
                    assert!(
                        s.tile_url_template.is_none(),
                        "builtin {} has template",
                        p.id
                    );
                    continue;
                }
                let t = s
                    .tile_url_template
                    .unwrap_or_else(|| panic!("{}/{} missing template", p.id, s.id));
                assert!(t.contains("{z}") && t.contains("{x}") && t.contains("{y}"));
                assert_eq!(
                    t.contains("{token}"),
                    p.token_label.is_some(),
                    "{}/{}: token placeholder must match token requirement",
                    p.id,
                    s.id
                );
            }
        }
    }

    #[test]
    fn lookups_resolve() {
        assert!(provider("esri").is_some());
        assert!(provider("nope").is_none());
        assert!(style("mapbox", "satellite").is_some());
        assert!(style("mapbox", "nope").is_none());
        assert!(style("nope", "satellite").is_none());
    }
}
