// io — I/O layer for hydra-engine: format parsing and output writing.
//
// This module owns all format-specific reading and writing. Writers are
// generic over `WritableSimulation` so the trait object can be provided by
// the simulation module without creating a circular module dependency.

/// Analysis artifact I/O — the persisted `analysis.json` schema.
pub mod analysis_io;
/// INP (EPANET input file) reader — public entry point is [`parse`].
pub mod inp_reader;
/// INP (EPANET input file) writer — public entry point is [`write_inp`].
pub mod inp_writer;
/// Binary `.out` result file reader.
pub mod out_reader;
/// Binary `.out` result file writer (used during simulation).
pub mod out_writer;
/// `.rpt` plain-text report writer.
pub mod rpt_writer;
/// EPANET unit conversion factors.
pub mod units;

pub use inp_writer::write_inp;

use std::fmt;

use crate::{Network, ValidationError};

// ── Parse entry point (§4 of crates/engine-wds/src/model/spec.md) ───────────

/// Error returned by [`parse`] when a model file cannot be processed.
#[derive(Debug)]
pub enum ParseError {
    /// The file format was not recognised (not an INP file).
    UnrecognisedFormat,
    /// The file parsed successfully but failed one or more §2.9 validation checks.
    ValidationFailed(Vec<ValidationError>),
    /// A specific field value was syntactically valid but semantically out of range.
    InvalidField {
        /// The name of the offending INP field.
        field: String,
        /// Human-readable explanation of why the value is invalid.
        reason: String,
    },
    /// A node or link ID was defined more than once (EPANET error 215).
    DuplicateId {
        /// Object class: `"node"` or `"link"`.
        object: &'static str,
        /// The duplicated ID.
        id: String,
    },
    /// A parse error annotated with the INP section and 1-based source line
    /// number where it occurred.
    AtLine {
        /// INP section name (upper-case, without brackets).
        section: String,
        /// 1-based line number in the input file.
        line: usize,
        /// The underlying parse error.
        source: Box<ParseError>,
    },
}

