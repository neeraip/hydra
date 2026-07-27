//! Basemap-provider commands: catalog listing with connection status, and
//! connect/disconnect of paid-provider tokens.
//!
//! Tokens live in the OS credential store (see
//! [`crate::basemap_providers::tokens`]) and are cached in
//! [`ProvidersState`]; the webview only ever sees a redacted preview.

use serde::Serialize;

use crate::basemap_providers::{catalog, tokens, validate, ProvidersState};

/// One catalog provider plus its connection status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasemapProviderDto {
    #[serde(flatten)]
    pub catalog: &'static catalog::CatalogProvider,
    /// `true` for free/built-in providers, or when a token is stored.
    pub connected: bool,
    /// Redacted stored token (`abcd…wxyz`); `None` when disconnected or free.
    pub token_preview: Option<String>,
}

/// Redact a token to its first and last four characters.
fn token_preview(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        // Too short to redact meaningfully; show nothing but the mask.
        "…".to_string()
    } else {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    }
}

/// Build the status DTO for one provider. A keyring read failure degrades to
/// "not connected" (with a warning log) rather than failing the whole list —
/// a headless or locked keychain must not blank the management UI.
fn status_for(
    provider: &'static catalog::CatalogProvider,
    state: &ProvidersState,
) -> BasemapProviderDto {
    let stored = if provider.token_label.is_some() {
        match state.token(provider.id) {
            Ok(token) => token,
            Err(e) => {
                tracing::warn!("basemap providers: token lookup failed: {e}");
                None
            }
        }
    } else {
        None
    };
    BasemapProviderDto {
        catalog: provider,
        connected: provider.token_label.is_none() || stored.is_some(),
        token_preview: stored.as_deref().map(token_preview),
    }
}

#[tauri::command]
/// Return the curated provider catalog with per-provider connection status.
pub fn list_basemap_providers(state: tauri::State<'_, ProvidersState>) -> Vec<BasemapProviderDto> {
    catalog::CATALOG
        .iter()
        .map(|p| status_for(p, &state))
        .collect()
}

#[tauri::command]
/// Validate and store a provider token, then return the provider's status.
///
/// Free providers need no token: validation and storage are skipped. For paid
/// providers the token is validated against a live tile fetch before being
/// written to the OS credential store and the in-memory cache.
///
/// Async so the blocking validation fetch runs on a `spawn_blocking` thread —
/// `reqwest::blocking` must not run on the async runtime, and a sync command
/// would block the main thread for up to the 10 s timeout.
pub async fn connect_basemap_provider(
    state: tauri::State<'_, ProvidersState>,
    provider_id: String,
    token: String,
) -> Result<BasemapProviderDto, String> {
    let provider = catalog::provider(&provider_id)
        .ok_or_else(|| format!("unknown provider '{provider_id}'"))?;
    if provider.token_label.is_none() {
        // Free / built-in: nothing to validate or store.
        return Ok(status_for(provider, &state));
    }
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(format!("{} requires a token", provider.display_name));
    }

    let task_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let client = task_state.client()?;
        validate::validate_provider_token(client, provider, &token)?;
        tokens::set_token(provider.id, &token)?;
        task_state.cache_token(provider.id, &token);
        Ok(())
    })
    .await
    .map_err(|e| format!("token validation task failed: {e}"))??;

    Ok(status_for(provider, &state))
}

#[tauri::command]
/// Delete a provider's stored token and drop it from the in-memory cache.
pub fn disconnect_basemap_provider(
    state: tauri::State<'_, ProvidersState>,
    provider_id: String,
) -> Result<(), String> {
    catalog::provider(&provider_id).ok_or_else(|| format!("unknown provider '{provider_id}'"))?;
    // Drop the cache first so a keyring failure cannot leave a stale token
    // serving tiles after the user asked to disconnect.
    state.drop_token(&provider_id);
    tokens::delete_token(&provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_preview_redacts_middle() {
        assert_eq!(token_preview("pk.abcdefghijklmnop"), "pk.a…mnop");
        assert_eq!(token_preview("123456789"), "1234…6789");
    }

    #[test]
    fn short_tokens_are_fully_masked() {
        for t in ["", "a", "12345678"] {
            assert_eq!(token_preview(t), "…", "token {t:?}");
        }
    }

    #[test]
    fn token_preview_respects_char_boundaries() {
        // Multibyte characters must not panic byte-based slicing.
        assert_eq!(token_preview("ééééXXXXXéééé"), "éééé…éééé");
    }
}
