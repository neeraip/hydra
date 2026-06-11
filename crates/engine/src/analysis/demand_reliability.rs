use super::errors::AnalysisComputeError;
use crate::io::out_reader;
use crate::io::units::make_ucf;
use crate::{DemandModel, FlowUnits, Network, NodeKind};
use std::collections::HashMap;
use std::io::Read;

/// Options that control the demand-reliability computation.
#[derive(Debug, Clone, Copy)]
pub struct DemandReliabilityOptions {
    /// Absolute flow volume (m³) below which a per-period shortfall is ignored.
    ///
    /// Prevents floating-point noise from counting sub-threshold rounding errors
    /// as real deficits. Default: `1e-9`.
    pub deficit_tolerance: f64,
}

impl Default for DemandReliabilityOptions {
    fn default() -> Self {
        Self {
            deficit_tolerance: 1.0e-9,
        }
    }
}

/// Per-junction demand-reliability metrics for a single simulation run.
///
/// Volumes are in m³ (internal SI units).
#[derive(Debug, Clone)]
pub struct DemandReliabilityNode {
    /// Zero-based index of this node in [`Network::nodes`].
    pub node_index: usize,
    /// String ID of the node as it appears in the INP file.
    pub node_id: String,
    /// Total volume of water demanded over all reporting periods (m³).
    pub required_volume: f64,
    /// Total volume of water actually delivered over all reporting periods (m³).
    pub delivered_volume: f64,
    /// Total unmet demand volume over all reporting periods (m³).
    pub unmet_volume: f64,
    /// Total surplus delivered beyond demand (m³); non-zero under PDA when head
    /// exceeds the required-pressure threshold.
    pub surplus_volume: f64,
    /// Number of reporting periods in which delivered demand was below required.
    pub deficit_periods: usize,
    /// Length of the longest consecutive run of deficit periods.
    pub longest_deficit_streak: usize,
    /// Maximum observed instantaneous deficit rate (m³/s) across all periods.
    pub max_deficit_rate: f64,
}

impl DemandReliabilityNode {
    /// Volume of demand that was actually served: `max(required − unmet, 0)` (m³).
    pub fn served_volume(&self) -> f64 {
        (self.required_volume - self.unmet_volume).max(0.0)
    }

    /// Ratio of served to required volume in `[0, 1]` (1.0 when required ≤ 0).
    pub fn reliability_ratio(&self) -> f64 {
        if self.required_volume <= 0.0 {
            1.0
        } else {
            self.served_volume() / self.required_volume
        }
    }
}

/// Network-level demand-reliability summary aggregated across all junctions.
///
/// Volumes are in m³ (internal SI units).
#[derive(Debug, Clone, Default)]
pub struct DemandReliabilitySummary {
    /// Number of junctions included in the analysis.
    pub node_count: usize,
    /// Number of reporting periods in the simulation.
    pub period_count: usize,
    /// Total required demand volume across all junctions and periods (m³).
    pub required_volume: f64,
    /// Total delivered demand volume across all junctions and periods (m³).
    pub delivered_volume: f64,
    /// Total unmet demand volume across all junctions and periods (m³).
    pub unmet_volume: f64,
    /// Total surplus delivered volume across all junctions and periods (m³).
    pub surplus_volume: f64,
    /// Total number of (junction, period) pairs with a deficit.
    pub deficit_periods: usize,
    /// Highest `max_deficit_rate` observed across all junctions (m³/s).
    pub max_node_deficit_rate: f64,
}

impl DemandReliabilitySummary {
    /// Network-wide served volume: `max(required − unmet, 0)` (m³).
    pub fn served_volume(&self) -> f64 {
        (self.required_volume - self.unmet_volume).max(0.0)
    }

    /// Network-wide reliability ratio in `[0, 1]` (1.0 when required ≤ 0).
    pub fn reliability_ratio(&self) -> f64 {
        if self.required_volume <= 0.0 {
            1.0
        } else {
            self.served_volume() / self.required_volume
        }
    }
}

/// Complete demand-reliability report for a single simulation run.
#[derive(Debug, Clone)]
pub struct DemandReliabilityReport {
    /// Demand model used during the simulation (`DDA` or `PDA`).
    pub demand_model: DemandModel,
    /// Duration of each reporting period (seconds).
    pub report_step_seconds: f64,
    /// Total number of reporting periods in the `.out` file.
    pub period_count: usize,
    /// Per-junction metrics, ordered to match [`Network::nodes`].
    pub nodes: Vec<DemandReliabilityNode>,
    /// Network-level summary aggregated from all junctions.
    pub summary: DemandReliabilitySummary,
}

