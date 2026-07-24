use std::collections::HashMap;

// ── §2.1 Scalar parameters ────────────────────────────────────────────────────

/// Head-loss formula used by the hydraulic solver (§2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadLossFormula {
    /// Hazen-Williams empirical formula (default).
    HazenWilliams,
    /// Darcy-Weisbach mechanistic formula.
    DarcyWeisbach,
    /// Chezy-Manning formula.
    ChezyManning,
}

/// Demand model: demand-driven (DDA) or pressure-driven (PDA) (§2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandModel {
    /// Demand-driven analysis: demands are always fully satisfied regardless of pressure.
    DemandDriven,
    /// Pressure-driven analysis: delivered demand scales with available pressure.
    PressureDriven,
}

/// Named user-facing flow unit variant (spec.md §3).
///
/// Identifies the scalar applied at the input/output boundary. Does not affect
/// the internal unit system or which formula constants are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowUnits {
    // US customary group (ft, ft³/s base)
    /// Cubic feet per second.
    Cfs,
    /// Gallons per minute.
    Gpm,
    /// Million gallons per day.
    Mgd,
    /// Imperial million gallons per day.
    Imgd,
    /// Acre-feet per day.
    Afd,
    // SI/metric group (m, m³/s base)
    /// Litres per second.
    Lps,
    /// Litres per minute.
    Lpm,
    /// Megalitres per day.
    Mld,
    /// Cubic metres per hour.
    Cmh,
    /// Cubic metres per day.
    Cmd,
    /// Cubic metres per second.
    Cms,
}

/// Deterministic advisory effort category for compute-heavy operations.
///
/// Returned by the simulation and analysis runtime estimators. The estimate
/// is advisory only — it does not alter solver behaviour, time-step selection,
/// or analysis algorithms. For identical inputs the output is always the same
/// (`Low` < `Medium` < `High` is a total order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RuntimeEstimate {
    #[default]
    /// Expected wall-clock time < ~600 ms.
    Low,
    /// Expected wall-clock time ~600 ms – 3 s.
    Medium,
    /// Expected wall-clock time > ~3 s.
    High,
}

/// Water quality simulation mode (§2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityMode {
    /// No quality simulation — quality engine is skipped entirely.
    None,
    /// Dissolved-constituent transport: concentration (mg/L) is advected,
    /// mixed, and subject to bulk and wall reactions (§6.5). Sources inject
    /// via `CONCENTRATION`, `MASS`, `SETPOINT`, or `FLOWPACED` types.
    Chemical,
    /// Water-age tracking: the "concentration" is residence time (hours).
    /// Incremented by δt/3600 at every quality sub-step; reservoirs hold
    /// age = 0. No reactions. Sources are implicit everywhere.
    Age,
    /// Source-trace analysis: fraction of flow (%) originating from the
    /// designated `trace_node`. That node is a 100 % source; all other
    /// fixed-grade inflows inject 0 %. No reactions.
    Trace,
}

/// Wall reaction order: zero-order or first-order (§2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallOrder {
    /// Zero-order wall reaction.
    Zero,
    /// First-order wall reaction.
    One,
}

/// Report output statistic aggregation type (from `TIMES` `STATISTIC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatisticType {
    /// Report all timesteps (default).
    #[default]
    Series,
    /// Report time-averaged values.
    Average,
    /// Report minimum values.
    Minimum,
    /// Report maximum values.
    Maximum,
    /// Report max − min.
    Range,
}

/// Status reporting level in the `[REPORT]` section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportStatus {
    /// No status reporting.
    #[default]
    No,
    /// Report status changes only.
    Yes,
    /// Report all solver iterations.
    Full,
}

/// Selection of nodes or links for reporting.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ReportSelection {
    /// No items reported (default).
    #[default]
    None,
    /// All items reported.
    All,
    /// Specific items by ID.
    Some(Vec<String>),
}

/// Per-field reporting options from the `[REPORT]` section.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportFieldOption {
    /// Whether this field is included in the `.rpt` output.
    pub enabled: bool,
    /// Optional decimal precision override for this field.
    pub precision: Option<u32>,
    /// Optional lower-bound threshold filter for this field.
    pub above: Option<f64>,
    /// Optional upper-bound threshold filter for this field.
    pub below: Option<f64>,
}

/// Options from the `[REPORT]` INP section. Controls `.rpt` file output
/// formatting; does not affect simulation results.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportOptions {
    /// Lines per report page (0 = no page breaks); default 0.
    pub page_size: u32,
    /// Solver status reporting level.
    pub status: ReportStatus,
    /// Whether to print input/output summary.
    pub summary: bool,
    /// Whether to print warning messages.
    pub messages: bool,
    /// Whether to print energy report.
    pub energy: bool,
    /// Which nodes to include in the report.
    pub nodes: ReportSelection,
    /// Which links to include in the report.
    pub links: ReportSelection,
    /// Alternate report output filename.
    pub file: Option<String>,
    /// Per-field formatting options (field name → options).
    pub fields: HashMap<String, ReportFieldOption>,
}

impl Default for ReportOptions {
    fn default() -> Self {
        ReportOptions {
            page_size: 0,
            status: ReportStatus::No,
            summary: true,
            messages: true,
            energy: false,
            nodes: ReportSelection::None,
            links: ReportSelection::None,
            file: None,
            fields: HashMap::new(),
        }
    }
}

