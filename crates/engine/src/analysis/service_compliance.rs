use super::errors::AnalysisComputeError;
use crate::io::out_reader;

/// Pressure thresholds used to classify reporting-period samples as in-limit or
/// out-of-limit in a service-compliance analysis.
///
/// All values are in the same pressure units as the `.out` file (metres of head
/// for Hydra-generated output).
#[derive(Debug, Clone, Copy)]
pub struct ServiceComplianceThresholds {
    /// Minimum acceptable pressure (m). Samples below this are counted as
    /// `below_min` violations.
    pub min_pressure: f64,
    /// Optional maximum acceptable pressure (m). When `Some`, samples above
    /// this are counted as `above_max` violations. `None` disables the upper
    /// bound check.
    pub max_pressure: Option<f64>,
}

impl ServiceComplianceThresholds {
    /// Create thresholds with only a minimum pressure bound (no upper limit).
    pub fn min_only(min_pressure: f64) -> Self {
        Self {
            min_pressure,
            max_pressure: None,
        }
    }
}

/// Per-node service-compliance metrics for a single simulation run.
#[derive(Debug, Clone, Default)]
pub struct ServiceComplianceNode {
    /// Zero-based index of this node in [`crate::Network::nodes`].
    pub node_index: usize,
    /// Total number of reporting-period pressure samples for this node.
    pub sample_count: usize,
    /// Number of samples with pressure within `[min_pressure, max_pressure]`.
    pub within_limits_count: usize,
    /// Number of samples below `min_pressure`.
    pub below_min_count: usize,
    /// Number of samples above `max_pressure` (always 0 when no upper limit is set).
    pub above_max_count: usize,
    /// Length of the longest consecutive run of out-of-limit samples.
    pub longest_violation_streak: usize,
    /// Integral of pressure deficit over time (m · periods).
    ///
    /// Accumulated as `sum(max(min_pressure − pressure, 0))` across all samples.
    pub pressure_deficit_integral: f64,
    /// Integral of pressure excess over time (m · periods).
    ///
    /// Accumulated as `sum(max(pressure − max_pressure, 0))` across all samples.
    pub pressure_excess_integral: f64,
    /// Worst (largest) observed pressure deficit: `max(min_pressure − pressure)` (m).
    pub worst_below_min: f64,
    /// Worst (largest) observed pressure excess: `max(pressure − max_pressure)` (m).
    pub worst_above_max: f64,
}

impl ServiceComplianceNode {
    /// Number of samples that fell outside the acceptable pressure range.
    pub fn violating_sample_count(&self) -> usize {
        self.sample_count.saturating_sub(self.within_limits_count)
    }

    /// Fraction of samples that were out-of-limit, in `[0, 1]`.
    pub fn violation_ratio(&self) -> f64 {
        if self.sample_count == 0 {
            0.0
        } else {
            self.violating_sample_count() as f64 / self.sample_count as f64
        }
    }
}

/// Network-level service-compliance summary aggregated across all nodes.
#[derive(Debug, Clone, Default)]
pub struct ServiceComplianceSummary {
    /// Number of reporting periods in the simulation.
    pub period_count: usize,
    /// Number of nodes included in the analysis.
    pub node_count: usize,
    /// Total number of (node, period) pressure samples.
    pub total_samples: usize,
    /// Number of samples within the acceptable pressure range.
    pub within_limits_samples: usize,
    /// Number of samples outside the acceptable pressure range.
    pub violating_samples: usize,
    /// Number of samples below `min_pressure`.
    pub below_min_samples: usize,
    /// Number of samples above `max_pressure`.
    pub above_max_samples: usize,
    /// Sum of per-node `pressure_deficit_integral` values.
    pub pressure_deficit_integral: f64,
    /// Sum of per-node `pressure_excess_integral` values.
    pub pressure_excess_integral: f64,
    /// Global worst pressure deficit across all nodes (m).
    pub worst_below_min: f64,
    /// Global worst pressure excess across all nodes (m).
    pub worst_above_max: f64,
    /// Highest per-node violation ratio observed across all nodes.
    pub max_node_violation_ratio: f64,
}

