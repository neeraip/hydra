// simulation — §8 of crates/simulation/spec.md
//
// The public-facing API of hydra. Exposes the full simulation lifecycle:
// create → load → run/step hydraulics → run/step quality → get results →
// destroy. No I/O is performed here; all I/O is the responsibility of adapters.

use std::{f64, time::SystemTime};

use super::accounting::{self, AccountingState};
use super::controls;
use super::timestep;
use crate::hydraulics::{self as hydraulics, SolveResult, SolverContext};
use crate::io::HydSnapshot;
use crate::quality::{self as quality, QualityState};
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
pub use results::{LinkResult, NodeResult, ResultRanges};
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

    // Live simulation state.
    node_states: Vec<NodeState>,
    link_states: Vec<LinkState>,
    current_t: f64,
    next_report_t: f64,  // next report time boundary
    report_count: usize, // number of report boundaries passed

    // Hydraulic result history.
    hyd_snapshots: Vec<HydSnapshot>,

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
}

// ── Session internal helpers ──────────────────────────────────────────────────

impl Simulation {
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

        let quality_enabled = network.options.quality_mode != QualityMode::None;
        let at_or_past_report = new_t >= self.next_report_t - 1e-6;
        if quality_enabled || at_or_past_report {
            self.hyd_snapshots.push(HydSnapshot {
                t: new_t,
                node_states: self.node_states.clone(),
                link_states: self.link_states.clone(),
            });
        }

        // Advance the report-time marker independently of snapshot count.
        let report_step = network.options.report_step;
        let report_start = network.options.report_start;
        while new_t >= self.next_report_t - 1e-6 && self.next_report_t <= duration + 1e-6 {
            self.report_count += 1;
            self.next_report_t = report_start + report_step * (self.report_count as f64);
        }
    }

    /// Find the snapshot closest to `t` (within 0.5 s tolerance).
    ///
    /// Uses binary search — snapshots are always in ascending time order.
    fn snapshot_near(&self, t: f64) -> Option<&HydSnapshot> {
        if self.hyd_snapshots.is_empty() {
            return None;
        }
        // Binary search for the insertion point, then check the immediate neighbours.
        let idx = self.hyd_snapshots.partition_point(|s| s.t < t);
        let candidates = [idx.checked_sub(1), Some(idx)]
            .into_iter()
            .flatten()
            .filter(|&i| i < self.hyd_snapshots.len());
        candidates
            .min_by(|&a, &b| {
                let da = (self.hyd_snapshots[a].t - t).abs();
                let db = (self.hyd_snapshots[b].t - t).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|i| &self.hyd_snapshots[i])
            .filter(|s| (s.t - t).abs() < 0.5)
    }

    /// Find the snapshot whose time matches `t` (within 0.5 s tolerance).
    fn find_snapshot_index_at(&self, t: f64) -> Option<usize> {
        self.hyd_snapshots
            .iter()
            .position(|s| (s.t - t).abs() < 0.5)
    }

    /// Return initial states from the first snapshot (or live states if no
    /// snapshot was recorded yet) without cloning.
    fn first_snapshot_states(&self) -> (&[NodeState], &[LinkState]) {
        match self.hyd_snapshots.first() {
            Some(s) => (&s.node_states, &s.link_states),
            None => (&self.node_states, &self.link_states),
        }
    }
}

// ── Free-standing helpers ─────────────────────────────────────────────────────

/// Initialise node states from the static network (§2.4).
fn init_node_states(network: &Network) -> Vec<NodeState> {
    network
        .nodes
        .iter()
        .map(|n| {
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
        })
        .collect()
}

