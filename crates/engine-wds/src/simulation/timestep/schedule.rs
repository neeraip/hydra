use crate::simulation::controls::{resolve_control_action, TIME_TRIGGER_TOL};
use crate::{LinkKind, LinkState, Network, NodeKind, NodeState, TriggerType};

/// Computes the adaptive hydraulic time step (§5.2).
///
/// Returns $
/// \Delta t = \min(\Delta t_h,\ \Delta t_{\text{report}},\
/// \Delta t_{\text{tank}},\ \Delta t_{\text{pattern}},\ t_{\text{duration}} - t)$,
/// clamped to at least 1 second.
pub(crate) fn adaptive_timestep(t: f64, network: &Network, node_states: &[NodeState]) -> f64 {
    let opts = &network.options;
    let dth = opts.hyd_step;

    let dt_end = opts.duration - t;
    if dt_end <= 0.0 {
        return 0.0;
    }

    let dt_report = if opts.report_step > 0.0 {
        let next = (t / opts.report_step).ceil() * opts.report_step;
        let dt = next - t;
        if dt > 0.0 {
            dt
        } else {
            opts.report_step
        }
    } else {
        dth
    };

    let dt_pattern = if opts.pattern_step > 0.0 {
        let shifted = t + opts.pattern_start;
        let next_boundary = (shifted / opts.pattern_step).ceil() * opts.pattern_step;
        let dt = next_boundary - shifted;
        if dt > 0.0 {
            dt
        } else {
            opts.pattern_step
        }
    } else {
        dth
    };

    let dt_tank: f64 = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            if let NodeKind::Tank(tank) = &node.kind {
                let ns = &node_states[i];
                let q_net = ns.net_flow;
                if q_net == 0.0 {
                    return None;
                }
                let dv_available = if q_net > 0.0 {
                    let v_max = tank.volume_from_level(tank.max_level, &network.curves);
                    (v_max - ns.volume).max(0.0)
                } else {
                    let v_min = tank.volume_from_level(tank.min_level, &network.curves);
                    (ns.volume - v_min).max(0.0)
                };
                let dt_tank_i = (dv_available / q_net.abs()).round();
                if dt_tank_i <= 0.0 {
                    return None;
                }
                Some(dt_tank_i)
            } else {
                None
            }
        })
        .fold(dth, f64::min);

    let dt = dth.min(dt_report).min(dt_tank).min(dt_pattern);
    dt.max(1.0).min(dt_end)
}

/// Near-zero flow guard (m³/s) used when predicting tank fill/drain times for
/// control timestep scheduling. Skips division when net flow is negligibly small.
/// Not the same as the quality engine's Q_STAG (3.154e-7 m³/s, the SI equivalent
/// of EPANET's QZERO = 1.114e-5 ft³/s); that threshold governs link stagnation
/// for quality transport purposes (§6.3.1 of quality/spec.md).
const QZERO: f64 = 1.0e-6;

