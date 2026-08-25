//! The analysis options (§2.9, §14.4): the option vocabulary as data.
//!
//! These are model types — what a run means — not parsing. The §14.4
//! grammar that reads the predecessor's `[OPTIONS]` lines into them
//! lives with the dialect tooling (format-blind extraction, phase 2).

/// Flow-unit selection; per §14.4 it selects the entire unit system of the
/// file's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowUnits {
    /// ft³/s (US customary system).
    #[default]
    Cfs,
    /// gal/min (US).
    Gpm,
    /// 10⁶ gal/day (US).
    Mgd,
    /// m³/s (SI).
    Cms,
    /// L/s (SI).
    Lps,
    /// 10⁶ L/day (SI).
    Mld,
}

impl FlowUnits {
    /// Whether the file's values are in US customary units.
    pub fn is_us(self) -> bool {
        matches!(self, FlowUnits::Cfs | FlowUnits::Gpm | FlowUnits::Mgd)
    }

    /// m³/s per one declared flow unit — the factor the binary results
    /// writer converts with, shared so the report blocks recover SI
    /// display values by exactly the mapping the file was written under.
    pub fn m3s_per_unit(self) -> f64 {
        match self {
            FlowUnits::Cfs => 0.028_316_846_592,
            FlowUnits::Gpm => 6.309_019_64e-5,
            FlowUnits::Mgd => 0.043_812_636_4,
            FlowUnits::Cms => 1.0,
            FlowUnits::Lps => 1.0e-3,
            FlowUnits::Mld => 1.0 / 86.4,
        }
    }

    /// Metres per one declared length unit — exactly 0.3048 for a
    /// US-unit file, 1 for an SI one.
    ///
    /// Public for the same reason `m3s_per_unit` is: values a file
    /// carries in its own units — a cross-section's geometry among them
    /// (§5) — are only recoverable through the mapping the file was read
    /// under, and a caller that writes such a value has to apply that
    /// same mapping in reverse. Duplicating the constant outside the
    /// engine is how the two come to disagree.
    pub fn m_per_length_unit(self) -> f64 {
        if self.is_us() {
            0.3048
        } else {
            1.0
        }
    }

    /// The variant for a binary-results unit code (the discriminant the
    /// writer stores, §14.9), or `None` for an unknown code.
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => FlowUnits::Cfs,
            1 => FlowUnits::Gpm,
            2 => FlowUnits::Mgd,
            3 => FlowUnits::Cms,
            4 => FlowUnits::Lps,
            5 => FlowUnits::Mld,
            _ => return None,
        })
    }
}

/// The infiltration relation selection (§3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfiltrationModel {
    /// Horton's exponential-decay capacity.
    #[default]
    Horton,
    /// Akan's cumulative-excess reformulation.
    ModifiedHorton,
    /// Mein–Larson two-stage Green–Ampt.
    GreenAmpt,
    /// Green–Ampt without the low-intensity event reset.
    ModifiedGreenAmpt,
    /// The SCS curve-number relation.
    CurveNumber,
}

/// The routing form the file requested. This engine has one solver (§6.1);
/// a reduced form is recorded and substituted with a notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingRequest {
    /// Steady-flow routing (reduced form; substituted).
    Steady,
    /// Kinematic-wave routing (reduced form; substituted). The predecessor's
    /// `XKINWAVE` reassigns here, as it does there.
    KinematicWave,
    /// The full dynamic treatment — this engine's solver.
    #[default]
    DynamicWave,
}

/// Link invert offsets expressed as heights above the vertex invert, or as
/// absolute elevations (§14.7 converts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkOffsets {
    /// Offsets are depths above the vertex invert.
    #[default]
    Depth,
    /// Offsets are absolute elevations.
    Elevation,
}

/// Pressurised force-main friction relation (§7.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForceMainEquation {
    /// Hazen–Williams.
    #[default]
    HazenWilliams,
    /// Darcy–Weisbach.
    DarcyWeisbach,
}

/// Normal-flow limit criteria (§6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NormalFlowCriteria {
    /// Water-surface slope test only.
    Slope,
    /// Upstream Froude test only.
    Froude,
    /// Either test limits (the default).
    #[default]
    Both,
    /// The limit is disabled.
    None,
}

/// A calendar date, as the file carries it. Ordering is calendar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    /// Four-digit year.
    pub year: i32,
    /// 1–12.
    pub month: u32,
    /// 1–31.
    pub day: u32,
}

