//! Engine identity: descriptors and the registry (spec §2).

use serde::Serialize;

/// Whether a registered engine is implemented in this distribution
/// (spec §2.3).
///
/// A `Planned` engine is registered so applications can present it and so
/// its key is reserved — it carries no implementation. Applications must
/// refuse to create projects, import models, or run simulations for one.
/// Resolving a planned key is **not** an [`UnknownEngineError`]: the
/// descriptor exists and its identity fields are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineStatus {
    /// Implemented and usable.
    Available,
    /// Registered and reserved; no implementation yet.
    Planned,
}

/// How strongly an engine claims a candidate model as its own (spec §2.5).
///
/// This is the whole vocabulary of the recognition contract. The foundation
/// layer holds no section names and no format grammar — the judgement is
/// authored entirely by the engine; this type only gives every engine the
/// same three words to express it in.
///
/// Recognition answers "whose is this?", never "can this run?". A
/// [`Definite`](Self::Definite) verdict is not a promise that the model is
/// well-formed: the owning engine's parse may still reject it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Recognition {
    /// The bytes carry a marker belonging to this engine's format and to no
    /// other.
    Definite,
    /// The bytes are shaped like this engine's format but carry nothing
    /// distinguishing them from another engine claiming the same shape.
    Plausible,
    /// Not this engine's — the format is unrecognised, or the bytes carry
    /// another format's marker.
    No {
        /// Optional engine-authored text saying what the engine believes the
        /// file is instead, e.g. "this looks like a SWMM model". Advisory:
        /// applications must behave identically without it.
        reason: Option<String>,
    },
}

impl Recognition {
    /// Whether this verdict is a claim at all (spec §2.5.1 consults
    /// `definite` before `plausible`, and ignores `no` entirely).
    pub fn claims(&self) -> bool {
        !matches!(self, Recognition::No { .. })
    }

    /// A plain refusal carrying no explanation.
    pub fn no() -> Self {
        Recognition::No { reason: None }
    }
}

/// One source-model file format an engine imports (spec §2.2).
///
/// This names a format for a file picker's filter. It is **not** a
/// validity test: `wds` and `uds` both claim the `inp` extension with
/// wholly incompatible contents, so deciding whether a file really is a
/// model of this format is the owning engine's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFormat {
    /// Human-facing format name, e.g. "EPANET input file".
    pub label: &'static str,
    /// Filename extensions, lowercase ASCII with no leading dot.
    pub extensions: &'static [&'static str],
}

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
    /// abbreviation (`wds`, `uds`).
    pub key: &'static str,
    /// Human-facing product name (e.g. "Water Distribution").
    pub label: &'static str,
    /// Two-character uppercase badge (e.g. "WD").
    pub pill: &'static str,
    /// Brand color for this engine, `#rrggbb`.
    pub accent: &'static str,
    /// One-sentence description of the engine's domain. Plain text.
    pub summary: &'static str,
    /// Whether this distribution can actually run the engine (spec §2.3).
    pub status: EngineStatus,
    /// Source-model formats this engine imports (spec §2.2). May be empty.
    pub import: &'static [ImportFormat],
}

impl EngineDescriptor {
    /// Whether this engine is implemented in this distribution.
    pub fn is_available(&self) -> bool {
        matches!(self.status, EngineStatus::Available)
    }
}

/// Every engine compiled into this distribution, in presentation order
/// (spec §2.4) — planned engines included, so applications can present
/// the full modelling scope rather than only what ships today.
///
/// Accents are chosen to be distinct from one another **and** from any hue
/// that carries state meaning in a consuming application — green reads as
/// success, amber as caution, red as failure, so an engine wearing one
/// would say something it does not mean. (The APWA Uniform Color Code
/// would put drainage on green; that standard governs excavation markings
/// on pavement, not software identity, and green is spoken for here.)
/// These are identity, not data presentation — the contract keeps colour
/// out of the element and result catalogs precisely so applications own
/// *those* palettes (spec §6).
pub const ENGINES: &[EngineDescriptor] = &[
    EngineDescriptor {
        key: "wds",
        label: "Water Distribution",
        pill: "WD",
        accent: "#4a90d9",
        summary: "Pressurized water distribution network simulation: hydraulics, \
                  water quality, and energy on the EPANET data model.",
        status: EngineStatus::Available,
        import: &[ImportFormat {
            label: "EPANET input file",
            extensions: &["inp"],
        }],
    },
    EngineDescriptor {
        key: "uds",
        label: "Urban Drainage",
        pill: "UD",
        accent: "#7a6ff0",
        summary: "Stormwater and wastewater collection network simulation: \
                  runoff, routing, and water quality on the SWMM data model.",
        status: EngineStatus::Available,
        import: &[ImportFormat {
            label: "SWMM input file",
            extensions: &["inp"],
        }],
    },
];

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
    fn registry_lists_the_domain_engines_in_order() {
        let keys: Vec<_> = ENGINES.iter().map(|e| e.key).collect();
        assert_eq!(keys, ["wds", "uds"]);
    }

    #[test]
    fn wds_and_uds_are_the_available_engines() {
        // Guards the availability contract in both directions: adding an
        // engine implementation without flipping its status leaves it
        // unusable, and flipping a status without an implementation lets
        // applications create projects that can never run.
        let available: Vec<_> = ENGINES
            .iter()
            .filter(|e| e.is_available())
            .map(|e| e.key)
            .collect();
        assert_eq!(available, ["wds", "uds"]);
    }

    #[test]
    fn every_engine_declares_a_usable_import_filter() {
        for e in ENGINES {
            assert!(
                !e.import.is_empty(),
                "engine {:?} declares no import format",
                e.key
            );
            for fmt in e.import {
                assert!(!fmt.label.is_empty());
                assert!(
                    !fmt.extensions.is_empty(),
                    "format {:?} lists no extensions",
                    fmt.label
                );
                for ext in fmt.extensions {
                    // A leading dot or any uppercase would silently break
                    // extension matching in every consumer.
                    assert!(
                        !ext.is_empty()
                            && ext
                                .chars()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                        "extension {ext:?} must be lowercase ASCII with no leading dot",
                    );
                }
            }
        }
    }

    #[test]
    fn the_inp_extension_is_shared_and_therefore_never_a_validity_test() {
        // wds and uds both claim `inp`. This is the concrete reason spec
        // §2.2 forbids treating an extension as a format check — if this
        // assertion ever fails, that rationale needs revisiting, not the
        // consumers that rely on it.
        let claimants: Vec<_> = ENGINES
            .iter()
            .filter(|e| e.import.iter().any(|f| f.extensions.contains(&"inp")))
            .map(|e| e.key)
            .collect();
        assert_eq!(claimants, ["wds", "uds"]);
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
        let err = engine_by_key("nope").unwrap_err();
        assert_eq!(err.key, "nope");
        assert!(err.to_string().contains("nope"));
    }

    #[test]
    fn a_withdrawn_key_is_unknown_rather_than_registered() {
        // Spec §2.4: `och` was withdrawn when 2D overland flow was
        // re-planned as future `uds` functionality. The key stays reserved
        // (never reused for a different domain) but resolves as unknown.
        assert!(engine_by_key("och").is_err());
    }
}
