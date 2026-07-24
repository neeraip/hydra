//! `hydra-engine-wds` — complete water distribution simulation engine.
//!
//! Hydra simulates the hydraulic behaviour and water quality dynamics of
//! pressurised water distribution networks over time. Its output is the complete
//! time history of flows, pressures, and constituent concentrations at every
//! point in the network.
//!
//! # Scope
//!
//! Hydra models:
//! - Extended-period steady-state hydraulics
//! - Pressure-driven and demand-driven demand models
//! - Conservative and reactive constituent transport (water quality, age, source tracing)
//!
//! Hydra does **not** model pressure transients, water-hammer effects, or
//! multi-phase (gas/liquid) flow.
//!
//! # Correctness criteria
//!
//! - **Solver convergence**: head and flow residuals satisfy the GGA stopping
//!   criteria to within the configured tolerances at every hydraulic time step.
//! - **Physical conservation**: mass balance and energy balance hold across the
//!   network to within floating-point precision.
//! - **INP compatibility**: a valid EPANET 2.3 `.inp` file is parsed faithfully
//!   and its network topology is represented without loss.
//!
//! Agreement with EPANET's numerical output is not a correctness criterion. On
//! well-posed networks the two will agree closely because they solve the same
//! governing equations; where they diverge, Hydra's result is authoritative.
//!
//! # Crate ownership
//!
//! This crate owns the WD network data model, EPANET INP/OUT format parsers and
//! writers, unit conversion, the GGA hydraulic solver, the Lagrangian quality
//! engine, the simulation session API, and post-simulation analytics.
//!
//! It does **not** own interface logic (CLI, GUI) and performs no network I/O —
//! simulation inputs (INP model bytes) are supplied in memory by callers.
//! One deliberate carve-out exists for local filesystem reads: `io::out_reader`
//! and `io::analysis_io` expose explicit path-based helpers that stream binary
//! `.out` result files and analysis artifacts from disk, so large results
//! never need to be loaded whole. All public types are defined within this
//! crate.
//!
//! # Internal module structure
//!
//! | Module | Responsibility |
//! |---|---|
//! | `model` | Network data model, state types, validation |
//! | `io` | Unit conversion, INP/OUT/RPT/analysis parsers and writers |
//! | `hydraulics` | GGA Newton-Raphson solver (see `hydraulics/spec.md`) |
//! | `quality` | Lagrangian transport engine (see `quality/spec.md`) |
//! | `simulation` | Session API, controls, timestep, accounting |
//! | `analysis` | Post-simulation analytics |
//!
//! Downstream crates (`hydra`, `hydra-cli`, `hydra-gui`) depend only on this
//! crate's public re-export surface; they do not depend on any internal module.

pub mod analysis;
#[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
mod hydraulics;
/// Parsing and output-writing utilities: INP parser, binary `.out` reader/writer, `.rpt` writer, and unit conversion.
pub mod io;
pub mod model;
mod quality;
pub mod simulation;

#[cfg(feature = "test-support")]
pub mod test_support;

#[cfg(feature = "test-support")]
pub use hydraulics::{build_solver_context, solve_hydraulic_step, SolverContext};

// ── Data model ────────────────────────────────────────────────────────────────

pub use model::{
    ActionValue, Curve, CurveKind, CurvePoint, DemandCategory, DemandModel, FavadCoeffs, FlowUnits,
    HeadLossFormula, Junction, Link, LinkBase, LinkKind, LinkState, LinkStatus, LogicOp, MixModel,
    Network, Node, NodeBase, NodeKind, NodeState, Pattern, Pipe, Premise, PremiseAttribute,
    PremiseObject, PremiseOperator, Pump, PumpCurveType, QualityMode, QualitySource,
    ReportFieldOption, ReportOptions, ReportSelection, ReportStatus, Reservoir, Rule, RuleAction,
    RuntimeEstimate, SimpleControl, SimulationOptions, SourceType, StatisticType, Tank,
    TriggerType, ValidationError, Valve, ValveType, WallOrder,
};

// ── Session API ───────────────────────────────────────────────────────────────

pub use simulation::{
    classify_simulation_runtime_millis, estimate_simulation_runtime,
    estimate_simulation_runtime_from_summary, estimate_simulation_runtime_millis_from_summary,
    FlowBalance, FlowBalanceSummary, HydSnapshot, HydraulicError, LinkProperty, LinkQuantity,
    LinkResult, MassBalance, NodeProperty, NodeQuantity, NodeResult, PumpEnergy, QualityError,
    ResultRanges, SessionError, SimWarning, Simulation, WarningKind, WritableSimulation,
    HYDRA_HYDRAULICS_VERSION, HYDRA_QUALITY_VERSION, HYDRA_SIMULATION_VERSION,
};

// ── Analytics ─────────────────────────────────────────────────────────────────

pub use analysis::{
    build_analysis_artifact, build_analysis_artifact_from_out,
    build_analysis_artifact_from_out_with_progress,
    build_analysis_artifact_from_out_with_progress_and_selection,
    compute_demand_reliability_from_out, compute_demand_reliability_from_out_with_options,
    compute_service_compliance_from_out, decode_analysis_artifact, encode_analysis_artifact,
    estimate_analysis_runtime_millis, AnalysisBytesError, AnalysisComputeError, AnalysisSelection,
    DemandReliabilityNode, DemandReliabilityOptions, DemandReliabilityReport,
    DemandReliabilitySummary, ServiceComplianceNode, ServiceComplianceReport,
    ServiceComplianceSummary, ServiceComplianceThresholds, HYDRA_ANALYSIS_VERSION,
};

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Serialise a [`Network`] back to EPANET 2.3 INP bytes.
pub fn write_inp(network: &Network) -> Vec<u8> {
    io::write_inp(network)
}

/// Compute the FNV-1a 64-bit network topology digest (model spec §4.5.7).
///
/// Stored in `.out` result files so consumers can detect results that are
/// stale relative to an edited network topology.
pub use io::compute_network_digest;
