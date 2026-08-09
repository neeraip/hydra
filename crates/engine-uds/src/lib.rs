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
#![doc = include_str!("interop/spec.md")]

pub mod descriptors;
pub mod hydraulics;
pub mod hydrology;
pub mod io;
pub mod model;
pub mod report_blocks;
pub mod simulation;
pub mod transport;

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_ENGINE_UDS_VERSION: &str = env!("CARGO_PKG_VERSION");
