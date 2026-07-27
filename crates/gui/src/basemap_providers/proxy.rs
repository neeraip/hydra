//! `basemap:` custom-protocol tile proxy.
//!
//! Route: `/provider/{providerId}/{styleId}/{z}/{x}/{y}` — looks up the
//! catalog entry, substitutes the tile URL template (token from the keyring,
//! cached in [`ProvidersState`] after the first read), fetches the tile
//! upstream, and relays body + `Content-Type`. Responses carry
//! `Access-Control-Allow-Origin: *` because the page origin
//! (`tauri://localhost` / `http://tauri.localhost`) is a different scheme.
//!
//! Status mapping:
//! - malformed path, unknown/built-in provider or style → `404`
//! - missing token for a tokenized template → `401`
//! - upstream `401`/`403` → `401` (so the frontend can detect token problems)
//! - upstream network error or any other non-200 → `502`
//! - success → `200` with `Cache-Control: public, max-age=600` — a short TTL
//!   on purpose: provider terms of service forbid persistent tile caching.

use std::borrow::Cow;

use tauri::http;

use super::catalog;
use super::ProvidersState;

/// A parsed proxy tile request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRequest {
    pub provider_id: String,
    pub style_id: String,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// Parse `/provider/{providerId}/{styleId}/{z}/{x}/{y}`.
pub fn parse_path(path: &str) -> Option<TileRequest> {
    let mut parts = path.trim_start_matches('/').split('/');
    if parts.next()? != "provider" {
        return None;
    }
    let provider_id = parts.next()?;
    let style_id = parts.next()?;
    let z: u8 = parts.next()?.parse().ok()?;
    let x: u32 = parts.next()?.parse().ok()?;
    let y: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || provider_id.is_empty() || style_id.is_empty() {
        return None;
    }
    Some(TileRequest {
        provider_id: provider_id.to_string(),
        style_id: style_id.to_string(),
        z,
        x,
        y,
    })
}

/// Substitute `{z}`/`{x}`/`{y}`/`{token}` in a tile URL template. Placeholder
/// order in the template is free — Esri's `{z}/{y}/{x}` needs no special
/// handling here.
pub fn substitute_template(template: &str, z: u8, x: u32, y: u32, token: Option<&str>) -> String {
    let url = template
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string());
    match token {
        Some(t) => url.replace("{token}", t),
        None => url,
    }
}

/// Handle one `basemap:` scheme request end to end (blocking; run off the
/// main thread — see the registration site in `main.rs`).
pub fn handle(
    state: &ProvidersState,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    let Some(tile) = parse_path(path) else {
        return respond(404, None, b"not found".as_slice().into());
    };
    let Some((provider, style)) = catalog::style(&tile.provider_id, &tile.style_id) else {
        return respond(404, None, b"unknown provider or style".as_slice().into());
    };
    // Built-in providers are rendered directly by the frontend; a template-less
    // style has nothing to proxy either way.
    let Some(template) = style.tile_url_template.filter(|_| !provider.builtin) else {
        return respond(404, None, b"style not proxied".as_slice().into());
    };

    let token = if template.contains("{token}") {
        match state.token(&tile.provider_id) {
            Ok(Some(t)) => Some(t),
            Ok(None) => return respond(401, None, b"no token configured".as_slice().into()),
            Err(e) => {
                tracing::warn!("basemap proxy: token read failed: {e}");
                return respond(502, None, b"token store unavailable".as_slice().into());
            }
        }
    } else {
        None
    };

    let url = substitute_template(template, tile.z, tile.x, tile.y, token.as_deref());
    let client = match state.client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("basemap proxy: http client init failed: {e}");
            return respond(502, None, b"http client unavailable".as_slice().into());
        }
    };

    let response = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("basemap proxy: upstream fetch failed: {e}");
            return respond(502, None, b"upstream fetch failed".as_slice().into());
        }
    };
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        // Pass token problems through as 401 so the frontend can prompt.
        return respond(401, None, b"upstream rejected token".as_slice().into());
    }
    if !status.is_success() {
        tracing::debug!("basemap proxy: upstream returned {status} for {}", path);
        return respond(502, None, b"upstream error".as_slice().into());
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| style.format.to_string());
    match response.bytes() {
        Ok(body) => respond(200, Some(&content_type), body.to_vec().into()),
        Err(e) => {
            tracing::debug!("basemap proxy: upstream body read failed: {e}");
            respond(502, None, b"upstream body read failed".as_slice().into())
        }
    }
}

fn respond(
    status: u16,
    content_type: Option<&str>,
    body: Cow<'static, [u8]>,
) -> http::Response<Cow<'static, [u8]>> {
    let mut builder = http::Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*");
    if let Some(ct) = content_type {
        builder = builder
            .header("Content-Type", ct)
            // Short-lived in-memory/webview caching only: provider ToS forbid
            // persistent tile caches.
            .header("Cache-Control", "public, max-age=600");
    }
    builder.body(body).unwrap_or_else(|e| {
        tracing::error!("basemap proxy: response build failed: {e}");
        http::Response::builder()
            .status(500)
            .body(Cow::Borrowed(b"internal error".as_slice()))
            .expect("static fallback response")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_paths() {
        let t = parse_path("/provider/mapbox/satellite/12/654/1583").unwrap();
        assert_eq!(t.provider_id, "mapbox");
        assert_eq!(t.style_id, "satellite");
        assert_eq!((t.z, t.x, t.y), (12, 654, 1583));
    }

    #[test]
    fn rejects_malformed_paths() {
        for path in [
            "/",
            "/provider",
            "/provider/mapbox",
            "/provider/mapbox/satellite",
            "/provider/mapbox/satellite/12",
            "/provider/mapbox/satellite/12/654",
            "/provider/mapbox/satellite/12/654/1583/extra",
            "/provider/mapbox/satellite/z/x/y",
            "/provider/mapbox/satellite/300/0/0", // z must fit u8
            "/provider/mapbox/satellite/12/-1/0", // no negative coordinates
            "/tiles/12/654/1583",
        ] {
            assert!(parse_path(path).is_none(), "should reject {path}");
        }
    }

    #[test]
    fn substitutes_zxy_and_token() {
        let url = substitute_template(
            "https://api.mapbox.com/v4/mapbox.satellite/{z}/{x}/{y}@2x.jpg90?access_token={token}",
            12,
            654,
            1583,
            Some("pk.secret"),
        );
        assert_eq!(
            url,
            "https://api.mapbox.com/v4/mapbox.satellite/12/654/1583@2x.jpg90?access_token=pk.secret"
        );
    }

    #[test]
    fn substitutes_esri_yx_order_via_template() {
        let url = substitute_template(
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
            5,
            9,
            12,
            None,
        );
        assert_eq!(
            url,
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/5/12/9"
        );
    }

    #[test]
    fn tokenless_substitution_leaves_no_placeholder() {
        let url = substitute_template(
            "https://x.test/{z}/{x}/{y}.png?key={token}",
            1,
            2,
            3,
            Some("k"),
        );
        assert!(!url.contains('{'), "unsubstituted placeholder in {url}");
    }
}
