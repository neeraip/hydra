//! Offline basemap subsystem: shared tile store, `basemap://` protocol,
//! and the region download pipeline.
//!
//! Design (see the project plan): tiles are stored once, globally, in a
//! single SQLite database; regions and project associations are metadata.
//! The webview reaches all basemap data — stored tiles today, bundled
//! overview/styles/assets next — exclusively through the `basemap://`
//! custom protocol, so the CSP never needs a remote tile host.

mod extract;
mod pmtiles;
mod protocol;
mod store;

pub use extract::{plan_extract, run_extract, tiles_in_bbox, ExtractPlan};
pub use pmtiles::{Archive, FileSource, HttpSource};
pub use store::{RegionInfo, TileCoord, TileStore};

use std::sync::OnceLock;

/// Tauri-managed handle to the tile store.
///
/// The store opens lazily on first use so a missing or unwritable app-data
/// directory degrades to per-request errors instead of aborting startup.
pub struct BasemapState {
    db_path: std::path::PathBuf,
    store: OnceLock<Result<TileStore, String>>,
    /// `Content-Encoding` of stored tiles, cached from store meta.
    encoding: OnceLock<Option<String>>,
}

impl BasemapState {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self {
            db_path,
            store: OnceLock::new(),
            encoding: OnceLock::new(),
        }
    }

    /// The store, opened on first use.
    pub fn store(&self) -> Result<&TileStore, String> {
        self.store
            .get_or_init(|| TileStore::open(&self.db_path))
            .as_ref()
            .map_err(|e| e.clone())
    }

    /// The `Content-Encoding` header value for stored tiles, if any
    /// (planet builds ship gzip-compressed MVT; we store bytes as-is).
    pub fn tile_encoding(&self) -> Option<String> {
        self.encoding
            .get_or_init(|| {
                self.store()
                    .ok()
                    .and_then(|s| s.meta_get("tile_compression").ok().flatten())
                    .filter(|v| v != "none")
            })
            .clone()
    }
}

/// Route a `basemap://` request against the (possibly absent) managed state.
pub fn protocol_response(
    state: Option<&BasemapState>,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<std::borrow::Cow<'static, [u8]>> {
    protocol::handle(state, request)
}
