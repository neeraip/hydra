//! The results a §13 analysis reads, as data (format-blind
//! extraction, phase 4).
//!
//! Post-simulation analytics fold over a persisted results set. The
//! engine defines the metadata, the per-period values, the scans and
//! the [`ResultsSource`] contract; where they come from — the
//! dialect's `.out` file, a test's vectors — is the caller's business.

/// One pump-energy record from the energy section (28 bytes).
#[derive(Debug, Clone)]
pub struct PumpEnergyRecord {
    /// 1-based link index of the pump.
    pub link_index: i32,
    /// Percentage of simulation time the pump was online (0–100).
    pub pct_online: f32,
    /// Average efficiency (%).
    pub avg_efficiency: f32,
    /// Average kWh per unit of flow.
    pub avg_kwh_per_flow: f32,
    /// Average power (kW).
    pub avg_kw: f32,
    /// Peak power (kW).
    pub peak_kw: f32,
    /// Average daily cost.
    pub avg_cost_per_day: f32,
}

/// The energy section: one record per pump plus the trailing demand charge.
#[derive(Debug, Clone)]
pub struct OutEnergy {
    /// Per-pump energy records.  Length = `n_pumps`.
    pub pumps: Vec<PumpEnergyRecord>,
    /// Demand charge (trailing REAL4 after all pump records).
    pub demand_charge: f32,
}

/// All node and link variable values for one reporting period.
#[derive(Debug, Clone)]
pub struct PeriodResult {
    // Node variables (each Vec has length `n_nodes`)
    /// Actual delivered demand at each node (flow units from prolog header).
    pub node_demand: Vec<f32>,
    /// Hydraulic head at each node (length units from prolog header).
    pub node_head: Vec<f32>,
    /// Gauge pressure at each node (pressure units from prolog header).
    pub node_pressure: Vec<f32>,
    /// Water quality value at each node (mg/L, h, or % depending on mode).
    pub node_quality: Vec<f32>,
    // Link variables (each Vec has length `n_links`)
    /// Volumetric flow rate through each link (flow units; positive = from→to).
    pub link_flow: Vec<f32>,
    /// Mean velocity through each link (velocity units).
    pub link_velocity: Vec<f32>,
    /// Head loss across each link (length units; positive = from head > to head).
    pub link_headloss: Vec<f32>,
    /// Water quality value in each link.
    pub link_quality: Vec<f32>,
    /// Link status flag (0 = closed/inactive, 1 = open/active).
    pub link_status: Vec<f32>,
    /// Link setting (pump speed ratio or valve setpoint).
    pub link_setting: Vec<f32>,
    /// Bulk reaction rate in each link (mass/time).
    pub link_reaction_rate: Vec<f32>,
    /// Darcy-Weisbach friction factor for each link (dimensionless).
    pub link_friction_factor: Vec<f32>,
}

/// Lightweight metadata extracted from the `.out` prolog header (first 60
/// bytes) and epilog (last 12 or 20 bytes, by format version).  Total I/O is
/// at most 80 bytes regardless of file size.
#[derive(Debug, Clone)]
pub struct OutMetadata {
    /// Number of nodes (junctions + reservoirs + tanks) in the network.
    pub n_nodes: usize,
    /// Number of tank/reservoir nodes in the network.
    pub n_tanks: usize,
    /// Number of links (pipes + pumps + valves) in the network.
    pub n_links: usize,
    /// Number of pumps in the network.
    pub n_pumps: usize,
    /// EPANET-compatible quality mode flag from the prolog header.
    /// 0=None, 1=Chemical, 2=Age, 3=Trace.
    pub quality_flag: i32,
    /// Simulation time at which reporting starts (seconds).
    pub report_start: f64,
    /// Duration of each reporting period (seconds).
    pub report_step: f64,
    /// Total simulation duration (seconds) from the prolog header
    /// (model spec §4.4.2). `0` for a steady-state run.
    pub duration: f64,
    /// Number of reporting periods written to the file.
    pub n_periods: usize,
}

/// Category for invalid or unreadable `.out` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutValidityKind {
    /// The file is missing.
    Missing,
    /// The file exists but could not be read due to an I/O error.
    Io,
    /// The file is truncated and does not contain all required bytes.
    Incomplete,
    /// The file bytes are malformed or internally inconsistent.
    Corrupt,
    /// The file appears structurally valid but uses unsupported values/version.
    Unsupported,
}

