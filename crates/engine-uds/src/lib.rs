//! `hydra-engine-uds` — Hydra's urban drainage (stormwater and wastewater) simulation engine.
//!
//! **Early scaffold.** This crate is the future home of Hydra's
//! urban drainage (stormwater and wastewater) simulation engine, operating on the SWMM data model the way
//! `hydra-engine-wds` operates on the EPANET data model. No simulation
//! capability is implemented yet; the crate is published so its name and
//! versioning track the Hydra workspace from the start. Development
//! happens at <https://github.com/neeraip/hydra>.
//!
//! The specification below is authoritative for this engine's behaviour and is
//! written ahead of the implementation.
#![doc = include_str!("spec.md")]
#![doc = include_str!("model/spec.md")]

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_ENGINE_UDS_VERSION: &str = env!("CARGO_PKG_VERSION");
