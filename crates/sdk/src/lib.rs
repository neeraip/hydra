//! Hydra — water distribution network simulation engine.
//!
//! This crate is the published library for the Hydra workspace. It re-exports the
//! complete user-facing API so that downstream users depend on a single crate
//! with all internal dependency versions pre-pinned and known to be compatible.
//!
//! # Quick start
//!
//! ```no_run
//! use hydra_sdk::{io, Simulation, NodeQuantity, LinkQuantity};
//!
//! let bytes = std::fs::read("network.inp").unwrap();
//! let network = io::parse(&bytes).unwrap();
//!
//! let mut sim = Simulation::create();
//! sim.load(network).unwrap();
//! sim.run().unwrap();
//!
//! for t in sim.snapshot_times() {
//!     let head = sim.get_node_result("J1", NodeQuantity::Head, t).unwrap();
//!     let flow = sim.get_link_result("P1", LinkQuantity::Flow, t).unwrap();
//!     println!("t={t:.0}s  head={head:.3}  flow={flow:.6}");
//! }
//! ```

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Session API ───────────────────────────────────────────────────────────────

pub use hydra_engine_wds::{
    classify_simulation_runtime_millis,
    estimate_simulation_runtime,
    estimate_simulation_runtime_from_summary,
    estimate_simulation_runtime_millis_from_summary,
    // Accounting result types.
    FlowBalance,
    FlowBalanceSummary,
    HydSnapshot,
    HydraulicError,
    LinkProperty,
    LinkQuantity,
    // Batch result types.
    LinkResult,
    MassBalance,
    NodeProperty,
    // Result query enums.
    NodeQuantity,
    NodeResult,
    PumpEnergy,
    QualityError,
    // Batch range computation.
    ResultRanges,
    // Error types.
    SessionError,
    // Non-fatal diagnostics.
    SimWarning,
    // Main simulation object.
    Simulation,
    WarningKind,
    // Trait required to call io::write_binary_output / io::write_report.
    WritableSimulation,
    HYDRA_HYDRAULICS_VERSION,
    HYDRA_QUALITY_VERSION,
    HYDRA_SIMULATION_VERSION,
};

// ── Analytics ─────────────────────────────────────────────────────────────────

pub use hydra_engine_wds::{
    build_analysis_artifact, build_analysis_artifact_from_out,
    build_analysis_artifact_from_out_with_progress,
    build_analysis_artifact_from_out_with_progress_and_selection,
    compute_demand_reliability_from_out, compute_demand_reliability_from_out_with_options,
    compute_service_compliance_from_out, decode_analysis_artifact, encode_analysis_artifact,
    estimate_analysis_runtime_millis, threshold_bands, AnalysisBytesError, AnalysisComputeError,
    AnalysisSelection, DemandReliabilityNode, DemandReliabilityOptions, DemandReliabilityReport,
    DemandReliabilitySummary, ServiceComplianceNode, ServiceComplianceReport,
    ServiceComplianceSummary, ServiceComplianceThresholds, HYDRA_ANALYSIS_VERSION,
};

// ── Data model ────────────────────────────────────────────────────────────────

pub use hydra_engine_wds::{
    // §2.8 — controls
    ActionValue,
    // §2.3 — curves
    Curve,
    CurveKind,
    CurvePoint,
    // §2.4 / §2.5 — nodes and demands
    DemandCategory,
    // §2.1 — top-level options and enums
    DemandModel,
    // §2.10 — FAVAD
    FavadCoeffs,
    FlowUnits,
    HeadLossFormula,
    // §2.4.2 — node subtypes
    Junction,
    // §2.6 — links
    Link,
    LinkBase,
    LinkKind,
    LinkState,
    LinkStatus,
    LogicOp,
    // §2.4.4 — tank mixing
    MixModel,
    Network,
    Node,
    NodeBase,
    NodeKind,
    NodeState,
    // §2.2 — patterns
    Pattern,
    Pipe,
    Premise,
    PremiseAttribute,
    PremiseObject,
    PremiseOperator,
    Pump,
    PumpCurveType,
    QualityMode,
    // §2.7 — quality sources
    QualitySource,
    // report options
    ReportFieldOption,
    ReportOptions,
    ReportSelection,
    ReportStatus,
    Reservoir,
    Rule,
    RuleAction,
    RuntimeEstimate,
    SimpleControl,
    SimulationOptions,
    SourceType,
    StatisticType,
    Tank,
    TriggerType,
    // §2.9 validation
    ValidationError,
    Valve,
    ValveType,
    WallOrder,
};

// ── I/O ───────────────────────────────────────────────────────────────────────

/// Parsing and output-writing utilities.
///
/// - [`io::parse`] — parse EPANET `.inp` bytes into a [`Network`].
/// - [`io::parse_tolerant`] — the same, but recovering a network that is
///   readable yet not simulable, together with its validation errors (model
///   spec §4.1.2). For editors; never for a model about to be run.
/// - [`io::out_writer`] / [`io::rpt_writer`] — write binary `.out` and text `.rpt` output.
/// - [`io::units`] — unit-conversion factors ([`io::units::Ucf`]) for
///   interpreting raw result values in display units.
///
/// Reading has three outcomes, and an integrator offering more than one engine
/// needs all three kept apart (model spec §4.1.2):
/// [`io::ReadError::ForeignDialect`] means the file is a sound model belonging
/// to a different tool and should be routed there, not reported as broken;
/// other [`io::ReadError`]s mean no network could be built at all; and
/// [`io::ParseError::NotSimulable`] means a network was recovered that cannot
/// be run yet — which [`io::parse_tolerant`] hands back rather than rejecting,
/// hence its narrower [`io::ReadError`] failure type.
pub mod io {
    pub use hydra_engine_wds::io::{
        analysis_io, compute_network_digest, out_reader, out_writer, parse, parse_tolerant,
        rpt_writer, units, write_inp, ParseError, ReadError,
    };
}

/// Serialise a [`Network`] back to EPANET 2.3 INP bytes.
///
/// The inverse of [`io::parse`]: all values are converted from the internal
/// unit system back to the user-declared unit system.
pub use hydra_engine_wds::write_inp;

/// Compute the FNV-1a 64-bit network topology digest stored in `.out` result
/// files (model spec §4.5.7). Lets consumers detect results that are stale
/// relative to an edited network topology.
pub use hydra_engine_wds::compute_network_digest;

// ── Foundation contracts ──────────────────────────────────────────────────────

/// Engine identity (descriptor + registry) and the reportable-output
/// contract shared by all engines and applications. See the `hydra-common`
/// crate spec for the authoritative contract definition.
pub use hydra_common as common;

/// Report blocks the water-distribution engine can produce, per the
/// `common` reportable-output contract.
pub use hydra_engine_wds::{produce_report_block, report_catalog};

/// Report generation: JSON templates, document assembly from engine
/// fragments, and deterministic txt/csv/html renderers. See the
/// `hydra-report` crate spec for the authoritative definition.
pub use hydra_report as report;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umbrella_reexports_simulation_type() {
        let _ = Simulation::create();
    }

    #[test]
    fn umbrella_reexports_common_io_parse() {
        let bytes = b"{\"invalid\":true}";
        let err = io::parse(bytes).expect_err("invalid model bytes should fail parse");
        assert!(matches!(
            err,
            io::ParseError::Read(io::ReadError::UnrecognisedFormat)
        ));
    }
}