/// Global simulation parameters (§2.1). All fields are static after loading.
///
/// `SimulationOptions::default()` returns the canonical default values defined
/// in §2.1.  Callers typically construct this with `..SimulationOptions::default()`
/// and override only the fields that differ from the spec defaults.
#[derive(Debug, Clone)]
pub struct SimulationOptions {
    /// Total simulation duration (s); 0 = single steady-state step.
    pub duration: f64,
    /// Hydraulic time step Δt_h (s); default 3600.
    pub hyd_step: f64,
    /// Quality time step δt_q (s); must satisfy 0 < δt_q ≤ hyd_step.
    pub qual_step: f64,
    /// Interval at which results are written to the output file (s); default 3600.
    pub report_step: f64,
    /// Simulation time at which reporting begins (s); default 0.
    pub report_start: f64,
    /// Pattern time step Δt_p (s); default 3600.
    pub pattern_step: f64,
    /// Time offset for pattern evaluation (s from midnight); default 0.
    pub pattern_start: f64,
    /// Wall-clock time of simulation t=0 (s from midnight).
    pub start_clocktime: f64,
    /// Head-loss formula used for pipes.
    pub head_loss_formula: HeadLossFormula,
    /// Demand allocation model (DDA or PDA).
    pub demand_model: DemandModel,
    /// Flow unit system used for input/output (does not affect internal solver).
    pub flow_units: FlowUnits,
    /// Kinematic viscosity of water (m²/s); default ≈ 1.022×10⁻⁶.
    pub viscosity: f64,
    /// Molecular diffusivity of the tracer chemical (m²/s); default ≈ 1.208×10⁻⁹.
    pub diffusivity: f64,
    /// Specific gravity of the fluid relative to water; default 1.0.
    pub specific_gravity: f64,
    /// Global demand multiplier applied to all base demands; default 1.0.
    pub demand_multiplier: f64,
    /// Pattern ID applied to demand categories with no explicit pattern;
    /// `None` means a multiplier of 1.0 is used.
    pub default_pattern: Option<String>,
    /// PDA minimum pressure — below this, delivered demand is 0 (m).
    pub pda_min_pressure: f64,
    /// PDA required pressure — at or above this, full demand is delivered (m).
    pub pda_required_pressure: f64,
    /// PDA pressure exponent n in the Wagner equation; default 0.5.
    pub pda_pressure_exponent: f64,
    /// Whether emitters allow reverse flow (backflow into the network).
    pub emitter_backflow: bool,
    /// Water quality simulation mode.
    pub quality_mode: QualityMode,
    /// Node ID for source tracing; required when `quality_mode = Trace`.
    pub trace_node: Option<String>,
    /// Chemical species name (e.g. "Chlorine"); from `QUALITY Chemical <name>`.
    pub chem_name: String,
    /// Chemical concentration units (e.g. "mg/L"); from `QUALITY Chemical <name> <units>`.
    pub chem_units: String,
    /// Maximum Newton-Raphson iterations; default 200.
    pub max_iter: u32,
    /// Extra frozen-status iterations on non-convergence; −1 = halt; default −1.
    pub extra_iter: i32,
    /// Head tolerance εH for link status transitions (m); default 1.524×10⁻⁴.
    pub head_tol: f64,
    /// Absolute flow tolerance εQ for link status transition tests (m³/s); default 2.832×10⁻⁶.
    /// Distinct from `flow_tol`: `flow_tol` governs solver convergence (relative criterion);
    /// `flow_change_tol` governs link status transition conditions only.
    pub flow_change_tol: f64,
    /// Relative flow accuracy (Hacc) for solver convergence; default 0.001.
    pub flow_tol: f64,
    /// Absolute per-link head balance error limit (m); 0 = disabled; default 0.
    pub head_error_limit: f64,
    /// Absolute maximum flow change per iteration (m³/s); 0 = disabled; default 0.
    pub flow_change_limit: f64,
    /// Minimum gradient clamp for emitter/pump coefficient linearisation; default 1e-7.
    pub rq_tol: f64,
    /// Relative flow accuracy threshold below which damping activates; 0 = disabled.
    pub damp_limit: f64,
    /// Status check interval (iterations); default 2.
    pub check_freq: u32,
    /// Iteration count after which status checks stop; default 10.
    pub max_check: u32,
    /// Bulk reaction order for pipes; default 1.0.
    pub bulk_order: f64,
    /// Bulk reaction order for tanks; default 1.0.
    pub tank_order: f64,
    /// Wall reaction order (zero or first); default first.
    pub wall_order: WallOrder,
    /// Global bulk reaction rate coefficient (1/day for first-order).
    pub bulk_coeff: f64,
    /// Global wall reaction rate coefficient (m/day for first-order).
    pub wall_coeff: f64,
    /// Limiting concentration for reactions (mg/L); 0 = no limit.
    pub conc_limit: f64,
    /// Global unit energy cost ($/kWh).
    pub energy_price: f64,
    /// Pattern ID modulating the energy price over time.
    pub energy_price_pattern: Option<String>,
    /// Global default pump efficiency fraction.
    pub energy_efficiency: f64,
    /// Global demand charge (cost per peak kW); 0 = disabled.
    pub peak_demand_charge: f64,
    /// Roughness–reaction correlation factor Rf; 0 = disabled.
    pub roughness_reaction_factor: f64,
    /// Rule evaluation sub-step duration (s).
    pub rule_timestep: f64,
    /// Segment merge tolerance Ctol; default 0.01.
    pub quality_tolerance: f64,
    /// Report statistic aggregation type (from `TIMES` `STATISTIC`).
    pub statistic: StatisticType,
}

impl Default for SimulationOptions {
    /// Returns the canonical default values from §2.1.
    fn default() -> Self {
        // qual_step default = hyd_step / 10; we use 3600 s nominal hyd_step
        // so qual_step = 360 s, which satisfies the [1, hyd_step] constraint.
        // Callers that set a different hyd_step must also update qual_step.
        let hyd_step: f64 = 3600.0;
        let qual_step: f64 = (hyd_step / 10.0).clamp(1.0, hyd_step);
        let rule_timestep: f64 = (hyd_step / 10.0).clamp(f64::MIN_POSITIVE, hyd_step);
        SimulationOptions {
            duration: 0.0,
            hyd_step,
            qual_step,
            report_step: 3600.0,
            report_start: 0.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            start_clocktime: 0.0,
            head_loss_formula: HeadLossFormula::HazenWilliams,
            demand_model: DemandModel::DemandDriven,
            flow_units: FlowUnits::Gpm,
            viscosity: 1.022e-6,
            diffusivity: 1.208e-9,
            specific_gravity: 1.0,
            demand_multiplier: 1.0,
            default_pattern: None,
            pda_min_pressure: 0.0,
            pda_required_pressure: 0.0,
            pda_pressure_exponent: 0.5,
            emitter_backflow: true,
            quality_mode: QualityMode::None,
            trace_node: None,
            chem_name: String::new(),
            chem_units: String::new(),
            max_iter: 200,
            extra_iter: -1,
            head_tol: 1.524e-4,
            flow_change_tol: 2.832e-6,
            flow_tol: 0.001,
            head_error_limit: 0.0,
            flow_change_limit: 0.0,
            rq_tol: 1.0e-7,
            damp_limit: 0.0,
            check_freq: 2,
            max_check: 10,
            bulk_order: 1.0,
            tank_order: 1.0,
            wall_order: WallOrder::One,
            bulk_coeff: 0.0,
            wall_coeff: 0.0,
            conc_limit: 0.0,
            energy_price: 0.0,
            energy_price_pattern: None,
            energy_efficiency: 0.75,
            peak_demand_charge: 0.0,
            roughness_reaction_factor: 0.0,
            rule_timestep,
            quality_tolerance: 0.01,
            statistic: StatisticType::Series,
        }
    }
}

