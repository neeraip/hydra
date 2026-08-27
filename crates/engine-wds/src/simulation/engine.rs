// simulation — §8 of crates/engine-wds/src/simulation/spec.md
//
// The public-facing API of hydra. Exposes the full simulation lifecycle:
// create → load → run/step hydraulics → run/step quality → get results →
// destroy. No I/O is performed here; all I/O is the responsibility of adapters.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::{f64, time::SystemTime};

use super::accounting::{self, AccountingState};
use super::controls;
use super::timestep;
use crate::hydraulics::{self as hydraulics, SolveResult, SolverContext};
use crate::quality::{self as quality, QualityState};
use crate::simulation::contract::HydSnapshot;
use crate::{
    FavadCoeffs, FlowUnits, LinkKind, LinkState, LinkStatus, Network, NodeKind, NodeState,
    QualityMode,
};

#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "mutation.rs"]
mod mutation;
#[path = "results.rs"]
mod results;
pub use results::{LinkResult, NodeResult};
#[path = "types.rs"]
mod types;
#[path = "writable.rs"]
mod writable;

use types::Phase;
pub use types::{
    LinkProperty, LinkQuantity, NodeProperty, NodeQuantity, SessionError, SimWarning, WarningKind,
};

// ── Session ───────────────────────────────────────────────────────────────────

/// A simulation session: owns network, solver context, results, and accounting.
///
/// Sessions are not thread-safe with respect to themselves (§8.3 invariants).
/// Multiple independent sessions may coexist in the same process.
pub struct Simulation {
    phase: Phase,

    // Loaded network + derived context.
    network: Option<Network>,
    favad: Option<FavadCoeffs>,
    solver_ctx: Option<SolverContext>,

    // Lazily-built id → index lookup maps for the loaded network.
    // Reset on every `load()`; safe to cache between loads because object IDs
    // and topology never change after load (mutations only set property values).
    id_index: OnceLock<IdIndex>,

    // Live simulation state.
    node_states: Vec<NodeState>,
    link_states: Vec<LinkState>,
    current_t: f64,
    next_report_t: f64,  // next report time boundary
    report_count: usize, // number of report boundaries passed

    // Whether a hydraulic step has been taken for the currently loaded
    // network. Distinct from "a snapshot exists": with `report_start` beyond
    // zero and quality disabled, steps are taken before any instant is
    // recorded, so the history cannot answer this question.
    has_stepped: bool,

    // The solved state at t, before the tank advance mutates it. Quality
    // advances from this over the step's interval, which is the same field
    // the replaced second pass read out of the history. One reused buffer,
    // not one per instant. `dt` is not final until the tank advance
    // finishes (it shortens for tank events), so the states have to be kept
    // rather than the advance brought forward.
    states_at_t: (Vec<NodeState>, Vec<LinkState>),

    // The instant most recently recorded. A session holds one, never a
    // history (§8.2 Retention): a caller wanting the series attaches a
    // result stream and reads it back from the serialized results.
    current_instant: Option<HydSnapshot>,

    // How many instants have been recorded. A streaming writer compares this
    // against its own count so falling behind is an error it can report,
    // rather than instants going missing without anyone noticing.
    instants_recorded: u64,

    // Quality.
    quality_state: Option<QualityState>,
    quality_t: f64,

    // Accounting.
    accounting: Option<AccountingState>,

    // Warnings.
    warnings: Vec<SimWarning>,
    /// Tracks which nodes have already emitted a NegativePressure warning
    /// to avoid O(N×T) accumulation — only the first occurrence is stored.
    neg_pressure_seen: Vec<bool>,

    // Wall-clock timestamps for the report.
    analysis_begun: Option<SystemTime>,
    analysis_ended: Option<SystemTime>,

    /// §5.2: the accepted interval of the previous period, kept only when
    /// that period was accepted after one or more error rejections (§5.3).
    /// The next period's Δt is capped at twice this value; `None` as soon
    /// as a period is accepted without rejection.
    pub(super) post_rejection_dt: Option<f64>,
}

/// Lazily-built lookup maps from object ID to 0-based index.
#[derive(Default)]
struct IdIndex {
    nodes: HashMap<String, usize>,
    links: HashMap<String, usize>,
}

// ── Session internal helpers ──────────────────────────────────────────────────

