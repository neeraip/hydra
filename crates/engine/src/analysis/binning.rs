use super::errors::AnalysisComputeError;
use crate::io::analysis_io::{
    AnalysisArtifact, AnalysisSource, ContinuousDistribution, DistributionSet, HistogramBin,
    StatusDistribution, SummaryStats, ANALYSIS_SCHEMA_VERSION,
};
use crate::io::out_reader;
use crate::RuntimeEstimate;
use crate::simulation::Simulation;

/// Selects which result variables are included in an [`AnalysisArtifact`].
///
/// Setting a field to `false` omits that variable entirely; the corresponding
/// histograms and summary statistics are not computed or stored. Use
/// [`AnalysisSelection::all`] to enable every variable.
#[derive(Debug, Clone, Copy)]
pub struct AnalysisSelection {
    /// Include nodal gauge-pressure histograms and summaries.
    pub pressure: bool,
    /// Include nodal hydraulic-head histograms and summaries.
    pub head: bool,
    /// Include link volumetric-flow histograms and summaries.
    pub flow: bool,
    /// Include link mean-velocity histograms and summaries.
    pub velocity: bool,
    /// Include link status (open/closed) distributions.
    pub status: bool,
}

impl AnalysisSelection {
    /// Returns an `AnalysisSelection` with every variable enabled.
    pub fn all() -> Self {
        Self {
            pressure: true,
            head: true,
            flow: true,
            velocity: true,
            status: true,
        }
    }

    fn any(self) -> bool {
        self.pressure || self.head || self.flow || self.velocity || self.status
    }

    fn any_continuous(self) -> bool {
        self.pressure || self.head || self.flow || self.velocity
    }

    fn continuous_count(self) -> usize {
        usize::from(self.pressure)
            + usize::from(self.head)
            + usize::from(self.flow)
            + usize::from(self.velocity)
    }
}

/// Estimate analysis effort from summary metadata and module selection.
pub fn estimate_analysis_runtime_millis(
    node_count: usize,
    link_count: usize,
    period_count: usize,
    selection: AnalysisSelection,
) -> RuntimeEstimate {
    if !selection.any() || period_count == 0 {
        return RuntimeEstimate::Low;
    }

    let nodes = node_count.max(1) as f64;
    let links = link_count.max(1) as f64;
    let periods = period_count.max(1) as f64;

    let node_modules = usize::from(selection.pressure) + usize::from(selection.head);
    let link_modules = usize::from(selection.flow) + usize::from(selection.velocity);
    let status_weight = if selection.status { 0.35 } else { 0.0 };

    let per_period_ops =
        nodes * node_modules as f64 + links * (link_modules as f64 + status_weight);
    let pass_factor = if selection.any_continuous() {
        1.75
    } else {
        1.0
    };
    let complexity_score = periods * per_period_ops * pass_factor;

    if complexity_score < 4_000_000.0 {
        RuntimeEstimate::Low
    } else if complexity_score < 40_000_000.0 {
        RuntimeEstimate::Medium
    } else {
        RuntimeEstimate::High
    }
}

/// Build an aggregated analysis artifact from simulation snapshots.
pub fn build_analysis_artifact(sim: &Simulation) -> Result<AnalysisArtifact, AnalysisComputeError> {
    let times = sim.snapshot_times();
    if times.is_empty() {
        return Err(AnalysisComputeError::NoSnapshots);
    }

    let mut pressure_values = Vec::new();
    let mut head_values = Vec::new();
    let mut flow_values = Vec::new();
    let mut velocity_values = Vec::new();
    let mut status = StatusDistribution::default();

    for t in &times {
        let node_results = sim.all_node_results_at(*t)?;
        for node in node_results {
            pressure_values.push(node.pressure);
            head_values.push(node.head);
        }

        let link_results = sim.all_link_results_at(*t)?;
        for link in link_results {
            flow_values.push(link.flow.abs());
            velocity_values.push(link.velocity);
            increment_status_from_sim_value(&mut status, link.status);
        }
    }

    Ok(AnalysisArtifact {
        schema_version: ANALYSIS_SCHEMA_VERSION,
        source: AnalysisSource {
            output_file: "network.out".to_string(),
            report_file: "report.json".to_string(),
            period_count: times.len(),
        },
        distributions: DistributionSet {
            pressure: make_distribution(&pressure_values, 20),
            head: make_distribution(&head_values, 20),
            flow: make_distribution(&flow_values, 20),
            velocity: make_distribution(&velocity_values, 20),
            status,
        },
        demand_reliability: None,
        service_compliance: None,
    })
}

