#![doc = include_str!("spec.md")]

/// Semver version of the quality engine, taken from `Cargo.toml` at compile time.
pub const HYDRA_QUALITY_VERSION: &str = env!("CARGO_PKG_VERSION");

use crate::{LinkKind, LinkState, MixModel, Network, NodeKind, NodeState, QualityMode};
use std::collections::VecDeque;

mod mixing;
#[cfg(test)]
mod quality_tests;
mod reactions;
mod shared;
#[cfg(test)]
mod test_support;
mod transport;
use shared::{
    qual_flow_dir, tank_outflow_conc, total_mass, PipeQuality, Segment, TankQuality, C_MAX,
};

pub use crate::io::MassBalance;
pub use shared::QualityError;
pub use shared::QualityState;

// ── §6 Initialisation ─────────────────────────────────────────────────────────

/// Initialises the quality state at the start of a simulation (§6.8).
///
/// Fills each pipe with a single full-volume segment at the concentration of
/// its upstream node (based on `initial_quality`). Tank quality is initialised
/// according to the tank's mixing model.
///
/// Returns `Err(ModeNone)` when `quality_mode = NONE`.
pub fn init_quality(
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
) -> Result<QualityState, QualityError> {
    if network.options.quality_mode == QualityMode::None {
        return Err(QualityError::ModeNone);
    }

    let n_nodes = network.nodes.len();
    let n_links = network.links.len();

    let mut node_conc: Vec<f64> = (0..n_nodes)
        .map(|i| network.nodes[i].base.initial_quality)
        .collect();

    // For Trace mode, the trace source node starts at 100 %.
    if network.options.quality_mode == QualityMode::Trace {
        if let Some(ref trace_id) = network.options.trace_node {
            if let Some(idx) = network.nodes.iter().position(|n| n.base.id == *trace_id) {
                node_conc[idx] = 100.0;
            }
        }
    }

    let flow_dir: Vec<i8> = (0..n_links)
        .map(|k| qual_flow_dir(link_states[k].flow))
        .collect();

    // Static incident-link index per node (includes stagnant links).
    let mut node_links: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    for (k, link) in network.links.iter().enumerate() {
        let from_0 = link.base.from_idx();
        let to_0 = link.base.to_idx();
        if from_0 < n_nodes {
            node_links[from_0].push(k);
        }
        if to_0 < n_nodes && to_0 != from_0 {
            node_links[to_0].push(k);
        }
    }

    // Per-pipe: initialise with one segment spanning the full pipe volume.
    let pipe_quality: Vec<Option<PipeQuality>> = (0..n_links)
        .map(|k| {
            let link = &network.links[k];
            if let LinkKind::Pipe(pipe) = &link.kind {
                let dir = flow_dir[k];
                // EPANET initsegs always uses N2 (downstream/to_node) regardless
                // of flow direction for the initial segment concentration.
                let downstream = link.base.to_node;
                let c = if downstream >= 1 && downstream <= n_nodes {
                    node_conc[downstream - 1]
                } else {
                    0.0
                };
                let r = pipe.diameter / 2.0;
                let vol = std::f64::consts::PI * r * r * pipe.length;
                let mut segs: VecDeque<Segment> = VecDeque::new();
                if vol > 0.0 {
                    segs.push_back(Segment {
                        volume: vol,
                        concentration: c,
                    });
                }
                Some(PipeQuality {
                    segments: segs,
                    flow_dir: dir,
                })
            } else {
                None
            }
        })
        .collect();

    // Per-tank: initialise by mixing model.
    let tank_quality: Vec<Option<TankQuality>> = (0..n_nodes)
        .map(|i| {
            let node = &network.nodes[i];
            let node_state = &node_states[i];
            if let NodeKind::Tank(tank) = &node.kind {
                let c = node_conc[i];
                let volume = node_state.volume;
                let v_max = tank.volume_from_level(tank.max_level, &network.curves);
                Some(match tank.mix_model {
                    MixModel::Cstr => TankQuality::Cstr { volume, conc: c },
                    MixModel::TwoCompartment => {
                        let v_mz = tank.mix_fraction * v_max;
                        let mix_vol = volume.min(v_mz);
                        let stag_vol = (volume - mix_vol).max(0.0);
                        TankQuality::TwoComp {
                            mix_vol,
                            mix_conc: c,
                            stag_vol,
                            stag_conc: c,
                        }
                    }
                    MixModel::Fifo => {
                        let mut segs: VecDeque<Segment> = VecDeque::new();
                        if volume > 0.0 {
                            segs.push_back(Segment {
                                volume,
                                concentration: c,
                            });
                        }
                        TankQuality::Fifo { segments: segs }
                    }
                    MixModel::Lifo => {
                        let segs = if volume > 0.0 {
                            vec![Segment {
                                volume,
                                concentration: c,
                            }]
                        } else {
                            vec![]
                        };
                        TankQuality::Lifo { segments: segs }
                    }
                })
            } else {
                None
            }
        })
        .collect();

    let mut state = QualityState {
        pipe_quality,
        tank_quality,
        node_conc,
        node_links,
        topo_order: Vec::new(),
        adjacency: Vec::new(),
        flow_dir,
        needs_topo: true,
        mass_balance: MassBalance::default(),
        pipe_rate_coeff: vec![0.0; n_links],
        tank_overflows: vec![false; n_nodes],
    };

    state.mass_balance.init = total_mass(&state);
    transport::rebuild_adjacency_and_topo(&mut state, network, link_states);
    state.needs_topo = false;

    Ok(state)
}

