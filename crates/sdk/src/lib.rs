//! Hydra — water infrastructure simulation.
//!
//! This crate is the published library for the Hydra workspace. It re-exports the
//! complete user-facing API so that downstream users depend on a single crate
//! with all internal dependency versions pre-pinned and known to be compatible.
//!
//! Hydra is a suite of domain engines. Water distribution (`wds`) is the
//! original engine and the source of every unprefixed simulation type
//! re-exported below; urban drainage ([`uds`], SWMM data model) ships as a
//! namespaced module. Resolve an engine through the registry
//! ([`common::ENGINES`]) rather than assuming which one a project uses.
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
    compute_demand_reliability_from_out, compute_demand_reliability_from_out_with_options,
    compute_service_compliance_from_out, threshold_bands, AnalysisComputeError,
    DemandReliabilityNode, DemandReliabilityOptions, DemandReliabilityReport,
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
    pub use hydra_engine_wds::model::units;
    pub use hydra_interop_epanet::{
        compute_network_digest, control_statements, out_reader, out_writer, parse, parse_tolerant,
        rpt_writer, rule_statements, write_inp, ParseError, ReadError,
    };
}

/// Serialise a [`Network`] back to EPANET 2.3 INP bytes.
///
/// The inverse of [`io::parse`]: all values are converted from the internal
/// unit system back to the user-declared unit system.
pub use hydra_interop_epanet::write_inp;

/// Compute the FNV-1a 64-bit network topology digest stored in `.out` result
/// files (model spec §4.4.7). Lets consumers detect results that are stale
/// relative to an edited network topology.
pub use hydra_interop_epanet::compute_network_digest;

// ── Foundation contracts ──────────────────────────────────────────────────────

/// Engine identity (descriptor + registry) and the reportable-output
/// contract shared by all engines and applications. See the `hydra-common`
/// crate spec for the authoritative contract definition.
pub use hydra_common as common;

/// Engine dispatch: route a model of unknown provenance to the engine that
/// owns it (`common` spec §2.5.1). An extension cannot answer this — `wds`
/// and `uds` both claim `.inp` — so ask [`engines::route`] rather than
/// assuming, and never fall back to a default engine.
pub use hydra_engines as engines;

/// The water-distribution engine's published element, quantity, and
/// result-variable catalogs (hydra-common spec §4–§6) — how an application
/// presents a wds model and its results without wds knowledge. The urban
/// drainage engine's counterpart is [`uds::descriptors`].
pub use hydra_engine_wds::descriptors;

// ── Urban drainage engine ─────────────────────────────────────────────────────

/// The urban drainage engine (`uds`): runoff, dynamic-wave routing, and
/// water quality on the SWMM data model.
///
/// Namespaced rather than flattened because its vocabulary overlaps the
/// water-distribution types above (both have networks, simulations, and
/// options). The engine is format-blind: a session is built from a parsed
/// model ([`uds::simulation::engine::Simulation::from_network`]), stepped
/// or run, and queried by element id. Every path between SWMM text or
/// files and that session — parsing, recognition, `.out`/`.rpt` output,
/// interface files — lives in the [`swmm`] dialect module.
pub use hydra_engine_uds as uds;

/// The SWMM dialect: INP import (`swmm::session::open` opens a `uds`
/// session straight from model text), OUT/RPT output, interface files,
/// recognition, and `swmm::inp_writer::write_inp` to serialise a network
/// back to SWMM input text. It writes the model as it stands *after*
/// import's validation and repairs, which is not a copy of the file it
/// came from — see the interop spec §14.13. The engine itself is
/// format-blind; every path from text to a running `uds` session goes
/// through here.
pub use hydra_interop_swmm as swmm;

/// The EPANET dialect (format-blind extraction): INP import, OUT/RPT
/// output, and recognition for the water distribution engine. The
/// legacy `hydra::io` module remains the conventional path.
pub use hydra_interop_epanet as epanet;

/// Report blocks the water-distribution engine can produce, per the
/// `common` reportable-output contract, and its criteria catalog and
/// consumption per the `common` criteria contract.
pub use hydra_engine_wds::{
    criteria_block_options, criteria_catalog, produce_report_block, report_block_options,
    report_catalog,
};

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
