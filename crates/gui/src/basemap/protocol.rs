//! `basemap://` custom-protocol handler.
//!
//! All map traffic in the app flows through this scheme so the webview CSP
//! never needs a remote host: locally stored region tiles now, and the
//! bundled world overview, styles, fonts, and sprites in the next milestone.
//! Responses carry `Access-Control-Allow-Origin: *` because the page origin
//! (`tauri://localhost` / `http://tauri.localhost`) is a different scheme.
//!
//! Routes:
//! - `GET /tiles/{z}/{x}/{y}` (optional `.mvt`/`.pbf` suffix) — a stored
//!   region tile. A miss returns `204 No Content`, which MapLibre treats as
//!   an empty tile rather than an error.
//! - anything else — `404`.

use std::borrow::Cow;
use tauri::http;

use super::store::TileCoord;
use super::BasemapState;

pub fn handle(
    state: Option<&BasemapState>,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    match route(path) {
        Route::Tile(coord) => {
            let Some((state, store)) = state.and_then(|s| s.store().ok().map(|store| (s, store)))
            else {
                return respond(500, &[], b"basemap store unavailable".as_slice().into());
            };
            match store.get_tile(coord) {
                Ok(Some(bytes)) => {
                    let encoding = state.tile_encoding();
                    let mut headers: Vec<(&str, String)> = vec![
                        ("Content-Type", "application/x-protobuf".into()),
                        // Tiles are immutable for a given build; let the
                        // webview cache them freely within a session.
                        ("Cache-Control", "public, max-age=86400".into()),
                    ];
                    if let Some(enc) = encoding {
                        headers.push(("Content-Encoding", enc));
                    }
                    respond(200, &headers, bytes.into())
                }
                Ok(None) => respond(204, &[], Cow::Borrowed(&[])),
                Err(e) => {
                    tracing::warn!("basemap tile read failed: {e}");
                    respond(500, &[], b"tile read failed".as_slice().into())
                }
            }
        }
        Route::NotFound => respond(404, &[], b"not found".as_slice().into()),
    }
}

enum Route {
    Tile(TileCoord),
    NotFound,
}

fn route(path: &str) -> Route {
    let mut parts = path.trim_start_matches('/').split('/');
    match parts.next() {
        Some("tiles") => {
            let (Some(z), Some(x), Some(y)) = (parts.next(), parts.next(), parts.next()) else {
                return Route::NotFound;
            };
            if parts.next().is_some() {
                return Route::NotFound;
            }
            let y = y
                .strip_suffix(".mvt")
                .or(y.strip_suffix(".pbf"))
                .unwrap_or(y);
            match (z.parse(), x.parse(), y.parse()) {
                (Ok(z), Ok(x), Ok(y)) => Route::Tile(TileCoord { z, x, y }),
                _ => Route::NotFound,
            }
        }
        _ => Route::NotFound,
    }
}

fn respond(
    status: u16,
    headers: &[(&str, String)],
    body: Cow<'static, [u8]>,
) -> http::Response<Cow<'static, [u8]>> {
    let mut builder = http::Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*");
    for (k, v) in headers {
        builder = builder.header(*k, v);
    }
    builder.body(body).unwrap_or_else(|e| {
        tracing::error!("basemap response build failed: {e}");
        http::Response::builder()
            .status(500)
            .body(Cow::Borrowed(b"internal error".as_slice()))
            .expect("static fallback response")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coord(r: Route) -> Option<TileCoord> {
        match r {
            Route::Tile(c) => Some(c),
            Route::NotFound => None,
        }
    }

    #[test]
    fn routes_tile_paths_with_and_without_suffix() {
        for path in [
            "/tiles/14/4823/6160",
            "/tiles/14/4823/6160.mvt",
            "/tiles/14/4823/6160.pbf",
        ] {
            let c = coord(route(path)).unwrap();
            assert_eq!((c.z, c.x, c.y), (14, 4823, 6160));
        }
    }

    #[test]
    fn rejects_malformed_tile_paths() {
        for path in [
            "/tiles/14/4823",
            "/tiles/14/4823/6160/extra",
            "/tiles/x/y/z",
            "/tiles/14/4823/6160.png",
            "/styles/liberty.json",
            "/",
        ] {
            assert!(coord(route(path)).is_none(), "should reject {path}");
        }
    }

    #[test]
    fn tile_zoom_overflow_is_rejected() {
        // z must fit u8; 300 must not wrap.
        assert!(coord(route("/tiles/300/0/0")).is_none());
    }
}
