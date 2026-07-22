// controls — §4 of crates/engine-wds/src/simulation/spec.md
//
// Evaluates simple controls (§4.1) and rule-based controls (§4.2) and applies
// resulting status and setting changes to links.

mod premises;
mod rules;
mod simple;

pub(crate) use rules::{apply_link_actions, eval_rules};
pub(crate) use simple::{apply_simple_controls, pswitch, resolve_control_action, TIME_TRIGGER_TOL};

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionValue, DemandCategory, Junction, Link, LinkBase, LinkKind, LinkState, LinkStatus,
        LogicOp, MixModel, Network, Node, NodeBase, NodeKind, NodeState, Pipe, Premise,
        PremiseAttribute, PremiseObject, PremiseOperator, Rule, RuleAction, SimpleControl,
        SimulationOptions, Tank, TriggerType,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Builds a minimal `Network` with one junction node and one pipe link.
    fn minimal_network() -> Network {
        let opts = SimulationOptions::default();
        let node = Node {
            base: NodeBase {
                id: "N1".into(),
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
        };
        let link = Link {
            base: LinkBase {
                id: "P1".into(),
                index: 1,
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
        };
        Network {
            title: vec![],
            options: opts,
            patterns: vec![],
            curves: vec![],
            nodes: vec![node],
            links: vec![link],
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

    /// Returns a default open link state.
    fn open_state() -> LinkState {
        LinkState {
            flow: 1.0,
            status: LinkStatus::Open,
            setting: 1.0,
            quality: 0.0,
            reaction_rate: 0.0,
        }
    }

    /// Returns a default node state.
    fn zero_node_state() -> NodeState {
        NodeState::default()
    }

    // ── §4.1 Simple controls ──────────────────────────────────────────────────

    #[test]
    fn timer_fires_at_exact_time() {
        let mut net = minimal_network();
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(3600.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        let node_states = vec![zero_node_state()];
        let mut link_states = vec![open_state()];
        let changed = apply_simple_controls(&net, &node_states, &mut link_states, 3600.0);
        assert!(changed);
        assert_eq!(link_states[0].status, LinkStatus::Closed);
    }

    #[test]
    fn timer_no_fire_at_wrong_time() {
        let mut net = minimal_network();
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(3600.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        let node_states = vec![zero_node_state()];
        let mut link_states = vec![open_state()];
        let changed = apply_simple_controls(&net, &node_states, &mut link_states, 7200.0);
        assert!(!changed);
        assert_eq!(link_states[0].status, LinkStatus::Open);
    }

    #[test]
    fn disabled_control_never_fires() {
        let mut net = minimal_network();
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(0.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: false,
        });
        let node_states = vec![zero_node_state()];
        let mut link_states = vec![open_state()];
        let changed = apply_simple_controls(&net, &node_states, &mut link_states, 0.0);
        assert!(!changed);
    }

    #[test]
    fn timeofday_fires_on_modulo_match() {
        let mut net = minimal_network();
        // start_clocktime = 0; trigger at 08:00 (28800 s)
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::TimeOfDay,
            trigger_time: Some(28800.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        let node_states = vec![zero_node_state()];
        let mut link_states = vec![open_state()];
        // t = 86400 + 28800 wraps to 28800 mod 86400
        let changed =
            apply_simple_controls(&net, &node_states, &mut link_states, 86400.0 + 28800.0);
        assert!(changed);
    }

    #[test]
    fn last_control_on_same_link_wins() {
        let mut net = minimal_network();
        // First control: close at t=0
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(0.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        // Second control: open at t=0 (same time, same link) — should win
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::Timer,
            trigger_time: Some(0.0),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Open),
            action_setting: None,
            enabled: true,
        });
        let node_states = vec![zero_node_state()];
        let mut link_states = vec![LinkState {
            flow: 0.0,
            status: LinkStatus::Closed,
            setting: 1.0,
            quality: 0.0,
            reaction_rate: 0.0,
        }];
        apply_simple_controls(&net, &node_states, &mut link_states, 0.0);
        assert_eq!(link_states[0].status, LinkStatus::Open);
    }

    #[test]
    fn hilevel_fires_when_volume_at_threshold() {
        let mut net = minimal_network();
        // Add a cylindrical tank node (diameter = 10 ft, so A = 25π ft²)
        net.nodes.push(Node {
            base: NodeBase {
                id: "T1".into(),
                index: 2,
                elevation: 10.0, // bottom_elevation = 10 - 0 = 10 ft
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level: 0.0,
                max_level: 20.0,
                initial_level: 5.0,
                diameter: 10.0,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow: false,
                head_pattern: None,
            }),
            source: None,
        });
        // Trigger: HiLevel at grade = 25 ft (head threshold).
        // Tank currently at head = 10 + 16 = 26 ft > 25 → fires.
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::HiLevel,
            trigger_time: None,
            trigger_node: Some(2),
            trigger_grade: Some(25.0),
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        let level = 16.0_f64;
        let head = 10.0 + level; // elevation + level
        let area = std::f64::consts::PI * 5.0 * 5.0;
        let volume = area * level;
        let node_states = vec![
            zero_node_state(),
            NodeState {
                head,
                level,
                volume,
                net_flow: 0.0,
                ..NodeState::default()
            },
        ];
        let mut link_states = vec![open_state()];
        let changed = apply_simple_controls(&net, &node_states, &mut link_states, 0.0);
        assert!(changed, "HiLevel should fire when tank is above threshold");
        assert_eq!(link_states[0].status, LinkStatus::Closed);
    }

    #[test]
    fn lowlevel_fires_when_volume_below_threshold() {
        let mut net = minimal_network();
        net.nodes.push(Node {
            base: NodeBase {
                id: "T1".into(),
                index: 2,
                elevation: 10.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Tank(Tank {
                min_level: 0.0,
                max_level: 20.0,
                initial_level: 5.0,
                diameter: 10.0,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow: false,
                head_pattern: None,
            }),
            source: None,
        });
        // Trigger: LowLevel at grade = 15 ft (head threshold).
        // Tank currently at head = 10 + 4 = 14 ft < 15 → fires.
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type: TriggerType::LowLevel,
            trigger_time: None,
            trigger_node: Some(2),
            trigger_grade: Some(15.0),
            action_status: Some(LinkStatus::Open),
            action_setting: None,
            enabled: true,
        });
        let level = 4.0_f64;
        let head = 10.0 + level; // elevation + level
        let area = std::f64::consts::PI * 5.0 * 5.0;
        let volume = area * level;
        let mut link_states = vec![LinkState {
            flow: 0.0,
            status: LinkStatus::Closed,
            setting: 1.0,
            quality: 0.0,
            reaction_rate: 0.0,
        }];
        let node_states = vec![
            zero_node_state(),
            NodeState {
                head,
                level,
                volume,
                net_flow: 0.0,
                ..NodeState::default()
            },
        ];
        let changed = apply_simple_controls(&net, &node_states, &mut link_states, 0.0);
        assert!(changed, "LowLevel should fire when tank is below threshold");
        assert_eq!(link_states[0].status, LinkStatus::Open);
    }

    // ── §4.2 Rule-based controls ──────────────────────────────────────────────

    fn make_rule(
        priority: f64,
        premises: Vec<Premise>,
        then_actions: Vec<RuleAction>,
        else_actions: Vec<RuleAction>,
    ) -> crate::Rule {
        Rule {
            priority,
            premises,
            then_actions,
            else_actions,
        }
    }

    fn head_premise(node: usize, op: PremiseOperator, val: f64, conn: Option<LogicOp>) -> Premise {
        Premise {
            object: PremiseObject::Node(node),
            attribute: PremiseAttribute::Head,
            operator: op,
            value: val,
            connective: conn,
        }
    }

    fn close_action(link: usize) -> RuleAction {
        RuleAction {
            link,
            value: ActionValue::Status(LinkStatus::Closed),
        }
    }

    fn open_action(link: usize) -> RuleAction {
        RuleAction {
            link,
            value: ActionValue::Status(LinkStatus::Open),
        }
    }

    #[test]
    fn rule_and_premises_all_true_fires() {
        let mut net = minimal_network();
        // Add second node for premise
        net.nodes.push(Node {
            base: NodeBase {
                id: "N2".into(),
                index: 2,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        net.links[0].base.to_node = 2;

        // Rule: IF N1.HEAD > 50 AND N2.HEAD > 30 THEN close P1
        net.rules.push(make_rule(
            1.0,
            vec![
                head_premise(1, PremiseOperator::Gt, 50.0, Some(LogicOp::And)),
                head_premise(2, PremiseOperator::Gt, 30.0, None),
            ],
            vec![close_action(1)],
            vec![],
        ));

        let node_states = vec![
            NodeState {
                head: 60.0,
                ..NodeState::default()
            },
            NodeState {
                head: 40.0,
                ..NodeState::default()
            },
        ];
        let link_states = vec![open_state()];
        let result = eval_rules(&net, &node_states, &link_states, 0.0);
        assert!(result.is_some());
        let (actions, _) = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0].1,
            ActionValue::Status(LinkStatus::Closed)
        ));
    }

    #[test]
    fn rule_and_premises_one_false_no_fire() {
        let mut net = minimal_network();
        net.nodes.push(Node {
            base: NodeBase {
                id: "N2".into(),
                index: 2,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        net.rules.push(make_rule(
            1.0,
            vec![
                head_premise(1, PremiseOperator::Gt, 50.0, Some(LogicOp::And)),
                head_premise(2, PremiseOperator::Gt, 30.0, None),
            ],
            vec![close_action(1)],
            vec![],
        ));

        let node_states = vec![
            NodeState {
                head: 60.0,
                ..NodeState::default()
            },
            NodeState {
                head: 20.0,
                ..NodeState::default()
            }, // N2 head < 30 → AND fails
        ];
        let link_states = vec![open_state()];
        let result = eval_rules(&net, &node_states, &link_states, 0.0);
        assert!(result.is_none());
    }

    #[test]
    fn rule_or_premises_second_clause_true_fires() {
        let mut net = minimal_network();
        net.nodes.push(Node {
            base: NodeBase {
                id: "N2".into(),
                index: 2,
                elevation: 0.0,
                initial_quality: 0.0,
            },
            kind: NodeKind::Junction(Junction {
                demands: vec![],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            source: None,
        });
        // Rule: IF N1.HEAD > 100 OR N2.HEAD > 30 THEN close P1
        // N1.HEAD = 40 (false), N2.HEAD = 40 (true) → OR true → fires
        net.rules.push(make_rule(
            1.0,
            vec![
                head_premise(1, PremiseOperator::Gt, 100.0, Some(LogicOp::Or)),
                head_premise(2, PremiseOperator::Gt, 30.0, None),
            ],
            vec![close_action(1)],
            vec![],
        ));

        let node_states = vec![
            NodeState {
                head: 40.0,
                ..NodeState::default()
            },
            NodeState {
                head: 40.0,
                ..NodeState::default()
            },
        ];
        let link_states = vec![open_state()];
        let result = eval_rules(&net, &node_states, &link_states, 0.0);
        assert!(result.is_some());
    }

    #[test]
    fn rule_conflict_higher_priority_wins() {
        let mut net = minimal_network();
        // Two rules both fire, both target link 1 status.
        // Rule A: priority 2.0 → open; Rule B: priority 1.0 → close.
        // Higher numeric priority wins → open wins.
        net.rules.push(make_rule(
            2.0,
            vec![head_premise(1, PremiseOperator::Ge, 0.0, None)],
            vec![open_action(1)],
            vec![],
        ));
        net.rules.push(make_rule(
            1.0,
            vec![head_premise(1, PremiseOperator::Ge, 0.0, None)],
            vec![close_action(1)],
            vec![],
        ));

        // Head = 10.0 clearly satisfies GE 0.0 with EPANET's tolerance (GE requires lhs >= rhs + 0.001)
        let node_states = vec![NodeState {
            head: 10.0,
            ..NodeState::default()
        }];
        let link_states = vec![open_state()]; // currently open; but close differs
        let result = eval_rules(&net, &node_states, &link_states, 0.0);
        assert!(result.is_some());
        let (actions, _) = result.unwrap();
        let status_action = actions.iter().find(|(l, _)| *l == 0).unwrap();
        assert!(
            matches!(status_action.1, ActionValue::Status(LinkStatus::Open)),
            "higher priority (2.0) rule that opens should win over priority 1.0 close rule"
        );
    }

    #[test]
    fn rule_else_actions_applied_when_premise_false() {
        let mut net = minimal_network();
        // Rule: IF N1.HEAD > 100 THEN close P1 ELSE open P1
        // N1 head = 50 → premise false → ELSE applies open
        net.rules.push(make_rule(
            1.0,
            vec![head_premise(1, PremiseOperator::Gt, 100.0, None)],
            vec![close_action(1)],
            vec![open_action(1)],
        ));

        let node_states = vec![NodeState {
            head: 50.0,
            ..NodeState::default()
        }];
        // Link is currently closed so the open ELSE action differs
        let link_states = vec![LinkState {
            flow: 0.0,
            status: LinkStatus::Closed,
            setting: 1.0,
            quality: 0.0,
            reaction_rate: 0.0,
        }];

        // Premise is false → THEN doesn't fire.  ELSE action (open) differs from
        // current state (closed).  §4.2.3: ELSE actions are applied when premise
        // is false. eval_rules returns Some with fired=false.
        let result = eval_rules(&net, &node_states, &link_states, 0.0);
        assert!(
            result.is_some(),
            "ELSE actions should be returned even when no THEN fires"
        );
        let (actions, fired) = result.unwrap();
        assert!(!fired, "THEN did not fire");
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions[0].1,
            ActionValue::Status(LinkStatus::Open)
        ));
    }

    #[test]
    fn apply_link_actions_updates_state() {
        let net = minimal_network();
        let mut link_states = vec![open_state()];
        let actions = vec![(0usize, ActionValue::Status(LinkStatus::Closed))];
        let changed = apply_link_actions(&mut link_states, &actions, &net);
        assert!(changed);
        assert_eq!(link_states[0].status, LinkStatus::Closed);
    }

    #[test]
    fn apply_link_actions_no_change_when_same() {
        let net = minimal_network();
        let mut link_states = vec![open_state()];
        let actions = vec![(0usize, ActionValue::Status(LinkStatus::Open))];
        let changed = apply_link_actions(&mut link_states, &actions, &net);
        assert!(!changed);
    }

    #[test]
    fn time_premise_matches_simulation_time() {
        let mut net = minimal_network();
        // Rule: IF TIME >= 3600 THEN close P1
        net.rules.push(make_rule(
            1.0,
            vec![Premise {
                object: PremiseObject::Clock,
                attribute: PremiseAttribute::Time,
                operator: PremiseOperator::Ge,
                value: 3600.0,
                connective: None,
            }],
            vec![close_action(1)],
            vec![],
        ));
        let node_states = vec![NodeState::default()];
        let link_states = vec![open_state()];
        assert!(eval_rules(&net, &node_states, &link_states, 3600.0).is_some());
        assert!(eval_rules(&net, &node_states, &link_states, 3599.0).is_none());
    }
}
