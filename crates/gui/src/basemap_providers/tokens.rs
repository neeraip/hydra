//! Provider-token storage in the OS credential store via the `keyring` crate.
//!
//! Service name `com.hydra.basemap-providers`, account = provider id. All
//! failures degrade to `Err(String)` — a broken keyring backend (e.g. a
//! headless CI session with no keychain) must never panic the app.

use keyring::Entry;

const SERVICE: &str = "com.hydra.basemap-providers";

fn entry(provider_id: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, provider_id)
        .map_err(|e| format!("keyring unavailable for '{provider_id}': {e}"))
}

/// Store (or overwrite) the token for a provider.
pub fn set_token(provider_id: &str, token: &str) -> Result<(), String> {
    entry(provider_id)?
        .set_password(token)
        .map_err(|e| format!("failed to store token for '{provider_id}': {e}"))
}

/// Read the token for a provider. `Ok(None)` when no token is stored.
pub fn get_token(provider_id: &str) -> Result<Option<String>, String> {
    match entry(provider_id)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("failed to read token for '{provider_id}': {e}")),
    }
}

/// Delete the token for a provider. Deleting an absent token is not an error.
pub fn delete_token(provider_id: &str) -> Result<(), String> {
    match entry(provider_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("failed to delete token for '{provider_id}': {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Route all keyring calls in this test binary to the crate's in-memory
    /// mock store so tests never touch (or require) a real OS keychain.
    ///
    /// LIMITATION: the mock builder scopes state to each `Entry` *instance*
    /// — a value set through one entry is invisible to a new entry for the
    /// same service/account. Since this module builds a fresh `Entry` per
    /// call, cross-call persistence (set → get roundtrip) exercises the real
    /// OS keychain only and is untested here; the tests below cover the
    /// absent-entry mappings and the write path.
    fn use_mock_store() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        });
    }

    #[test]
    fn get_of_absent_token_is_none() {
        use_mock_store();
        assert_eq!(get_token("never-stored").unwrap(), None);
    }

    #[test]
    fn delete_of_absent_token_is_ok() {
        use_mock_store();
        delete_token("never-stored").unwrap();
    }

    #[test]
    fn set_token_accepts_a_value() {
        use_mock_store();
        set_token("test-provider", "pk.abc123").unwrap();
    }
}