// ── §2.2 Patterns ─────────────────────────────────────────────────────────────

/// A repeating sequence of dimensionless multipliers (§2.2). Static.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// String identifier for this pattern.
    pub id: String,
    /// Multipliers [F₀, F₁, …, F_{L−1}]. Length ≥ 1.
    pub factors: Vec<f64>,
}

impl Pattern {
    /// Returns the multiplier active at simulation time `t` (seconds).
    ///
    /// Implements §2.2: $p = \lfloor (t + t_{\text{start}}) / \Delta t_p \rfloor$,
    /// active multiplier = $F[p \bmod L]$.
    ///
    /// `pattern_step` and `pattern_start` come from `SimulationOptions`.
    pub fn eval(&self, t: f64, pattern_step: f64, pattern_start: f64) -> f64 {
        let p = ((t + pattern_start) / pattern_step).floor() as i64;
        let l = self.factors.len() as i64;
        // Use rem_euclid so negative t values wrap correctly.
        let idx = p.rem_euclid(l) as usize;
        self.factors[idx]
    }
}

// ── §2.3 Curves ───────────────────────────────────────────────────────────────

/// Semantic kind of a piecewise-linear curve (§2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// A curve not yet assigned to a specific usage. Skips kind-specific
    /// validation (y-monotonicity, range checks) since its purpose is unknown.
    Generic,
    /// Pump head vs. flow curve.
    PumpHead,
    /// Pump efficiency vs. flow curve.
    PumpEfficiency,
    /// Constant-HP pump volume curve.
    PumpVolume,
    /// Tank volume vs. level curve.
    TankVolume,
    /// General Purpose Valve headloss vs. flow curve.
    GpvHeadloss,
    /// Positional Control Valve loss ratio vs. flow curve.
    PcvLossRatio,
}

/// A single (x, y) sample point on a curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvePoint {
    /// The independent-axis value (e.g. flow rate for a pump head curve).
    pub x: f64,
    /// The dependent-axis value (e.g. head gain for a pump head curve).
    pub y: f64,
}

/// A piecewise-linear mapping from x to y (§2.3). Static.
///
/// `points` has x strictly increasing and length ≥ 2.
#[derive(Debug, Clone)]
pub struct Curve {
    /// String identifier for this curve.
    pub id: String,
    /// Semantic kind, used to select validation rules.
    pub kind: CurveKind,
    /// Ordered sample points with strictly increasing x-values. Length ≥ 2.
    pub points: Vec<CurvePoint>,
}

impl Curve {
    /// Evaluates the curve at `x` using piecewise-linear interpolation.
    ///
    /// Implements §2.3: when `x` is inside the range the bracketing segment is
    /// used directly.  When `x` is outside the range the nearest endpoint
    /// segment is extended linearly (extrapolation).
    ///
    /// For single-point curves, returns the y-value of that point (constant).
    pub fn eval(&self, x: f64) -> f64 {
        let pts = &self.points;
        if pts.len() == 1 {
            return pts[0].y;
        }
        // Below the first point — extrapolate from the first segment.
        if x <= pts[0].x {
            let p0 = &pts[0];
            let p1 = &pts[1];
            let dx = p1.x - p0.x;
            debug_assert!(dx > 0.0, "curve points must have strictly increasing x");
            return p0.y + (p1.y - p0.y) * (x - p0.x) / dx;
        }
        // Above the last point — extrapolate from the last segment.
        let last = pts.len() - 1;
        if x >= pts[last].x {
            let p0 = &pts[last - 1];
            let p1 = &pts[last];
            let dx = p1.x - p0.x;
            debug_assert!(dx > 0.0, "curve points must have strictly increasing x");
            return p0.y + (p1.y - p0.y) * (x - p0.x) / dx;
        }
        // Interior — binary search for the bracketing segment [k-1, k].
        let k = pts.partition_point(|p| p.x <= x);
        // k is the first index where pts[k].x > x, so the bracket is [k-1, k].
        let p0 = &pts[k - 1];
        let p1 = &pts[k];
        p0.y + (p1.y - p0.y) * (x - p0.x) / (p1.x - p0.x)
    }
}

// ── §2.5 Demand categories ────────────────────────────────────────────────────

/// A single demand category attached to a junction (§2.5).
#[derive(Debug, Clone)]
pub struct DemandCategory {
    /// Base demand flow rate (internal m³/s); multiplied by pattern and global multiplier.
    pub base_demand: f64,
    /// Pattern ID; `None` falls back to the global default pattern (§2.1).
    pub pattern: Option<String>,
    /// Optional descriptive name from `[DEMANDS]`.
    pub name: Option<String>,
}

impl Junction {
    /// Total instantaneous demand at time `t` (§2.5).
    ///
    /// $D_i(t) = \sum_k \text{base}_k \times D_{\text{mult}} \times F_{\text{pattern}}(t)$
    ///
    /// The pattern lookup procedure:
    /// 1. Use the demand category's own pattern ID if set.
    /// 2. Otherwise fall back to `default_pattern` from options (if set).
    /// 3. Otherwise use multiplier 1.0.
    pub fn total_demand(
        &self,
        t: f64,
        opts: &SimulationOptions,
        patterns: &[Pattern],
        pattern_index: &HashMap<String, usize>,
    ) -> f64 {
        let lookup = |id: &str| pattern_index.get(id).map(|&i| &patterns[i]);
        self.demands
            .iter()
            .map(|d| {
                let multiplier = d
                    .pattern
                    .as_deref()
                    .or(opts.default_pattern.as_deref())
                    .and_then(lookup)
                    .map_or(1.0, |pat| {
                        pat.eval(t, opts.pattern_step, opts.pattern_start)
                    });
                d.base_demand * opts.demand_multiplier * multiplier
            })
            .sum()
    }
}

// ── §2.7 Quality sources ──────────────────────────────────────────────────────

/// Quality source injection type (§2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// Injects at a fixed concentration (mg/L).
    Concentration,
    /// Injects at a fixed mass flow rate (mg/min).
    Mass,
    /// Overrides node concentration with a fixed setpoint (mg/L).
    Setpoint,
    /// Scales concentration proportional to outflow from node (mg/L applied to outflow).
    FlowPaced,
}

