#![doc = include_str!("spec.md")]

/// Semver version of the analysis engine, taken from `Cargo.toml` at compile time.
pub const HYDRA_ANALYSIS_VERSION: &str = env!("CARGO_PKG_VERSION");

mod artifact;
mod binning;
mod demand_reliability;
mod errors;
mod service_compliance;

pub use artifact::{decode_analysis_artifact, encode_analysis_artifact, AnalysisBytesError};
pub use binning::{
    build_analysis_artifact, build_analysis_artifact_from_out,
    build_analysis_artifact_from_out_with_progress,
    build_analysis_artifact_from_out_with_progress_and_selection, estimate_analysis_runtime_millis,
    AnalysisSelection,
};
pub use demand_reliability::{
    compute_demand_reliability_from_out, compute_demand_reliability_from_out_with_options,
    DemandReliabilityNode, DemandReliabilityOptions, DemandReliabilityReport,
    DemandReliabilitySummary,
};
pub use errors::AnalysisComputeError;
pub use service_compliance::{
    compute_service_compliance_from_out, ServiceComplianceNode, ServiceComplianceReport,
    ServiceComplianceSummary, ServiceComplianceThresholds,
};
