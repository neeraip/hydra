#![doc = include_str!("spec.md")]

/// Semver version of the simulation engine, taken from `Cargo.toml` at compile time.
pub const HYDRA_SIMULATION_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) mod accounting;
pub(crate) mod controls;
pub(crate) mod timestep;

mod engine;
mod estimator;

// Re-export public API.
pub use crate::hydraulics::{HydraulicError, HYDRA_HYDRAULICS_VERSION};
pub use crate::quality::{QualityError, HYDRA_QUALITY_VERSION};
pub use engine::{
    LinkProperty, LinkQuantity, LinkResult, NodeProperty, NodeQuantity, NodeResult, ResultRanges,
    SessionError, SimWarning, Simulation, WarningKind,
};
pub use estimator::{
    classify_simulation_runtime_millis, estimate_simulation_runtime,
    estimate_simulation_runtime_from_summary, estimate_simulation_runtime_millis_from_summary,
};
// FlowBalance, PumpEnergy, MassBalance, HydSnapshot are in crate::io.
pub use crate::io::{
    FlowBalance, FlowBalanceSummary, HydSnapshot, MassBalance, PumpEnergy, WritableSimulation,
};
