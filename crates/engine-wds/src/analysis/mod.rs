#![doc = include_str!("spec.md")]

/// Semver version of the analysis engine, taken from `Cargo.toml` at compile time.
pub mod source;
pub const HYDRA_ANALYSIS_VERSION: &str = env!("CARGO_PKG_VERSION");

mod binning;
mod criteria;
mod demand_reliability;
mod errors;
mod report_blocks;
mod service_compliance;

pub use binning::threshold_bands;
pub use criteria::{criteria_block_options, criteria_catalog};
pub use demand_reliability::{
    compute_demand_reliability_from_out, compute_demand_reliability_from_out_with_options,
    DemandReliabilityNode, DemandReliabilityOptions, DemandReliabilityReport,
    DemandReliabilitySummary,
};
pub use errors::AnalysisComputeError;
pub use report_blocks::{produce_report_block, report_block_options, report_catalog};
pub use service_compliance::{
    compute_service_compliance_from_out, ServiceComplianceNode, ServiceComplianceReport,
    ServiceComplianceSummary, ServiceComplianceThresholds,
};