/// Build an aggregated analysis artifact from a persisted `.out` file.
pub fn build_analysis_artifact_from_out(
    out_path: &std::path::Path,
) -> Result<AnalysisArtifact, AnalysisComputeError> {
    build_analysis_artifact_from_out_with_progress_and_selection(
        out_path,
        AnalysisSelection::all(),
        |_percent, _phase, _processed, _total| {},
    )
}

/// Build an aggregated analysis artifact from a persisted `.out` file,
/// reporting incremental progress through read and aggregation stages.
pub fn build_analysis_artifact_from_out_with_progress<F>(
    out_path: &std::path::Path,
    on_progress: F,
) -> Result<AnalysisArtifact, AnalysisComputeError>
where
    F: FnMut(f64, &'static str, usize, usize),
{
    build_analysis_artifact_from_out_with_progress_and_selection(
        out_path,
        AnalysisSelection::all(),
        on_progress,
    )
}

/// Build an [`AnalysisArtifact`] from a `.out` file with per-period progress
/// callbacks and configurable variable selection.
///
/// `on_progress` is called after each reporting period with
/// `(pct_complete: f64, phase_label: &str, current_period: usize, total_periods: usize)`.
/// Pass a no-op closure when progress reporting is not needed.
pub fn build_analysis_artifact_from_out_with_progress_and_selection<F>(
    out_path: &std::path::Path,
    selection: AnalysisSelection,
    mut on_progress: F,
) -> Result<AnalysisArtifact, AnalysisComputeError>
where
    F: FnMut(f64, &'static str, usize, usize),
{
    let meta = out_reader::read_metadata_checked(out_path)
        .map_err(|e| AnalysisComputeError::OutRead(e.to_string()))?;

    if meta.n_periods == 0 {
        return Err(AnalysisComputeError::NoSnapshots);
    }

    if !selection.any() {
        on_progress(100.0, "Finalizing analysis artifact", 0, 0);
        return Ok(AnalysisArtifact {
            schema_version: ANALYSIS_SCHEMA_VERSION,
            source: AnalysisSource {
                output_file: "network.out".to_string(),
                report_file: "report.json".to_string(),
                period_count: meta.n_periods,
            },
            distributions: DistributionSet::default(),
            demand_reliability: None,
            service_compliance: None,
        });
    }

    on_progress(0.0, "Scanning periods", 0, meta.n_periods);

    let mut pressure_stats = selection.pressure.then_some(RunningStats::default());
    let mut head_stats = selection.head.then_some(RunningStats::default());
    let mut flow_stats = selection.flow.then_some(RunningStats::default());
    let mut velocity_stats = selection.velocity.then_some(RunningStats::default());
    let mut status = StatusDistribution::default();
    let needs_pass2 = selection.any_continuous();
    let module_finalize_steps = selection.continuous_count();
    let total_work =
        (meta.n_periods + if needs_pass2 { meta.n_periods } else { 0 } + module_finalize_steps + 1)
            as f64;

    // Pass 1: compute exact min/max/mean and status counts.
    for period in 0..meta.n_periods {
        let pr = out_reader::read_period(out_path, &meta, period)
            .map_err(AnalysisComputeError::OutRead)?;

        if let Some(stats) = pressure_stats.as_mut() {
            for v in pr.node_pressure.iter().copied() {
                stats.observe(v as f64);
            }
        }
        if let Some(stats) = head_stats.as_mut() {
            for v in pr.node_head.iter().copied() {
                stats.observe(v as f64);
            }
        }
        if let Some(stats) = flow_stats.as_mut() {
            for v in pr.link_flow.iter().copied() {
                stats.observe((v as f64).abs());
            }
        }
        if let Some(stats) = velocity_stats.as_mut() {
            for v in pr.link_velocity.iter().copied() {
                stats.observe(v as f64);
            }
        }
        if selection.status {
            for v in pr.link_status {
                increment_status_from_out_value(&mut status, v as f64);
            }
        }

        let percent = 100.0 * (period + 1) as f64 / total_work;
        on_progress(percent, "Scanning periods", period + 1, meta.n_periods);
    }

    let mut pressure_hist = pressure_stats
        .as_ref()
        .map(|stats| HistogramAccumulator::new_from_stats(stats, 20));
    let mut head_hist = head_stats
        .as_ref()
        .map(|stats| HistogramAccumulator::new_from_stats(stats, 20));
    let mut flow_hist = flow_stats
        .as_ref()
        .map(|stats| HistogramAccumulator::new_from_stats(stats, 20));
    let mut velocity_hist = velocity_stats
        .as_ref()
        .map(|stats| HistogramAccumulator::new_from_stats(stats, 20));

    if needs_pass2 {
        // Pass 2: fill histograms using fixed ranges from pass 1.
        on_progress(
            100.0 * meta.n_periods as f64 / total_work,
            "Binning periods",
            0,
            meta.n_periods,
        );
        for period in 0..meta.n_periods {
            let pr = out_reader::read_period(out_path, &meta, period)
                .map_err(AnalysisComputeError::OutRead)?;

            if let Some(hist) = pressure_hist.as_mut() {
                for v in pr.node_pressure {
                    hist.observe(v as f64);
                }
            }
            if let Some(hist) = head_hist.as_mut() {
                for v in pr.node_head {
                    hist.observe(v as f64);
                }
            }
            if let Some(hist) = flow_hist.as_mut() {
                for v in pr.link_flow {
                    hist.observe((v as f64).abs());
                }
            }
            if let Some(hist) = velocity_hist.as_mut() {
                for v in pr.link_velocity {
                    hist.observe(v as f64);
                }
            }

            let done_work = meta.n_periods + period + 1;
            let percent = 100.0 * done_work as f64 / total_work;
            on_progress(percent, "Binning periods", period + 1, meta.n_periods);
        }
    }

    let mut completed_work = meta.n_periods + if needs_pass2 { meta.n_periods } else { 0 };
    let mut pressure = ContinuousDistribution::default();
    let mut head = ContinuousDistribution::default();
    let mut flow = ContinuousDistribution::default();
    let mut velocity = ContinuousDistribution::default();

    if selection.pressure {
        on_progress(
            100.0 * completed_work as f64 / total_work,
            "Computing pressure distribution",
            meta.n_periods,
            meta.n_periods,
        );
        if let (Some(hist), Some(stats)) = (pressure_hist.take(), pressure_stats) {
            pressure = hist.into_distribution(&stats);
        }
        completed_work += 1;
    }

    if selection.head {
        on_progress(
            100.0 * completed_work as f64 / total_work,
            "Computing head distribution",
            meta.n_periods,
            meta.n_periods,
        );
        if let (Some(hist), Some(stats)) = (head_hist.take(), head_stats) {
            head = hist.into_distribution(&stats);
        }
        completed_work += 1;
    }

    if selection.flow {
        on_progress(
            100.0 * completed_work as f64 / total_work,
            "Computing flow distribution",
            meta.n_periods,
            meta.n_periods,
        );
        if let (Some(hist), Some(stats)) = (flow_hist.take(), flow_stats) {
            flow = hist.into_distribution(&stats);
        }
        completed_work += 1;
    }

    if selection.velocity {
        on_progress(
            100.0 * completed_work as f64 / total_work,
            "Computing velocity distribution",
            meta.n_periods,
            meta.n_periods,
        );
        if let (Some(hist), Some(stats)) = (velocity_hist.take(), velocity_stats) {
            velocity = hist.into_distribution(&stats);
        }
        completed_work += 1;
    }

    on_progress(
        100.0 * completed_work as f64 / total_work,
        "Finalizing analysis artifact",
        meta.n_periods,
        meta.n_periods,
    );

    Ok(AnalysisArtifact {
        schema_version: ANALYSIS_SCHEMA_VERSION,
        source: AnalysisSource {
            output_file: "network.out".to_string(),
            report_file: "report.json".to_string(),
            period_count: meta.n_periods,
        },
        distributions: DistributionSet {
            pressure,
            head,
            flow,
            velocity,
            status: if selection.status {
                status
            } else {
                StatusDistribution::default()
            },
        },
        demand_reliability: None,
        service_compliance: None,
    })
}

fn increment_status_from_sim_value(status: &mut StatusDistribution, value: f64) {
    match value.round() as i32 {
        0 => status.closed += 1,
        1 => status.open += 1,
        2 => status.active += 1,
        _ => status.other += 1,
    }
}

fn increment_status_from_out_value(status: &mut StatusDistribution, value: f64) {
    // `.out` stores EPANET StatusType codes: 0,1,2,3,4,6,7.
    match value.round() as i32 {
        3 => status.open += 1,
        4 | 6 => status.active += 1,
        0 | 1 | 2 | 7 => status.closed += 1,
        _ => status.other += 1,
    }
}

#[derive(Clone, Copy)]
struct RunningStats {
    min: f64,
    max: f64,
    sum: f64,
    count: u64,
}

impl Default for RunningStats {
    fn default() -> Self {
        Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            sum: 0.0,
            count: 0,
        }
    }
}

impl RunningStats {
    fn observe(&mut self, value: f64) {
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.sum += value;
        self.count += 1;
    }

    fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}

struct HistogramAccumulator {
    min: f64,
    max: f64,
    width: f64,
    counts: Vec<u64>,
}

impl HistogramAccumulator {
    fn new_from_stats(stats: &RunningStats, n_bins: usize) -> Self {
        let bins = n_bins.max(1);
        let span = (stats.max - stats.min).abs();
        let width = if stats.count == 0 || span <= f64::EPSILON {
            1.0
        } else {
            (stats.max - stats.min) / bins as f64
        };
        Self {
            min: if stats.count == 0 { 0.0 } else { stats.min },
            max: if stats.count == 0 { 0.0 } else { stats.max },
            width,
            counts: vec![0; bins],
        }
    }

    fn observe(&mut self, value: f64) {
        if self.counts.is_empty() {
            return;
        }
        if (self.max - self.min).abs() <= f64::EPSILON {
            self.counts[0] += 1;
            return;
        }
        let mut idx = ((value - self.min) / self.width).floor() as isize;
        if idx < 0 {
            idx = 0;
        }
        if idx as usize >= self.counts.len() {
            idx = self.counts.len() as isize - 1;
        }
        self.counts[idx as usize] += 1;
    }

    fn into_distribution(self, stats: &RunningStats) -> ContinuousDistribution {
        if stats.count == 0 {
            return ContinuousDistribution::default();
        }

        if (stats.max - stats.min).abs() <= f64::EPSILON {
            return ContinuousDistribution {
                bins: vec![HistogramBin {
                    start: stats.min,
                    end: stats.max,
                    count: stats.count,
                }],
                summary: SummaryStats {
                    min: stats.min,
                    max: stats.max,
                    mean: stats.mean(),
                    p05: stats.min,
                    p25: stats.min,
                    p50: stats.min,
                    p75: stats.min,
                    p95: stats.min,
                },
                thresholds: None,
            };
        }

        let bins: Vec<HistogramBin> = self
            .counts
            .iter()
            .enumerate()
            .map(|(i, &count)| HistogramBin {
                start: self.min + i as f64 * self.width,
                end: self.min + (i + 1) as f64 * self.width,
                count,
            })
            .collect();

        let p05 = approx_quantile_from_bins(&bins, 0.05, stats.min, stats.max);
        let p25 = approx_quantile_from_bins(&bins, 0.25, stats.min, stats.max);
        let p50 = approx_quantile_from_bins(&bins, 0.50, stats.min, stats.max);
        let p75 = approx_quantile_from_bins(&bins, 0.75, stats.min, stats.max);
        let p95 = approx_quantile_from_bins(&bins, 0.95, stats.min, stats.max);

        ContinuousDistribution {
            bins,
            summary: SummaryStats {
                min: stats.min,
                max: stats.max,
                mean: stats.mean(),
                p05,
                p25,
                p50,
                p75,
                p95,
            },
            thresholds: None,
        }
    }
}

fn approx_quantile_from_bins(bins: &[HistogramBin], q: f64, min: f64, max: f64) -> f64 {
    if bins.is_empty() {
        return 0.0;
    }
    let total: u64 = bins.iter().map(|b| b.count).sum();
    if total == 0 {
        return 0.0;
    }

    let target = (q.clamp(0.0, 1.0) * (total.saturating_sub(1)) as f64).round() as u64;
    let mut cumulative = 0u64;
    for bin in bins {
        let next = cumulative + bin.count;
        if target < next {
            if bin.count == 0 {
                return bin.start;
            }
            let offset_in_bin = target.saturating_sub(cumulative) as f64 / bin.count as f64;
            let estimate = bin.start + (bin.end - bin.start) * offset_in_bin;
            return estimate.clamp(min, max);
        }
        cumulative = next;
    }

    max
}

fn make_distribution(values: &[f64], n_bins: usize) -> ContinuousDistribution {
    if values.is_empty() {
        return ContinuousDistribution::default();
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;

    let bins = if (max - min).abs() <= f64::EPSILON {
        vec![HistogramBin {
            start: min,
            end: max,
            count: sorted.len() as u64,
        }]
    } else {
        let width = (max - min) / n_bins as f64;
        let mut counts = vec![0u64; n_bins];
        for value in &sorted {
            let mut idx = ((value - min) / width).floor() as usize;
            if idx >= n_bins {
                idx = n_bins - 1;
            }
            counts[idx] += 1;
        }

        (0..n_bins)
            .map(|i| HistogramBin {
                start: min + i as f64 * width,
                end: min + (i + 1) as f64 * width,
                count: counts[i],
            })
            .collect()
    };

    ContinuousDistribution {
        bins,
        summary: SummaryStats {
            min,
            max,
            mean,
            p05: quantile(&sorted, 0.05),
            p25: quantile(&sorted, 0.25),
            p50: quantile(&sorted, 0.50),
            p75: quantile(&sorted, 0.75),
            p95: quantile(&sorted, 0.95),
        },
        thresholds: None,
    }
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let clamped_q = q.clamp(0.0, 1.0);
    let pos = clamped_q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let w = pos - lo as f64;
        sorted[lo] * (1.0 - w) + sorted[hi] * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantile_interpolates() {
        let s = vec![1.0, 2.0, 3.0, 4.0];
        assert!((quantile(&s, 0.5) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn analysis_estimate_increases_with_periods() {
        let selection = AnalysisSelection::all();
        let short = estimate_analysis_runtime_millis(500, 600, 24, selection);
        let long = estimate_analysis_runtime_millis(500, 600, 240, selection);
        assert!(long >= short);
    }

    #[test]
    fn analysis_estimate_increases_with_selected_modules() {
        let fast = AnalysisSelection {
            pressure: true,
            head: false,
            flow: false,
            velocity: false,
            status: true,
        };
        let full = AnalysisSelection::all();
        let fast_est = estimate_analysis_runtime_millis(1_200, 1_400, 96, fast);
        let full_est = estimate_analysis_runtime_millis(1_200, 1_400, 96, full);
        assert!(full_est >= fast_est);
    }

    #[test]
    fn extreme_analysis_case_maps_to_high_effort() {
        let full = AnalysisSelection::all();
        let effort = estimate_analysis_runtime_millis(100_000, 120_000, 20_000, full);
        assert_eq!(effort, RuntimeEstimate::High);
    }
}
