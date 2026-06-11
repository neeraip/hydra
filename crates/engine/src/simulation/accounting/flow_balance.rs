use crate::{DemandModel, Network, NodeKind, NodeState};

use super::FlowBalance;

pub(super) fn accumulate_flow_balance(
    flow_balance: &mut FlowBalance,
    network: &Network,
    node_states: &[NodeState],
    dt: f64,
    t: f64,
    overflow_volume: f64,
) {
    let opts = &network.options;

    for (i, node) in network.nodes.iter().enumerate() {
        let ns = &node_states[i];
        match &node.kind {
            NodeKind::Reservoir(_) => {
                if ns.net_flow < 0.0 {
                    flow_balance.total_inflow += ns.net_flow.abs() * dt;
                } else {
                    flow_balance.total_outflow += ns.net_flow * dt;
                }
            }
            NodeKind::Junction(j) => {
                let d = ns.demand_flow;
                if d < 0.0 {
                    flow_balance.total_inflow += d.abs() * dt;
                } else {
                    flow_balance.total_outflow += (d + ns.emitter_flow + ns.leakage_flow) * dt;
                }
                if opts.demand_model == DemandModel::PressureDriven {
                    let d_full = j.total_demand(t, opts, &network.patterns, &network.pattern_index);
                    let deficit = (d_full - d).max(0.0);
                    flow_balance.demand_deficit += deficit * dt;
                }
            }
            NodeKind::Tank(_) => {}
        }
    }

    flow_balance.total_outflow += overflow_volume;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DemandCategory, DemandModel, Junction, Network, Node, NodeBase, NodeKind, NodeState,
        Pattern, Reservoir, SimulationOptions,
    };
    use std::collections::HashMap;

    fn balance_network(demand_model: DemandModel) -> Network {
        let options = SimulationOptions {
            demand_model,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..Default::default()
        };

        let mut pattern_index = HashMap::new();
        pattern_index.insert("pat".to_string(), 0);

        Network {
            title: vec![],
            options,
            patterns: vec![Pattern {
                id: "pat".to_string(),
                factors: vec![2.0],
            }],
            curves: vec![],
            nodes: vec![
                Node {
                    base: NodeBase {
                        id: "R1".to_string(),
                        index: 1,
                        elevation: 0.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Reservoir(Reservoir { head_pattern: None }),
                    source: None,
                },
                Node {
                    base: NodeBase {
                        id: "J1".to_string(),
                        index: 2,
                        elevation: 0.0,
                        initial_quality: 0.0,
                    },
                    kind: NodeKind::Junction(Junction {
                        demands: vec![DemandCategory {
                            base_demand: 3.0,
                            pattern: Some("pat".to_string()),
                            name: None,
                        }],
                        emitter_coeff: 0.0,
                        emitter_exp: 0.5,
                    }),
                    source: None,
                },
            ],
            links: vec![],
            controls: vec![],
            rules: vec![],
            pattern_index,
            report: crate::ReportOptions::default(),
            coordinates: std::collections::HashMap::new(),
            vertices: std::collections::HashMap::new(),
            node_tags: std::collections::HashMap::new(),
            link_tags: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn accumulate_flow_balance_tracks_reservoir_junction_and_overflow_terms() {
        let network = balance_network(DemandModel::DemandDriven);
        let mut flow_balance = FlowBalance {
            total_inflow: 0.0,
            total_outflow: 0.0,
            demand_deficit: 0.0,
            initial_tank_volume: 0.0,
        };
        let node_states = vec![
            NodeState {
                net_flow: -4.0,
                ..NodeState::default()
            },
            NodeState {
                demand_flow: 2.0,
                emitter_flow: 0.5,
                leakage_flow: 0.25,
                ..NodeState::default()
            },
        ];

        accumulate_flow_balance(&mut flow_balance, &network, &node_states, 10.0, 0.0, 7.0);

        assert_eq!(flow_balance.total_inflow, 40.0);
        assert_eq!(flow_balance.total_outflow, 34.5);
        assert_eq!(flow_balance.demand_deficit, 0.0);
    }

    #[test]
    fn accumulate_flow_balance_adds_pressure_driven_demand_deficit() {
        let network = balance_network(DemandModel::PressureDriven);
        let mut flow_balance = FlowBalance {
            total_inflow: 0.0,
            total_outflow: 0.0,
            demand_deficit: 0.0,
            initial_tank_volume: 0.0,
        };
        let node_states = vec![
            NodeState::default(),
            NodeState {
                demand_flow: 4.0,
                ..NodeState::default()
            },
        ];

        accumulate_flow_balance(&mut flow_balance, &network, &node_states, 10.0, 0.0, 0.0);

        assert_eq!(flow_balance.total_outflow, 40.0);
        assert_eq!(flow_balance.demand_deficit, 20.0);
    }
}
