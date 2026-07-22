use std::{collections::BTreeSet, time::Instant};

use crate::{
    DemandModel, FavadCoeffs, LinkKind, LinkState, LinkStatus, Network, NodeKind, NodeState,
    PumpCurveType,
};

use super::assembly::{assemble_links, assemble_node_residuals};
use super::diagnostics::SolvePhaseTimings;
use super::pump::{fit_pump_coeffs, initialise_flows, link_py};
use super::shared::{HydraulicError, PumpCoeffs, SolveResult};
use super::valve::apply_valve_coefficients;
use super::{
    apply_emitter_coeffs, apply_favad_leakage_coeffs, apply_pda_demand_coeffs, bad_valve,
    check_link_status, check_valve_status, leakage_converged, pipe_resistance,
    update_emitter_flows, update_leakage_flows, update_pda_demand_flows, SparseSolver,
};

/// Updates link flows from the new head vector (§3.7).
///
/// EPANET GGA flow update:
///   dq = Y[k] − P[k]·(H₁ − H₂)
///   Q[k] = Q[k] − dq
///
/// Equivalently: Q_new = Q_old − Y[k] + P[k]·(H₁ − H₂)
/// Updates link flows after solving the linear system (§3.7).
/// Fused flow update + convergence metrics in a single link pass.
/// Replaces the separate update_flows, convergence fold, head_error_ok, and
/// flow_change_ok passes — matching EPANET's newlinkflows which also fuses
/// flow updates with convergence metric accumulation.
struct FlowUpdateResult {
    link_sq: f64,
    link_dsq: f64,
    head_ok: bool,
    flow_ok: bool,
}

