//! The results the report blocks read, as data (format-blind
//! extraction, phase 3).
//!
//! Post-simulation analytics fold over reporting periods. The engine
//! defines what a period and its metadata are; where they come from —
//! the dialect's results file, a test's vector — is the caller's
//! business, supplied through [`PeriodSource`].

/// What the blocks need to know about a results set before reading any
/// period. Neutral: identifiers, counts and clocks, never file layout.
#[derive(Debug, Clone)]
pub struct ResultsMeta {
    pub subcatchment_ids: Vec<String>,
    pub node_ids: Vec<String>,
    pub link_ids: Vec<String>,
    pub pollutant_ids: Vec<String>,
    /// Values per element in each period record.
    pub n_subcatch_vars: usize,
    pub n_node_vars: usize,
    pub n_link_vars: usize,
    /// Recorded reporting periods.
    pub n_periods: usize,
    /// The reporting step (s).
    pub report_step_s: i32,
    /// The §2.9 flow-unit selection the values are recorded in.
    pub flow_units: crate::model::options::FlowUnits,
}

/// One reporting period's values, element-major per kind.
#[derive(Debug, Clone)]
pub struct PeriodValues {
    /// The record's own timestamp (Unix seconds).
    pub epoch_s: f64,
    /// `n_subcatchments × n_subcatch_vars`.
    pub subcatchments: Vec<f32>,
    /// `n_nodes × n_node_vars`.
    pub nodes: Vec<f32>,
    /// `n_links × n_link_vars`.
    pub links: Vec<f32>,
    /// The fifteen system series.
    pub system: [f32; 15],
}

/// A results set the blocks can fold over: metadata once, then one
/// sequential pass per computation.
pub trait PeriodSource {
    fn meta(&self) -> &ResultsMeta;
    /// Fold every period through `f`, in period order.
    fn scan(&self, f: &mut dyn FnMut(usize, &PeriodValues)) -> Result<(), String>;
}
