use crate::io::analysis_io::AnalysisArtifact;

/// Error returned when serialising or deserialising an [`AnalysisArtifact`].
#[derive(Debug)]
pub enum AnalysisBytesError {
    /// The JSON payload is malformed or does not match the expected schema.
    Json(serde_json::Error),
}

impl std::fmt::Display for AnalysisBytesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "analysis artifact JSON error: {e}"),
        }
    }
}

impl std::error::Error for AnalysisBytesError {}

impl From<serde_json::Error> for AnalysisBytesError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Encode an analysis artifact to canonical JSON bytes.
///
/// The produced bytes should be written to `analysis.json` alongside the
/// `results.out` file for the same run. The file is versioned via
/// `schema_version`; callers must treat it as read-only and must not
/// recompute heavy analytics at render time.
///
/// If `results.out` (or the INP that produced it) changes, the corresponding
/// `analysis.json` must be deleted as stale before a new artifact is produced.
pub fn encode_analysis_artifact(
    artifact: &AnalysisArtifact,
) -> Result<Vec<u8>, AnalysisBytesError> {
    Ok(serde_json::to_vec_pretty(artifact)?)
}

/// Decode canonical JSON bytes into an analysis artifact.
pub fn decode_analysis_artifact(bytes: &[u8]) -> Result<AnalysisArtifact, AnalysisBytesError> {
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::analysis_io::{
        AnalysisArtifact, AnalysisSource, ContinuousDistribution, DistributionSet, HistogramBin,
        StatusDistribution, SummaryStats, ANALYSIS_SCHEMA_VERSION,
    };

    #[test]
    fn round_trip_analysis_bytes() {
        let artifact = AnalysisArtifact {
            schema_version: ANALYSIS_SCHEMA_VERSION,
            source: AnalysisSource {
                output_file: "network.out".to_string(),
                report_file: "report.json".to_string(),
                period_count: 25,
            },
            distributions: DistributionSet {
                pressure: ContinuousDistribution {
                    bins: vec![HistogramBin {
                        start: 0.0,
                        end: 10.0,
                        count: 3,
                    }],
                    summary: SummaryStats {
                        min: 0.0,
                        max: 10.0,
                        mean: 4.0,
                        p05: 0.5,
                        p25: 2.0,
                        p50: 4.0,
                        p75: 6.0,
                        p95: 9.5,
                    },
                    thresholds: None,
                },
                head: ContinuousDistribution::default(),
                flow: ContinuousDistribution::default(),
                velocity: ContinuousDistribution::default(),
                status: StatusDistribution {
                    open: 10,
                    closed: 2,
                    active: 1,
                    other: 0,
                },
            },
            demand_reliability: None,
            service_compliance: None,
        };

        let bytes = encode_analysis_artifact(&artifact).expect("encode artifact");
        let decoded = decode_analysis_artifact(&bytes).expect("decode artifact");

        assert_eq!(decoded.schema_version, ANALYSIS_SCHEMA_VERSION);
        assert_eq!(decoded.source.period_count, 25);
        assert_eq!(decoded.distributions.pressure.bins.len(), 1);
        assert_eq!(decoded.distributions.status.open, 10);
    }
}