/// A quality source attached to a node (§2.7).
#[derive(Debug, Clone)]
pub struct QualitySource {
    /// 1-based node index.
    pub node: usize,
    /// Injection type (concentration, mass, setpoint, or flow-paced).
    pub kind: SourceType,
    /// Base injection value before pattern scaling.
    pub base_value: f64,
    /// Optional pattern ID modulating injection over time.
    pub pattern: Option<String>,
}

impl QualitySource {
    /// Effective injection value at time `t` (§2.7).
    ///
    /// = `base_value` × $F_{\text{pattern}}(t)$, or `base_value` if no pattern.
    pub fn effective_value(
        &self,
        t: f64,
        opts: &SimulationOptions,
        patterns: &[Pattern],
        pattern_index: &HashMap<String, usize>,
    ) -> f64 {
        let multiplier = self
            .pattern
            .as_deref()
            .and_then(|id| pattern_index.get(id).map(|&i| &patterns[i]))
            .map_or(1.0, |pat| {
                pat.eval(t, opts.pattern_step, opts.pattern_start)
            });
        self.base_value * multiplier
    }
}

// ── §2.4 Nodes ────────────────────────────────────────────────────────────────

/// Properties common to all node types (§2.4.1). Static.
#[derive(Debug, Clone)]
pub struct NodeBase {
    /// String identifier for this node.
    pub id: String,
    /// 1-based index assigned at load time.
    pub index: usize,
    /// Node elevation (internal length units, m).
    pub elevation: f64,
    /// Initial water quality concentration (mg/L, h, or % depending on mode).
    pub initial_quality: f64,
}

/// An ordinary demand node whose head is solved at each hydraulic step (§2.4.2).
#[derive(Debug, Clone)]
pub struct Junction {
    /// Demand categories attached to this junction (§2.5).
    pub demands: Vec<DemandCategory>,
    /// Emitter discharge coefficient Ke (m³/s per m^ne); 0 = no emitter.
    pub emitter_coeff: f64,
    /// Emitter pressure exponent ne; default 0.5.
    pub emitter_exp: f64,
}

/// A fixed-grade node whose head is known at all times (§2.4.3).
#[derive(Debug, Clone)]
pub struct Reservoir {
    /// Optional pattern ID modulating head over time.
    pub head_pattern: Option<String>,
}

impl Reservoir {
    /// Hydraulic head at time `t` (§2.4.3).
    ///
    /// If a `head_pattern` is set, $H = \text{elevation} \times F_{\text{pattern}}(t)$;
    /// otherwise $H = \text{elevation}$.
    pub fn head(
        &self,
        elevation: f64,
        t: f64,
        opts: &SimulationOptions,
        patterns: &[Pattern],
        pattern_index: &HashMap<String, usize>,
    ) -> f64 {
        let multiplier = self
            .head_pattern
            .as_deref()
            .and_then(|id| pattern_index.get(id).map(|&i| &patterns[i]))
            .map_or(1.0, |pat| {
                pat.eval(t, opts.pattern_step, opts.pattern_start)
            });
        elevation * multiplier
    }
}

/// Tank mixing model (§2.4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixModel {
    /// Completely mixed (single compartment).
    Cstr,
    /// Two-compartment mixing.
    TwoCompartment,
    /// First-in first-out plug flow.
    Fifo,
    /// Last-in first-out.
    Lifo,
}

/// A storage node whose head evolves over time (§2.4.4).
#[derive(Debug, Clone)]
pub struct Tank {
    /// Minimum operating level above bottom elevation (m).
    pub min_level: f64,
    /// Maximum operating level above bottom elevation (m).
    pub max_level: f64,
    /// Initial water level above bottom elevation (m).
    pub initial_level: f64,
    /// Diameter of cylindrical tank (m); used when `vol_curve` is `None`.
    pub diameter: f64,
    /// Explicit minimum volume (m³); if > 0, overrides the value computed
    /// from `diameter` and `min_level`.
    pub min_volume: f64,
    /// Optional volume curve ID (kind = `TankVolume`).
    pub volume_curve: Option<String>,
    /// Water mixing model used inside this tank.
    pub mix_model: MixModel,
    /// Inlet-zone volume fraction; meaningful only for `TwoCompartment`.
    pub mix_fraction: f64,
    /// Bulk reaction rate coefficient (overrides global).
    pub bulk_coeff: f64,
    /// Whether tank can overflow (spill above `max_level`).
    pub overflow: bool,
    /// Optional pattern ID modulating head over time for fixed-head operation.
    pub head_pattern: Option<String>,
}

impl Tank {
    /// Bottom elevation = node elevation − `min_level` (§2.4.4).
    pub fn bottom_elevation(&self, node_elevation: f64) -> f64 {
        node_elevation - self.min_level
    }

    /// Hydraulic head from the current level (§2.4.4).
    ///
    /// $H = \text{bottom\_elevation} + \text{level}$
    pub fn head_from_level(&self, node_elevation: f64, level: f64) -> f64 {
        self.bottom_elevation(node_elevation) + level
    }

    /// Cross-section area $A$ at the given level (§2.4.4).
    ///
    /// - Cylindrical tank: $A = \pi d^2 / 4$ (constant).
    /// - Volume-curve tank: $A(h) = dV/dh$ approximated at `level` via the
    ///   finite-difference slope of the two bracketing curve points.
    ///
    /// `curves` is the full network curve table; `vol_curve` is resolved by ID.
    pub fn area(&self, level: f64, curves: &[Curve]) -> f64 {
        if let Some(ref curve_id) = self.volume_curve {
            if let Some(curve) = curves.iter().find(|c| c.id == *curve_id) {
                return Self::area_from_volume_curve(curve, level);
            }
        }
        // Cylindrical fallback.
        std::f64::consts::PI * self.diameter * self.diameter / 4.0
    }