/// Computes the shortest time until a simple control fires and changes a
/// link's status or setting (§5.2.1).
pub(crate) fn control_timestep(
    t: f64,
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
) -> f64 {
    let clock_start = network.options.start_clocktime;
    let mut dt_min = f64::INFINITY;

    for ctrl in &network.controls {
        if !ctrl.enabled {
            continue;
        }
        let mut tc: f64 = 0.0;

        match ctrl.trigger_type {
            TriggerType::HiLevel | TriggerType::LowLevel => {
                let (node_1based, grade) = match (ctrl.trigger_node, ctrl.trigger_grade) {
                    (Some(n), Some(g)) => (n, g),
                    _ => continue,
                };
                if node_1based < 1 || node_1based > network.nodes.len() {
                    continue;
                }
                let node_idx = node_1based - 1;
                let node = &network.nodes[node_idx];
                let tank = match &node.kind {
                    NodeKind::Tank(t) => t,
                    _ => continue,
                };

                let h = node_states[node_idx].head;
                let q = node_states[node_idx].net_flow;
                if q.abs() <= QZERO {
                    continue;
                }

                let approaching = match ctrl.trigger_type {
                    TriggerType::HiLevel => h < grade && q > 0.0,
                    TriggerType::LowLevel => h > grade && q < 0.0,
                    _ => false,
                };
                if !approaching {
                    continue;
                }

                let bottom = tank.bottom_elevation(node.base.elevation);
                let level_at_grade = grade - bottom;
                let v_at_grade = tank.volume_from_level(level_at_grade, &network.curves);
                let v_current = node_states[node_idx].volume;
                let v_diff = v_at_grade - v_current;
                tc = (v_diff / q).round();
            }
            TriggerType::Timer => {
                if let Some(trigger_time) = ctrl.trigger_time {
                    // §5.2.1: a trigger within ε_t of `t` fires at `t` itself
                    // (§4.1), so only strictly future triggers schedule a step.
                    if trigger_time > t + TIME_TRIGGER_TOL {
                        tc = trigger_time - t;
                    }
                }
            }
            TriggerType::TimeOfDay => {
                if let Some(trigger_time) = ctrl.trigger_time {
                    let t1 = (t + clock_start) % 86400.0;
                    let t2 = trigger_time;
                    tc = if t2 >= t1 { t2 - t1 } else { 86400.0 - t1 + t2 };
                    // §5.2.1: the current occurrence fires at `t` itself —
                    // schedule the next one a day away.
                    if tc <= TIME_TRIGGER_TOL {
                        tc = 86400.0;
                    }
                }
            }
        }

        if tc <= 0.0 {
            continue;
        }

        let link_idx = ctrl.link;
        if link_idx < 1 || link_idx > link_states.len() {
            continue;
        }
        let ls = &link_states[link_idx - 1];
        let link = &network.links[link_idx - 1];
        let is_pump_or_pipe = matches!(link.kind, LinkKind::Pipe(_) | LinkKind::Pump(_));
        let is_pump = matches!(link.kind, LinkKind::Pump(_));
        let is_valve = matches!(link.kind, LinkKind::Valve(_));
        let (eff_status, eff_setting) = resolve_control_action(
            ctrl.action_status,
            ctrl.action_setting,
            is_pump_or_pipe,
            is_pump,
            is_valve,
        );
        let would_change = eff_status.is_some_and(|s| s != ls.status)
            || eff_setting.is_some_and(|s| s != ls.setting);
        if !would_change {
            continue;
        }

        if tc < dt_min {
            dt_min = tc;
        }
    }

    dt_min
}

#[cfg(test)]
mod tests {
    use super::adaptive_timestep;
    use crate::{
        DemandCategory, Junction, Link, LinkBase, LinkKind, LinkStatus, MixModel, Network, Node,
        NodeBase, NodeKind, NodeState, Pipe, SimulationOptions, Tank,
    };