/// Initialise link states from static network data (§2.6).
fn init_link_states(network: &Network) -> Vec<LinkState> {
    network
        .links
        .iter()
        .map(|l| {
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
        })
        .collect()
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

/// Find a node's 0-based index by string ID.
fn node_index_by_id(network: &Network, id: &str) -> Option<usize> {
    network.nodes.iter().position(|n| n.base.id == id)
}

/// Find a link's 0-based index by string ID.
fn link_index_by_id(network: &Network, id: &str) -> Option<usize> {
    network.links.iter().position(|l| l.base.id == id)
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
    fn session_snapshot_recorded_after_step() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        // At least one snapshot should be recorded.
        assert!(!sess.hyd_snapshots.is_empty());
    }

    #[test]
    fn get_node_head_after_hydraulics() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        // Get node result at first snapshot time.
        let snap_t = sess.hyd_snapshots[0].t;
        let head = sess
            .get_node_result("R1", NodeQuantity::Head, snap_t)
            .expect("get_node_result failed");
        // Reservoir head should be its elevation (100 ft).
        assert!((head - 100.0).abs() < 1.0, "head = {head}");
    }

    #[test]
    fn get_link_flow_after_hydraulics() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let snap_t = sess.hyd_snapshots[0].t;
        let flow = sess
            .get_link_result("P1", LinkQuantity::Flow, snap_t)
            .expect("get_link_result failed");
        // Flow must be non-negative (demand-driven network).
        assert!(flow >= 0.0, "flow = {flow}");
    }

    #[test]
    fn friction_factor_zero_for_non_darcy_weisbach() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let snap_t = sess.hyd_snapshots[0].t;
        let friction_factor = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor, snap_t)
            .expect("get_link_result failed");

        assert_eq!(friction_factor, 0.0);
    }

    #[test]
    fn friction_factor_positive_for_darcy_weisbach_pipe() {
        let mut network = simple_network();
        network.options.head_loss_formula = HeadLossFormula::DarcyWeisbach;

        let mut sess = Simulation::create();
        sess.load(network).expect("load failed");
        sess.run_hydraulics().expect("run_hydraulics failed");
        let snap_t = sess.hyd_snapshots[0].t;
        let friction_factor = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor, snap_t)
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
        let snap_t = sess.hyd_snapshots[0].t;
        let friction_factor = sess
            .get_link_result("P1", LinkQuantity::FrictionFactor, snap_t)
            .expect("get_link_result failed");

        assert_eq!(friction_factor, 0.0);
    }

    #[test]
    fn unknown_node_id_returns_error() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().unwrap();
        let t = sess.hyd_snapshots[0].t;
        let err = sess.get_node_result("ZZZZ", NodeQuantity::Head, t);
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

    #[test]
    fn step_quality_direct_loop_terminates() {
        // Regression test for the runaway quality loop bug: calling step_quality()
        // directly (without run_quality()) must initialise quality state on the
        // first call and terminate normally when quality_t reaches duration.
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

        // Drive quality via step_quality, exactly as the CLI progress loop does.
        let mut steps = 0usize;
        let mut total_t = 0.0_f64;
        loop {
            let dt = sess.step_quality().expect("step_quality");
            if dt == 0.0 {
                break;
            }
            total_t += dt;
            steps += 1;
            assert!(
                steps < 1000,
                "step_quality did not terminate within 1000 steps"
            );
        }

        assert_eq!(sess.phase, Phase::QualityDone);
        assert!((total_t - 2.0 * 3600.0).abs() < 1.0, "total_t = {total_t}");
    }

    #[test]
    fn step_quality_and_run_quality_produce_same_results() {
        // Regression test: step_quality loop must produce the same quality
        // values as run_quality, ensuring lazy-init in step_quality is correct.
        let mut net = simple_network();
        net.options.duration = 2.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.qual_step = 360.0;
        net.options.report_step = 3600.0;
        net.options.report_start = 0.0;
        net.options.quality_mode = QualityMode::Age;

        // Session A: use run_quality().
        let mut sess_a = Simulation::create();
        sess_a.load(net.clone()).expect("load");
        sess_a.run_hydraulics().expect("run_hydraulics");
        sess_a.run_quality().expect("run_quality");

        // Session B: drive quality via step_quality loop (CLI-style).
        let mut sess_b = Simulation::create();
        sess_b.load(net).expect("load");
        sess_b.run_hydraulics().expect("run_hydraulics");
        loop {
            let dt = sess_b.step_quality().expect("step_quality");
            if dt == 0.0 {
                break;
            }
        }

        // Both sessions must produce the same quality at every snapshot.
        let times_a = sess_a.snapshot_times();
        let times_b = sess_b.snapshot_times();
        assert_eq!(times_a, times_b);
        for &t in &times_a {
            let q_a = sess_a
                .get_node_result("J1", NodeQuantity::Quality, t)
                .unwrap();
            let q_b = sess_b
                .get_node_result("J1", NodeQuantity::Quality, t)
                .unwrap();
            assert!(
                (q_a - q_b).abs() < 1e-9,
                "quality mismatch at t={t}: run_quality={q_a}, step_quality={q_b}"
            );
        }
    }

    #[test]
    fn step_quality_returns_zero_immediately_when_quality_none() {
        // When quality mode is None, step_quality must return 0.0 immediately
        // and transition to QualityDone — it must not loop indefinitely.
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let dt = sess.step_quality().expect("step_quality");
        assert_eq!(dt, 0.0);
        assert_eq!(sess.phase, Phase::QualityDone);
    }

    // ── Additional results-coverage tests ────────────────────────────────────

    #[test]
    fn mean_velocity_positive_for_flowing_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let v = sess
            .get_link_result("P1", LinkQuantity::MeanVelocity, t)
            .expect("get_link_result");
        assert!(v > 0.0, "expected positive velocity, got {v}");
    }

    #[test]
    fn unit_head_loss_positive_for_flowing_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let uhl = sess
            .get_link_result("P1", LinkQuantity::UnitHeadLoss, t)
            .expect("get_link_result");
        assert!(uhl > 0.0, "expected positive unit head loss, got {uhl}");
    }

    #[test]
    fn link_status_open_returns_one() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let status = sess
            .get_link_result("P1", LinkQuantity::Status, t)
            .expect("get_link_result");
        // Pipe is Open → encoding 1.0.
        assert_eq!(status, 1.0);
    }

    #[test]
    fn link_setting_returns_setting_for_pipe() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let setting = sess
            .get_link_result("P1", LinkQuantity::Setting, t)
            .expect("get_link_result");
        // Pipe initial_setting = 1.0; roughness-based pipes pass setting through.
        assert!(setting.is_finite(), "setting = {setting}");
    }

    #[test]
    fn gauge_pressure_for_junction_equals_head_minus_elevation() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let head = sess
            .get_node_result("J1", NodeQuantity::Head, t)
            .expect("head");
        let gp = sess
            .get_node_result("J1", NodeQuantity::GaugePressure, t)
            .expect("gauge_pressure");
        // J1 elevation = 0.0, so GaugePressure = Head − 0.
        assert!((gp - head).abs() < 1e-9, "gp={gp}, head={head}");
    }

    #[test]
    fn demand_for_reservoir_returns_net_flow() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let t = sess.hyd_snapshots[0].t;
        let demand = sess
            .get_node_result("R1", NodeQuantity::Demand, t)
            .expect("demand");
        // Reservoir net_flow should be negative (outflow to supply junction).
        assert!(demand < 0.0, "reservoir net_flow = {demand}");
    }

    #[test]
    fn no_snapshot_at_time_returns_error() {
        let mut sess = Simulation::create();
        sess.load(simple_network()).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let err = sess.get_node_result("J1", NodeQuantity::Head, 999_999.0);
        assert!(matches!(err, Err(SessionError::NoSnapshotAtTime { .. })));
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
        let ts = sess.snapshot_times();

        assert_eq!(ts, vec![0.0, 7200.0]);
    }

    #[test]
    fn snapshots_remain_per_step_when_quality_enabled() {
        let mut net = simple_network();
        net.options.duration = 3.0 * 3600.0;
        net.options.hyd_step = 3600.0;
        net.options.report_step = 2.0 * 3600.0;
        net.options.report_start = 0.0;
        net.options.quality_mode = QualityMode::Age;

        let mut sess = Simulation::create();
        sess.load(net).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");
        let ts = sess.snapshot_times();

        assert_eq!(ts, vec![0.0, 3600.0, 7200.0, 10800.0]);
    }
}