impl ParseError {
    /// Attach section and line context to an error that does not already have it.
    pub(crate) fn at_line(self, section: &str, line: usize) -> ParseError {
        match self {
            Self::AtLine { .. } | Self::ValidationFailed(_) => self,
            other => Self::AtLine {
                section: section.to_string(),
                line,
                source: Box::new(other),
            },
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnrecognisedFormat => write!(f, "unrecognised model file format"),
            Self::ValidationFailed(errs) => write!(f, "validation failed: {} error(s)", errs.len()),
            Self::InvalidField { field, reason } => {
                write!(f, "invalid field '{field}': {reason}")
            }
            Self::DuplicateId { object, id } => {
                write!(f, "duplicate {object} ID '{id}'")
            }
            Self::AtLine {
                section,
                line,
                source,
            } => {
                write!(f, "[{section}] line {line}: {source}")
            }
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AtLine { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Parse a model file from raw bytes, returning a fully validated `Network`.
///
/// Format detection is by content: if the first non-whitespace byte is `[` or
/// `;` the input is treated as an EPANET 2.3 INP file. Anything else is an
/// error.
pub fn parse(bytes: &[u8]) -> Result<Network, ParseError> {
    let first = bytes
        .iter()
        .find(|&&b| !b.is_ascii_whitespace())
        .copied()
        .unwrap_or(0);

    match first {
        b'[' | b';' => inp_reader::parse_inp(bytes),
        _ => Err(ParseError::UnrecognisedFormat),
    }
}

// ── Result types (moved from hydra-simulation) ────────────────────────────────

/// Non-fatal diagnostic condition attached to a simulation time step (§8.4).
#[derive(Debug, Clone)]
pub struct SimWarning {
    /// Simulation time (s) at which the condition was observed.
    pub t: f64,
    /// The category and details of the non-fatal condition.
    pub kind: WarningKind,
}

/// Category of non-fatal diagnostic (§8.4).
#[derive(Debug, Clone)]
pub enum WarningKind {
    /// Hydraulic solver exceeded `max_iter`; continued with `extra_iter` frozen-status loop.
    UnbalancedHydraulics,
    /// Negative pressure at a junction in DDA mode.
    NegativePressure {
        /// Zero-based index of the junction in `Network::nodes`.
        node_index: usize,
    },
    /// Pump operation in reverse-flow (XHEAD) condition.
    PumpXHead {
        /// Zero-based index of the pump in `Network::links`.
        link_index: usize,
    },
}

/// Node result quantities available via `get_node_result` (§8.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeQuantity {
    /// Hydraulic head (internal length unit).
    Head,
    /// Gauge pressure = head − elevation (internal length unit).
    GaugePressure,
    /// Demand delivered (internal volume/time unit).
    Demand,
    /// Water quality (units depend on `quality_mode`).
    Quality,
}

/// Link result quantities available via `get_link_result` (§8.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkQuantity {
    /// Flow rate (internal volume/time unit; positive = from→to).
    Flow,
    /// Mean velocity = flow / (π(D/2)²) (internal length/time unit; pipes only; else 0).
    MeanVelocity,
    /// Unit head loss = |Δh| / length (pipes only; else 0).
    UnitHeadLoss,
    /// Darcy–Weisbach friction factor (DW formula only; pipes only; else 0).
    FrictionFactor,
    /// Water quality (units depend on `quality_mode`).
    Quality,
    /// Link status as a float: 0 = Closed, 1 = Open, 2 = Active.
    Status,
    /// Link setting (pump speed fraction or valve pressure setting).
    Setting,
}

/// Accumulated energy statistics for a single pump (§7.1).
///
/// Indexed parallel to `network.links`; entries for non-pump links are
/// uninitialised and should not be read.
#[derive(Debug, Clone, Default)]
pub struct PumpEnergy {
    /// Accumulated electrical energy (kWh).
    pub kwh: f64,
    /// Accumulated time-weighted energy intensity (kWh / (flow unit)).
    pub kwh_per_flow: f64,
    /// Total time (s) the pump carried positive flow.
    pub time_online: f64,
    /// Peak electrical power observed (kW).
    pub max_kw: f64,
    /// Accumulated energy cost (currency, matching `energy_price` units).
    pub total_cost: f64,
    /// Accumulated `η * Δt` while pump was running, used to derive `avg_efficiency`.
    pub efficiency_sum: f64,
}

impl PumpEnergy {
    /// Time-weighted average efficiency fraction while pump was running (§7.1).
    pub fn avg_efficiency(&self) -> f64 {
        if self.time_online > 0.0 {
            self.efficiency_sum / self.time_online
        } else {
            0.0
        }
    }
}

/// Volumetric flow balance accumulated over the full simulation (§7.2).
#[derive(Debug, Clone)]
pub struct FlowBalance {
    /// Integrated supply into the network (m³).
    pub total_inflow: f64,
    /// Integrated withdrawal from the network (m³).
    pub total_outflow: f64,
    /// Integrated unmet demand in PDA mode (m³); not in the ratio.
    pub demand_deficit: f64,
    /// Total tank volume at simulation start (m³).
    pub initial_tank_volume: f64,
}

impl FlowBalance {
    /// Volume balance ratio ρ_v (§7.2).
    ///
    /// `current_tank_volume` is the current total volume across all tanks.
    pub fn balance_ratio(&self, current_tank_volume: f64) -> f64 {
        let delta_v = current_tank_volume - self.initial_tank_volume;
        let numerator = self.total_outflow + delta_v.max(0.0);
        let denominator = self.total_inflow + (-delta_v).max(0.0);
        if denominator == 0.0 {
            1.0
        } else {
            numerator / denominator
        }
    }

    /// Compute the complete flow balance summary given the final tank volume.
    pub fn summarize(&self, final_tank_volume: f64) -> FlowBalanceSummary {
        let tank_change = final_tank_volume - self.initial_tank_volume;
        let unaccounted = self.total_inflow - self.total_outflow - tank_change;
        let ratio = self.balance_ratio(final_tank_volume);
        FlowBalanceSummary {
            total_inflow: self.total_inflow,
            total_outflow: self.total_outflow,
            tank_change,
            unaccounted,
            ratio,
        }
    }
}

/// Derived flow balance results ready for display or serialisation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlowBalanceSummary {
    /// Total volume supplied into the network (m³).
    pub total_inflow: f64,
    /// Total volume consumed / withdrawn (m³).
    pub total_outflow: f64,
    /// Change in total tank storage (current − initial), positive = net fill.
    pub tank_change: f64,
    /// Unaccounted volume: inflow − outflow − tank_change.
    pub unaccounted: f64,
    /// Volume balance ratio (≈ 1.0 when balanced).
    pub ratio: f64,
}

/// Running constituent mass balance (§6.9).
#[derive(Debug, Clone, Default)]
pub struct MassBalance {
    /// Mass present in the network at simulation start (mg).
    pub init: f64,
    /// Total mass injected by sources over the simulation (mg).
    pub added: f64,
    /// Total mass removed by demand withdrawals (mg).
    pub demand: f64,
    /// Net mass consumed by reactions (positive = removed from water = decay).
    pub reacted: f64,
    /// Mass present in the network at simulation end (mg).
    pub final_mass: f64,
    /// Mass consumed by bulk pipe reactions (mg).
    pub reacted_bulk: f64,
    /// Mass consumed by pipe wall reactions (mg).
    pub reacted_wall: f64,
    /// Mass consumed by tank reactions (mg).
    pub reacted_tank: f64,
    /// Alias for `added`; retained for EPANET compatibility.
    pub source: f64,
}

impl MassBalance {
    /// Balance ratio ρ_m (§6.9). A value ≈ 1 confirms conservation.
    pub fn ratio(&self) -> f64 {
        let input = self.init + self.added + (-self.reacted).max(0.0);
        let output = self.demand + self.reacted.max(0.0) + self.final_mass;
        if input <= 0.0 {
            return 1.0;
        }
        output / input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_rejects_unrecognised_format() {
        let bytes = b"{\"not\":\"inp\"}";
        let err = parse(bytes).expect_err("should reject non-INP content");
        assert!(matches!(err, ParseError::UnrecognisedFormat));
    }

    #[test]
    fn parse_accepts_whitespace_then_inp_section() {
        let inp_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/single_pipe_hw.inp");
        let bytes = std::fs::read(inp_path).expect("read fixture inp");
        let mut with_prefix = b"\n\t  ".to_vec();
        with_prefix.extend_from_slice(&bytes);

        let network = parse(&with_prefix).expect("parse fixture as INP");
        assert!(!network.nodes.is_empty());
        assert!(!network.links.is_empty());
    }

    #[test]
    fn pump_energy_avg_efficiency_zero_when_offline() {
        let pe = PumpEnergy::default();
        assert_eq!(pe.avg_efficiency(), 0.0);
    }

    #[test]
    fn pump_energy_avg_efficiency_time_weighted() {
        let pe = PumpEnergy {
            efficiency_sum: 1800.0,
            time_online: 3600.0,
            ..PumpEnergy::default()
        };
        assert!((pe.avg_efficiency() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn flow_balance_ratio_accounts_for_storage_change_direction() {
        let fb = FlowBalance {
            total_inflow: 100.0,
            total_outflow: 90.0,
            demand_deficit: 0.0,
            initial_tank_volume: 50.0,
        };
        // Tank fills by 10: numerator adds +10.
        assert!((fb.balance_ratio(60.0) - 1.0).abs() < 1e-12);
        // Tank drains by 10: denominator adds +10.
        assert!((fb.balance_ratio(40.0) - (90.0 / 110.0)).abs() < 1e-12);
    }

    #[test]
    fn mass_balance_ratio_defaults_to_one_when_no_input_mass() {
        let mb = MassBalance::default();
        assert_eq!(mb.ratio(), 1.0);
    }
}

/// Hydraulic state snapshot at a single simulation time (§8.2).
#[derive(Debug, Clone)]
pub struct HydSnapshot {
    /// Simulation time (s).
    pub t: f64,
    /// Per-node hydraulic and quality state at time `t`.
    pub node_states: Vec<crate::NodeState>,
    /// Per-link hydraulic and quality state at time `t`.
    pub link_states: Vec<crate::LinkState>,
}

// ── WritableSimulation trait ──────────────────────────────────────────────────

/// Read-only view of a completed (or in-progress) simulation that the writers
/// need. Implemented by `crate::simulation::Simulation`.
///
/// The trait is intentionally narrow — it exposes only what the writers
/// actually access, avoiding leaking internal solver state into the public API.
pub trait WritableSimulation {
    /// The `Network` data model for this simulation.
    fn net(&self) -> &crate::Network;
    /// All hydraulic snapshots stored during the simulation.
    fn snapshots(&self) -> &[HydSnapshot];
    /// Pump energy record at `link_index`, or `None` if no accounting state is
    /// available (e.g. hydraulics not yet run).
    fn pump_energy_at(&self, link_index: usize) -> Option<&PumpEnergy>;
    /// Peak simultaneous electrical demand across all pumps (kW).
    fn peak_demand_kw(&self) -> f64;
    /// Mass balance from the quality engine. `None` if quality not yet run.
    fn mass_balance(&self) -> Option<&MassBalance>;
    /// Non-fatal diagnostics emitted during the simulation.
    fn warnings(&self) -> &[SimWarning];
    /// Look up a pump's energy record by its string ID. Returns `None` if the
    /// ID is unknown or the link is not a pump.
    fn pump_energy_by_id(&self, pump_id: &str) -> Option<&PumpEnergy>;
    /// The hydraulic and quality analysis start and finish wall-clock times.
    fn analysis_times(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>);
    /// Flow balance from accounting. `None` if hydraulics not yet run.
    fn flow_balance(&self) -> Option<&FlowBalance>;
    /// Derived flow balance summary. `None` if hydraulics not yet run or
    /// if the simulation lacks the data needed to compute final tank volume.
    fn flow_balance_summary(&self) -> Option<FlowBalanceSummary>;
}
