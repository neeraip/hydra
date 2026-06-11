use crate::{Network, NodeKind, NodeState};

/// Result of updating a single tank's level (§5.3).
#[derive(Debug, Clone)]
pub(crate) struct TankUpdate {
    /// 0-based node index of the tank.
    pub node_index: usize,
    /// New hydraulic head after the update (m).
    pub new_head: f64,
    /// New level after the update (m).
    pub new_level: f64,
    /// New volume after the update (m³).
    pub new_volume: f64,
    /// Overflow volume (m³) accumulated during this step, if the tank has
    /// `overflow = true` and water exceeded `max_level`. Zero otherwise.
    pub overflow_volume: f64,
}

/// Updates all tank levels after a time step of duration `dt` (§5.3, ∥).
pub(crate) fn update_tank_levels(
    network: &Network,
    node_states: &[NodeState],
    dt: f64,
) -> Vec<TankUpdate> {
    network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            if let NodeKind::Tank(tank) = &node.kind {
                let ns = &node_states[i];
                let v_new_raw = ns.volume + ns.net_flow * dt;

                let v_min = tank.volume_from_level(tank.min_level, &network.curves);
                let v_max = tank.volume_from_level(tank.max_level, &network.curves);

                let (v_new, overflow_volume) = if v_new_raw < v_min {
                    (v_min, 0.0)
                } else if v_new_raw > v_max {
                    let overflow = if tank.overflow {
                        v_new_raw - v_max
                    } else {
                        0.0
                    };
                    (v_max, overflow)
                } else {
                    let v_next_sec = v_new_raw + ns.net_flow;
                    if ns.net_flow > 0.0 && v_next_sec >= v_max {
                        let overflow = if tank.overflow {
                            (v_new_raw - v_max).max(0.0)
                        } else {
                            0.0
                        };
                        (v_max, overflow)
                    } else {
                        (v_new_raw, 0.0)
                    }
                };

                let h_new_level = tank.level_from_volume(v_new, &network.curves);
                let h_new_head = tank.head_from_level(node.base.elevation, h_new_level);

                Some(TankUpdate {
                    node_index: i,
                    new_head: h_new_head,
                    new_level: h_new_level,
                    new_volume: v_new,
                    overflow_volume,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::update_tank_levels;
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
    fn tank_level_advances_normally() {
        let a = std::f64::consts::PI * 25.0;
        let v0 = a * 5.0;
        let opts = SimulationOptions {
            duration: 86400.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 10.0, 0.0, 20.0, 5.0, 10.0, false);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: v0,
            level: 5.0,
            net_flow: 1.0,
            ..NodeState::default()
        }];
        let updates = update_tank_levels(&net, &node_states, 100.0);
        assert_eq!(updates.len(), 1);
        let u = &updates[0];
        let v_new = v0 + 100.0;
        let level_new = v_new / a;
        approx::assert_abs_diff_eq!(u.new_volume, v_new, epsilon = 1e-9);
        approx::assert_abs_diff_eq!(u.new_level, level_new, epsilon = 1e-9);
        approx::assert_abs_diff_eq!(u.new_head, 10.0 + level_new, epsilon = 1e-9);
        assert_eq!(u.overflow_volume, 0.0);
    }

    #[test]
    fn tank_clamped_at_min_level() {
        let a = std::f64::consts::PI * 25.0;
        let v_min = 0.0;
        let opts = SimulationOptions {
            duration: 86400.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 1.0, 10.0, false);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: a * 1.0,
            level: 1.0,
            net_flow: -1.0,
            ..NodeState::default()
        }];
        let updates = update_tank_levels(&net, &node_states, 10000.0);
        assert_eq!(updates.len(), 1);
        let u = &updates[0];
        approx::assert_abs_diff_eq!(u.new_volume, v_min, epsilon = 1e-9);
        approx::assert_abs_diff_eq!(u.new_level, 0.0, epsilon = 1e-9);
        assert_eq!(u.overflow_volume, 0.0);
    }

    #[test]
    fn tank_overflow_recorded_when_flag_set() {
        let a = std::f64::consts::PI * 25.0;
        let v_max = a * 10.0;
        let v0 = a * 9.0;
        let q = 10.0_f64;
        let dt = 100.0_f64;
        let v_new_raw = v0 + q * dt;
        let expected_overflow = v_new_raw - v_max;

        let opts = SimulationOptions {
            duration: 86400.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 9.0, 10.0, true);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: v0,
            level: 9.0,
            net_flow: q,
            ..NodeState::default()
        }];
        let updates = update_tank_levels(&net, &node_states, dt);
        assert_eq!(updates.len(), 1);
        let u = &updates[0];
        approx::assert_abs_diff_eq!(u.new_volume, v_max, epsilon = 1e-9);
        approx::assert_abs_diff_eq!(u.new_level, 10.0, epsilon = 1e-9);
        approx::assert_abs_diff_eq!(u.overflow_volume, expected_overflow, epsilon = 1e-9);
    }

    #[test]
    fn tank_no_overflow_recorded_when_flag_false() {
        let a = std::f64::consts::PI * 25.0;
        let v_max = a * 10.0;
        let v0 = a * 9.0;

        let opts = SimulationOptions {
            duration: 86400.0,
            ..SimulationOptions::default()
        };
        let tank = cylindrical_tank_node(1, 0.0, 0.0, 10.0, 9.0, 10.0, false);
        let (net, _) = network_with_tank(opts, tank);
        let node_states = vec![NodeState {
            volume: v0,
            level: 9.0,
            net_flow: 100.0,
            ..NodeState::default()
        }];
        let updates = update_tank_levels(&net, &node_states, 1000.0);
        assert_eq!(updates.len(), 1);
        let u = &updates[0];
        approx::assert_abs_diff_eq!(u.new_volume, v_max, epsilon = 1e-9);
        assert_eq!(
            u.overflow_volume, 0.0,
            "overflow=false should never record overflow"
        );
    }

    #[test]
    fn junction_nodes_produce_no_tank_updates() {
        let opts = SimulationOptions {
            duration: 86400.0,
            ..SimulationOptions::default()
        };
        let net = Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![junction_node(1), junction_node(2)],
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
        let node_states = vec![NodeState::default(), NodeState::default()];
        let updates = update_tank_levels(&net, &node_states, 3600.0);
        assert!(updates.is_empty(), "no tanks → no updates");
    }
}
