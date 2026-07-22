use super::{
    mixing::{reservoir_source_conc, stagnant_conc, update_tank_mix},
    shared::*,
};
use crate::{LinkKind, LinkState, Network, NodeKind, NodeState, QualityMode, SourceType};
use std::collections::{HashSet, VecDeque};

/// Builds the per-node adjacency list and computes topological order in
/// O(n_nodes + n_links). The adjacency is reused across all quality sub-steps
/// within a hydraulic period (flow is constant). The topological order is used
/// to process nodes upstream-first during transport.
pub(super) fn rebuild_adjacency_and_topo(
    state: &mut QualityState,
    network: &Network,
    link_states: &[LinkState],
) {
    let n_nodes = network.nodes.len();

    // ── Build adjacency and per-node outgoing-edge list for Kahn's ────────
    //
    // `adjacency[node]` = vec of (link_idx, is_inflow_to_this_node).
    // `outgoing[node]`  = vec of downstream 0-based node indices (for topo).
    let mut adj: Vec<Vec<(usize, bool)>> = vec![Vec::new(); n_nodes];
    let mut in_deg = vec![0usize; n_nodes];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];

    for (k, link) in network.links.iter().enumerate() {
        let flow = link_states[k].flow;
        if flow.abs() < Q_STAG {
            continue;
        }
        let upstream_node_0 = if flow > 0.0 {
            link.base.from_idx()
        } else {
            link.base.to_idx()
        };
        let downstream_node_0 = if flow > 0.0 {
            link.base.to_idx()
        } else {
            link.base.from_idx()
        };
        if downstream_node_0 < n_nodes {
            adj[downstream_node_0].push((k, true));
            in_deg[downstream_node_0] += 1;
        }
        if upstream_node_0 < n_nodes {
            adj[upstream_node_0].push((k, false));
            if downstream_node_0 < n_nodes {
                outgoing[upstream_node_0].push(downstream_node_0);
            }
        }
    }

    // ── Kahn's algorithm using the outgoing adjacency list ────────────────
    let mut queue: VecDeque<usize> = (0..n_nodes).filter(|&i| in_deg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n_nodes);

    while let Some(node_0) = queue.pop_front() {
        order.push(node_0);
        for &ds_0 in &outgoing[node_0] {
            if in_deg[ds_0] > 0 {
                in_deg[ds_0] -= 1;
                if in_deg[ds_0] == 0 {
                    queue.push_back(ds_0);
                }
            }
        }
    }

    // Append any cycle members in original index order (§6.3.1 step 4).
    if order.len() < n_nodes {
        let in_order: HashSet<usize> = order.iter().copied().collect();
        for i in 0..n_nodes {
            if !in_order.contains(&i) {
                order.push(i);
            }
        }
    }

    state.adjacency = adj;
    state.topo_order = order;
}