// ── §6.2 Quality sub-step loop ────────────────────────────────────────────────

/// Advances the quality simulation through one hydraulic period of `dt_h`
/// seconds at hydraulic time `t` (§6.2).
///
/// The flow field (link_states) is held constant across all quality sub-steps.
pub fn advance_quality(
    state: &mut QualityState,
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    dt_h: f64,
    t: f64,
) {
    if network.options.quality_mode == QualityMode::None {
        return;
    }

    // Rebuild topo if flow directions changed since the last period.
    let new_dirs: Vec<i8> = link_states
        .iter()
        .map(|ls| qual_flow_dir(ls.flow))
        .collect();
    let dirs_changed = new_dirs
        .iter()
        .enumerate()
        .any(|(k, &dir)| dir != state.flow_dir[k]);

    if dirs_changed || state.needs_topo {
        for (k, &new_dir) in new_dirs.iter().enumerate() {
            if let Some(Some(pq)) = state.pipe_quality.get_mut(k) {
                // Reverse segments only on actual sign flip (not stagnant transitions).
                if (new_dir as i16) * (pq.flow_dir as i16) < 0 {
                    pq.segments.make_contiguous().reverse();
                }
                pq.flow_dir = new_dir;
            }
            state.flow_dir[k] = new_dir;
        }
        transport::rebuild_adjacency_and_topo(state, network, link_states);
        state.needs_topo = false;
    }

    let dq = network.options.qual_step;

    // Pre-compute tank overflow flags for this hydraulic period.
    // EPANET's two-phase architecture (all hydraulics, then all quality) means
    // the quality engine sees tank->V from the FINAL hydraulic step — not the
    // per-period snapshot.  For a tank that fills to capacity during a period,
    // tank->V >= Vmax for ALL quality sub-steps.  We replicate this by checking
    // whether the tank would reach Vmax at any point during this period.
    // EPANET's tanklevels() also snaps to Vmax when within one second of flow,
    // so we include that tolerance here.
    for (i, node) in network.nodes.iter().enumerate() {
        if let NodeKind::Tank(tank) = &node.kind {
            let node_state = &node_states[i];
            let v_max = tank.volume_from_level(tank.max_level, &network.curves);
            let end_vol = node_state.volume + node_state.net_flow * dt_h;
            // Match EPANET's "within next second" snap: if end_vol plus one
            // more second of net_flow would reach Vmax, treat as at Vmax.
            state.tank_overflows[i] = node_state.net_flow > 0.0
                && (end_vol >= v_max || end_vol + node_state.net_flow >= v_max);
        }
    }

    let mut t_q = 0.0_f64;
    while t_q < dt_h {
        let dt = dq.min(dt_h - t_q);
        let accumulate_rates = (t + t_q) >= network.options.report_start;

        // §6.5 React in pipes (before transport).
        reactions::react_pipe_segs(state, network, link_states, dt, accumulate_rates);

        // §6.5 React in tanks (before transport; FIFO/LIFO/CSTR per §6.5.3;
        // Two-Comp reactions applied after mixing per §6.7.2).
        reactions::react_tanks(
            state,
            network,
            network.options.quality_mode == QualityMode::Chemical,
            dt,
            accumulate_rates,
        );

        // §6.1 AGE: increment all concentrations by δt/3600, BEFORE transport.
        // EPANET ages segments inside pipereact()/tankreact() (i.e. the react
        // phase) so that transport sees already-aged values.  Placing the
        // increment after transport delays node_conc by one sub-step.
        if network.options.quality_mode == QualityMode::Age {
            let inc = dt / 3600.0;
            for pq in state.pipe_quality.iter_mut().flatten() {
                for seg in &mut pq.segments {
                    seg.concentration = (seg.concentration + inc).min(C_MAX);
                }
            }
            for tq_opt in state.tank_quality.iter_mut().flatten() {
                reactions::age_inc_tank(tq_opt, inc);
            }
        }

        // §6.3.2–§6.4 Combined transport: advect, mix, and push in topological
        // order.  EPANET processes each node in one pass — inflow segments are
        // consumed, the node concentration is computed, and new segments are
        // pushed into outflow links — so that water traverses the entire
        // network within a single sub-step.  Splitting these into separate
        // phases (advect-all → mix-all → push-all) introduces a one-hop-per-
        // sub-step transport delay.
        let _volout = transport::transport_step(
            state,
            network,
            node_states,
            link_states,
            dt,
            t + t_q,
            accumulate_rates,
        );

        // Synchronise node_conc for tank nodes from their quality state.
        // For FIFO/LIFO, preserve the withdrawal-weighted average computed
        // in update_tank_mix (EPANET doesn't re-sync after reactions).
        for i in 0..network.nodes.len() {
            if let Some(tq) = &state.tank_quality[i] {
                match tq {
                    TankQuality::Fifo { .. } | TankQuality::Lifo { .. } => {}
                    _ => state.node_conc[i] = tank_outflow_conc(tq),
                }
            }
        }

        t_q += dt;
    }

    state.mass_balance.final_mass = total_mass(state);
}

