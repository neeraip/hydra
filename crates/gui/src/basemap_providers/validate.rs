//! Token validation: fetch one known tile and interpret the status code.

use super::catalog::CatalogProvider;
use super::proxy::substitute_template;

/// Validate a token by fetching the `z0` world tile of the provider's first
/// proxyable style. `Ok(())` on HTTP 200; `Err("invalid token")` on 401/403;
/// `Err` with the status otherwise. Uses the shared 10 s-timeout client.
pub fn validate_provider_token(
    client: &reqwest::blocking::Client,
    provider: &CatalogProvider,
    token: &str,
) -> Result<(), String> {
    let template = provider
        .styles
        .iter()
        .find_map(|s| s.tile_url_template)
        .ok_or_else(|| {
            format!(
                "provider '{}' has no proxyable style to validate",
                provider.id
            )
        })?;
    // z0/x0/y0 exists for every global tileset in the catalog.
    let url = substitute_template(template, 0, 0, 0, Some(token));
    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("validation request failed: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
    {
        Err("invalid token".to_string())
    } else {
        Err(format!("validation failed: upstream returned {status}"))
    }
}