impl Simulation {
    /// Return the id → index maps, building them on first use (O(N + L) once
    /// per loaded network; subsequent lookups are O(1)).
    fn id_index(&self) -> &IdIndex {
        self.id_index.get_or_init(|| {
            let mut idx = IdIndex::default();
            if let Some(network) = &self.network {
                idx.nodes = network
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| (n.base.id.clone(), i))
                    .collect();
                idx.links = network
                    .links
                    .iter()
                    .enumerate()
                    .map(|(i, l)| (l.base.id.clone(), i))
                    .collect();
            }
            idx
        })
    }

    /// Find a node's 0-based index by string ID — O(1) via the lazily-built map.
    fn node_index_by_id(&self, id: &str) -> Option<usize> {
        self.id_index().nodes.get(id).copied()
    }

    /// Find a link's 0-based index by string ID — O(1) via the lazily-built map.
    fn link_index_by_id(&self, id: &str) -> Option<usize> {
        self.id_index().links.get(id).copied()
    }
    fn require_phase(&self, expected: Phase) -> Result<(), SessionError> {
        if self.phase != expected {
            Err(SessionError::InvalidPhase {
                expected: expected.name().to_string(),
                actual: self.phase.name().to_string(),
            })
        } else {
            Ok(())
        }
    }

    fn require_loaded_network(&self) -> Result<&Network, SessionError> {
        self.network
            .as_ref()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: Phase::Created.name().to_string(),
            })
    }

    // ── Snapshot helpers ──────────────────────────────────────────────────────

    /// Record a snapshot at `new_t`.
    ///
    /// With quality enabled, snapshots are recorded at every hydraulic step so
    /// the quality engine can observe intermediate flow-field changes.
    /// With quality disabled, snapshots are recorded only at report boundaries
    /// to avoid retaining O(steps) cloned state that is never consumed.
    fn maybe_record_snapshot(&mut self, new_t: f64) {
        let network = match &self.network {
            Some(n) => n,
            None => return,
        };
        let duration = network.options.duration;
        if new_t > duration + 1e-6 {
            return;
        }

        // Only reporting instants are recorded. Every hydraulic step used to
        // be kept when quality was enabled, so the second pass could replay
        // the flow field; quality now advances within the step and the extra
        // instants have no reader. The writer already emitted only reporting
        // instants, so the results file never contained them.
        let at_or_past_report = new_t >= self.next_report_t - 1e-6;
        if at_or_past_report {
            match self.current_instant.as_mut() {
                // Reuse the buffers rather than allocating an instant per
                // reporting time; the previous one has been streamed out.
                Some(inst) => {
                    inst.t = new_t;
                    inst.node_states.clone_from(&self.node_states);
                    inst.link_states.clone_from(&self.link_states);
                }
                None => {
                    self.current_instant = Some(HydSnapshot {
                        t: new_t,
                        node_states: self.node_states.clone(),
                        link_states: self.link_states.clone(),
                    })
                }
            }
            self.instants_recorded += 1;
        }

        // Advance the report-time marker independently of snapshot count.
        let report_step = network.options.report_step;
        let report_start = network.options.report_start;
        while new_t >= self.next_report_t - 1e-6 && self.next_report_t <= duration + 1e-6 {
            self.report_count += 1;
            self.next_report_t = report_start + report_step * (self.report_count as f64);
        }
    }

    /// Write the quality engine's current concentrations onto the live states,
    /// so the instant recorded at this time carries them.
    ///
    /// Reaction rate is stamped only once quality has advanced at least once.
    /// The replaced second pass wrote reaction rate on the write-back after an
    /// advance but not during initialisation, so the instant at t=0 carried a
    /// zero rate; reproducing that is what keeps the first period identical.
    fn stamp_quality_onto_live_states(&mut self) {
        let Some(qs) = self.quality_state.as_ref() else {
            return;
        };
        let Some(network) = self.network.as_ref() else {
            return;
        };
        let advanced = self.quality_t > 0.0;
        for (i, ns) in self.node_states.iter_mut().enumerate() {
            ns.quality = qs.node_conc[i];
        }
        for (k, ls) in self.link_states.iter_mut().enumerate() {
            ls.quality = quality::avg_link_quality(
                qs,
                k,
                network.links[k].base.from_idx(),
                network.links[k].base.to_idx(),
            );
            if advanced {
                ls.reaction_rate = qs.pipe_rate_coeff[k];
            }
        }
    }
}