/// The analysis options: everything `[OPTIONS]` configures, in SI, with the
/// predecessor's defaults resolved (a zero sentinel resolving to the stated
/// engine default once parsing completes).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisOptions {
    /// The file's unit system selection.
    pub flow_units: FlowUnits,
    /// Infiltration relation.
    pub infiltration: InfiltrationModel,
    /// The routing form the file requested (§6.1: one solver; the request is
    /// recorded, and reduced forms carry a substitution notice).
    pub routing_request: RoutingRequest,
    /// Ponding enabled globally.
    pub allow_ponding: bool,
    /// Link offset convention.
    pub link_offsets: LinkOffsets,
    /// Force-main friction relation.
    pub force_main: ForceMainEquation,
    /// Normal-flow limit criteria.
    pub normal_flow: NormalFlowCriteria,
    /// Process switches (§14.4): an object-free subsystem is also ignored
    /// automatically at validation.
    pub ignore_rainfall: bool,
    /// Ignore snowmelt.
    pub ignore_snowmelt: bool,
    /// Ignore groundwater.
    pub ignore_groundwater: bool,
    /// Ignore sewer inflow.
    pub ignore_rdii: bool,
    /// Ignore flow routing (also set by `FLOW_ROUTING NONE`).
    pub ignore_routing: bool,
    /// Keep the overland mesh parsed and preserved but run the model
    /// one-dimensional (§14.15 `IGNORE_2D`).
    pub ignore_overland: bool,
    /// Ignore constituent transport.
    pub ignore_quality: bool,
    /// Wet hydrology step (s); default 300.
    pub wet_step: f64,
    /// Dry hydrology step (s); default 3600, floored at the wet step.
    pub dry_step: f64,
    /// User routing step cap (s); default 20.
    pub routing_step: f64,
    /// Rule-evaluation clock (s); 0 = every routing step.
    pub rule_step: f64,
    /// Reporting step (s); default 900.
    pub report_step: f64,
    /// Routing step floor (s); default 0.5.
    pub min_routing_step: f64,
    /// Courant factor (`VARIABLE_STEP`); 0 = constraint-seeded stepping
    /// without the Courant term; default 0.75.
    pub courant_factor: f64,
    /// Iteration budget per trial (§6.4); default 8.
    pub max_trials: u32,
    /// Head convergence tolerance (m); default 1.524 mm.
    pub head_tol: f64,
    /// Minimum assembled vertex surface area (m²); default 1.167.
    pub min_surface_area: f64,
    /// User minimum conduit slope, as a fraction; 0 = none.
    pub min_slope: f64,
    /// Worker threads requested.
    pub threads: u32,
    /// Simulation start.
    pub start_date: Date,
    /// Start time of day (s).
    pub start_time: f64,
    /// Simulation end.
    pub end_date: Date,
    /// End time of day (s).
    pub end_time: f64,
    /// Reporting starts here rather than at the simulation start.
    pub report_start: Option<(Date, f64)>,
    /// Street-sweeping season, day-of-year bounds; defaults 1–365.
    pub sweep_start: u32,
    /// Sweeping season end.
    pub sweep_end: u32,
    /// Antecedent dry days seeding initial buildup.
    pub dry_days: f64,
    /// A scratch directory for applications; the engine performs no
    /// filesystem I/O and never reads it.
    pub temp_dir: Option<String>,
    /// Pressurisation celerity (m/s), the §6.2 slot's stated wave speed —
    /// a session option, not a file keyword; default 50, minimum 5.
    pub pressure_celerity: f64,
    /// Relative network continuity tolerance per trial (§6.4); a session
    /// option; default 1e-3.
    pub continuity_tol: f64,
    /// Per-step local head-error tolerance (m) for the §6.5 error test;
    /// 0 disables the test; a session option; default 1e-3.
    pub routing_err_tol: f64,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        AnalysisOptions {
            flow_units: FlowUnits::Cfs,
            infiltration: InfiltrationModel::Horton,
            routing_request: RoutingRequest::DynamicWave,
            allow_ponding: false,
            link_offsets: LinkOffsets::Depth,
            force_main: ForceMainEquation::HazenWilliams,
            normal_flow: NormalFlowCriteria::Both,
            ignore_rainfall: false,
            ignore_snowmelt: false,
            ignore_groundwater: false,
            ignore_rdii: false,
            ignore_routing: false,
            ignore_overland: false,
            ignore_quality: false,
            wet_step: 300.0,
            dry_step: 3600.0,
            routing_step: 20.0,
            rule_step: 0.0,
            report_step: 900.0,
            min_routing_step: 0.5,
            courant_factor: 0.75,
            max_trials: 8,
            head_tol: 1.524e-3,
            min_surface_area: 1.167,
            min_slope: 0.0,
            threads: 1,
            start_date: Date {
                year: 2004,
                month: 1,
                day: 1,
            },
            start_time: 0.0,
            end_date: Date {
                year: 2004,
                month: 1,
                day: 1,
            },
            end_time: 0.0,
            report_start: None,
            sweep_start: 1,
            sweep_end: 365,
            dry_days: 0.0,
            temp_dir: None,
            pressure_celerity: 50.0,
            continuity_tol: 1.0e-3,
            routing_err_tol: 1.0e-3,
        }
    }
}