    fn cylindrical_tank_node(
        index: usize,
        elevation: f64,
        min_level: f64,
        max_level: f64,
        init_level: f64,
        diameter: f64,
        overflow: bool,
    ) -> Node {
        Node {
            base: NodeBase {
                id: format!("T{index}"),
                index,
                elevation,
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level,
                max_level,
                initial_level: init_level,
                diameter,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow,
                head_pattern: None,
            }),
            source: None,
        }
    }

    fn junction_node(index: usize) -> Node {
        Node {
            base: NodeBase {
                id: format!("J{index}"),
                index,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![DemandCategory {
                    base_demand: 0.0,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        }
    }

    fn dummy_link(index: usize) -> Link {
        Link {
            base: LinkBase {
                id: format!("P{index}"),
                index,
                from_node: 1,
                to_node: 1,
                initial_status: LinkStatus::Open,
                initial_setting: Some(1.0),
            },
            kind: LinkKind::Pipe(Pipe {
                length: 1000.0,
                diameter: 1.0,
                roughness: 100.0,
                minor_loss: 0.0,
                check_valve: false,
                bulk_coeff: None,
                wall_coeff: None,
                leak_coeff_1: 0.0,
                leak_coeff_2: 0.0,
            }),
        }
    }

    fn network_with_tank(opts: SimulationOptions, tank_node: Node) -> (Network, Vec<NodeState>) {
        let n_nodes = tank_node.base.index;
        let net = Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![tank_node],
            links: vec![dummy_link(1)],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states: Vec<NodeState> = (0..n_nodes).map(|_| NodeState::default()).collect();
        (net, node_states)
    }

    #[test]
    fn adaptive_dt_bounded_by_hyd_step() {
        let opts = SimulationOptions {
            duration: 86400.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let net = Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![junction_node(1)],
            links: vec![dummy_link(1)],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states = vec![NodeState::default()];
        let dt = adaptive_timestep(0.0, &net, &node_states);
        assert!(dt <= 3600.0, "dt={dt} should be ≤ hyd_step=3600");
        assert!(dt >= 1.0, "dt={dt} should be ≥ 1 s");
    }

    #[test]
    fn adaptive_dt_clipped_to_end_of_simulation() {
        let opts = SimulationOptions {
            duration: 100.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let net = Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![junction_node(1)],
            links: vec![dummy_link(1)],
            controls: vec![],
            rules: vec![],
            pattern_index: std::collections::HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        };
        let node_states = vec![NodeState::default()];
        let dt = adaptive_timestep(0.0, &net, &node_states);
        assert_eq!(dt, 100.0, "dt should equal remaining simulation time");
    }

    #[test]
    fn adaptive_dt_constrained_by_tank_fill_time() {
        let a = std::f64::consts::PI * 25.0;
        let v_current = a * 8.0;
        let q_net = 1.0_f64;

        let opts = SimulationOptions {
            duration: 86400.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 8.0, 10.0, false);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: v_current,
            level: 8.0,
            net_flow: q_net,
            ..NodeState::default()
        }];
        let dt = adaptive_timestep(0.0, &net, &node_states);
        let expected_dt_tank = (a * 2.0) / q_net;
        assert!(
            dt <= expected_dt_tank + 1e-9,
            "dt={dt} should be ≤ dt_tank≈{expected_dt_tank}"
        );
        assert!(dt >= 1.0);
    }

    #[test]
    fn adaptive_dt_zero_flow_tank_not_constraining() {
        let opts = SimulationOptions {
            duration: 86400.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 5.0, 10.0, false);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: 0.0,
            level: 5.0,
            net_flow: 0.0,
            ..NodeState::default()
        }];
        let dt = adaptive_timestep(0.0, &net, &node_states);
        assert_eq!(
            dt, 3600.0,
            "tank with zero net_flow should not constrain dt"
        );
    }

    #[test]
    fn control_timestep_ignores_out_of_range_trigger_node() {
        use super::control_timestep;
        use crate::{LinkState, SimpleControl, TriggerType};

        let opts = SimulationOptions {
            duration: 86400.0,
            hyd_step: 3600.0,
            report_step: 3600.0,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 5.0, 10.0, false);
        let (mut net, _) = network_with_tank(opts, tank);
        // trigger_node = 0 previously underflowed `node_1based - 1`; an index
        // past the node list previously panicked on the slice access.
        for bad_node in [0usize, 99] {
            net.controls = vec![SimpleControl {
                link: 1,
                trigger_type: TriggerType::HiLevel,
                trigger_time: None,
                trigger_node: Some(bad_node),
                trigger_grade: Some(5.0),
                action_status: Some(LinkStatus::Closed),
                action_setting: None,
                enabled: true,
            }];
            let node_states = vec![NodeState::default()];
            let link_states = vec![LinkState::default()];
            let dt = control_timestep(0.0, &net, &node_states, &link_states);
            assert!(
                dt.is_infinite(),
                "out-of-range trigger node must be skipped, got dt={dt}"
            );
        }
    }
}
