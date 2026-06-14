// analysis_io — canonical analysis artifact schema and file I/O boundary.
//
// This module defines the persisted `analysis.json` contract used by
// interface crates. Heavy computation is not performed here.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Schema version for the persisted analysis artifact format. Increment when
/// the JSON layout changes incompatibly.
pub const ANALYSIS_SCHEMA_VERSION: u32 = 1;

/// Root object of a persisted analysis artifact (written as JSON to disk).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisArtifact {
    /// Schema version; must equal [`ANALYSIS_SCHEMA_VERSION`] to be readable.
    pub schema_version: u32,
    /// Provenance: which output and report files were analysed.
    pub source: AnalysisSource,
    /// Histograms and summary statistics for the continuous result variables.
    pub distributions: DistributionSet,
    /// Optional pre-computed demand-reliability summary, when requested.
    #[serde(default)]
    pub demand_reliability: Option<StoredDemandReliability>,
    /// Optional pre-computed service-compliance summary, when requested.
    #[serde(default)]
    pub service_compliance: Option<StoredServiceCompliance>,
}

/// File provenance for an analysis artifact.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisSource {
    /// Path of the `.out` binary file used as input.
    pub output_file: String,
    /// Path of the `.rpt` report file used as input.
    pub report_file: String,
    /// Number of reporting periods found in the `.out` file.
    pub period_count: usize,
}

/// Collection of per-variable result distributions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DistributionSet {
    /// Distribution of nodal pressure values.
    pub pressure: ContinuousDistribution,
    /// Distribution of nodal hydraulic head values.
    pub head: ContinuousDistribution,
    /// Distribution of link flow rates.
    pub flow: ContinuousDistribution,
    /// Distribution of link velocities.
    pub velocity: ContinuousDistribution,
    /// Distribution of link status categories.
    pub status: StatusDistribution,
}

/// Histogram and summary statistics for a single continuous result variable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContinuousDistribution {
    /// Fixed-width histogram bins covering the full observed range.
    pub bins: Vec<HistogramBin>,
    /// Five-number summary plus mean for the variable.
    pub summary: SummaryStats,
    /// Optional threshold breakdown (below / within / above user-defined limits).
    pub thresholds: Option<ThresholdBreakdown>,
}

/// A single histogram bin.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistogramBin {
    /// Inclusive lower bound of this bin.
    pub start: f64,
    /// Exclusive upper bound of this bin.
    pub end: f64,
    /// Number of samples that fell within `[start, end)`.
    pub count: u64,
}

/// Descriptive statistics for a continuous variable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SummaryStats {
    /// Minimum observed value.
    pub min: f64,
    /// Maximum observed value.
    pub max: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// 5th percentile.
    pub p05: f64,
    /// 25th percentile (first quartile).
    pub p25: f64,
    /// 50th percentile (median).
    pub p50: f64,
    /// 75th percentile (third quartile).
    pub p75: f64,
    /// 95th percentile.
    pub p95: f64,
}

/// Sample counts partitioned relative to a threshold pair.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThresholdBreakdown {
    /// Samples below the lower threshold.
    pub below: u64,
    /// Samples within `[lower, upper]`.
    pub within: u64,
    /// Samples above the upper threshold.
    pub above: u64,
}

/// Counts of link-status observations across all periods.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusDistribution {
    /// Number of (link, period) samples with status = `Open`.
    pub open: u64,
    /// Number of (link, period) samples with status = `Closed`.
    pub closed: u64,
    /// Number of (link, period) samples with status = `Active`.
    pub active: u64,
    /// Number of (link, period) samples with any other status.
    pub other: u64,
}

/// Pre-computed demand reliability summary stored inside the analysis artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDemandReliability {
    /// String representation of the demand model (`"DDA"` or `"PDA"`).
    pub demand_model: String,
    /// Number of reporting periods in the simulation.
    pub period_count: usize,
    /// Network-wide reliability ratio in `[0, 1]`.
    pub reliability_ratio: f64,
    /// Total required demand volume (m³).
    pub required_volume: f64,
    /// Total unmet demand volume (m³).
    pub unmet_volume: f64,
    /// Total surplus delivered volume (m³).
    pub surplus_volume: f64,
    /// Total number of (junction, period) pairs with a deficit.
    pub deficit_periods: usize,
    /// Highest per-node max-deficit-rate observed (m³/s).
    pub max_node_deficit_rate: f64,
    /// String ID of the junction with the lowest reliability ratio.
    pub worst_node_id: Option<String>,
    /// Reliability ratio of the worst-performing junction.
    pub worst_node_reliability: Option<f64>,
}

/// Pre-computed service compliance summary stored inside the analysis artifact.
/// The thresholds used at analysis time are stored alongside the results so that
/// callers can detect when settings have since changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredServiceCompliance {
    /// Number of reporting periods in the simulation.
    pub period_count: usize,
    /// Number of nodes included in the analysis.
    pub node_count: usize,
    /// Fraction of all (node, period) samples that were within limits, in `[0, 1]`.
    pub compliance_ratio: f64,
    /// Number of samples below `min_pressure_threshold`.
    pub below_min_samples: usize,
    /// Number of samples above `max_pressure_threshold`.
    pub above_max_samples: usize,
    /// Sum of per-node pressure deficit integrals (m · periods).
    pub pressure_deficit_integral: f64,
    /// Sum of per-node pressure excess integrals (m · periods).
    pub pressure_excess_integral: f64,
    /// Global worst pressure deficit observed (m).
    pub worst_below_min: f64,
    /// Global worst pressure excess observed (m).
    pub worst_above_max: f64,
    /// Highest per-node violation ratio across all nodes.
    pub max_node_violation_ratio: f64,
    /// The minimum pressure threshold used at analysis time (m).
    pub min_pressure_threshold: f64,
    /// The optional maximum pressure threshold used at analysis time (m).
    pub max_pressure_threshold: Option<f64>,
}

/// Read raw artifact bytes from disk.
pub fn read_analysis_bytes(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(path)
}

/// Write raw artifact bytes to disk, creating parent directories if needed.
pub fn write_analysis_bytes(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_schema_v1() {
        let artifact = AnalysisArtifact {
            schema_version: ANALYSIS_SCHEMA_VERSION,
            ..AnalysisArtifact::default()
        };
        assert_eq!(artifact.schema_version, 1);
    }
}