impl ServiceComplianceSummary {
    /// Fraction of all samples that were in-limit, in `[0, 1]`.
    pub fn compliance_ratio(&self) -> f64 {
        if self.total_samples == 0 {
            1.0
        } else {
            self.within_limits_samples as f64 / self.total_samples as f64
        }
    }

    /// Fraction of all samples that were out-of-limit: `1 − compliance_ratio`.
    pub fn violation_ratio(&self) -> f64 {
        1.0 - self.compliance_ratio()
    }
}

/// Complete service-compliance report for a single simulation run.
#[derive(Debug, Clone)]
pub struct ServiceComplianceReport {
    /// Pressure thresholds used to classify samples.
    pub thresholds: ServiceComplianceThresholds,
    /// Duration of each reporting period (seconds).
    pub report_step_seconds: f64,
    /// Total number of reporting periods in the `.out` file.
    pub period_count: usize,
    /// Per-node metrics, ordered to match [`crate::Network::nodes`].
    pub nodes: Vec<ServiceComplianceNode>,
    /// Network-level summary aggregated from all nodes.
    pub summary: ServiceComplianceSummary,
}

/// Compute node-pressure service compliance metrics from a persisted `.out` file.
///
/// **Inputs** (via [`ServiceComplianceThresholds`]):
/// - pressure samples by node and reporting period (read from `.out`)
/// - reporting period duration
/// - minimum pressure threshold
/// - optional maximum pressure threshold
///
/// Pressure thresholds are interpreted in the same pressure units stored in the
/// `.out` file. Uses a streaming pass over periods — all node pressures are
/// never held in memory simultaneously.
///
/// **Outputs** (in [`ServiceComplianceReport`]):
/// - per-node: in-limit sample count/ratio, below-minimum and above-maximum
///   counts, longest continuous violation streak, pressure deficit/excess
///   integrals over time
/// - network-level summary statistics
pub fn compute_service_compliance_from_out(
    out_path: &std::path::Path,
    thresholds: ServiceComplianceThresholds,
) -> Result<ServiceComplianceReport, AnalysisComputeError> {
    validate_thresholds(thresholds)?;

    let meta = out_reader::read_metadata_checked(out_path)
        .map_err(|e| AnalysisComputeError::OutRead(e.to_string()))?;

    if meta.n_periods == 0 {
        return Err(AnalysisComputeError::NoSnapshots);
    }

    let dt = if meta.report_step > 0.0 {
        meta.report_step
    } else {
        1.0
    };

    let mut nodes = vec![ServiceComplianceNode::default(); meta.n_nodes];
    for (i, node) in nodes.iter_mut().enumerate() {
        node.node_index = i;
    }
    let mut current_streaks = vec![0usize; meta.n_nodes];

    for period in 0..meta.n_periods {
        let period_results = out_reader::read_period(out_path, &meta, period)
            .map_err(AnalysisComputeError::OutRead)?;

        for (i, pressure) in period_results.node_pressure.iter().enumerate() {
            observe_pressure_sample(
                &mut nodes[i],
                &mut current_streaks[i],
                *pressure as f64,
                thresholds,
                dt,
            );
        }
    }

    let mut summary = ServiceComplianceSummary {
        period_count: meta.n_periods,
        node_count: meta.n_nodes,
        total_samples: meta.n_nodes.saturating_mul(meta.n_periods),
        ..ServiceComplianceSummary::default()
    };

    for node in &nodes {
        summary.within_limits_samples += node.within_limits_count;
        summary.below_min_samples += node.below_min_count;
        summary.above_max_samples += node.above_max_count;
        summary.pressure_deficit_integral += node.pressure_deficit_integral;
        summary.pressure_excess_integral += node.pressure_excess_integral;
        summary.worst_below_min = summary.worst_below_min.max(node.worst_below_min);
        summary.worst_above_max = summary.worst_above_max.max(node.worst_above_max);
        summary.max_node_violation_ratio =
            summary.max_node_violation_ratio.max(node.violation_ratio());
    }

    summary.violating_samples = summary
        .total_samples
        .saturating_sub(summary.within_limits_samples);

    Ok(ServiceComplianceReport {
        thresholds,
        report_step_seconds: dt,
        period_count: meta.n_periods,
        nodes,
        summary,
    })
}