// ── Free-standing helpers ─────────────────────────────────────────────────────

/// Initialise node states from the static network (§2.4).
fn init_node_states(network: &Network) -> Vec<NodeState> {
    (0..network.nodes.len())
        .map(|i| init_node_state(network, i))
        .collect()
}

/// Derive the initial state of node `i` from static network data (§2.4).
///
/// Used both at load and by `set_node_property` when an initial-state-affecting
/// property (e.g. elevation) is mutated before the first hydraulic step
/// (spec §8.3 mutation semantics).
fn init_node_state(network: &Network, i: usize) -> NodeState {
    let n = &network.nodes[i];
    let mut ns = NodeState::default();
    // Initial head: 0.0 for junctions (matching EPANET's calloc-zeroed NodeHead),
    // elevation for reservoirs, or head_from_level for tanks.
    ns.head = match &n.kind {
        NodeKind::Junction(_) => 0.0,
        NodeKind::Reservoir(_) => n.base.elevation,
        NodeKind::Tank(t) => t.head_from_level(n.base.elevation, t.initial_level),
    };
    ns.level = match &n.kind {
        NodeKind::Tank(t) => t.initial_level,
        _ => 0.0,
    };
    ns.volume = match &n.kind {
        NodeKind::Tank(t) => {
            // Use volume curve if present, otherwise π r² h.
            if let Some(ref cv_id) = t.volume_curve {
                if let Some(curve) = network.curves.iter().find(|c| c.id == *cv_id) {
                    return NodeState {
                        head: ns.head,
                        level: t.initial_level,
                        volume: curve.eval(t.initial_level),
                        quality: n.base.initial_quality,
                        ..NodeState::default()
                    };
                }
            }
            std::f64::consts::PI * (t.diameter / 2.0).powi(2) * t.initial_level
        }
        _ => 0.0,
    };
    ns.quality = n.base.initial_quality;
    ns
}

/// Initialise link states from static network data (§2.6).
fn init_link_states(network: &Network) -> Vec<LinkState> {
    (0..network.links.len())
        .map(|k| init_link_state(network, k))
        .collect()
}

/// Derive the initial state of link `k` from static network data (§2.6).
///
/// Used both at load and by `set_link_property` when the initial status or
/// setting is mutated before the first hydraulic step (spec §8.3 mutation
/// semantics), so a pre-run mutation behaves exactly like loading a network
/// that had the mutated value from the start.
fn init_link_state(network: &Network, k: usize) -> LinkState {
    let l = &network.links[k];
    let flow = if l.base.initial_status == LinkStatus::Closed {
        1.0e-6 // QZERO
    } else {
        match &l.kind {
            LinkKind::Pipe(pipe) => {
                // Flow at 1 fps velocity = cross-section area (§3.1).
                std::f64::consts::PI * pipe.diameter * pipe.diameter / 4.0
            }
            LinkKind::Pump(pump) => {
                let speed = l.base.initial_setting.unwrap_or(1.0);
                let q0 = pump_design_flow(pump, &network.curves);
                speed * q0
            }
            LinkKind::Valve(v) => {
                // Same as pipe: area at 1 fps.
                std::f64::consts::PI * v.diameter * v.diameter / 4.0
            }
        }
    };
    // EPANET inithyd: for non-GPV valves with InitStatus != Active,
    // setting is cleared to MISSING (NaN), preventing automatic status
    // transitions. Then, PRV/PSV/FCV with a surviving (non-None)
    // setting are forced Active. GPV always starts Open.
    let mut status = l.base.initial_status;
    let mut setting = l.base.initial_setting;
    if let LinkKind::Valve(v) = &l.kind {
        if v.valve_type == crate::ValveType::Gpv {
            // GPV: always Open (EPANET never sets GPV to Active).
            status = LinkStatus::Open;
        } else {
            if status != LinkStatus::Active {
                setting = None;
            }
            if matches!(
                v.valve_type,
                crate::ValveType::Prv | crate::ValveType::Psv | crate::ValveType::Fcv
            ) && setting.is_some()
            {
                status = LinkStatus::Active;
            }
        }
    }
    LinkState {
        flow,
        status,
        setting: setting.unwrap_or(f64::NAN),
        quality: 0.0,
        reaction_rate: 0.0,
    }
}

