//! Engine dispatch: deciding which Hydra engine owns a given model.
//!
//! The registry in `hydra-common` is inert data — it depends on nothing, so
//! it can describe engines but never invoke them. Each engine's recognition
//! lives in that engine. This crate is the one layer that sees both, so it
//! is where the routing policy of the foundation contract (hydra-common spec
//! §2.5.1) is implemented — exactly once, rather than duplicated into every
//! interface that needs it.
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("network.inp")?;
//! let engine = hydra_engines::route(&bytes)?;
//! println!("{} ({})", engine.label, engine.key);
//! # Ok(()) }
//! ```

use std::fmt;

use hydra_common::{EngineDescriptor, Recognition, ENGINES};

mod session;

pub use session::{AdvanceError, EngineSession, Progress, SessionWarning, WriteSeek};

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_ENGINES_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a model could not be routed to an engine (spec §2.5.1).
///
/// Both variants are terminal. Routing never falls back to a default engine:
/// handing a model to a solver that models different physics would return a
/// confident, wrong answer rather than an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// More than one engine claimed the model equally strongly, so the
    /// evidence in the file is not sufficient to choose.
    Ambiguous {
        /// Keys of the engines that tied, in registry order.
        candidates: Vec<&'static str>,
    },
    /// No available engine claimed the model.
    Unrecognised {
        /// Engine-authored explanations gathered from the engines that
        /// declined (spec §2.5). Often empty; when present, far more useful
        /// than the bare failure — e.g. "this looks like a SWMM model".
        notes: Vec<String>,
    },
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // With a single candidate "it could be wds" invites the obvious
            // question; say what is actually missing instead.
            Self::Ambiguous { candidates } if candidates.len() == 1 => write!(
                f,
                "this is shaped like a {} model but carries nothing that identifies it as one, \
                 and its format is shared with other engines",
                candidates[0]
            ),
            Self::Ambiguous { candidates } => write!(
                f,
                "cannot tell which engine this model belongs to — it could be {}",
                candidates.join(" or ")
            ),
            Self::Unrecognised { notes } if !notes.is_empty() => {
                write!(f, "{}", notes.join("; "))
            }
            Self::Unrecognised { .. } => {
                write!(f, "no available engine recognises this model")
            }
        }
    }
}

impl std::error::Error for RouteError {}

/// Ask one engine whether the bytes are its own.
///
/// Planned engines are never consulted — they have no implementation to
/// consult (spec §2.3, §2.5.1). An *available* engine missing from this
/// match is a wiring bug, caught by `every_available_engine_has_a_recognizer`.
fn verdict(engine: &EngineDescriptor, bytes: &[u8]) -> Recognition {
    if !engine.is_available() {
        return Recognition::no();
    }
    match engine.key {
        "wds" => hydra_engine_wds::io::recognize(bytes),
        "uds" => hydra_engine_uds::io::recognize(bytes),
        _ => Recognition::no(),
    }
}

/// Route a model of unknown provenance to the engine that owns it
/// (spec §2.5.1).
///
/// Only a single `definite` claim routes. A `plausible` claim never does,
/// even when it is the only one: it means the engine cannot tell the model
/// from another engine's, which is not a basis for choosing. There is
/// deliberately no default — see [`RouteError`].
///
/// This does not parse the model. A successful route means an engine claimed
/// the bytes, not that they form a valid or simulable network; that remains
/// the owning engine's parse and validation step.
pub fn route(bytes: &[u8]) -> Result<&'static EngineDescriptor, RouteError> {
    let mut definite = Vec::new();
    let mut plausible = Vec::new();
    let mut notes = Vec::new();

    for engine in ENGINES {
        match verdict(engine, bytes) {
            Recognition::Definite => definite.push(engine),
            Recognition::Plausible => plausible.push(engine),
            Recognition::No { reason } => notes.extend(reason),
        }
    }

    // Only a definite claim can route (spec §2.5.1 rule 1). A plausible one
    // means "I cannot tell this from another engine's model", so acting on
    // it — even as the sole claimant — is exactly the guess this contract
    // exists to prevent.
    match definite.len() {
        1 => Ok(definite[0]),
        0 if plausible.is_empty() => Err(RouteError::Unrecognised { notes }),
        0 => Err(RouteError::Ambiguous {
            candidates: plausible.iter().map(|e| e.key).collect(),
        }),
        _ => Err(RouteError::Ambiguous {
            candidates: definite.iter().map(|e| e.key).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPANET: &str = "[JUNCTIONS]\nJ1 100\n\n[PIPES]\nP1 J1 J2 100 300 100 0 Open\n";
    const SWMM: &str = "[JUNCTIONS]\nJ1 100 3\n\n[SUBCATCHMENTS]\nS1 RG1 J1 10 50 500 0.5 0\n";
    /// Only sections SWMM also declares, so nothing identifies the format.
    const AMBIGUOUS: &str = "[TITLE]\nA network\n\n[JUNCTIONS]\nJ1 100\n";

    #[test]
    fn routes_an_epanet_model_to_wds() {
        assert_eq!(route(EPANET.as_bytes()).unwrap().key, "wds");
    }

    #[test]
    fn routes_a_swmm_model_to_uds() {
        assert_eq!(route(SWMM.as_bytes()).unwrap().key, "uds");
    }

    #[test]
    fn refuses_rather_than_defaulting_when_nothing_identifies_the_format() {
        // The whole point of the contract: with only shared sections present
        // this could be either format, so routing must decline rather than
        // quietly hand it to an engine — both INP engines answer `plausible`
        // and neither claim is a basis for choosing.
        let err = route(AMBIGUOUS.as_bytes()).unwrap_err();
        assert_eq!(
            err,
            RouteError::Ambiguous {
                candidates: vec!["wds", "uds"]
            }
        );
        // Ambiguous must read differently from unrecognised: this one is
        // answered by naming an engine, the other is not.
        assert!(err.to_string().contains("wds or uds"), "{err}");
    }

    #[test]
    fn refuses_bytes_that_are_no_engines_model() {
        let err = route(b"PK\x03\x04not a model").unwrap_err();
        assert_eq!(err, RouteError::Unrecognised { notes: vec![] });
        assert!(err.to_string().contains("no available engine"));
    }

    #[test]
    fn planned_engines_are_never_consulted() {
        // Consulting one could only ever produce a claim it cannot honour.
        for engine in ENGINES.iter().filter(|e| !e.is_available()) {
            assert!(!verdict(engine, EPANET.as_bytes()).claims());
        }
    }

    #[test]
    fn every_available_engine_has_a_recognizer() {
        // Guards the wiring in `verdict`: an engine whose status flips to
        // available without being added there would silently never claim
        // anything, and every model of its own would fail to route.
        for engine in ENGINES.iter().filter(|e| e.is_available()) {
            assert!(
                matches!(engine.key, "wds" | "uds"),
                "engine {:?} is available but has no recognizer wired in verdict()",
                engine.key
            );
        }
    }
}