fn validate_thresholds(
    thresholds: ServiceComplianceThresholds,
) -> Result<(), AnalysisComputeError> {
    if !thresholds.min_pressure.is_finite() {
        return Err(AnalysisComputeError::InvalidInput(
            "minimum pressure threshold must be finite".to_string(),
        ));
    }

    if let Some(max_pressure) = thresholds.max_pressure {
        if !max_pressure.is_finite() {
            return Err(AnalysisComputeError::InvalidInput(
                "maximum pressure threshold must be finite".to_string(),
            ));
        }
        if max_pressure <= thresholds.min_pressure {
            return Err(AnalysisComputeError::InvalidInput(
                "maximum pressure threshold must be greater than minimum pressure threshold"
                    .to_string(),
            ));
        }
    }

    Ok(())
}

fn observe_pressure_sample(
    node: &mut ServiceComplianceNode,
    current_streak: &mut usize,
    pressure: f64,
    thresholds: ServiceComplianceThresholds,
    dt_seconds: f64,
) {
    node.sample_count += 1;

    let mut violation = false;

    if pressure < thresholds.min_pressure {
        let deficit = thresholds.min_pressure - pressure;
        node.below_min_count += 1;
        node.pressure_deficit_integral += deficit * dt_seconds;
        node.worst_below_min = node.worst_below_min.max(deficit);
        violation = true;
    }

    if let Some(max_pressure) = thresholds.max_pressure {
        if pressure > max_pressure {
            let excess = pressure - max_pressure;
            node.above_max_count += 1;
            node.pressure_excess_integral += excess * dt_seconds;
            node.worst_above_max = node.worst_above_max.max(excess);
            violation = true;
        }
    }

    if violation {
        *current_streak += 1;
        node.longest_violation_streak = node.longest_violation_streak.max(*current_streak);
    } else {
        node.within_limits_count += 1;
        *current_streak = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_sample_updates_counts_streaks_and_integrals() {
        let thresholds = ServiceComplianceThresholds {
            min_pressure: 30.0,
            max_pressure: Some(80.0),
        };
        let dt = 3600.0;
        let mut node = ServiceComplianceNode {
            node_index: 0,
            ..ServiceComplianceNode::default()
        };
        let mut streak = 0usize;

        let samples = [25.0, 20.0, 35.0, 90.0, 85.0, 40.0];
        for pressure in samples {
            observe_pressure_sample(&mut node, &mut streak, pressure, thresholds, dt);
        }

        assert_eq!(node.sample_count, 6);
        assert_eq!(node.within_limits_count, 2);
        assert_eq!(node.below_min_count, 2);
        assert_eq!(node.above_max_count, 2);
        assert_eq!(node.longest_violation_streak, 2);
        assert_eq!(node.violating_sample_count(), 4);
        assert!((node.violation_ratio() - (4.0 / 6.0)).abs() < 1e-12);
        assert!((node.pressure_deficit_integral - 15.0 * dt).abs() < 1e-12);
        assert!((node.pressure_excess_integral - 15.0 * dt).abs() < 1e-12);
        assert!((node.worst_below_min - 10.0).abs() < 1e-12);
        assert!((node.worst_above_max - 10.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_thresholds_are_rejected() {
        let bad = ServiceComplianceThresholds {
            min_pressure: 30.0,
            max_pressure: Some(30.0),
        };
        let err = validate_thresholds(bad).expect_err("expected invalid threshold error");
        assert!(matches!(err, AnalysisComputeError::InvalidInput(_)));
    }
}