/// Compute delivered-vs-required demand reliability metrics from a persisted
/// `.out` file and the corresponding loaded network.
///
/// **Inputs**:
/// - delivered demand samples by node and reporting period (read from `.out`)
/// - required demand recomputed from network demand categories and patterns
///   at each reporting time
/// - reporting period duration
/// - optional deficit tolerance (via [`DemandReliabilityOptions`])
///
/// Delivered demand is read from `.out` and converted into internal flow units.
/// Uses streaming period reads — all per-period values are never materialised
/// in memory simultaneously.
///
/// **Outputs** (in [`DemandReliabilityReport`]):
/// - per-junction: required, delivered, unmet, and surplus demand volumes;
///   reliability ratio; deficit period count and longest deficit streak;
///   maximum observed deficit rate
/// - network-level summary statistics
pub fn compute_demand_reliability_from_out(
    out_path: &std::path::Path,
    network: &Network,
) -> Result<DemandReliabilityReport, AnalysisComputeError> {
    compute_demand_reliability_from_out_with_options(
        out_path,
        network,
        DemandReliabilityOptions::default(),
    )
}

/// Like [`compute_demand_reliability_from_out`] but with explicit [`DemandReliabilityOptions`].
pub fn compute_demand_reliability_from_out_with_options(
    out_path: &std::path::Path,
    network: &Network,
    options: DemandReliabilityOptions,
) -> Result<DemandReliabilityReport, AnalysisComputeError> {
    validate_options(options)?;

    let meta = out_reader::read_metadata_checked(out_path)
        .map_err(|e| AnalysisComputeError::OutRead(e.to_string()))?;

    if meta.n_periods == 0 {
        return Err(AnalysisComputeError::NoSnapshots);
    }

    if meta.n_nodes != network.nodes.len() {
        return Err(AnalysisComputeError::InvalidInput(format!(
            "node count mismatch: .out has {} nodes, network has {} nodes",
            meta.n_nodes,
            network.nodes.len()
        )));
    }

    let dt = if meta.report_step > 0.0 {
        meta.report_step
    } else {
        1.0
    };

    let flow_units_code = read_flow_units_code(out_path)?;
    let flow_units = flow_units_from_code(flow_units_code).ok_or_else(|| {
        AnalysisComputeError::InvalidInput(format!(
            "unsupported flow units code in .out header: {flow_units_code}"
        ))
    })?;
    let out_to_internal_flow = 1.0 / make_ucf(flow_units, 1.0).flow;

    let owned_pattern_index = if network.pattern_index.is_empty() && !network.patterns.is_empty() {
        Some(build_pattern_index(network))
    } else {
        None
    };
    let pattern_index = owned_pattern_index
        .as_ref()
        .unwrap_or(&network.pattern_index);

    let junction_indices: Vec<usize> = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            if matches!(node.kind, NodeKind::Junction(_)) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    let mut nodes: Vec<DemandReliabilityNode> = junction_indices
        .iter()
        .map(|&index| DemandReliabilityNode {
            node_index: index,
            node_id: network.nodes[index].base.id.clone(),
            required_volume: 0.0,
            delivered_volume: 0.0,
            unmet_volume: 0.0,
            surplus_volume: 0.0,
            deficit_periods: 0,
            longest_deficit_streak: 0,
            max_deficit_rate: 0.0,
        })
        .collect();
    let mut current_streaks = vec![0usize; nodes.len()];

    for period in 0..meta.n_periods {
        let t = meta.report_start + period as f64 * dt;
        let period_results = out_reader::read_period(out_path, &meta, period)
            .map_err(AnalysisComputeError::OutRead)?;

        for (j, (node_stats, node_index)) in
            nodes.iter_mut().zip(junction_indices.iter()).enumerate()
        {
            let junction = match &network.nodes[*node_index].kind {
                NodeKind::Junction(junction) => junction,
                _ => continue,
            };

            let required = junction
                .total_demand(t, &network.options, &network.patterns, pattern_index)
                .max(0.0);

            let delivered_output = period_results.node_demand[*node_index] as f64;
            let delivered_internal = (delivered_output * out_to_internal_flow).max(0.0);

            observe_demand_sample(
                node_stats,
                &mut current_streaks[j],
                required,
                delivered_internal,
                dt,
                options.deficit_tolerance,
            );
        }
    }

    let mut summary = DemandReliabilitySummary {
        node_count: nodes.len(),
        period_count: meta.n_periods,
        ..DemandReliabilitySummary::default()
    };

    for node in &nodes {
        summary.required_volume += node.required_volume;
        summary.delivered_volume += node.delivered_volume;
        summary.unmet_volume += node.unmet_volume;
        summary.surplus_volume += node.surplus_volume;
        summary.deficit_periods += node.deficit_periods;
        summary.max_node_deficit_rate = summary.max_node_deficit_rate.max(node.max_deficit_rate);
    }

    Ok(DemandReliabilityReport {
        demand_model: network.options.demand_model,
        report_step_seconds: dt,
        period_count: meta.n_periods,
        nodes,
        summary,
    })
}