    /// Computes $A(h) = dV/dh$ from a `TankVolume` curve.
    ///
    /// Uses the slope of the bracketing segment (same interpolation as §2.3),
    /// clamped to the slope of the nearest endpoint segment when outside range.
    fn area_from_volume_curve(curve: &Curve, level: f64) -> f64 {
        let pts = &curve.points;
        // Below the first point — use slope of the first segment.
        if level <= pts[0].x {
            let dx = pts[1].x - pts[0].x;
            return (pts[1].y - pts[0].y) / dx;
        }
        let last = pts.len() - 1;
        // Above the last point — use slope of the last segment.
        if level >= pts[last].x {
            let dx = pts[last].x - pts[last - 1].x;
            return (pts[last].y - pts[last - 1].y) / dx;
        }
        // Interior — binary search for the bracketing segment [k-1, k].
        let k = pts.partition_point(|p| p.x <= level);
        let dx = pts[k].x - pts[k - 1].x;
        (pts[k].y - pts[k - 1].y) / dx
    }

    /// Volume corresponding to `level` using the volume curve (§2.4.4),
    /// or the cylindrical approximation when no curve is present.
    pub fn volume_from_level(&self, level: f64, curves: &[Curve]) -> f64 {
        if let Some(ref curve_id) = self.volume_curve {
            if let Some(curve) = curves.iter().find(|c| c.id == *curve_id) {
                return curve.eval(level);
            }
        }
        // Cylindrical.
        let a = std::f64::consts::PI * self.diameter * self.diameter / 4.0;
        a * level
    }

    /// Level corresponding to `volume` (inverse of `volume_from_level`).
    ///
    /// For cylindrical tanks: $h = V / A$.
    /// For volume-curve tanks: binary-search the curve for the bracketing
    /// segment and invert the linear segment.
    pub fn level_from_volume(&self, volume: f64, curves: &[Curve]) -> f64 {
        if let Some(ref curve_id) = self.volume_curve {
            if let Some(curve) = curves.iter().find(|c| c.id == *curve_id) {
                return Self::invert_volume_curve(curve, volume);
            }
        }
        // Cylindrical.
        let a = std::f64::consts::PI * self.diameter * self.diameter / 4.0;
        volume / a
    }

    /// Inverts a `TankVolume` curve: given a volume, returns the level.
    fn invert_volume_curve(curve: &Curve, volume: f64) -> f64 {
        let pts = &curve.points;
        // Below minimium volume — extrapolate from first segment.
        if volume <= pts[0].y {
            let dv = pts[1].y - pts[0].y;
            if dv == 0.0 {
                return pts[0].x;
            }
            return pts[0].x + (pts[1].x - pts[0].x) * (volume - pts[0].y) / dv;
        }
        let last = pts.len() - 1;
        // Above maximum volume — extrapolate from last segment.
        if volume >= pts[last].y {
            let dv = pts[last].y - pts[last - 1].y;
            if dv == 0.0 {
                return pts[last].x;
            }
            return pts[last - 1].x
                + (pts[last].x - pts[last - 1].x) * (volume - pts[last - 1].y) / dv;
        }
        // Interior — find bracketing segment by y (volume), then invert.
        let k = pts.partition_point(|p| p.y <= volume);
        let dv = pts[k].y - pts[k - 1].y;
        if dv == 0.0 {
            return pts[k - 1].x;
        }
        pts[k - 1].x + (pts[k].x - pts[k - 1].x) * (volume - pts[k - 1].y) / dv
    }
}

/// Type-specific data for a node.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// Demand node.
    Junction(Junction),
    /// Fixed-grade boundary node.
    Reservoir(Reservoir),
    /// Variable-level storage node.
    Tank(Tank),
}

/// A node in the network graph (§2.4).
#[derive(Debug, Clone)]
pub struct Node {
    /// Static properties common to all node types.
    pub base: NodeBase,
    /// Type-specific static data.
    pub kind: NodeKind,
    /// At most one quality source per node (§2.7).
    pub source: Option<QualitySource>,
}

// ── §2.6 Links ────────────────────────────────────────────────────────────────

/// Operational status of a link (§2.6.1 and §3.9).
///
/// `XPressure` and `XFcv` are computed states (per-step only);
/// they are not valid as `init_status` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// Fully open.
    Open,
    /// Fully closed.
    Closed,
    /// Actively controlled (valves).
    Active,
    /// PRV/PSV: reverse pressure gradient present (computed state only).
    XPressure,
    /// FCV: flow setpoint cannot be enforced (computed state only).
    XFcv,
    /// Pump: head gain required exceeds shutoff head (computed state only; §3.9).
    XHead,
    /// Pump: constant-HP pump with Q ≤ 0 (computed state only; §3.9).
    TempClosed,
}

/// Properties common to all link types (§2.6.1). Static.
#[derive(Debug, Clone)]
pub struct LinkBase {
    /// String identifier for this link.
    pub id: String,
    /// 1-based index assigned at load time.
    pub index: usize,
    /// 1-based index of the start node (positive flow direction: from → to).
    pub from_node: usize,
    /// 1-based index of the end node.
    pub to_node: usize,
    /// Initial operational status at simulation start.
    pub initial_status: LinkStatus,
    /// Initial relative speed ω for pumps; initial setpoint for valves; unused for pipes.
    /// `None` means MISSING — the valve is "fixed" (§2.6.4) and its
    /// status will not be changed by automatic status logic.
    pub initial_setting: Option<f64>,
}

impl LinkBase {
    /// 0-based index of the start node (positive flow direction: from → to).
    ///
    /// All internal solvers and writers use 0-based indexing; this accessor
    /// centralises the single `from_node - 1` conversion in one place.
    #[inline]
    pub fn from_idx(&self) -> usize {
        self.from_node - 1
    }

    /// 0-based index of the end node.
    #[inline]
    pub fn to_idx(&self) -> usize {
        self.to_node - 1
    }
}

/// A pipe in the network (§2.6.2).
#[derive(Debug, Clone)]
pub struct Pipe {
    /// Pipe length (internal length units, m).
    pub length: f64,
    /// Internal diameter (internal length units, m).
    pub diameter: f64,
    /// Roughness coefficient (Hazen-Williams C or Darcy-Weisbach ε).
    pub roughness: f64,
    /// Minor loss coefficient (dimensionless); 0 = no minor loss.
    pub minor_loss: f64,
    /// Whether a check valve is installed (prevents reverse flow).
    pub check_valve: bool,
    /// `None` → use global `bulk_coeff`.
    pub bulk_coeff: Option<f64>,
    /// `None` → use global `wall_coeff`.
    pub wall_coeff: Option<f64>,
    /// FAVAD fixed-area leakage coefficient (m³/s per m^0.5) per end node.
    pub leak_coeff_1: f64,
    /// FAVAD variable-area leakage coefficient (m³/s per m^1.5) per end node.
    pub leak_coeff_2: f64,
}

