//! Engine identity: descriptors and the registry (spec §2).

use serde::Serialize;

/// Immutable identity of one Hydra engine (spec §2.1).
///
/// `key` and the `label`/`pill` pair are two deliberately separate naming
/// systems: the key carries the accurate domain umbrella and never changes
/// once released (it is persisted in project metadata and report
/// templates); the label carries the familiar practitioner term and may be
/// revised between releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    /// Stable machine identifier: lowercase ASCII domain-umbrella
    /// abbreviation (`wds`, `uds`, `och`).
    pub key: &'static str,
    /// Human-facing product name (e.g. "Water Distribution").
    pub label: &'static str,
    /// Two-character uppercase badge (e.g. "WD").
    pub pill: &'static str,
    /// Brand color for this engine, `#rrggbb`.
    pub accent: &'static str,
    /// One-sentence description of the engine's domain. Plain text.
    pub summary: &'static str,
}

/// Every engine compiled into this distribution, in presentation order
/// (spec §2.2).
pub const ENGINES: &[EngineDescriptor] = &[EngineDescriptor {
    key: "wds",
    label: "Water Distribution",
    pill: "WD",
    accent: "#4a90d9",
    summary: "Pressurized water distribution network simulation — hydraulics, \
              water quality, and energy on the EPANET data model.",
}];

/// Lookup failure for [`engine_by_key`].
///
/// Applications must treat this as an explicit unsupported state (e.g. a
/// project created by a newer Hydra carrying an engine this build lacks) —
/// never as a fallback to a default engine (spec §2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownEngineError {
    /// The key that failed to resolve.
    pub key: String,
}

impl std::fmt::Display for UnknownEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown engine key: {:?}", self.key)
    }
}

impl std::error::Error for UnknownEngineError {}

/// Resolve an engine key to its descriptor (spec §2.2).
pub fn engine_by_key(key: &str) -> Result<&'static EngineDescriptor, UnknownEngineError> {
    ENGINES
        .iter()
        .find(|e| e.key == key)
        .ok_or_else(|| UnknownEngineError { key: key.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_wds_first() {
        assert_eq!(ENGINES[0].key, "wds");
        assert_eq!(ENGINES[0].label, "Water Distribution");
        assert_eq!(ENGINES[0].pill, "WD");
    }

    #[test]
    fn descriptor_field_invariants_hold_for_every_engine() {
        for e in ENGINES {
            assert!(
                e.key.chars().all(|c| c.is_ascii_lowercase()),
                "key {:?} must be lowercase ASCII",
                e.key
            );
            assert_eq!(
                e.pill.chars().count(),
                2,
                "pill {:?} must be 2 chars",
                e.pill
            );
            assert!(e.pill.chars().all(|c| c.is_ascii_uppercase()));
            assert!(
                e.accent.len() == 7 && e.accent.starts_with('#'),
                "accent {:?} must be #rrggbb",
                e.accent
            );
            assert!(!e.summary.is_empty());
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = ENGINES.iter().map(|e| e.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ENGINES.len());
    }

    #[test]
    fn lookup_resolves_and_rejects() {
        assert_eq!(engine_by_key("wds").unwrap().pill, "WD");
        let err = engine_by_key("och").unwrap_err();
        assert_eq!(err.key, "och");
        assert!(err.to_string().contains("och"));
    }
}