fn validate_options(options: DemandReliabilityOptions) -> Result<(), AnalysisComputeError> {
    if !options.deficit_tolerance.is_finite() || options.deficit_tolerance < 0.0 {
        return Err(AnalysisComputeError::InvalidInput(
            "deficit tolerance must be a finite value >= 0".to_string(),
        ));
    }
    Ok(())
}

fn observe_demand_sample(
    node: &mut DemandReliabilityNode,
    current_streak: &mut usize,
    required_rate: f64,
    delivered_rate: f64,
    dt_seconds: f64,
    deficit_tolerance: f64,
) {
    let required = required_rate.max(0.0);
    let delivered = delivered_rate.max(0.0);

    let deficit = (required - delivered).max(0.0);
    let surplus = (delivered - required).max(0.0);

    node.required_volume += required * dt_seconds;
    node.delivered_volume += delivered * dt_seconds;
    node.unmet_volume += deficit * dt_seconds;
    node.surplus_volume += surplus * dt_seconds;

    if deficit > deficit_tolerance {
        node.deficit_periods += 1;
        *current_streak += 1;
        node.longest_deficit_streak = node.longest_deficit_streak.max(*current_streak);
        node.max_deficit_rate = node.max_deficit_rate.max(deficit);
    } else {
        *current_streak = 0;
    }
}

fn build_pattern_index(network: &Network) -> HashMap<String, usize> {
    network
        .patterns
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i))
        .collect()
}

fn read_flow_units_code(path: &std::path::Path) -> Result<i32, AnalysisComputeError> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| AnalysisComputeError::OutRead(format!("failed to open .out file: {e}")))?;

    let mut header = [0u8; 44];
    file.read_exact(&mut header)
        .map_err(|e| AnalysisComputeError::OutRead(format!("failed to read .out header: {e}")))?;

    Ok(i32::from_le_bytes(header[40..44].try_into().unwrap()))
}

fn flow_units_from_code(code: i32) -> Option<FlowUnits> {
    match code {
        0 => Some(FlowUnits::Cfs),
        1 => Some(FlowUnits::Gpm),
        2 => Some(FlowUnits::Mgd),
        3 => Some(FlowUnits::Imgd),
        4 => Some(FlowUnits::Afd),
        5 => Some(FlowUnits::Lps),
        6 => Some(FlowUnits::Lpm),
        7 => Some(FlowUnits::Mld),
        8 => Some(FlowUnits::Cmh),
        9 => Some(FlowUnits::Cmd),
        10 => Some(FlowUnits::Cms),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_sample_tracks_unmet_and_streaks() {
        let mut node = DemandReliabilityNode {
            node_index: 1,
            node_id: "J1".to_string(),
            required_volume: 0.0,
            delivered_volume: 0.0,
            unmet_volume: 0.0,
            surplus_volume: 0.0,
            deficit_periods: 0,
            longest_deficit_streak: 0,
            max_deficit_rate: 0.0,
        };
        let mut streak = 0usize;
        let dt = 60.0;

        observe_demand_sample(&mut node, &mut streak, 10.0, 8.0, dt, 0.0);
        observe_demand_sample(&mut node, &mut streak, 10.0, 6.0, dt, 0.0);
        observe_demand_sample(&mut node, &mut streak, 10.0, 10.0, dt, 0.0);

        assert!((node.required_volume - 1800.0).abs() < 1e-12);
        assert!((node.delivered_volume - 1440.0).abs() < 1e-12);
        assert!((node.unmet_volume - 360.0).abs() < 1e-12);
        assert_eq!(node.deficit_periods, 2);
        assert_eq!(node.longest_deficit_streak, 2);
        assert!((node.max_deficit_rate - 4.0).abs() < 1e-12);
        assert!((node.reliability_ratio() - 0.8).abs() < 1e-12);
    }

    #[test]
    fn flow_units_codes_map_correctly() {
        assert_eq!(flow_units_from_code(0), Some(FlowUnits::Cfs));
        assert_eq!(flow_units_from_code(1), Some(FlowUnits::Gpm));
        assert_eq!(flow_units_from_code(10), Some(FlowUnits::Cms));
        assert_eq!(flow_units_from_code(11), None);
    }

    #[test]
    fn invalid_options_are_rejected() {
        let bad = DemandReliabilityOptions {
            deficit_tolerance: -1.0,
        };
        let err = validate_options(bad).expect_err("expected invalid option");
        assert!(matches!(err, AnalysisComputeError::InvalidInput(_)));
    }
}