/// Pump head-curve type (§2.6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpCurveType {
    /// Three-point power-function curve.
    PowerFunction,
    /// Constant horsepower.
    ConstHp,
    /// User-supplied head-flow curve.
    Custom,
}

/// A pump link (§2.6.3).
#[derive(Debug, Clone)]
pub struct Pump {
    /// The form of the head-curve relationship for this pump.
    pub curve_type: PumpCurveType,
    /// Curve ID for head vs. flow (`PumpHead` kind); `None` for `ConstHp`.
    pub head_curve: Option<String>,
    /// Rated power (W); only used for `ConstHp`.
    pub power: Option<f64>,
    /// Optional efficiency curve ID (`PumpEfficiency` kind).
    pub efficiency_curve: Option<String>,
    /// Fallback efficiency fraction when no curve is available.
    pub default_efficiency: f64,
    /// Pattern ID modulating pump speed over time.
    pub speed_pattern: Option<String>,
    /// Per-pump unit energy price ($/kWh); `None` → use global.
    pub energy_price: Option<f64>,
    /// Pattern ID modulating energy price over time.
    pub price_pattern: Option<String>,
}

/// Valve type (§2.6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValveType {
    /// Pressure Reducing Valve.
    Prv,
    /// Pressure Sustaining Valve.
    Psv,
    /// Flow Control Valve.
    Fcv,
    /// Throttle Control Valve.
    Tcv,
    /// General Purpose Valve.
    Gpv,
    /// Positional Control Valve.
    Pcv,
    /// Pressure Breaker Valve.
    Pbv,
}

/// A valve link (§2.6.4).
///
/// For GPV, `curve` holds the head-loss curve ID (kind = `GpvHeadloss`).
/// For PCV, `curve` holds the loss-ratio curve ID (kind = `PcvLossRatio`).
/// For all other valve types it is `None` and the setpoint is encoded in
/// `LinkBase::init_setting`.
#[derive(Debug, Clone)]
pub struct Valve {
    /// The type of this valve.
    pub valve_type: ValveType,
    /// Nominal diameter (internal length units, m).
    pub diameter: f64,
    /// Minor loss coefficient (dimensionless); 0 = no minor loss.
    pub minor_loss: f64,
    /// Curve ID; required for `Gpv` (kind = `GpvHeadloss`) and `Pcv`
    /// (kind = `PcvLossRatio`); `None` for all other valve types.
    pub curve: Option<String>,
}

/// Type-specific data for a link.
#[derive(Debug, Clone)]
pub enum LinkKind {
    /// Pipe link.
    Pipe(Pipe),
    /// Pump link.
    Pump(Pump),
    /// Valve link.
    Valve(Valve),
}

/// A link in the network graph (§2.6).
#[derive(Debug, Clone)]
pub struct Link {
    /// Static properties common to all link types.
    pub base: LinkBase,
    /// Type-specific static data.
    pub kind: LinkKind,
}

// ── §2.8 Controls ─────────────────────────────────────────────────────────────

/// Trigger kind for a simple control (§2.8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerType {
    /// Fires after elapsed simulation time.
    Timer,
    /// Fires at a specific time of day.
    TimeOfDay,
    /// Fires when a node grade rises above a threshold.
    HiLevel,
    /// Fires when a node grade falls below a threshold.
    LowLevel,
}

/// A simple control that fires at most once per hydraulic time step (§2.8.1).
#[derive(Debug, Clone)]
pub struct SimpleControl {
    /// 1-based link index.
    pub link: usize,
    /// What kind of event triggers this control.
    pub trigger_type: TriggerType,
    /// Absolute simulation time (s) for `Timer`; seconds from midnight for `TimeOfDay`.
    pub trigger_time: Option<f64>,
    /// 1-based node index; used for `HiLevel`/`LowLevel`.
    pub trigger_node: Option<usize>,
    /// Hydraulic grade threshold (m); used for `HiLevel`/`LowLevel`.
    pub trigger_grade: Option<f64>,
    /// Target status; `None` if only a setting change is intended.
    pub action_status: Option<LinkStatus>,
    /// Target setting value; `None` if only a status change is intended.
    pub action_setting: Option<f64>,
    /// Whether this control is active; disabled controls are never evaluated.
    pub enabled: bool,
}

/// The object a rule premise refers to (§2.8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PremiseObject {
    /// Network node (1-based index).
    Node(usize),
    /// Network link (1-based index).
    Link(usize),
    /// Simulation clock.
    Clock,
}

/// Attribute tested by a rule premise (§2.8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PremiseAttribute {
    /// Hydraulic head (m).
    Head,
    /// Pressure (m).
    Pressure,
    /// Demand (flow units).
    Demand,
    /// Tank water level (m).
    Level,
    /// Link flow rate (flow units).
    Flow,
    /// Link status (open/closed).
    Status,
    /// Link setting value.
    Setting,
    /// Pump power (kW).
    Power,
    /// Time to fill a tank (hours).
    FillTime,
    /// Time to drain a tank (hours).
    DrainTime,
    /// Time of day (hours from midnight).
    ClockTime,
    /// Elapsed simulation time (hours).
    Time,
}

/// Relational operator in a rule premise (§2.8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PremiseOperator {
    /// Equal.
    Eq,
    /// Not equal.
    Neq,
    /// Less than.
    Lt,
    /// Greater than.
    Gt,
    /// Less than or equal.
    Le,
    /// Greater than or equal.
    Ge,
}

/// Logical connective joining consecutive premises (§2.8.2).
///
/// `And` binds more tightly than `Or`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOp {
    /// Logical conjunction (binds tighter than `Or`).
    And,
    /// Logical disjunction.
    Or,
}

/// A single predicate clause in a rule (§2.8.2).
#[derive(Debug, Clone)]
pub struct Premise {
    /// The network object being tested.
    pub object: PremiseObject,
    /// The attribute of that object to compare.
    pub attribute: PremiseAttribute,
    /// The relational comparison operator.
    pub operator: PremiseOperator,
    /// The right-hand-side threshold value.
    pub value: f64,
    /// Connective joining this premise to the next; `None` for the last premise.
    pub connective: Option<LogicOp>,
}

/// Value applied by a rule action (§2.8.2).
#[derive(Debug, Clone)]
pub enum ActionValue {
    /// Set the link's operational status.
    Status(LinkStatus),
    /// Set the link's numeric setting.
    Setting(f64),
}