fn update_flows_and_check(
    p: &[f64],
    y: &[f64],
    node_heads: &[f64],
    flows: &mut [f64],
    prev_flows: &mut [f64],
    statuses: &[LinkStatus],
    is_const_hp_pump: &[bool],
    head_error_limit: f64,
    flow_change_limit: f64,
    link_from: &[usize],
    link_to: &[usize],
) -> FlowUpdateResult {
    let mut link_sq = 0.0_f64;
    let mut link_dsq = 0.0_f64;
    let mut head_ok = true;
    let mut flow_ok = true;

    let n_links = flows.len();
    for k in 0..n_links {
        prev_flows[k] = flows[k];

        let from_node_index = link_from[k];
        let to_node_index = link_to[k];
        let head_drop = node_heads[from_node_index] - node_heads[to_node_index];
        let yk = y[k];
        let pk = p[k];
        let mut dq = yk - pk * head_drop;

        // ConstHp pump flow cap: if the unconstrained correction would exceed
        // the current flow, cap the step to half the current flow (§3.8).
        if is_const_hp_pump[k] && matches!(statuses[k], LinkStatus::Open) && dq > flows[k] {
            dq = flows[k] / 2.0;
        }

        flows[k] -= dq;
        link_sq += flows[k].abs();
        link_dsq += dq.abs();

        if head_error_limit > 0.0 && pk > 0.0 {
            let eps = (head_drop - yk / pk).abs();
            if eps > head_error_limit {
                head_ok = false;
            }
        }

        if flow_change_limit > 0.0 && dq.abs() > flow_change_limit {
            flow_ok = false;
        }
    }

    FlowUpdateResult {
        link_sq,
        link_dsq,
        head_ok,
        flow_ok,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DemandTerm {
    pub(super) base_demand: f64,
    pub(super) pattern_idx: Option<usize>,
}

/// Pre-computed working context; allocated once before simulation and reused
/// across all hydraulic time steps (§3.6 Phase 1 + 2).
///
/// Created by [`build_solver_context`]. Not part of the data model.
pub struct SolverContext {
    pub(crate) node_junc_step_opt: Vec<Option<usize>>,
    pub(crate) junc_nodes: Vec<usize>,
    pub(super) junction_demand_terms: Vec<Vec<DemandTerm>>,
    pub(crate) pipe_r: Vec<f64>,
    pub(crate) pump_coeffs: Vec<Option<PumpCoeffs>>,
    pub(crate) pump_curve_idx: Vec<Option<usize>>,
    pump_qmax_inner: Vec<f64>,
    pub(crate) link_aij_pos: Vec<Option<usize>>,
    pub(crate) link_from: Vec<usize>,
    pub(crate) link_to: Vec<usize>,
    pub(crate) sparse: SparseSolver,
    pub(crate) p: Vec<f64>,
    pub(crate) y: Vec<f64>,
    pub(super) node_heads: Vec<f64>,
    pub(super) node_elevations: Vec<f64>,
    pub(super) junction_demands: Vec<f64>,
    pub(super) xflow: Vec<f64>,
    pub(super) prev_flows: Vec<f64>,
    pub(crate) flows: Vec<f64>,
    pub(crate) statuses: Vec<LinkStatus>,
    pub(crate) settings: Vec<f64>,
    pub(crate) emitter_flows: Vec<f64>,
    pub(crate) prev_emitter_flows: Vec<f64>,
    pub(crate) leakage_fa_flows: Vec<f64>,
    pub(crate) leakage_va_flows: Vec<f64>,
    pub(crate) prev_leakage_flows: Vec<f64>,
    pub(crate) pda_demand_flows: Vec<f64>,
    pub(crate) prev_pda_demand_flows: Vec<f64>,
    /// Scratch buffer for O(n_links) net-flow accumulation at end of step.
    /// Zeroed before use; only tank/reservoir entries are read.
    net_flow_accum: Vec<f64>,
    /// Precomputed per-node `h_min` for tank level checks (§2.10).
    /// `NEG_INFINITY` for non-tanks and empty tanks so the "draining" check
    /// never fires.
    pub(super) node_h_min: Vec<f64>,
    /// Precomputed per-node `h_max` for tank level checks (§2.10).
    /// `INFINITY` for non-tanks, empty tanks, and overflow tanks so the
    /// "full tank" check never fires.
    pub(super) node_h_max: Vec<f64>,
    /// True if any node has a non-zero FAVAD leakage coefficient (§3.7).
    /// Precomputed once so the per-convergence `.any()` scan is avoided.
    pub(super) has_leakage: bool,
    /// Precomputed ConstHp pump bool mask: `true` iff link `k` is a `ConstHp` pump (§3.3).
    /// Replaces the per-iteration `LinkKind::Pump` pattern match in the flow
    /// update loop with a single slice read.
    pub(super) is_const_hp_pump: Vec<bool>,
    /// Indices of nodes with a non-zero emitter coefficient (§3.6).
    /// Only these nodes participate in emitter coefficient assembly / flow update.
    pub(super) emitter_node_indices: Vec<usize>,
    /// Indices of nodes with any non-zero FAVAD leakage coefficient (§3.6).
    /// Only these nodes participate in leakage assembly / flow update.
    pub(super) favad_node_indices: Vec<usize>,
    /// Indices of junction nodes with at least one non-zero base demand (§3.6).
    /// Used as the candidate set for PDA demand coefficient assembly / flow update.
    pub(super) pda_node_indices: Vec<usize>,
    initialised: bool,
}

impl SolverContext {
    /// Maximum flow rate for pump at link index `k` at unit speed (§3.4.2).
    ///
    /// Returns `f64::INFINITY` (no flow limit) if `k` is not a pump or is out
    /// of range, matching the constant-power pump convention: with
    /// $Q_{\max} = \infty$ the out-of-range (XFLOW) check never triggers.
    pub fn pump_qmax(&self, k: usize) -> f64 {
        self.pump_qmax_inner
            .get(k)
            .copied()
            .unwrap_or(f64::INFINITY)
    }

    /// Re-derive the head-loss resistance coefficient for link `k` from the
    /// network's current pipe data (§3.2).
    ///
    /// Called once per link at context build and again by the session API when
    /// a pipe's roughness is mutated (`../simulation/spec.md` §8.3) so the
    /// next hydraulic solve uses the updated value. Non-pipe links keep a
    /// zero resistance.
    pub(crate) fn refresh_pipe_resistance(&mut self, network: &Network, k: usize) {
        if let LinkKind::Pipe(pipe) = &network.links[k].kind {
            self.pipe_r[k] = pipe_resistance(
                pipe.length,
                pipe.diameter,
                pipe.roughness,
                network.options.head_loss_formula,
            );
        }
    }

    /// Re-derive the elevation-dependent precomputes for node `i` from the
    /// network's current data: the elevation snapshot used for valve setpoint
    /// conversion (§3.5, §3.9) and the per-node `h_min`/`h_max` tank level
    /// limits (§2.10). Non-tanks and empty tanks get sentinels so neither
    /// tank-level condition fires; overflow tanks keep `h_max = INFINITY` so
    /// the "full tank" branch never fires.
    ///
    /// Called once per node at context build and again by the session API when
    /// a node's elevation is mutated (`../simulation/spec.md` §8.3).
    pub(crate) fn refresh_node_elevation(&mut self, network: &Network, i: usize) {
        let node = &network.nodes[i];
        self.node_elevations[i] = node.base.elevation;
        self.node_h_min[i] = f64::NEG_INFINITY;
        self.node_h_max[i] = f64::INFINITY;
        if let NodeKind::Tank(t) = &node.kind {
            if t.diameter > 0.0 || t.volume_curve.is_some() {
                self.node_h_min[i] = t.head_from_level(node.base.elevation, t.min_level);
                if !t.overflow {
                    self.node_h_max[i] = t.head_from_level(node.base.elevation, t.max_level);
                }
            }
        }
    }
}

const NO_JUNCTION: usize = usize::MAX;

/// Builds the solver context from the validated network (§3.6 Phase 1 + 2).
///
/// Call once after network load. The returned context is reused for all steps.
pub fn build_solver_context(
    network: &Network,
    favad: &FavadCoeffs,
) -> Result<SolverContext, HydraulicError> {
    let n_nodes = network.nodes.len();
    let n_links = network.links.len();

    let mut node_junc_idx = vec![NO_JUNCTION; n_nodes];
    let mut junc_nodes: Vec<usize> = Vec::new();
    for (i, node) in network.nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Junction(_)) {
            node_junc_idx[i] = junc_nodes.len();
            junc_nodes.push(i);
        }
    }
    let node_junc_step_opt: Vec<Option<usize>> = (0..n_nodes)
        .map(|i| {
            if node_junc_idx[i] == NO_JUNCTION {
                None
            } else {
                Some(node_junc_idx[i])
            }
        })
        .collect();
    let junc_count = junc_nodes.len();

    let default_pattern_idx = network
        .options
        .default_pattern
        .as_ref()
        .and_then(|id| network.pattern_index.get(id).copied());
    let mut junction_demand_terms: Vec<Vec<DemandTerm>> = vec![Vec::new(); n_nodes];
    for (i, node) in network.nodes.iter().enumerate() {
        if let NodeKind::Junction(j) = &node.kind {
            let terms = &mut junction_demand_terms[i];
            terms.reserve(j.demands.len());
            for d in &j.demands {
                let pattern_idx = d
                    .pattern
                    .as_ref()
                    .and_then(|id| network.pattern_index.get(id).copied())
                    .or(default_pattern_idx);
                terms.push(DemandTerm {
                    base_demand: d.base_demand,
                    pattern_idx,
                });
            }
        }
    }

    let curve_id_to_index: std::collections::HashMap<&str, usize> = network
        .curves
        .iter()
        .enumerate()
        .map(|(idx, curve)| (curve.id.as_str(), idx))
        .collect();

    let mut pump_coeffs: Vec<Option<PumpCoeffs>> = vec![None; n_links];
    let mut pump_curve_idx: Vec<Option<usize>> = vec![None; n_links];
    for (k, link) in network.links.iter().enumerate() {
        if let LinkKind::Pump(pump) = &link.kind {
            if let Some(curve_id) = &pump.head_curve {
                pump_curve_idx[k] = curve_id_to_index.get(curve_id.as_str()).copied();
            }
            if matches!(
                pump.curve_type,
                PumpCurveType::PowerFunction | PumpCurveType::Custom
            ) {
                if let Some(curve_id) = &pump.head_curve {
                    if let Some(curve_idx) = curve_id_to_index.get(curve_id.as_str()).copied() {
                        let curve = &network.curves[curve_idx];
                        pump_coeffs[k] = fit_pump_coeffs(curve);
                    }
                }
            }
        }
    }

    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); junc_count];
    for link in &network.links {
        let n1 = link.base.from_idx();
        let n2 = link.base.to_idx();
        let ji1 = node_junc_idx[n1];
        let ji2 = node_junc_idx[n2];
        if ji1 != NO_JUNCTION && ji2 != NO_JUNCTION && ji1 != ji2 {
            adj[ji1].insert(ji2);
            adj[ji2].insert(ji1);
        }
    }

    let sparse = SparseSolver::new(junc_count, &adj);

    // §2.8 Compute link_aij_pos via direct CSC scan instead of a HashMap,
    // eliminating the heap allocation and hash-lookup overhead at startup.
    let mut link_aij_pos: Vec<Option<usize>> = vec![None; n_links];
    for (k, link) in network.links.iter().enumerate() {
        let n1 = link.base.from_idx();
        let n2 = link.base.to_idx();
        let ji1 = node_junc_idx[n1];
        let ji2 = node_junc_idx[n2];
        if ji1 != NO_JUNCTION && ji2 != NO_JUNCTION {
            let p1 = sparse.row[ji1];
            let p2 = sparse.row[ji2];
            let (col, row_s) = if p1 < p2 { (p1, p2) } else { (p2, p1) };
            // Walk column `col` in the CSC structure to find `row_s`.
            for pos in sparse.xlnz[col]..sparse.xlnz[col + 1] {
                if sparse.nzsub[pos] == row_s {
                    link_aij_pos[k] = Some(pos);
                    break;
                }
            }
        }
    }

    let link_from: Vec<usize> = network.links.iter().map(|l| l.base.from_idx()).collect();
    let link_to: Vec<usize> = network.links.iter().map(|l| l.base.to_idx()).collect();

    let mut emitter_flows = vec![0.0f64; n_nodes];
    for (i, node) in network.nodes.iter().enumerate() {
        if let NodeKind::Junction(j) = &node.kind {
            if j.emitter_coeff > 0.0 {
                emitter_flows[i] = 1.0;
            }
        }
    }

    let mut leakage_fa_flows = vec![0.0f64; n_nodes];
    let mut leakage_va_flows = vec![0.0f64; n_nodes];
    for i in 0..n_nodes {
        if favad.c_fa[i] > 0.0 {
            leakage_fa_flows[i] = 1.0;
        }
        if favad.c_va[i] > 0.0 {
            leakage_va_flows[i] = 1.0;
        }
    }

    let pda_demand_flows = vec![0.0f64; n_nodes];

    let mut pump_qmax_inner = vec![f64::INFINITY; n_links];
    for (k, link) in network.links.iter().enumerate() {
        if let LinkKind::Pump(pump) = &link.kind {
            pump_qmax_inner[k] = match pump.curve_type {
                PumpCurveType::PowerFunction => {
                    if let Some(c) = &pump_coeffs[k] {
                        (c.h0 / c.r).powf(1.0 / c.n)
                    } else {
                        f64::INFINITY
                    }
                }
                PumpCurveType::Custom => {
                    if let Some(curve_idx) = pump_curve_idx[k] {
                        network.curves[curve_idx]
                            .points
                            .last()
                            .map_or(f64::INFINITY, |pt| pt.x)
                    } else {
                        f64::INFINITY
                    }
                }
                PumpCurveType::ConstHp => f64::INFINITY,
            };
        }
    }

    // §3.7 Precompute has_leakage: avoids per-convergence O(n_nodes) scan.
    let has_leakage = favad.c_fa.iter().chain(favad.c_va.iter()).any(|&c| c > 0.0);

    // §3.3 Precompute ConstHp pump bool mask: replaces per-iteration
    // LinkKind::Pump pattern match in the flow update loop.
    let is_const_hp_pump: Vec<bool> = network
        .links
        .iter()
        .map(|l| matches!(&l.kind, LinkKind::Pump(p) if p.curve_type == PumpCurveType::ConstHp))
        .collect();

    // §3.6 Precompute demand/leakage node index lists to avoid walking all
    // nodes in the per-Newton-iteration demand assembly and update functions.
    let emitter_node_indices: Vec<usize> = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            if let NodeKind::Junction(j) = &n.kind {
                if j.emitter_coeff > 0.0 {
                    return Some(i);
                }
            }
            None
        })
        .collect();

    let favad_node_indices: Vec<usize> = (0..n_nodes)
        .filter(|&i| favad.c_fa[i] > 0.0 || favad.c_va[i] > 0.0)
        .collect();

    let pda_node_indices: Vec<usize> = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            if let NodeKind::Junction(j) = &n.kind {
                if j.demands.iter().any(|d| d.base_demand.abs() > 0.0) {
                    return Some(i);
                }
            }
            None
        })
        .collect();

    let mut ctx = SolverContext {
        node_junc_step_opt,
        junc_nodes,
        junction_demand_terms,
        pipe_r: vec![0.0; n_links],
        pump_coeffs,
        pump_curve_idx,
        pump_qmax_inner,
        link_aij_pos,
        link_from,
        link_to,
        sparse,
        p: vec![0.0; n_links],
        y: vec![0.0; n_links],
        node_heads: vec![0.0; n_nodes],
        node_elevations: vec![0.0; n_nodes],
        junction_demands: vec![0.0; n_nodes],
        xflow: vec![0.0; n_nodes],
        prev_flows: vec![0.0; n_links],
        flows: vec![0.0; n_links],
        statuses: vec![LinkStatus::Open; n_links],
        settings: vec![0.0; n_links],
        emitter_flows,
        prev_emitter_flows: vec![0.0; n_nodes],
        leakage_fa_flows,
        leakage_va_flows,
        prev_leakage_flows: vec![0.0; n_nodes],
        pda_demand_flows,
        prev_pda_demand_flows: vec![0.0; n_nodes],
        net_flow_accum: vec![0.0; n_nodes],
        node_h_min: vec![f64::NEG_INFINITY; n_nodes],
        node_h_max: vec![f64::INFINITY; n_nodes],
        has_leakage,
        is_const_hp_pump,
        emitter_node_indices,
        favad_node_indices,
        pda_node_indices,
        initialised: false,
    };

    // Fill the pipe resistances (§3.2) and elevation-derived precomputes
    // (§2.10 tank h_min/h_max, valve elevation datum) through the same
    // refresh path used by the session API on property mutation
    // (`../simulation/spec.md` §8.3), so both derive from one formula.
    for k in 0..n_links {
        ctx.refresh_pipe_resistance(network, k);
    }
    for i in 0..n_nodes {
        ctx.refresh_node_elevation(network, i);
    }

    Ok(ctx)
}