/// Single-pass transport: for each node in topological order, consume inflow
/// segments, compute the node's outflow concentration, then push new segments
/// into all outflow links.  This matches EPANET's `transport()` structure and
/// allows water to traverse the entire network within one quality sub-step.
///
/// Returns the per-node outflow volume (for source injection stagnation check).
pub(super) fn transport_step(
    state: &mut QualityState,
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    dt: f64,
    t: f64,
    accumulate_rates: bool,
) -> Vec<f64> {
    let n_nodes = network.nodes.len();
    let tol = network.options.quality_tolerance;

    let mut volout_per_node = vec![0.0_f64; n_nodes];

    // Take ownership of the adjacency lists for the duration of this
    // transport step.  The adjacency is read-only (rebuilt only when the
    // flow field changes between hydraulic periods), so this avoids a
    // per-node clone while still satisfying the borrow checker's need for
    // mutable access to other QualityState fields.
    let adjacency = std::mem::take(&mut state.adjacency);

    // Iterate topo_order by index to avoid cloning the Vec.
    let topo_len = state.topo_order.len();
    for ti in 0..topo_len {
        let node_0 = state.topo_order[ti];
        let node = &network.nodes[node_0];
        let node_state = &node_states[node_0];

        // ── Step 1: accumulate inflow from all links entering this node ──
        let mut volin = 0.0_f64;
        let mut massin = 0.0_f64;
        let mut volout = 0.0_f64;

        for &(k, is_inflow) in &adjacency[node_0] {
            let flow = link_states[k].flow;
            let volume = flow.abs() * dt;
            if is_inflow {
                if matches!(network.links[k].kind, LinkKind::Pipe(_)) {
                    let pq = match state.pipe_quality[k].as_mut() {
                        Some(p) => p,
                        None => continue,
                    };
                    let mut swept_vol = 0.0_f64;
                    let mut swept_mass = 0.0_f64;
                    while swept_vol < volume {
                        let remaining = volume - swept_vol;
                        match pq.segments.front_mut() {
                            None => break,
                            Some(seg) => {
                                if seg.volume <= remaining {
                                    swept_vol += seg.volume;
                                    swept_mass += seg.concentration * seg.volume;
                                    pq.segments.pop_front();
                                } else {
                                    swept_mass += seg.concentration * remaining;
                                    swept_vol += remaining;
                                    seg.volume -= remaining;
                                    break;
                                }
                            }
                        }
                    }
                    volin += swept_vol;
                    massin += swept_mass;
                } else {
                    // Non-pipe (pump/valve): instantaneous pass-through of
                    // upstream node concentration (already computed since we
                    // process nodes in topological order).
                    let us_0 = if flow > 0.0 {
                        network.links[k].base.from_idx()
                    } else {
                        network.links[k].base.to_idx()
                    };
                    let c = state.node_conc[us_0];
                    volin += volume;
                    massin += c * volume;
                }
            } else {
                volout += volume;
            }
        }

        // ── Step 2: compute this node's new outflow concentration ─────────
        match &node.kind {
            NodeKind::Junction(_) => {
                // Dilute inflow with any external negative demand (source inflow).
                let neg_demand = (-node_state.demand_flow).max(0.0);
                if neg_demand > 0.0 {
                    volin += neg_demand * dt;
                }
                if volin > 0.0 {
                    state.node_conc[node_0] = massin / volin;
                } else if matches!(
                    network.options.quality_mode,
                    QualityMode::Chemical | QualityMode::Age
                ) {
                    state.node_conc[node_0] = stagnant_conc(
                        node_0,
                        &state.node_links[node_0],
                        network,
                        link_states,
                        &state.pipe_quality,
                    );
                }
                // Add demand outflow.
                let demand_out = node_state.demand_flow.max(0.0) * dt;
                volout += demand_out;
                // §6.9: Track mass removed by consumer demand. A negative
                // demand is an external inflow — its volume already mixed in
                // via `volin` above — so it must not be charged to the
                // withdrawal side of the ledger.
                if demand_out > 0.0 {
                    state.mass_balance.demand += state.node_conc[node_0] * demand_out;
                }
            }
            NodeKind::Reservoir(_) => {
                state.node_conc[node_0] = reservoir_source_conc(
                    node_0,
                    network,
                    node_states,
                    network.options.quality_mode,
                    t,
                );
            }
            NodeKind::Tank(tank) => {
                let c_in = if volin > 0.0 {
                    massin / volin
                } else {
                    state.node_conc[node_0]
                };
                let v_net = node_state.net_flow * dt;
                let mut v_out = (volin - v_net).max(0.0);

                // EPANET tankmix3/4: if tank is full and still filling,
                // all inflow passes through (overflow).
                // Uses pre-computed overflow flag (see advance_quality) to
                // match EPANET's two-phase architecture where the quality
                // engine always sees tank->V from the final hydraulic state.
                if state.tank_overflows[node_0] && v_net > 0.0 {
                    v_out = volin;
                }

                let kb = if tank.bulk_coeff != 0.0 {
                    tank.bulk_coeff
                } else {
                    network.options.bulk_coeff
                };
                let reactive = network.options.quality_mode == QualityMode::Chemical;

                let outflow_conc = update_tank_mix(
                    &mut state.tank_quality[node_0],
                    network,
                    tank,
                    c_in,
                    volin,
                    v_out,
                    v_net,
                    kb,
                    network.options.tank_order,
                    network.options.conc_limit,
                    dt,
                    reactive,
                    &mut state.mass_balance,
                    accumulate_rates,
                );
                state.node_conc[node_0] = outflow_conc;
            }
        }

        volout_per_node[node_0] = volout;

        // ── Step 2b: Apply source injection (EPANET does this before push) ──
        if network.options.quality_mode == QualityMode::Chemical
            || network.options.quality_mode == QualityMode::Trace
        {
            if let Some(src) = &node.source {
                let c_src = src.effective_value(
                    t,
                    &network.options,
                    &network.patterns,
                    &network.pattern_index,
                );
                if c_src != 0.0 && volout > 0.0 {
                    let c = state.node_conc[node_0];
                    let source_qual = match src.kind {
                        SourceType::Concentration => {
                            match &node.kind {
                                NodeKind::Reservoir(_) | NodeKind::Tank(_) => {
                                    // For reservoirs/tanks, source quality IS the node quality.
                                    state.node_conc[node_0] = c_src;
                                    0.0
                                }
                                NodeKind::Junction(_) => {
                                    if node_state.demand_flow < 0.0 {
                                        -c_src * node_state.demand_flow * dt / volout
                                    } else {
                                        0.0
                                    }
                                }
                            }
                        }
                        // c_src is in mg/min; volout is in m³ (SI).
                        // mass_added_mg = (c_src / 60) * dt_s
                        // volume_L = volout * 1000
                        // result in mg/L (same units as node_conc).
                        SourceType::Mass => (c_src / 60.0) * dt / (volout * 1000.0),
                        SourceType::Setpoint => (c_src - c).max(0.0),
                        SourceType::FlowPaced => c_src,
                    };

                    // EPANET findsourcequal(): massadded = csource * volout.
                    // For CONCEN type, csource is the raw concentration (c_src)
                    // regardless of reservoir handling.  For all other types,
                    // csource = source_qual (the computed adjustment).
                    let mass_csource = match src.kind {
                        SourceType::Concentration => c_src,
                        _ => source_qual,
                    };
                    let mass_added = mass_csource * volout;
                    state.mass_balance.added += mass_added.max(0.0);
                    if accumulate_rates {
                        state.mass_balance.source += mass_added.max(0.0);
                    }

                    if source_qual != 0.0 {
                        state.node_conc[node_0] =
                            (state.node_conc[node_0] + source_qual).clamp(0.0, C_MAX);
                    }

                    // For Trace mode on reservoir: set quality to 100 %
                    if network.options.quality_mode == QualityMode::Trace {
                        if let Some(ref trace_id) = network.options.trace_node {
                            if node.base.id == *trace_id {
                                if matches!(node.kind, NodeKind::Reservoir(_)) {
                                    state.node_conc[node_0] = 100.0;
                                } else {
                                    let added = (100.0 - state.node_conc[node_0]).max(0.0);
                                    state.mass_balance.added += added * volout;
                                    state.node_conc[node_0] = 100.0;
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Step 3: push new segments into all outflow pipes ──────────────
        let c = state.node_conc[node_0];
        for &(k, is_inflow) in &adjacency[node_0] {
            if is_inflow {
                continue;
            }
            if !matches!(network.links[k].kind, LinkKind::Pipe(_)) {
                continue; // pumps/valves have no segments
            }
            let pq = match state.pipe_quality[k].as_mut() {
                Some(p) => p,
                None => continue,
            };
            let v_new = link_states[k].flow.abs() * dt;
            push_segment_merge(
                &mut pq.segments,
                Segment {
                    volume: v_new,
                    concentration: c,
                },
                tol,
            );
        }
    }

    state.adjacency = adjacency;
    volout_per_node
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimulationOptions;

    #[test]
    fn segment_merge_within_tolerance() {
        let mut segs: VecDeque<Segment> = VecDeque::new();
        segs.push_back(Segment {
            volume: 10.0,
            concentration: 1.0,
        });
        push_segment_merge(
            &mut segs,
            Segment {
                volume: 5.0,
                concentration: 1.005,
            },
            0.01,
        );
        assert_eq!(segs.len(), 1);
        approx::assert_abs_diff_eq!(segs[0].concentration, 15.025 / 15.0, epsilon = 1e-12);
        approx::assert_abs_diff_eq!(segs[0].volume, 15.0, epsilon = 1e-12);
    }

    #[test]
    fn segment_no_merge_outside_tolerance() {
        let mut segs: VecDeque<Segment> = VecDeque::new();
        segs.push_back(Segment {
            volume: 10.0,
            concentration: 1.0,
        });
        push_segment_merge(
            &mut segs,
            Segment {
                volume: 5.0,
                concentration: 1.1,
            },
            0.01,
        );
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn topo_sort_simple_chain() {
        let opts = SimulationOptions {
            quality_mode: crate::QualityMode::Chemical,
            ..SimulationOptions::default()
        };
        let net = Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![
                super::super::test_support::junction_node(1, 0.0),
                super::super::test_support::junction_node(2, 0.0),
                super::super::test_support::junction_node(3, 0.0),
            ],
            links: vec![
                super::super::test_support::link(
                    1,
                    1,
                    2,
                    super::super::test_support::default_pipe(1000.0, 1.0),
                ),
                super::super::test_support::link(
                    2,
                    2,
                    3,
                    super::super::test_support::default_pipe(1000.0, 1.0),
                ),
            ],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states = vec![NodeState::default(); 3];
        let link_states = vec![
            super::super::test_support::link_state_q(1.0),
            super::super::test_support::link_state_q(1.0),
        ];

        let mut state = super::super::init_quality(&net, &node_states, &link_states).unwrap();
        rebuild_adjacency_and_topo(&mut state, &net, &link_states);

        let pos: Vec<usize> = state.topo_order.clone();
        let p0 = pos.iter().position(|&i| i == 0).unwrap();
        let p1 = pos.iter().position(|&i| i == 1).unwrap();
        let p2 = pos.iter().position(|&i| i == 2).unwrap();
        assert!(p0 < p1);
        assert!(p1 < p2);
    }
}