/// A single action applied by a rule (§2.8.2).
#[derive(Debug, Clone)]
pub struct RuleAction {
    /// 1-based link index.
    pub link: usize,
    /// The new status or setting value to apply to the link.
    pub value: ActionValue,
}

/// A rule-based control (§2.8.2).
#[derive(Debug, Clone)]
pub struct Rule {
    /// Numeric priority; lower value wins when rules conflict.
    pub priority: f64,
    /// Ordered list of predicate clauses forming the rule condition.
    pub premises: Vec<Premise>,
    /// Actions applied when the rule condition evaluates to true.
    pub then_actions: Vec<RuleAction>,
    /// Actions applied when the rule condition evaluates to false.
    pub else_actions: Vec<RuleAction>,
}

// ── Top-level network ─────────────────────────────────────────────────────────

/// The complete network data model (§2). Populated once at load time.
///
/// Nodes and links are stored in `Vec`s; their `base.index` fields are
/// 1-based, so `vec[i]` has `base.index == i + 1`.
#[derive(Debug, Clone)]
pub struct Network {
    /// Up to 3 title lines from the `[TITLE]` section (for binary output prolog).
    pub title: Vec<String>,
    /// Simulation parameters from `[OPTIONS]` and `[TIMES]` sections.
    pub options: SimulationOptions,
    /// All patterns from the `[PATTERNS]` section.
    pub patterns: Vec<Pattern>,
    /// All curves from the `[CURVES]` section.
    pub curves: Vec<Curve>,
    /// All nodes (junctions, reservoirs, tanks).
    pub nodes: Vec<Node>,
    /// All links (pipes, pumps, valves).
    pub links: Vec<Link>,
    /// Simple controls from the `[CONTROLS]` section.
    pub controls: Vec<SimpleControl>,
    /// Rule-based controls from the `[RULES]` section.
    pub rules: Vec<Rule>,
    /// Index mapping pattern ID → position in `patterns`. Built once at load
    /// time via [`Network::build_pattern_index`] so hot-path lookups are O(1).
    pub pattern_index: HashMap<String, usize>,
    /// Report formatting options from the `[REPORT]` INP section.
    pub report: ReportOptions,
    /// Node coordinates from the `[COORDINATES]` INP section: node ID → (x, y).
    pub coordinates: HashMap<String, (f64, f64)>,
    /// Link vertex points from the `[VERTICES]` INP section: link ID → [(x, y), …].
    pub vertices: HashMap<String, Vec<(f64, f64)>>,
    /// Node tags from the `[TAGS]` INP section: node ID → tag string.
    pub node_tags: HashMap<String, String>,
    /// Link tags from the `[TAGS]` INP section: link ID → tag string.
    pub link_tags: HashMap<String, String>,
}

impl Network {
    /// Populates `pattern_index` from the current `patterns` vec. Must be
    /// called once after construction (before any simulation work).
    pub fn build_pattern_index(&mut self) {
        self.pattern_index = self
            .patterns
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
    }

    /// O(1) pattern lookup by ID. Returns `None` if the ID does not exist.
    pub fn pattern_by_id(&self, id: &str) -> Option<&Pattern> {
        self.pattern_index.get(id).map(|&i| &self.patterns[i])
    }
}

// ── §2.10 FAVAD load-time aggregation ─────────────────────────────────────────

/// Per-junction FAVAD resistance coefficients computed at load time (§2.10).
///
/// Indexed 0-based (junction `i` → entry `i`). Only junctions carry FAVAD
/// coefficients; reservoirs and tanks are excluded.
///
/// These values live outside the `Network` struct because the spec explicitly
/// states they are not stored in the data model proper (§2.10). Compute once
/// via [`Network::compute_favad`] before the first hydraulic solve.
#[derive(Debug, Clone)]
pub struct FavadCoeffs {
    /// $c_{\text{fa},i}$ indexed by 0-based junction position in `network.nodes`.
    pub c_fa: Vec<f64>,
    /// $c_{\text{va},i}$ indexed by 0-based junction position in `network.nodes`.
    pub c_va: Vec<f64>,
}

