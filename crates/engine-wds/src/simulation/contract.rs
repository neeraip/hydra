//! The engine's session vocabulary: warnings, reported quantities,
//! balances, snapshots, and the [`WritableSimulation`] contract every
//! results writer reads through (format-blind extraction, phase 4).

use crate::{LinkStatus, Network};

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
    /// A hydraulic step at the 1 s rejection floor still exceeded
    /// `level_err_tol`; the step was accepted with degraded tank-level
    /// accuracy (§5.3).
    TankLevelAccuracy {
        /// Zero-based index of the worst-error tank in `Network::nodes`.
        node_index: usize,
    },
    /// A pump declares both an initial speed ≠ 1 and a speed pattern; the
    /// pattern's multipliers are the speed schedule and supersede the initial
    /// speed from the first step (simulation spec §5.4). Reported once at
    /// load so the dead field is visible rather than silently ignored.
    PumpSpeedPatternSupersedesSetting {
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
    ///
    /// Panics when [`WritableSimulation::has_network`] is false. Every writer
    /// in this module checks that first and returns an error instead, so the
    /// panic is reachable only by calling this directly on a session that has
    /// not loaded a model.
    fn net(&self) -> &Network;
    /// Whether a network has been loaded and [`WritableSimulation::net`] can
    /// answer.
    ///
    /// A session created but not yet loaded has no network, and the writers
    /// used to reach straight past that into a panic across the published API.
    /// Defaulted to `true`: an implementor holding a network outright need not
    /// think about it.
    fn has_network(&self) -> bool {
        true
    }
    /// All hydraulic snapshots stored during the simulation.
    fn snapshots(&self) -> &[HydSnapshot];
    /// Simulation time (s) through which recorded snapshots are **final** —
    /// no longer subject to change as the session advances (simulation spec
    /// §8.3, streaming serialization). With quality enabled, snapshots carry
    /// provisional quality values until the quality phase writes its results
    /// back through their time; a streaming writer must not emit a snapshot
    /// whose time lies beyond this frontier. The default suits completed
    /// simulations, where every snapshot is final.
    fn finalized_through(&self) -> f64 {
        f64::INFINITY
    }
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

/// Map Hydra `LinkStatus` to EPANET `StatusType` enum value (0–10).
///
/// The result catalog (§6) declares one item per code this produces, and a
/// test pins the two together — an undeclared code renders as no value at
/// all, so a link in a failure state would silently vanish from the view.
pub fn status_out_code(status: LinkStatus) -> f32 {
    match status {
        LinkStatus::XHead => 0.0,
        LinkStatus::TempClosed => 1.0,
        LinkStatus::Closed => 2.0,
        LinkStatus::Open => 3.0,
        LinkStatus::Active => 4.0,
        LinkStatus::XFcv => 6.0,
        LinkStatus::XPressure => 7.0,
    }
}
