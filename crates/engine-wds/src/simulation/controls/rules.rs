use crate::{ActionValue, LinkState, Network, NodeState};

use super::premises::evaluate_premises;
use super::simple::resolve_control_action;

/// Evaluates §4.2 rules against the current hydraulic state at time `t`.
///
/// Returns `Some(actions)` when at least one THEN action differs from the
/// current link state (i.e., at least one rule "fires" per §4.2.1 step 3).
/// The returned vector contains the conflict-resolved set of actions (both THEN
/// actions from fired rules and ELSE actions from non-fired rules with ELSE
/// clauses), ready for application by [`apply_link_actions`].
///
/// Returns `None` when no actions are produced at all.
/// When no THEN fires but ELSE actions exist, still returns `Some` so ELSE
/// actions are applied (§4.2.3).
///
/// The boolean flag in the return indicates whether any THEN rule fired
/// (true = terminate sub-step loop; false = continue).
///
/// Conflict resolution (§4.2.3): when two or more actions target the same
/// `(link, attribute)` pair, the action from the rule with the **numerically
/// highest priority value** wins.
pub(crate) fn eval_rules(
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    t: f64,
) -> Option<(Vec<(usize, ActionValue)>, bool)> {
    struct Pending {
        link: usize,
        value: ActionValue,
        priority: f64,
        is_then: bool,
        is_status: bool,
    }

    let pending_cap: usize = network
        .rules
        .iter()
        .map(|r| r.then_actions.len().max(r.else_actions.len()))
        .sum();
    let mut pending: Vec<Pending> = Vec::with_capacity(pending_cap);
    let mut any_then_fire = false;

    for rule in &network.rules {
        let premise_true = evaluate_premises(&rule.premises, network, node_states, link_states, t);
        let actions = if premise_true {
            &rule.then_actions
        } else {
            &rule.else_actions
        };

        for action in actions {
            let link_1based = action.link;
            if link_1based < 1 || link_1based > link_states.len() {
                continue;
            }
            let link = link_1based - 1;
            let is_status = matches!(action.value, ActionValue::Status(_));

            if premise_true {
                let differs = match &action.value {
                    ActionValue::Status(s) => *s != link_states[link].status,
                    ActionValue::Setting(v) => *v != link_states[link].setting,
                };
                if differs {
                    any_then_fire = true;
                }
            }

            pending.push(Pending {
                link,
                value: action.value.clone(),
                priority: rule.priority,
                is_then: premise_true,
                is_status,
            });
        }
    }

    if !any_then_fire {
        let mut else_pending: Vec<&Pending> = Vec::with_capacity(pending.len());
        for p in &pending {
            if !p.is_then {
                else_pending.push(p);
            }
        }
        if else_pending.is_empty() {
            return None;
        }

        let mut else_sorted = else_pending;
        else_sorted.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut seen: std::collections::HashSet<(usize, bool)> =
            std::collections::HashSet::with_capacity(else_sorted.len());
        let mut result: Vec<(usize, ActionValue)> = Vec::with_capacity(else_sorted.len());
        for p in else_sorted {
            let key = (p.link, p.is_status);
            if seen.insert(key) {
                result.push((p.link, p.value.clone()));
            }
        }
        if result.is_empty() {
            return None;
        }
        return Some((result, false));
    }

    pending.sort_by(|a, b| {
        b.priority
            .partial_cmp(&a.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut seen: std::collections::HashSet<(usize, bool)> =
        std::collections::HashSet::with_capacity(pending.len());
    let mut result: Vec<(usize, ActionValue)> = Vec::with_capacity(pending.len());
    for p in pending {
        let key = (p.link, p.is_status);
        if seen.insert(key) {
            result.push((p.link, p.value));
        }
    }

    Some((result, true))
}

/// Applies a conflict-resolved set of link actions to `link_states`.
pub(crate) fn apply_link_actions(
    link_states: &mut [LinkState],
    actions: &[(usize, ActionValue)],
    network: &Network,
) -> bool {
    let mut any_changed = false;
    for (link_0, value) in actions {
        let link_0 = *link_0;
        if link_0 >= link_states.len() {
            continue;
        }
        let is_pump = matches!(network.links[link_0].kind, crate::LinkKind::Pump(_));
        let is_pump_or_pipe =
            is_pump || matches!(network.links[link_0].kind, crate::LinkKind::Pipe(_));
        let is_valve = matches!(network.links[link_0].kind, crate::LinkKind::Valve(_));
        let link_state = &mut link_states[link_0];
        match value {
            ActionValue::Status(s) => {
                let (eff_status, eff_setting) =
                    resolve_control_action(Some(*s), None, is_pump_or_pipe, is_pump, is_valve);
                if let Some(st) = eff_status {
                    if st != link_state.status {
                        link_state.status = st;
                        any_changed = true;
                    }
                }
                if let Some(sv) = eff_setting {
                    if sv != link_state.setting {
                        link_state.setting = sv;
                        any_changed = true;
                    }
                }
            }
            ActionValue::Setting(v) => {
                let (eff_status, eff_setting) =
                    resolve_control_action(None, Some(*v), is_pump_or_pipe, is_pump, is_valve);
                if let Some(st) = eff_status {
                    if st != link_state.status {
                        link_state.status = st;
                        any_changed = true;
                    }
                }
                if let Some(sv) = eff_setting {
                    if sv != link_state.setting {
                        link_state.setting = sv;
                        any_changed = true;
                    }
                }
            }
        }
    }
    any_changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionValue, DemandCategory, Junction, Link, LinkBase, LinkKind, LinkStatus, Network, Node,
        NodeBase, NodeKind, NodeState, Pump, PumpCurveType, Rule, RuleAction, SimulationOptions,
    };
    use std::collections::HashMap;

    fn pump_network_with_rules(rules: Vec<Rule>) -> Network {
        Network {
            title: vec![],
            options: SimulationOptions::default(),
            patterns: vec![],
            curves: vec![],
            nodes: vec![Node {
                base: NodeBase {
                    id: "J1".to_string(),
                    index: 1,
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
            }],
            links: vec![Link {
                base: LinkBase {
                    id: "PU1".to_string(),
                    index: 1,
                    from_node: 1,
                    to_node: 1,
                    initial_status: LinkStatus::Open,
                    initial_setting: Some(1.0),
                },
                kind: LinkKind::Pump(Pump {
                    curve_type: PumpCurveType::PowerFunction,
                    head_curve: None,
                    power: None,
                    efficiency_curve: None,
                    default_efficiency: 0.75,
                    speed_pattern: None,
                    energy_price: None,
                    price_pattern: None,
                }),
            }],
            controls: vec![],
            rules,
            pattern_index: HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: HashMap::new(),
            vertices: HashMap::new(),
            node_tags: HashMap::new(),
            link_tags: HashMap::new(),
        }
    }

    #[test]
    fn eval_rules_returns_else_actions_when_no_then_rule_fires() {
        let network = pump_network_with_rules(vec![Rule {
            priority: 1.0,
            premises: vec![],
            then_actions: vec![RuleAction {
                link: 1,
                value: ActionValue::Status(LinkStatus::Closed),
            }],
            else_actions: vec![RuleAction {
                link: 1,
                value: ActionValue::Setting(2.0),
            }],
        }]);
        let node_states = vec![NodeState::default()];
        let link_states = vec![LinkState::default()];

        let (actions, any_then) =
            eval_rules(&network, &node_states, &link_states, 0.0).expect("else actions expected");

        assert!(!any_then);
        assert_eq!(actions.len(), 1);
        match &actions[0].1 {
            ActionValue::Setting(value) => assert_eq!(*value, 2.0),
            _ => panic!("expected setting action"),
        }
    }

    #[test]
    fn apply_link_actions_maps_pump_status_to_setting() {
        let network = pump_network_with_rules(vec![]);
        let mut link_states = vec![LinkState::default()];
        let actions = vec![(0usize, ActionValue::Status(LinkStatus::Closed))];

        let changed = apply_link_actions(&mut link_states, &actions, &network);

        assert!(changed);
        assert_eq!(link_states[0].status, LinkStatus::Closed);
        assert_eq!(link_states[0].setting, 0.0);
    }
}