/// Compute the design flow Q0 for a pump (spec §3.10).
///
/// - PowerFunction: Q0 = middle curve point flow (q1).
/// - Custom: Q0 = midpoint of curve flow range.
/// - ConstHp: Q0 = 0.028317 m³/s (= 1 ft³/s fixed initial guess, spec §3.10).
fn pump_design_flow(pump: &crate::Pump, curves: &[crate::Curve]) -> f64 {
    match pump.curve_type {
        crate::PumpCurveType::ConstHp => 0.028317,
        crate::PumpCurveType::PowerFunction => {
            if let Some(ref cid) = pump.head_curve {
                if let Some(curve) = curves.iter().find(|c| &c.id == cid) {
                    if curve.points.len() >= 3 {
                        return curve.points[1].x; // q1 design point
                    } else if !curve.points.is_empty() {
                        return curve.points[0].x;
                    }
                }
            }
            0.028317 // fallback: 1 ft³/s in m³/s (spec §3.10)
        }
        crate::PumpCurveType::Custom => {
            if let Some(ref cid) = pump.head_curve {
                if let Some(curve) = curves.iter().find(|c| &c.id == cid) {
                    if curve.points.len() >= 2 {
                        let first = curve.points.first().unwrap().x;
                        let last = curve.points.last().unwrap().x;
                        return (first + last) / 2.0;
                    }
                }
            }
            0.028317 // fallback: 1 ft³/s in m³/s (spec §3.10)
        }
    }
}

