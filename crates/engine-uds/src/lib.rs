//! `hydra-engine-uds` — Hydra's urban drainage (stormwater and wastewater) simulation engine.
//!
//! A complete simulation engine on the SWMM data model, the way
//! `hydra-engine-wds` operates on the EPANET data model: SWMM 5.x input
//! parsing with behaviourally faithful validation; rainfall-runoff
//! hydrology with infiltration, control measures, snow, groundwater, and
//! sewer inflow; a dynamic-wave router over the Preissmann-slot closure
//! with the full structure catalogue and HEC-22 street inlets;
//! constituent build-up, wash-off, network transport, and treatment;
//! rule-based operational control with PID; conservation ledgers; and
//! the predecessor's hotstart, routing-interface, binary-results, and
//! text-report formats. Development happens at
//! <https://github.com/neeraip/hydra>.
//!
//! The specification below is authoritative for this engine's behaviour and is
//! written ahead of the implementation.
#![doc = include_str!("spec.md")]
#![doc = include_str!("model/spec.md")]
#![doc = include_str!("hydrology/spec.md")]
#![doc = include_str!("hydraulics/spec.md")]
#![doc = include_str!("transport/spec.md")]
#![doc = include_str!("simulation/spec.md")]
#![doc = include_str!("report_blocks/spec.md")]

pub mod descriptors;
pub mod hydraulics;
pub mod hydrology;
pub mod model;

// ── Test-only dialect mount (format-blind extraction) ──────────────────
// The engine's own behavioural tests open models from .inp text. A
// dev-dependency on hydra-interop-swmm would hand them a second build of
// this crate with incompatible types, so the dialect sources compile
// directly into the test build instead, against `crate::engine_api` =
// this crate. Integration tests (tests/) use the real dependency.
#[cfg(test)]
pub(crate) use crate as engine_api;
#[cfg(test)]
// The whole dialect compiles into the test build; the engine's tests
// exercise the slice they need, and the crate's own build lints the
// rest properly.
#[allow(dead_code, unused_imports)]
#[path = "../../interop-swmm/src/dialect/mod.rs"]
pub(crate) mod dialect;
pub mod overland;
pub mod report_blocks;
pub mod simulation;
pub mod transport;

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_ENGINE_UDS_VERSION: &str = env!("CARGO_PKG_VERSION");