/// Solves the hydraulic equations for one time step (§3).
///
/// Updates `node_states` and `link_states` in-place. Simple controls must be
/// applied by the caller *before* this function. Post-convergence re-evaluation
/// of junction-head-triggered simple controls (§3.8 "pswitch") is performed
/// internally at convergence.
pub fn solve_hydraulic_step(
    network: &Network,
    favad: &FavadCoeffs,
    ctx: &mut SolverContext,
    node_states: &mut [NodeState],
    link_states: &mut [LinkState],
    t: f64,
    pswitch_fn: fn(&Network, &[NodeState], &mut [LinkStatus], &mut [f64]) -> bool,
) -> Result<SolveResult, HydraulicError> {
    let options = &network.options;
    let n_nodes = network.nodes.len();
    let timing_enabled = SolvePhaseTimings::enabled();
    let mut timings = SolvePhaseTimings::default();
    let setup_started = timing_enabled.then(Instant::now);

    for (i, node) in network.nodes.iter().enumerate() {
        ctx.node_heads[i] = match &node.kind {
            NodeKind::Junction(_) => node_states[i].head,
            NodeKind::Reservoir(r) => {
                let h = r.head(
                    node.base.elevation,
                    t,
                    options,
                    &network.patterns,
                    &network.pattern_index,
                );
                // Reservoir head is fixed for the entire hydraulic step; write
                // it once here so the per-iteration head-extract loop doesn't
                // need to re-copy it after every Cholesky solve.
                node_states[i].head = h;
                h
            }
            NodeKind::Tank(_) => node_states[i].head,
        };
    }

    for (k, ls) in link_states.iter().enumerate() {
        ctx.flows[k] = ls.flow;
        ctx.statuses[k] = ls.status;
        ctx.settings[k] = ls.setting;
    }
    if !ctx.initialised {
        initialise_flows(
            network,
            &mut ctx.flows,
            &ctx.statuses,
            &ctx.settings,
            &ctx.pump_coeffs,
            &ctx.pump_curve_idx,
        );
        ctx.initialised = true;
    }

    if let Some(started) = setup_started {
        timings.setup += started.elapsed();
    }

    let phase_started = timing_enabled.then(Instant::now);
    for (i, demand) in ctx.junction_demands.iter_mut().enumerate() {
        let terms = &ctx.junction_demand_terms[i];
        if terms.is_empty() {
            *demand = 0.0;
            continue;
        }

        let mut total = 0.0;
        for term in terms {
            let factor = match term.pattern_idx {
                Some(pi) => {
                    network.patterns[pi].eval(t, options.pattern_step, options.pattern_start)
                }
                None => 1.0,
            };
            total += term.base_demand * factor;
        }
        *demand = total * options.demand_multiplier;
    }
    if let Some(started) = phase_started {
        timings.demand += started.elapsed();
    }

    let mut result = SolveResult::Unbalanced;
    let mut status_frozen = false;
    let is_pda = matches!(options.demand_model, DemandModel::PressureDriven);
    let max_total = options.max_iter as usize
        + if options.extra_iter > 0 {
            options.extra_iter as usize
        } else {
            0
        };

    // EPANET uses a counter (nextcheck) incremented by CheckFreq after each
    // periodic check, and reset to iter + CheckFreq after convergence with a
    // status change.  A simple modulo can diverge from this sequence after
    // intermediate convergence events, so we replicate the counter.
    let mut next_check = options.check_freq as usize;

    for iter in 1..=max_total {
        timings.iterations = iter;
        let phase_started = timing_enabled.then(Instant::now);
        compute_py_coeffs(network, ctx)?;
        if let Some(started) = phase_started {
            timings.py += started.elapsed();
        }

        let phase_started = timing_enabled.then(Instant::now);
        ctx.sparse.clear();
        assemble_links(
            network,
            &mut ctx.sparse,
            &ctx.link_aij_pos,
            &ctx.node_junc_step_opt,
            &ctx.p,
            &ctx.y,
            &ctx.flows,
            &ctx.node_heads,
            &mut ctx.xflow,
            &ctx.link_from,
            &ctx.link_to,
        );

        apply_emitter_coeffs(
            network,
            &mut ctx.sparse,
            &ctx.node_junc_step_opt,
            &ctx.emitter_node_indices,
            &ctx.emitter_flows,
            &mut ctx.xflow,
        );

        apply_favad_leakage_coeffs(
            network,
            &mut ctx.sparse,
            &ctx.node_junc_step_opt,
            favad,
            &ctx.favad_node_indices,
            &ctx.leakage_fa_flows,
            &ctx.leakage_va_flows,
            &mut ctx.xflow,
        );

        if is_pda {
            apply_pda_demand_coeffs(
                network,
                &mut ctx.sparse,
                &ctx.node_junc_step_opt,
                &ctx.pda_node_indices,
                &ctx.junction_demands,
                &ctx.pda_demand_flows,
                &mut ctx.xflow,
                options.pda_min_pressure,
                options.pda_required_pressure,
                options.pda_pressure_exponent,
            );
        }

        assemble_node_residuals(
            network,
            &mut ctx.sparse,
            &ctx.node_junc_step_opt,
            &ctx.junction_demands,
            &mut ctx.xflow,
        );

        apply_valve_coefficients(
            network,
            &mut ctx.sparse,
            &ctx.node_junc_step_opt,
            &mut ctx.p,
            &mut ctx.y,
            &ctx.flows,
            &ctx.statuses,
            &ctx.settings,
            &ctx.node_elevations,
            &ctx.xflow,
            &ctx.link_aij_pos,
            &ctx.node_heads,
        );
        if let Some(started) = phase_started {
            timings.assembly += started.elapsed();
        }

        let phase_started = timing_enabled.then(Instant::now);
        if let Err(step) = ctx.sparse.factorize_solve() {
            // `step` is the elimination step at which Cholesky broke down.
            // Map it back through the inverse permutation (`row[ji]` = the
            // elimination step of junction ji) to the original junction, then
            // to the network node index for the bad-valve check.
            let node_idx = ctx
                .sparse
                .row
                .iter()
                .position(|&perm| perm == step)
                .and_then(|ji| ctx.junc_nodes.get(ji).copied());
            let demoted = if let Some(ni) = node_idx {
                bad_valve(network, &mut ctx.statuses, ni)
            } else {
                false
            };
            if demoted {
                continue;
            }
            return Err(HydraulicError::SingularMatrix {
                junction_step: step,
            });
        }
        if let Some(started) = phase_started {
            timings.linear_solve += started.elapsed();
            timings.sparse_reset += ctx.sparse.last_timings.reset;
            timings.sparse_factor += ctx.sparse.last_timings.factor;
            timings.sparse_forward += ctx.sparse.last_timings.forward;
            timings.sparse_backward += ctx.sparse.last_timings.backward;
        }

        let phase_started = timing_enabled.then(Instant::now);
        for (ji, &node_i) in ctx.junc_nodes.iter().enumerate() {
            let perm = ctx.sparse.row[ji];
            let h = ctx.sparse.f[perm];
            ctx.node_heads[node_i] = h;
            node_states[node_i].head = h;
        }
        // Reservoir heads are written once in the setup pass above and do not
        // change within the Newton loop — no secondary copy is needed here.
        if let Some(started) = phase_started {
            timings.head_extract += started.elapsed();
        }

        let phase_started = timing_enabled.then(Instant::now);
        let flow_result = update_flows_and_check(
            &ctx.p,
            &ctx.y,
            &ctx.node_heads,
            &mut ctx.flows,
            &mut ctx.prev_flows,
            &ctx.statuses,
            &ctx.is_const_hp_pump,
            options.head_error_limit,
            options.flow_change_limit,
            &ctx.link_from,
            &ctx.link_to,
        );

        ctx.prev_emitter_flows.copy_from_slice(&ctx.emitter_flows);
        let (emit_qsum, emit_dqsum) = update_emitter_flows(
            network,
            &ctx.node_heads,
            &ctx.emitter_node_indices,
            &mut ctx.emitter_flows,
        );

        for i in 0..n_nodes {
            ctx.prev_leakage_flows[i] = ctx.leakage_fa_flows[i] + ctx.leakage_va_flows[i];
        }
        let (leak_qsum, leak_dqsum) = update_leakage_flows(
            network,
            &ctx.node_heads,
            favad,
            &ctx.favad_node_indices,
            &mut ctx.leakage_fa_flows,
            &mut ctx.leakage_va_flows,
        );

        ctx.prev_pda_demand_flows
            .copy_from_slice(&ctx.pda_demand_flows);
        let (pda_qsum, pda_dqsum) = if is_pda {
            update_pda_demand_flows(
                network,
                &ctx.node_heads,
                &ctx.pda_node_indices,
                &ctx.junction_demands,
                &mut ctx.pda_demand_flows,
                options.pda_min_pressure,
                options.pda_required_pressure,
                options.pda_pressure_exponent,
            )
        } else {
            (0.0, 0.0)
        };
        if let Some(started) = phase_started {
            timings.updates += started.elapsed();
        }

        let s_q: f64 = flow_result.link_sq + emit_qsum + pda_qsum + leak_qsum;
        let ds_q: f64 = flow_result.link_dsq + emit_dqsum + pda_dqsum + leak_dqsum;
        let rel_err = if s_q > options.flow_tol {
            ds_q / s_q
        } else {
            ds_q
        };
        let flow_converged = rel_err <= options.flow_tol;
        let converged = flow_converged && flow_result.head_ok && flow_result.flow_ok;

        let phase_started = timing_enabled.then(Instant::now);
        let valve_changed = if !status_frozen {
            check_valve_status(
                network,
                &mut ctx.statuses,
                &ctx.settings,
                &ctx.flows,
                node_states,
                &ctx.node_elevations,
                options.head_tol,
                options.flow_change_tol,
            )
        } else {
            false
        };

        let do_link_status = if converged && !status_frozen {
            // EPANET: when converged, always run linkstatus (inside the
            // hasconverged branch, not the periodic-check branch).
            true
        } else if !status_frozen && iter <= options.max_check as usize && iter == next_check {
            // EPANET: periodic check at nextcheck counter.
            next_check += options.check_freq as usize;
            true
        } else {
            false
        };
        let link_changed = if do_link_status {
            check_link_status(
                network,
                &mut ctx.statuses,
                &ctx.settings,
                &ctx.flows,
                node_states,
                &ctx.node_h_min,
                &ctx.node_h_max,
                options.head_tol,
                options.flow_change_tol,
                &ctx.pump_coeffs,
            )
        } else {
            false
        };

        let status_changed = valve_changed || link_changed;
        if let Some(started) = phase_started {
            timings.status_checks += started.elapsed();
        }

        if converged && !status_changed {
            if ctx.has_leakage
                && !leakage_converged(
                    network,
                    &ctx.node_heads,
                    favad,
                    &ctx.favad_node_indices,
                    &ctx.leakage_fa_flows,
                    &ctx.leakage_va_flows,
                )
            {
                continue;
            }

            if status_frozen {
                result = SolveResult::Converged;
                break;
            }
            let pswitch_changed =
                pswitch_fn(network, node_states, &mut ctx.statuses, &mut ctx.settings);
            if !pswitch_changed {
                result = SolveResult::Converged;
                break;
            }
            // pswitch changed — fall through and reset nextcheck.
        }

        // EPANET: when converged with any status change (valve, link, or
        // pswitch), reset nextcheck = iter + CheckFreq.  We only reach here
        // if some status change prevented the break above.
        if converged {
            next_check = iter + options.check_freq as usize;
        }

        if iter >= options.max_iter as usize {
            if options.extra_iter < 0 {
                break;
            }
            status_frozen = true;
        }
    }

    let phase_started = timing_enabled.then(Instant::now);
    for (k, ls) in link_states.iter_mut().enumerate() {
        ls.flow = ctx.flows[k];
        ls.status = ctx.statuses[k];
        ls.setting = ctx.settings[k];
    }

    // Single O(n_links) pass: accumulate signed net flow at every node.
    // Only tank/reservoir entries are read below, but we accumulate all nodes
    // to avoid a separate branch per link.
    ctx.net_flow_accum.fill(0.0);
    for (k, link) in network.links.iter().enumerate() {
        if matches!(
            ctx.statuses[k],
            LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
        ) {
            continue;
        }
        let flow = ctx.flows[k];
        ctx.net_flow_accum[link.base.from_idx()] -= flow;
        ctx.net_flow_accum[link.base.to_idx()] += flow;
    }

    // Fused node-state write: one pass over all nodes.
    for (i, node) in network.nodes.iter().enumerate() {
        match &node.kind {
            NodeKind::Junction(_) => {
                node_states[i].demand_flow = if is_pda && ctx.junction_demands[i] > 0.0 {
                    ctx.pda_demand_flows[i]
                } else {
                    ctx.junction_demands[i]
                };
                node_states[i].emitter_flow = ctx.emitter_flows[i];
                node_states[i].leakage_flow = ctx.leakage_fa_flows[i] + ctx.leakage_va_flows[i];
            }
            NodeKind::Reservoir(_) | NodeKind::Tank(_) => {
                node_states[i].net_flow = ctx.net_flow_accum[i];
            }
        }
    }
    if let Some(started) = phase_started {
        timings.post += started.elapsed();
        timings.emit(t, ctx.junc_nodes.len(), network.links.len());
    }

    Ok(result)
}