/// Structured validation error for `.out` reads.
#[derive(Debug, Clone)]
pub struct OutValidityError {
    /// Category of the validity failure.
    pub kind: OutValidityKind,
    /// Human-readable description of the specific problem.
    pub detail: String,
}

impl std::fmt::Display for OutValidityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = match self.kind {
            OutValidityKind::Missing => "missing",
            OutValidityKind::Io => "io",
            OutValidityKind::Incomplete => "incomplete",
            OutValidityKind::Corrupt => "corrupt",
            OutValidityKind::Unsupported => "unsupported",
        };
        write!(f, "Invalid .out ({tag}): {}", self.detail)
    }
}

impl std::error::Error for OutValidityError {}

impl OutMetadata {
    /// Build the snapshot-time vector from prolog header fields.
    pub fn snapshot_times(&self) -> Vec<f64> {
        (0..self.n_periods)
            .map(|i| self.report_start + (i as f64) * self.report_step)
            .collect()
    }
}

/// Global min/max ranges across sampled periods for common result variables.
///
/// All values are in the units stored in the `.out` file (which match the
/// user-declared unit system in the INP `[OPTIONS]` section).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResultRanges {
    /// Minimum nodal gauge pressure across all sampled periods.
    pub pressure_min: f64,
    /// Maximum nodal gauge pressure across all sampled periods.
    pub pressure_max: f64,
    /// Minimum nodal hydraulic head across all sampled periods.
    pub head_min: f64,
    /// Maximum nodal hydraulic head across all sampled periods.
    pub head_max: f64,
    /// Minimum nodal demand across all sampled periods.
    pub demand_min: f64,
    /// Maximum nodal demand across all sampled periods.
    pub demand_max: f64,
    /// Minimum link flow rate across all sampled periods.
    pub flow_min: f64,
    /// Maximum link flow rate across all sampled periods.
    pub flow_max: f64,
    /// Minimum link velocity across all sampled periods.
    pub velocity_min: f64,
    /// Maximum link velocity across all sampled periods.
    pub velocity_max: f64,
    /// Minimum link unit headloss across all sampled periods.
    pub headloss_min: f64,
    /// Maximum link unit headloss across all sampled periods.
    pub headloss_max: f64,
    /// Global min/max quality value across all periods and nodes.
    /// `None` when the file was written with `quality_flag == 0` (no quality run).
    pub quality_min: Option<f64>,
    /// Global maximum quality value across all periods and nodes.
    /// `None` when `quality_flag == 0`.
    pub quality_max: Option<f64>,
}

impl Default for ResultRanges {
    fn default() -> Self {
        Self {
            pressure_min: f64::INFINITY,
            pressure_max: f64::NEG_INFINITY,
            head_min: f64::INFINITY,
            head_max: f64::NEG_INFINITY,
            demand_min: f64::INFINITY,
            demand_max: f64::NEG_INFINITY,
            flow_min: f64::INFINITY,
            flow_max: f64::NEG_INFINITY,
            velocity_min: f64::INFINITY,
            velocity_max: f64::NEG_INFINITY,
            headloss_min: f64::INFINITY,
            headloss_max: f64::NEG_INFINITY,
            quality_min: None,
            quality_max: None,
        }
    }
}

impl ResultRanges {
    /// Replace infinities with sensible defaults and ensure max > min.
    pub fn sanitise(&mut self) {
        fn fix(min: &mut f64, max: &mut f64, default_min: f64, default_max: f64) {
            if !min.is_finite() {
                *min = default_min;
            }
            if !max.is_finite() {
                *max = default_max;
            }
            if (*max - *min).abs() < 1e-9 {
                *max = *min + 1.0;
            }
        }
        fix(&mut self.pressure_min, &mut self.pressure_max, 0.0, 80.0);
        fix(&mut self.head_min, &mut self.head_max, 0.0, 100.0);
        fix(&mut self.demand_min, &mut self.demand_max, 0.0, 10.0);
        fix(&mut self.flow_min, &mut self.flow_max, 0.0, 100.0);
        fix(&mut self.velocity_min, &mut self.velocity_max, 0.0, 5.0);
        fix(&mut self.headloss_min, &mut self.headloss_max, 0.0, 10.0);
        if let (Some(qmin), Some(qmax)) = (&mut self.quality_min, &mut self.quality_max) {
            fix(qmin, qmax, 0.0, 1.0);
        }
    }