impl Network {
    /// Computes per-junction FAVAD resistance coefficients (§2.10).
    ///
    /// For each pipe, the $K_1$ and $K_2$ FAVAD coefficients are split between
    /// its two end nodes according to whether the opposite end is a junction or a
    /// fixed-grade node (reservoir or tank).  Only junctions accumulate FAVAD
    /// coefficients; reservoirs and tanks are skipped.
    ///
    /// Returns a [`FavadCoeffs`] whose `c_fa` and `c_va` vectors are indexed by
    /// the 0-based position of each node in `self.nodes`; non-junction entries
    /// are always 0.
    pub fn compute_favad(&self) -> FavadCoeffs {
        let n = self.nodes.len();
        let mut k_fa = vec![0.0_f64; n];
        let mut k_va = vec![0.0_f64; n];

        let is_junction = |idx_1based: usize| -> bool {
            if idx_1based < 1 || idx_1based > n {
                return false;
            }
            matches!(self.nodes[idx_1based - 1].kind, NodeKind::Junction(_))
        };

        for link in &self.links {
            let pipe = match &link.kind {
                LinkKind::Pipe(p) => p,
                _ => continue,
            };
            if pipe.leak_coeff_1 == 0.0 && pipe.leak_coeff_2 == 0.0 {
                continue;
            }

            let f = link.base.from_node;
            let t = link.base.to_node;
            let f_is_junc = is_junction(f);
            let t_is_junc = is_junction(t);

            // Splitting rule (§2.10): if both ends are junctions each gets ½;
            // if only one end is a junction it gets the full coefficient.
            let both_junctions = f_is_junc && t_is_junc;
            let factor = if both_junctions { 0.5 } else { 1.0 };

            if f_is_junc {
                k_fa[f - 1] += factor * pipe.leak_coeff_1;
                k_va[f - 1] += factor * pipe.leak_coeff_2;
            }
            if t_is_junc {
                k_fa[t - 1] += factor * pipe.leak_coeff_1;
                k_va[t - 1] += factor * pipe.leak_coeff_2;
            }
        }

        // Invert to resistance coefficients (§2.10).
        let c_fa: Vec<f64> = k_fa
            .iter()
            .map(|&k| if k > 0.0 { 1.0 / (k * k) } else { 0.0 })
            .collect();
        let c_va: Vec<f64> = k_va
            .iter()
            .map(|&k| {
                if k > 0.0 {
                    1.0 / k.powf(2.0 / 3.0)
                } else {
                    0.0
                }
            })
            .collect();

        FavadCoeffs { c_fa, c_va }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pattern::eval ─────────────────────────────────────────────────────────

    #[test]
    fn pattern_eval_selects_first_factor_at_t_zero() {
        let p = Pattern {
            id: "P1".into(),
            factors: vec![0.5, 1.0, 1.5],
        };
        // t=0, start=0, step=3600 → p= floor(0/3600)=0 → idx 0.
        assert_eq!(p.eval(0.0, 3600.0, 0.0), 0.5);
    }

    #[test]
    fn pattern_eval_wraps_beyond_length() {
        let p = Pattern {
            id: "P1".into(),
            factors: vec![0.5, 1.0, 1.5],
        };
        // t=3*3600=10800 → p=3 → idx = 3 % 3 = 0 → 0.5.
        assert_eq!(p.eval(3.0 * 3600.0, 3600.0, 0.0), 0.5);
        // t=4*3600=14400 → p=4 → idx = 4 % 3 = 1 → 1.0.
        assert_eq!(p.eval(4.0 * 3600.0, 3600.0, 0.0), 1.0);
    }

    #[test]
    fn pattern_eval_pattern_start_shifts_index() {
        let p = Pattern {
            id: "P1".into(),
            factors: vec![10.0, 20.0, 30.0],
        };
        // t=0, start=3600, step=3600 → p = floor(3600/3600) = 1 → 20.0.
        assert_eq!(p.eval(0.0, 3600.0, 3600.0), 20.0);
    }

    // ── Curve::eval ──────────────────────────────────────────────────────────

    fn two_point_curve() -> Curve {
        Curve {
            id: "C".into(),
            kind: CurveKind::Generic,
            points: vec![
                CurvePoint { x: 0.0, y: 0.0 },
                CurvePoint { x: 10.0, y: 20.0 },
            ],
        }
    }

    #[test]
    fn curve_eval_single_point_returns_constant() {
        let c = Curve {
            id: "C".into(),
            kind: CurveKind::Generic,
            points: vec![CurvePoint { x: 5.0, y: 42.0 }],
        };
        assert_eq!(c.eval(0.0), 42.0);
        assert_eq!(c.eval(100.0), 42.0);
    }

    #[test]
    fn curve_eval_interior_interpolation() {
        let c = two_point_curve();
        // At x=5 (midpoint) → y = 10.0.
        assert!((c.eval(5.0) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn curve_eval_below_range_extrapolates() {
        let c = two_point_curve();
        // Below x=0 — extend first segment: slope = 20/10 = 2.
        // eval(-5) = 0 + 2 * (-5 - 0) = -10.
        assert!((c.eval(-5.0) - (-10.0)).abs() < 1e-12);
    }

    #[test]
    fn curve_eval_above_range_extrapolates() {
        let c = two_point_curve();
        // Above x=10 — extend last segment: slope = 2.
        // eval(15) = 0 + 2 * (15 - 0) = 30.
        assert!((c.eval(15.0) - 30.0).abs() < 1e-12);
    }

    // ── Tank::head_from_level ─────────────────────────────────────────────────

    #[test]
    fn tank_head_from_level() {
        let t = Tank {
            min_level: 2.0,
            max_level: 10.0,
            initial_level: 5.0,
            diameter: 4.0,
            min_volume: 0.0,
            volume_curve: None,
            mix_model: MixModel::Cstr,
            mix_fraction: 1.0,
            bulk_coeff: 0.0,
            overflow: false,
            head_pattern: None,
        };
        // elevation=50, min_level=2 → bottom=48; head = 48 + level=5 = 53.
        assert!((t.head_from_level(50.0, 5.0) - 53.0).abs() < 1e-12);
    }

    // ── Tank::volume_from_level ───────────────────────────────────────────────

    #[test]
    fn tank_volume_from_level_cylindrical() {
        let t = Tank {
            min_level: 0.0,
            max_level: 10.0,
            initial_level: 5.0,
            diameter: 4.0, // radius = 2
            min_volume: 0.0,
            volume_curve: None,
            mix_model: MixModel::Cstr,
            mix_fraction: 1.0,
            bulk_coeff: 0.0,
            overflow: false,
            head_pattern: None,
        };
        // V = π * (4/2)² * level = π * 4 * 3 = 12π.
        let expected = std::f64::consts::PI * 4.0 * 3.0;
        assert!((t.volume_from_level(3.0, &[]) - expected).abs() < 1e-10);
    }

    // ── Junction::total_demand ────────────────────────────────────────────────

    #[test]
    fn junction_total_demand_no_pattern_uses_base_times_multiplier() {
        let j = Junction {
            demands: vec![DemandCategory {
                base_demand: 0.01,
                pattern: None,
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        let opts = SimulationOptions {
            demand_multiplier: 2.0,
            ..Default::default()
        };
        let total = j.total_demand(0.0, &opts, &[], &HashMap::new());
        // base_demand=0.01, multiplier=2.0, pattern factor=1.0 → 0.02.
        assert!((total - 0.02).abs() < 1e-12);
    }

    #[test]
    fn junction_total_demand_with_pattern_factor() {
        let pat = Pattern {
            id: "P1".into(),
            factors: vec![0.5, 2.0],
        };
        let mut pattern_index = HashMap::new();
        pattern_index.insert("P1".to_string(), 0usize);
        let j = Junction {
            demands: vec![DemandCategory {
                base_demand: 0.1,
                pattern: Some("P1".into()),
                name: None,
            }],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        let opts = SimulationOptions::default();
        // t=3600, step=3600, start=0 → p=1 → factor=2.0.
        let total = j.total_demand(3600.0, &opts, &[pat], &pattern_index);
        // 0.1 * 1.0 * 2.0 = 0.2.
        assert!((total - 0.2).abs() < 1e-12);
    }

    #[test]
    fn junction_total_demand_sums_multiple_categories() {
        let j = Junction {
            demands: vec![
                DemandCategory {
                    base_demand: 0.01,
                    pattern: None,
                    name: None,
                },
                DemandCategory {
                    base_demand: 0.02,
                    pattern: None,
                    name: None,
                },
            ],
            emitter_coeff: 0.0,
            emitter_exp: 0.5,
        };
        let opts = SimulationOptions::default();
        let total = j.total_demand(0.0, &opts, &[], &HashMap::new());
        assert!((total - 0.03).abs() < 1e-12);
    }
}