/// Computes P/Y linearisation coefficients for all links (§3.3).
fn compute_py_coeffs(network: &Network, ctx: &mut SolverContext) -> Result<(), HydraulicError> {
    let options = &network.options;
    let formula = options.head_loss_formula;
    let viscosity = options.viscosity;
    let sp_grav = options.specific_gravity;
    let curves = &network.curves;
    let links = &network.links;

    // Split borrows: take immutable refs to inputs and mutable refs to outputs.
    // This lets the compiler prove the slices don't alias, eliding per-element
    // bounds checks across all zip'd iterators (§2.4).
    let flows = &ctx.flows;
    let statuses = &ctx.statuses;
    let settings = &ctx.settings;
    let pipe_r = &ctx.pipe_r;
    let pump_coeffs = &ctx.pump_coeffs;
    let pump_curve_idx = &ctx.pump_curve_idx;
    let p = &mut ctx.p;
    let y = &mut ctx.y;

    for (((((((p_out, y_out), link), flow), status), setting), pr_k), (pc_k, curve_idx)) in p
        .iter_mut()
        .zip(y.iter_mut())
        .zip(links.iter())
        .zip(flows.iter())
        .zip(statuses.iter())
        .zip(settings.iter())
        .zip(pipe_r.iter())
        .zip(pump_coeffs.iter().zip(pump_curve_idx.iter()))
    {
        let py = link_py(
            link,
            *flow,
            *status,
            *setting,
            *pr_k,
            pc_k,
            curves,
            formula,
            viscosity,
            sp_grav,
            *curve_idx,
            options.rq_tol,
        )?;
        *p_out = py.p;
        *y_out = py.y;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{io::parse, LinkKind, NodeKind};

    fn load_fixture(name: &str) -> Network {
        let path = format!(
            "{}/../../tests/fixtures/{}.inp",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let input = std::fs::read(path).expect("fixture should be readable");
        parse(&input).expect("fixture should parse")
    }

    #[test]
    fn build_solver_context_initialises_emitter_guess_for_emitter_junctions() {
        let network = load_fixture("emitter");
        let favad = network.compute_favad();

        let ctx = build_solver_context(&network, &favad).expect("solver context should build");

        let emitter_nodes: Vec<usize> = network
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| match &node.kind {
                NodeKind::Junction(j) if j.emitter_coeff > 0.0 => Some(idx),
                _ => None,
            })
            .collect();
        assert!(!emitter_nodes.is_empty());
        for idx in emitter_nodes {
            assert_eq!(ctx.emitter_flows[idx], 1.0);
        }
        assert!(!ctx.initialised);
    }

    #[test]
    fn build_solver_context_uses_custom_curve_endpoint_for_pump_qmax() {
        let network = load_fixture("pump_head_curve");
        let favad = network.compute_favad();

        let ctx = build_solver_context(&network, &favad).expect("solver context should build");
        let (pump_idx, expected_qmax) = network
            .links
            .iter()
            .enumerate()
            .find_map(|(idx, link)| match &link.kind {
                LinkKind::Pump(pump) if pump.curve_type == PumpCurveType::Custom => {
                    let curve_id = pump.head_curve.as_ref()?;
                    let curve = network.curves.iter().find(|curve| curve.id == *curve_id)?;
                    Some((idx, curve.points.last()?.x))
                }
                _ => None,
            })
            .expect("fixture should contain a custom pump curve");

        assert_eq!(ctx.pump_qmax(pump_idx), expected_qmax);
        assert_eq!(ctx.link_from.len(), network.links.len());
        assert_eq!(ctx.link_to.len(), network.links.len());

        // Non-pump links and out-of-range indices carry no flow limit.
        for (idx, link) in network.links.iter().enumerate() {
            if !matches!(link.kind, LinkKind::Pump(_)) {
                assert_eq!(ctx.pump_qmax(idx), f64::INFINITY);
            }
        }
        assert_eq!(ctx.pump_qmax(network.links.len()), f64::INFINITY);
    }
}