    /// Update ranges from a single [`PeriodResult`].
    pub fn update_from_period(&mut self, pr: &PeriodResult) {
        for &v in &pr.node_pressure {
            let v = v as f64;
            if v < self.pressure_min {
                self.pressure_min = v;
            }
            if v > self.pressure_max {
                self.pressure_max = v;
            }
        }
        for &v in &pr.node_head {
            let v = v as f64;
            if v < self.head_min {
                self.head_min = v;
            }
            if v > self.head_max {
                self.head_max = v;
            }
        }
        for &v in &pr.node_demand {
            let v = v as f64;
            if v < self.demand_min {
                self.demand_min = v;
            }
            if v > self.demand_max {
                self.demand_max = v;
            }
        }
        for &v in &pr.link_flow {
            let v = v as f64;
            if v < self.flow_min {
                self.flow_min = v;
            }
            if v > self.flow_max {
                self.flow_max = v;
            }
        }
        for &v in &pr.link_velocity {
            let v = v as f64;
            if v < self.velocity_min {
                self.velocity_min = v;
            }
            if v > self.velocity_max {
                self.velocity_max = v;
            }
        }
        for &v in &pr.link_headloss {
            let v = v as f64;
            if v < self.headloss_min {
                self.headloss_min = v;
            }
            if v > self.headloss_max {
                self.headloss_max = v;
            }
        }
        // Quality arrays are populated only when quality_flag != 0.  When they
        // are non-empty, fold them into the running quality min/max.
        for &v in pr.node_quality.iter().chain(pr.link_quality.iter()) {
            let v = v as f64;
            match &mut self.quality_min {
                Some(m) => {
                    if v < *m {
                        *m = v;
                    }
                }
                None => {
                    self.quality_min = Some(v);
                }
            }
            match &mut self.quality_max {
                Some(m) => {
                    if v > *m {
                        *m = v;
                    }
                }
                None => {
                    self.quality_max = Some(v);
                }
            }
        }
    }
}

/// Cross-period statistics accumulated by streaming every period of a `.out` file.
#[derive(Debug)]
pub struct AnalyticsScan {
    /// Per-node minimum pressure across all periods. `f64::INFINITY` when no data.
    pub node_min_pressure: Vec<f64>,
    /// Per-link maximum absolute velocity across all periods.
    pub link_max_velocity: Vec<f64>,
    /// Per-link maximum unit headloss across all periods, in the file's
    /// stored ratio (m/km ≡ ft/kft for pipes; head gain/loss for pumps
    /// and valves, which consumers filter out).
    pub link_max_unit_headloss: Vec<f64>,
    /// Per-node minimum quality across all periods (mode's unit).
    /// `f64::INFINITY` when no data.
    pub node_min_quality: Vec<f64>,
    /// Per-node maximum quality across all periods (mode's unit).
    pub node_max_quality: Vec<f64>,
    /// Per-period mass-balance percentage (outflow / inflow × 100, capped at 100).
    pub mb_series: Vec<f64>,
    /// Cumulative demand summed over all nodes and periods where demand is positive
    /// (network inflow), in raw `.out` units.
    pub total_inflow: f64,
    /// Cumulative demand summed over all nodes and periods where demand is negative
    /// (network outflow), stored as a positive value, in raw `.out` units.
    pub total_outflow: f64,
    /// Per-tank head series: `tank_head[ti][p]` = head of tank `ti` at period `p`.
    /// Tank relative index `ti = node_idx − (n_nodes − n_tanks)`.
    pub tank_head: Vec<Vec<f64>>,
}

/// A results set an analysis can read: neutral metadata once, then the
/// per-period and whole-run reads the blocks fold over.
pub trait ResultsSource {
    fn meta(&self) -> &OutMetadata;
    /// The file's declared flow-unit code (model spec §4.4.2).
    fn flow_units_code(&self) -> Result<i32, OutValidityError>;
    /// Node indices that are tanks or reservoirs, in node order.
    fn tank_node_indices(&self) -> Result<Vec<usize>, String>;
    /// The pump energy table.
    fn read_energy(&self) -> Result<OutEnergy, String>;
    /// One reporting period's values.
    fn read_period(&self, period: usize) -> Result<PeriodResult, String>;
    /// Value ranges over at most `max_samples` sampled periods.
    fn scan_ranges(&self, max_samples: usize) -> Result<ResultRanges, String>;
    /// Cross-period statistics from one streaming pass.
    fn scan_analytics(&self) -> Result<AnalyticsScan, String>;
}