fn link_status_to_f64(status: LinkStatus) -> f64 {
    match status {
        LinkStatus::Closed | LinkStatus::XPressure | LinkStatus::XHead | LinkStatus::TempClosed => {
            0.0
        }
        LinkStatus::Open => 1.0,
        LinkStatus::Active | LinkStatus::XFcv => 2.0,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DemandCategory, HeadLossFormula, Junction, Link, LinkBase, LinkKind, Node, NodeBase,
        NodeKind, Pipe, Reservoir, SimulationOptions,
    };

    /// Two-node (reservoir + junction), one-pipe network. No tanks, no pumps.
    fn simple_network() -> Network {
        let options = SimulationOptions {
            duration: 3600.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            report_start: 0.0,
            ..SimulationOptions::default()
        };
        Network {
            title: vec![],
            options,
            patterns: vec![],
            curves: vec![],
            nodes: vec![
                Node {
                    base: NodeBase {
                        id: "R1".into(),
                        index: 1,
                        elevation: 100.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Reservoir(Reservoir { head_pattern: None }),
                    source: None,
                },
                Node {
                    base: NodeBase {
                        id: "J1".into(),
                        index: 2,
                        elevation: 0.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Junction(Junction {
                        demands: vec![DemandCategory {
                            base_demand: 0.01,
                            pattern: None,
                            name: None,
                        }],
                        emitter_coeff: 0.0,
                        emitter_exp: 0.5,
                    }),
                    source: None,
                },
            ],
            links: vec![Link {
                base: LinkBase {
                    id: "P1".into(),
                    index: 1,
                    from_node: 1,
                    to_node: 2,
                    initial_status: LinkStatus::Open,
                    initial_setting: Some(1.0),
                },
                kind: LinkKind::Pipe(Pipe {
                    length: 1000.0,
                    diameter: 0.3,
                    roughness: 100.0,
                    minor_loss: 0.0,
                    check_valve: false,
                    bulk_coeff: None,
                    wall_coeff: None,
                    leak_coeff_1: 0.0,
                    leak_coeff_2: 0.0,
                }),
            }],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn session_create_and_load() {
        let mut sess = Simulation::create();
        assert_eq!(sess.phase, Phase::Created);
        sess.load(simple_network()).expect("load failed");
        assert_eq!(sess.phase, Phase::Loaded);
    }

    #[test]
    fn session_run_hydraulics_completes() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        assert_eq!(sess.phase, Phase::HydraulicsDone);
    }

    #[test]
    fn id_index_lookups_and_reload_invalidation() {
        let mut sess = Simulation::from_network(simple_network()).expect("load failed");
        // Lazily-built map: O(1) lookups resolve to load-time indices.
        assert_eq!(sess.node_index_by_id("R1"), Some(0));
        assert_eq!(sess.node_index_by_id("J1"), Some(1));
        assert_eq!(sess.link_index_by_id("P1"), Some(0));
        assert_eq!(sess.node_index_by_id("nope"), None);
        assert_eq!(sess.link_index_by_id("nope"), None);

        // Re-loading a network with different IDs must discard the cached map.
        let mut renamed = simple_network();
        renamed.nodes[1].base.id = "J2".into();
        renamed.links[0].base.id = "P2".into();
        sess.load(renamed).expect("reload failed");
        assert_eq!(sess.node_index_by_id("J1"), None);
        assert_eq!(sess.node_index_by_id("J2"), Some(1));
        assert_eq!(sess.link_index_by_id("P1"), None);
        assert_eq!(sess.link_index_by_id("P2"), Some(0));
    }

    #[test]
    fn session_snapshot_recorded_after_step() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        // The run recorded at least one instant and is holding it.
        assert!(sess.current_time().is_some());
    }

    #[test]
    fn get_node_head_after_hydraulics() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let head = sess
            .get_node_result("R1", NodeQuantity::Head)
            .expect("get_node_result failed");
        // Reservoir head should be its elevation (100 ft).
        assert!((head - 100.0).abs() < 1.0, "head = {head}");
    }

    #[test]
    fn get_link_flow_after_hydraulics() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let flow = sess
            .get_link_result("P1", LinkQuantity::Flow)
            .expect("get_link_result failed");
        // Flow must be non-negative (demand-driven network).
        assert!(flow >= 0.0, "flow = {flow}");
    }

    /// §8.2: friction factor is produced for every pipe whatever head-loss
    /// formula ran, back-computed from the loss the solve actually produced.
    /// It returned 0 for anything but Darcy-Weisbach, and Hazen-Williams is
    /// the default, so the column was zero for most models — in a file format
    /// called EPANET-compatible, whose own `output.c` gates the same
    /// inversion on `Type <= PIPE && ABS(flow) > TINY` and nothing else.
    #[test]
    fn friction_factor_is_produced_under_hazen_williams() {
        let mut sess = Simulation::create();
        let network = simple_network();
        assert_eq!(
            network.options.head_loss_formula,
            HeadLossFormula::HazenWilliams,
            "this fixture is the default-formula case"
        );
        sess.load(network).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let f = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor)
            .expect("get_link_result failed");

        assert!(f > 0.0, "friction factor should be reported, got {f}");
        // Plausible for turbulent flow in a pipe; the point is that it is a
        // real inversion of the observed loss, not a placeholder.
        assert!(f < 1.0, "friction factor implausibly large: {f}");
    }

    #[test]
    fn friction_factor_positive_for_darcy_weisbach_pipe() {
        let mut network = simple_network();
        network.options.head_loss_formula = HeadLossFormula::DarcyWeisbach;

        let mut sess = Simulation::create();
        sess.load(network).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let friction_factor = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor)
            .expect("get_link_result failed");

        assert!(
            friction_factor.is_finite(),
            "friction_factor = {friction_factor}"
        );
        assert!(friction_factor > 0.0, "friction_factor = {friction_factor}");
    }

    #[test]
    fn friction_factor_zero_for_zero_flow_pipe() {
        let mut network = simple_network();
        network.options.head_loss_formula = HeadLossFormula::DarcyWeisbach;
        if let NodeKind::Junction(junction) = &mut network.nodes[1].kind {
            junction.demands[0].base_demand = 0.0;
        }

        let mut sess = Simulation::create();
        sess.load(network).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let friction_factor = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor)
            .expect("get_link_result failed");

        assert_eq!(friction_factor, 0.0);
    }

    #[test]
    fn unknown_node_id_returns_error() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().unwrap();
        let err = sess.get_node_result("ZZZZ", NodeQuantity::Head);
        assert!(matches!(err, Err(SessionError::UnknownId(_))));
    }

    #[test]
    fn wrong_phase_returns_error() {
        let mut sess = Simulation::create();
        // run_hydraulics without load
        let err = sess.run_hydraulics();
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
    }

    #[test]
    fn set_link_property_changes_roughness() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.set_link_property("P1", LinkProperty::Roughness, 50.0)
            .expect("set_link_property");
        let network = sess.network.as_ref().unwrap();
        if let LinkKind::Pipe(p) = &network.links[0].kind {
            assert!((p.roughness - 50.0).abs() < 1e-10);
        } else {
            panic!("expected pipe");
        }
    }

    #[test]
    fn flow_balance_accessible_after_hydraulics() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().unwrap();
        let fb = sess.get_flow_balance().expect("get_flow_balance");
        // After a full run the balance ratio should be close to 1.
        let ratio = fb.balance_ratio(
            sess.node_states
                .iter()
                .enumerate()
                .filter_map(|(i, ns)| {
                    if matches!(
                        sess.network.as_ref().unwrap().nodes[i].kind,
                        NodeKind::Tank(_)
                    ) {
                        Some(ns.volume)
                    } else {
                        None
                    }
                })
                .sum::<f64>(),
        );
        // No tanks → numerator/denominator = outflow/inflow ≈ 1.
        assert!(ratio >= 0.0);
    }

    /// "A step has been taken" and "an instant has been recorded" are two
    /// questions. They were answered by one field, the emptiness of the history,
    /// and they come apart exactly here: with reporting starting after the run
    /// does and quality disabled, the session steps for an hour before it
    /// records anything. Only the separate `current_t` guard kept the old
    /// conflation from mis-classifying a mutation as a pre-run one.
    #[test]
    fn stepping_and_recording_are_independent_questions() {
        let mut net = simple_network();
        net.options.duration = 4.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.report_step = 3600.0;
        // Reporting begins an hour into a run that starts at zero.
        net.options.report_start = 3600.0;
        net.options.quality_mode = QualityMode::None;

        let mut sess = Simulation::create();
        sess.load(net).expect("load");
        assert!(!sess.has_stepped, "no step taken yet");
        assert!(sess.current_time().is_none(), "nothing recorded yet");

        sess.step_hydraulics().expect("first step");
        assert!(sess.has_stepped, "a step has been taken");
        assert!(
            sess.current_time().is_none(),
            "the first reported instant is still an hour away, so the history \
             cannot answer whether a step was taken"
        );
    }

    #[test]
    fn quality_completes_with_the_hydraulic_run() {
        // Was a regression test for a runaway quality loop driven through
        // step_quality. Quality no longer has a loop of its own; it finishes
        // when the run does.
        let mut net = simple_network();
        net.options.duration = 2.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.qual_step = 360.0;
        net.options.report_step = 3600.0;
        net.options.report_start = 0.0;
        net.options.quality_mode = QualityMode::Age;

        let mut sess = Simulation::create();
        sess.load(net).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");

        // The CLI progress loop used to drive quality after hydraulics. There
        // is nothing left to drive: quality advanced with each step and has
        // already reached the duration.
        assert!(
            (sess.quality_t - 2.0 * 3600.0).abs() < 1.0,
            "quality_t = {}",
            sess.quality_t
        );
    }

    #[test]
    fn stepping_and_running_produce_the_same_quality() {
        // Was a comparison of two ways to drive the second quality pass. With
        // quality riding the hydraulic step there is only one pass, so the
        // claim worth holding is that driving it one step at a time gives the
        // same answer as running it in a single call. Left as a comparison of
        // no-ops it would pass with quality entirely broken.
        let mut net = simple_network();
        net.options.duration = 2.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.qual_step = 360.0;
        net.options.report_step = 3600.0;
        net.options.report_start = 0.0;
        net.options.quality_mode = QualityMode::Age;

        // Session A: one call.
        let mut sess_a = Simulation::create();
        sess_a.load(net.clone()).expect("load");
        sess_a.run().expect("run");

        // Session B: stepped, exactly as the CLI and GUI run loops drive it.
        let mut sess_b = Simulation::create();
        sess_b.load(net).expect("load");
        loop {
            let dt = sess_b.step_hydraulics().expect("step_hydraulics");
            if dt == 0.0 {
                break;
            }
        }

        // A session holds one instant, so the two runs are compared by what
        // they walked through and where they finished: the same number of
        // reporting instants, the same final time, and the same quality
        // there. A drive mode that skipped or doubled a step would move the
        // count even where it happened to land on the same final value.
        assert_eq!(
            sess_a.instants_recorded(),
            sess_b.instants_recorded(),
            "the two drive modes recorded different numbers of instants"
        );
        assert_eq!(sess_a.current_time(), sess_b.current_time());
        let t = sess_a.current_time().expect("an instant");
        let q_a = sess_a.get_node_result("J1", NodeQuantity::Quality).unwrap();
        let q_b = sess_b.get_node_result("J1", NodeQuantity::Quality).unwrap();
        assert!(
            (q_a - q_b).abs() < 1e-9,
            "quality mismatch at t={t}: run={q_a}, stepped={q_b}"
        );
    }

    // ── Additional results-coverage tests ────────────────────────────────────

    #[test]
    fn mean_velocity_positive_for_flowing_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let v = sess
            .get_link_result("P1", LinkQuantity::MeanVelocity)
            .expect("get_link_result");
        assert!(v > 0.0, "expected positive velocity, got {v}");
    }

    #[test]
    fn unit_head_loss_positive_for_flowing_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let uhl = sess
            .get_link_result("P1", LinkQuantity::UnitHeadLoss)
            .expect("get_link_result");
        assert!(uhl > 0.0, "expected positive unit head loss, got {uhl}");
    }

    #[test]
    fn link_status_open_returns_one() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let status = sess
            .get_link_result("P1", LinkQuantity::Status)
            .expect("get_link_result");
        // Pipe is Open → encoding 1.0.
        assert_eq!(status, 1.0);
    }

    #[test]
    fn link_setting_returns_setting_for_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let setting = sess
            .get_link_result("P1", LinkQuantity::Setting)
            .expect("get_link_result");
        // Pipe initial_setting = 1.0; roughness-based pipes pass setting through.
        assert!(setting.is_finite(), "setting = {setting}");
    }

    #[test]
    fn gauge_pressure_for_junction_equals_head_minus_elevation() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let head = sess
            .get_node_result("J1", NodeQuantity::Head)
            .expect("head");
        let gp = sess
            .get_node_result("J1", NodeQuantity::GaugePressure)
            .expect("gauge_pressure");
        // J1 elevation = 0.0, so GaugePressure = Head − 0.
        assert!((gp - head).abs() < 1e-9, "gp={gp}, head={head}");
    }

    #[test]
    fn demand_for_reservoir_returns_net_flow() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let demand = sess
            .get_node_result("R1", NodeQuantity::Demand)
            .expect("demand");
        // Reservoir net_flow should be negative (outflow to supply junction).
        assert!(demand < 0.0, "reservoir net_flow = {demand}");
    }

    /// Asking for a result before the run has recorded anything is an error,
    /// not a zero. There is no time to ask for any more (§8.2), so the only
    /// way to have no answer is to have taken no step.
    #[test]
    fn results_before_the_first_instant_are_an_error() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        let err = sess.get_node_result("J1", NodeQuantity::Head);
        assert!(matches!(err, Err(SessionError::NoResultsYet)), "{err:?}");

        sess.step_hydraulics().expect("one step");
        sess.get_node_result("J1", NodeQuantity::Head)
            .expect("answers once an instant exists");
    }

    #[test]
    fn node_ids_empty_before_load() {
        let sess = Simulation::create();
        assert!(sess.node_ids().is_empty());
    }

    #[test]
    fn link_ids_empty_before_load() {
        let sess = Simulation::create();
        assert!(sess.link_ids().is_empty());
    }

    #[test]
    fn pump_ids_empty_before_load() {
        let sess = Simulation::create();
        assert!(sess.pump_ids().is_empty());
    }

    #[test]
    fn flow_units_none_before_load() {
        let sess = Simulation::create();
        assert!(sess.flow_units().is_none());
    }

    #[test]
    fn get_pump_energy_error_for_non_pump() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        // "P1" is a pipe, not a pump; expect UnknownId.
        let err = sess.get_pump_energy("P1");
        assert!(matches!(err, Err(SessionError::UnknownId(_))));
    }

    #[test]
    fn link_status_to_f64_encoding() {
        assert_eq!(link_status_to_f64(LinkStatus::Open), 1.0);
        assert_eq!(link_status_to_f64(LinkStatus::Closed), 0.0);
        assert_eq!(link_status_to_f64(LinkStatus::TempClosed), 0.0);
        assert_eq!(link_status_to_f64(LinkStatus::XHead), 0.0);
        assert_eq!(link_status_to_f64(LinkStatus::Active), 2.0);
        assert_eq!(link_status_to_f64(LinkStatus::XFcv), 2.0);
    }

    // ── from_network / mutation coverage ─────────────────────────────────────

    #[test]
    fn from_network_succeeds_with_valid_network() {
        let sess = Simulation::from_network(simple_network()).expect("from_network");
        assert_eq!(sess.phase, Phase::Loaded);
    }

    #[test]
    fn set_node_property_elevation_changes_elevation() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.set_node_property("J1", NodeProperty::Elevation, 25.0)
            .expect("set_node_property");
        let elev = sess.network.as_ref().unwrap().nodes[1].base.elevation;
        assert!((elev - 25.0).abs() < 1e-10);
    }

    #[test]
    fn set_node_property_initial_quality_changes_quality() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.set_node_property("J1", NodeProperty::InitialQuality, 0.8)
            .expect("set_node_property");
        let iq = sess.network.as_ref().unwrap().nodes[1].base.initial_quality;
        assert!((iq - 0.8).abs() < 1e-10);
    }

    #[test]
    fn set_link_property_initial_status_closes_link() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.set_link_property("P1", LinkProperty::InitialStatus, 0.0)
            .expect("set_link_property");
        let status = sess.network.as_ref().unwrap().links[0].base.initial_status;
        assert_eq!(status, LinkStatus::Closed);
    }

    #[test]
    fn set_link_property_initial_setting_changes_setting() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.set_link_property("P1", LinkProperty::InitialSetting, 1.5)
            .expect("set_link_property");
        let setting = sess.network.as_ref().unwrap().links[0].base.initial_setting;
        assert_eq!(setting, Some(1.5));
    }

    #[test]
    fn set_node_property_unknown_id_returns_error() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        let err = sess.set_node_property("ZZZZ", NodeProperty::Elevation, 1.0);
        assert!(matches!(err, Err(SessionError::UnknownId(_))));
    }

    #[test]
    fn set_node_property_before_load_returns_invalid_phase() {
        let mut sess = Simulation::create();
        let err = sess.set_node_property("J1", NodeProperty::Elevation, 1.0);
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
    }

    #[test]
    fn peak_demand_cost_is_zero_when_no_pumps() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        // No pumps in simple_network, so peak demand cost is 0.
        assert_eq!(sess.peak_demand_cost(), 0.0);
    }

    #[test]
    fn snapshots_are_report_only_when_quality_none() {
        let mut net = simple_network();
        net.options.duration = 3.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.report_step = 2.0 * 3600.0;
        net.options.report_start = 0.0;
        net.options.quality_mode = QualityMode::None;

        let mut sess = Simulation::create();
        sess.load(net).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");

        // Reporting every two hours over three: instants at t = 0 and 7200.
        assert_eq!(sess.instants_recorded(), 2);
        assert_eq!(sess.current_time(), Some(7200.0));
    }

    /// Enabling quality used to multiply what a run retained: every hydraulic
    /// step was kept so the second pass could replay the flow field, while the
    /// results file still carried only the reporting instants. Quality now
    /// advances inside the step, so it costs no retention at all and the two
    /// modes keep exactly the same instants.
    #[test]
    fn quality_no_longer_changes_what_a_run_retains() {
        let build = |mode| {
            let mut net = simple_network();
            net.options.duration = 3.0 * 3600.0;
            net.options.hyd_step = 3600.0;
            net.options.report_step = 2.0 * 3600.0;
            net.options.report_start = 0.0;
            net.options.quality_mode = mode;
            let mut sess = Simulation::create();
            sess.load(net).expect("load");
            sess.run_hydraulics().expect("run_hydraulics");
            (sess.instants_recorded(), sess.current_time())
        };

        let without = build(QualityMode::None);
        let with = build(QualityMode::Age);

        // Reporting every two hours over three: instants at t = 0 and 7200.
        // The hourly hydraulic steps at 3600 and 10800 are computed and not
        // recorded.
        assert_eq!(without, (2, Some(7200.0)));
        assert_eq!(
            with, without,
            "quality must not cost instants: was {with:?}, hydraulics-only {without:?}"
        );
    }
}