/// Volume-weighted average concentration for a given link (EPANET's `avgqual()`).
///
/// For pipe/valve/CV links with segment data, computes:
///   avg = Σ(c_i · v_i) / Σ(v_i)
/// Falls back to `(C_from + C_to) / 2` when no segments or zero total volume.
pub fn avg_link_quality(
    state: &QualityState,
    link_index: usize,
    from_node: usize,
    to_node: usize,
) -> f64 {
    if let Some(Some(pq)) = state.pipe_quality.get(link_index) {
        let (mass, vol) = pq.segments.iter().fold((0.0, 0.0), |(m, v), s| {
            (m + s.concentration * s.volume, v + s.volume)
        });
        if vol > 0.0 {
            mass / vol
        } else {
            let c1 = state.node_conc.get(from_node).copied().unwrap_or(0.0);
            let c2 = state.node_conc.get(to_node).copied().unwrap_or(0.0);
            (c1 + c2) / 2.0
        }
    } else {
        let c1 = state.node_conc.get(from_node).copied().unwrap_or(0.0);
        let c2 = state.node_conc.get(to_node).copied().unwrap_or(0.0);
        (c1 + c2) / 2.0
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Network, NodeState, SimulationOptions};

    // ── §6.9 Mass balance ──────────────────────────────────────────────────

    #[test]
    fn mass_balance_ratio_equals_one_for_inert_system() {
        // A single pipe, no reaction, no sources: initial mass should equal final.
        let options = SimulationOptions {
            quality_mode: crate::QualityMode::Chemical,
            qual_step: 10.0,
            ..SimulationOptions::default()
        };
        let network = Network {
            title: vec![],
            options,
            patterns: vec![],
            curves: vec![],
            nodes: vec![
                test_support::junction_node(1, 0.0),
                test_support::junction_node(2, 0.0),
            ],
            links: vec![test_support::link(
                1,
                1,
                2,
                test_support::default_pipe(100.0, 1.0),
            )],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states = vec![NodeState::default(); 2];
        let link_states = vec![test_support::link_state_q(1.0)];
        let mut state = init_quality(&network, &node_states, &link_states).unwrap();
        // Set concentration to 5.0 everywhere.
        if let Some(pq) = &mut state.pipe_quality[0] {
            for seg in &mut pq.segments {
                seg.concentration = 5.0;
            }
        }
        state.mass_balance.init = total_mass(&state);

        // Advance one short hydraulic period.
        advance_quality(&mut state, &network, &node_states, &link_states, 100.0, 0.0);

        // Balance ratio should be close to 1.0 (no sinks/sources/reactions).
        let ratio = state.mass_balance.ratio();
        approx::assert_abs_diff_eq!(ratio, 1.0, epsilon = 0.05);
    }

    #[test]
    fn age_mode_increments_concentration_each_substep() {
        // AGE mode: sub-step of 3600 s → each segment gains 1.0 hour.
        let options = SimulationOptions {
            quality_mode: crate::QualityMode::Age,
            qual_step: 3600.0,
            hyd_step: 3600.0,
            ..SimulationOptions::default()
        };
        let network = Network {
            title: vec![],
            options,
            patterns: vec![],
            curves: vec![],
            nodes: vec![
                test_support::junction_node(1, 0.0),
                test_support::junction_node(2, 0.0),
            ],
            links: vec![test_support::link(
                1,
                1,
                2,
                test_support::default_pipe(100.0, 1.0),
            )],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states = vec![NodeState::default(); 2];
        let link_states = vec![test_support::link_state_q(0.0)]; // zero flow → no advection

        let mut state = init_quality(&network, &node_states, &link_states).unwrap();
        // Set initial age to 0.
        if let Some(pq) = &mut state.pipe_quality[0] {
            for seg in &mut pq.segments {
                seg.concentration = 0.0;
            }
        }

        advance_quality(
            &mut state,
            &network,
            &node_states,
            &link_states,
            3600.0,
            0.0,
        );

        // After one 3600-s sub-step, pipe segments should have age = 1.0 h.
        if let Some(pq) = &state.pipe_quality[0] {
            for seg in &pq.segments {
                approx::assert_abs_diff_eq!(seg.concentration, 1.0, epsilon = 1e-12);
            }
        }

        // With zero inflow at junctions, AGE mode still updates junction quality
        // from adjacent segment concentrations (EPANET noflowqual behavior).
        approx::assert_abs_diff_eq!(state.node_conc[0], 1.0, epsilon = 1e-12);
        approx::assert_abs_diff_eq!(state.node_conc[1], 1.0, epsilon = 1e-12);
    }
}
