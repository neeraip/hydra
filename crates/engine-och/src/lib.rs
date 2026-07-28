//! `hydra-engine-och` — Hydra's open-channel hydraulics simulation engine.
//!
//! **Early scaffold.** This crate is the future home of Hydra's
//! open-channel hydraulics simulation engine, operating on the HEC-RAS domain the way
//! `hydra-engine-wds` operates on the EPANET data model. No simulation
//! capability is implemented yet; the crate is published so its name and
//! versioning track the Hydra workspace from the start. Development
//! happens at <https://github.com/neeraip/hydra>.

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_ENGINE_OCH_VERSION: &str = env!("CARGO_PKG_VERSION");
