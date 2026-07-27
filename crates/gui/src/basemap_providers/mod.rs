//! Basemap-provider backend: curated catalog, keyring token storage,
//! token validation, and the `basemap:` tile proxy.
//!
//! The frontend renders built-in (OpenFreeMap) styles directly; every other
//! catalog style is fetched through the `basemap:` custom protocol so paid
//! tokens never reach the webview. See the module docs of [`catalog`],
//! [`tokens`], [`validate`], and [`proxy`] for details.

pub mod catalog;
pub mod proxy;
pub mod tokens;
pub mod validate;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;

/// Shared managed state for the provider subsystem: an in-memory token cache
/// (avoids a keyring round-trip per tile) and a lazily built blocking HTTP
/// client shared by the proxy and token validation.
///
/// Cheaply clonable (`Arc` inner) so protocol-handler threads and
/// `spawn_blocking` closures can hold it with a `'static` lifetime.
#[derive(Clone, Default)]
pub struct ProvidersState {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    token_cache: Mutex<HashMap<String, String>>,
    client: OnceLock<reqwest::blocking::Client>,
}

impl ProvidersState {
    /// The shared blocking HTTP client (10 s timeout).
    ///
    /// Blocking: must only be used off the async runtime (protocol-handler
    /// threads, `spawn_blocking`).
    pub fn client(&self) -> Result<&reqwest::blocking::Client, String> {
        if let Some(client) = self.inner.client.get() {
            return Ok(client);
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("failed to build http client: {e}"))?;
        // A concurrent init may win the race; the loser's client is dropped.
        Ok(self.inner.client.get_or_init(|| client))
    }

    /// Token for a provider: cache first, then the keyring (caching a hit).
    pub fn token(&self, provider_id: &str) -> Result<Option<String>, String> {
        if let Some(token) = self.inner.token_cache.lock().get(provider_id) {
            return Ok(Some(token.clone()));
        }
        let token = tokens::get_token(provider_id)?;
        if let Some(token) = &token {
            self.inner
                .token_cache
                .lock()
                .insert(provider_id.to_string(), token.clone());
        }
        Ok(token)
    }

    /// Put a token into the in-memory cache.
    pub fn cache_token(&self, provider_id: &str, token: &str) {
        self.inner
            .token_cache
            .lock()
            .insert(provider_id.to_string(), token.to_string());
    }

    /// Drop a provider's token from the in-memory cache.
    pub fn drop_token(&self, provider_id: &str) {
        self.inner.token_cache.lock().remove(provider_id);
    }
}
