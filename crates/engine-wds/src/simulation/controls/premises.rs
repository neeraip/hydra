use crate::{
    LinkState, LinkStatus, LogicOp, Network, NodeKind, NodeState, Premise, PremiseAttribute,
    PremiseObject, PremiseOperator,
};

/// Specific weight of water at standard conditions (N/m³).
const GAMMA_WATER: f64 = 9810.0;

/// FILLTIME/DRAINTIME premises are expressed in HOURS (§4.2.2, EPANET
/// convention); the volume/flow quotient below is in seconds.
const SECONDS_PER_HOUR: f64 = 3600.0;

/// Power unit conversion: 1 kW = 1000 W.
const KW_PER_W: f64 = 1.0e-3;

/// Evaluates the premise list for a single rule (§4.2.2).
///
/// AND binds more tightly than OR. Consecutive premises are joined by the
/// `connective` field of the preceding premise; `None` marks the last premise.
pub(crate) fn evaluate_premises(
    premises: &[Premise],
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    t: f64,
) -> bool {
    if premises.is_empty() {
        return false;
    }

    let mut or_result = false;
    let mut clause = true;

    for premise in premises {
        let truth = eval_single_premise(premise, network, node_states, link_states, t);
        clause = clause && truth;

        match premise.connective {
            Some(LogicOp::Or) | None => {
                or_result = or_result || clause;
                clause = true;
            }
            Some(LogicOp::And) => {}
        }
    }

    or_result
}

fn eval_single_premise(
    premise: &Premise,
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    t: f64,
) -> bool {
    let lhs = premise_lhs(premise, network, node_states, link_states, t);
    if lhs.is_nan() {
        return false;
    }

    if matches!(premise.object, PremiseObject::Clock) {
        apply_operator_exact(lhs, premise.operator, premise.value)
    } else {
        apply_operator(lhs, premise.operator, premise.value)
    }
}

fn premise_lhs(
    premise: &Premise,
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
    t: f64,
) -> f64 {
    match premise.object {
        PremiseObject::Clock => match premise.attribute {
            PremiseAttribute::Time => t,
            PremiseAttribute::ClockTime => {
                (t + network.options.start_clocktime).rem_euclid(86400.0)
            }
            _ => f64::NAN,
        },

        PremiseObject::Node(idx) => {
            if idx < 1 || idx > node_states.len() {
                return f64::NAN;
            }
            let node_state = &node_states[idx - 1];
            let node = &network.nodes[idx - 1];

            match premise.attribute {
                PremiseAttribute::Head => node_state.head,
                PremiseAttribute::Pressure => node_state.head - node.base.elevation,
                PremiseAttribute::Demand => node_state.demand_flow,
                PremiseAttribute::Level => node_state.level,
                PremiseAttribute::FillTime => {
                    // Hours, not seconds: the premise threshold is stored in
                    // hours (EPANET convention, §4.2.2).
                    if let NodeKind::Tank(tank) = &node.kind {
                        let v_max = tank.volume_from_level(tank.max_level, &network.curves);
                        let q_net = node_state.net_flow;
                        if q_net > 0.0 {
                            (v_max - node_state.volume) / (SECONDS_PER_HOUR * q_net)
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        f64::INFINITY
                    }
                }
                PremiseAttribute::DrainTime => {
                    // Hours, not seconds (§4.2.2).
                    if let NodeKind::Tank(tank) = &node.kind {
                        let v_min = tank.volume_from_level(tank.min_level, &network.curves);
                        let q_net = node_state.net_flow;
                        if q_net < 0.0 {
                            (node_state.volume - v_min) / (SECONDS_PER_HOUR * -q_net)
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        f64::INFINITY
                    }
                }
                _ => f64::NAN,
            }
        }

        PremiseObject::Link(idx) => {
            if idx < 1 || idx > link_states.len() {
                return f64::NAN;
            }
            let link_state = &link_states[idx - 1];
            let link = &network.links[idx - 1];

            match premise.attribute {
                PremiseAttribute::Flow => link_state.flow.abs(),
                PremiseAttribute::Status => link_status_as_f64(link_state.status),
                PremiseAttribute::Setting => link_state.setting,
                PremiseAttribute::Power => {
                    let flow = link_state.flow;
                    if flow <= 0.0 {
                        return 0.0;
                    }
                    let from = link.base.from_node;
                    let to = link.base.to_node;
                    if from < 1 || from > node_states.len() || to < 1 || to > node_states.len() {
                        return f64::NAN;
                    }
                    let head_gain = node_states[to - 1].head - node_states[from - 1].head;
                    if head_gain <= 0.0 {
                        return 0.0;
                    }
                    GAMMA_WATER * network.options.specific_gravity * flow * head_gain * KW_PER_W
                }
                _ => f64::NAN,
            }
        }
    }
}

fn apply_operator(lhs: f64, op: PremiseOperator, rhs: f64) -> bool {
    const TOL: f64 = 1.0e-3;
    match op {
        PremiseOperator::Eq => (lhs - rhs).abs() <= TOL,
        PremiseOperator::Neq => (lhs - rhs).abs() >= TOL,
        PremiseOperator::Lt => lhs <= rhs + TOL,
        PremiseOperator::Le => lhs <= rhs - TOL,
        PremiseOperator::Gt => lhs >= rhs - TOL,
        PremiseOperator::Ge => lhs >= rhs + TOL,
    }
}

fn apply_operator_exact(lhs: f64, op: PremiseOperator, rhs: f64) -> bool {
    match op {
        PremiseOperator::Eq => (lhs - rhs).abs() < f64::EPSILON,
        PremiseOperator::Neq => (lhs - rhs).abs() >= f64::EPSILON,
        PremiseOperator::Lt => lhs < rhs,
        PremiseOperator::Gt => lhs > rhs,
        PremiseOperator::Le => lhs <= rhs,
        PremiseOperator::Ge => lhs >= rhs,
    }
}

fn link_status_as_f64(s: LinkStatus) -> f64 {
    match s {
        LinkStatus::Open => 1.0,
        LinkStatus::Closed
        | LinkStatus::XPressure
        | LinkStatus::XFcv
        | LinkStatus::XHead
        | LinkStatus::TempClosed => 0.0,
        LinkStatus::Active => 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MixModel, Node, NodeBase, SimulationOptions, Tank};
    use std::collections::HashMap;

    /// A single-tank network with a 1 m² cross-section (levels 0–10 m, so the
    /// full volume is 10 m³) for FILLTIME/DRAINTIME evaluation tests.
    fn tank_network() -> Network {
        let unit_area_diameter = (4.0 / std::f64::consts::PI).sqrt(); // A = 1 m²
        Network {
            title: vec![],
            options: SimulationOptions::default(),
            patterns: vec![],
            curves: vec![],
            nodes: vec![Node {
                base: NodeBase {
                    id: "T1".to_string(),
                    index: 1,
                    elevation: 10.0,
                    initial_quality: 0.0,
                },
                kind: NodeKind::Tank(Tank {
                    min_level: 0.0,
                    max_level: 10.0,
                    initial_level: 5.0,
                    diameter: unit_area_diameter,
                    min_volume: 0.0,
                    volume_curve: None,
                    mix_model: MixModel::Cstr,
                    mix_fraction: 1.0,
                    bulk_coeff: 0.0,
                    overflow: false,
                }),
                source: None,
            }],
            links: vec![],
            controls: vec![],
            rules: vec![],
            pattern_index: HashMap::new(),
            report: crate::ReportOptions::default(),
            coordinates: HashMap::new(),
            vertices: HashMap::new(),
            node_tags: HashMap::new(),
            link_tags: HashMap::new(),
        }
    }

    fn tank_premise(attribute: PremiseAttribute, operator: PremiseOperator, value: f64) -> Premise {
        Premise {
            object: PremiseObject::Node(1),
            attribute,
            operator,
            value,
            connective: None,
        }
    }

    fn eval_with_state(premise: Premise, volume: f64, net_flow: f64) -> bool {
        let network = tank_network();
        let node_states = vec![NodeState {
            volume,
            net_flow,
            ..NodeState::default()
        }];
        evaluate_premises(&[premise], &network, &node_states, &[], 0.0)
    }

    /// FILLTIME compares in HOURS (§4.2.2): a tank 1.8 m³ from full filling at
    /// 0.001 m³/s needs 1800 s = 0.5 h. `FILLTIME > 2` must NOT fire — the
    /// old seconds-valued comparison (1800 > 2) fired the moment fill time
    /// crossed 2 *seconds*.
    #[test]
    fn filltime_premise_compares_in_hours_not_seconds() {
        let premise = tank_premise(PremiseAttribute::FillTime, PremiseOperator::Gt, 2.0);
        assert!(
            !eval_with_state(premise, 8.2, 0.001),
            "0.5 h fill time must not satisfy FILLTIME > 2 (hours)"
        );
    }

    /// The same threshold fires once the fill time genuinely exceeds 2 hours
    /// (1.8 m³ remaining at 1e-4 m³/s = 5 h).
    #[test]
    fn filltime_premise_fires_when_fill_time_exceeds_threshold_hours() {
        let premise = tank_premise(PremiseAttribute::FillTime, PremiseOperator::Gt, 2.0);
        assert!(eval_with_state(premise, 8.2, 1.0e-4));
    }

    /// DRAINTIME mirrors FILLTIME: 7.2 m³ draining at 0.001 m³/s is 2 h, so
    /// `DRAINTIME < 1` is false; 1.8 m³ (0.5 h) satisfies it.
    #[test]
    fn draintime_premise_compares_in_hours_not_seconds() {
        let premise = tank_premise(PremiseAttribute::DrainTime, PremiseOperator::Lt, 1.0);
        assert!(!eval_with_state(premise, 7.2, -0.001));

        let premise = tank_premise(PremiseAttribute::DrainTime, PremiseOperator::Lt, 1.0);
        assert!(eval_with_state(premise, 1.8, -0.001));
    }

    /// A non-filling tank still reports an infinite fill time.
    #[test]
    fn filltime_premise_is_infinite_when_not_filling() {
        let premise = tank_premise(PremiseAttribute::FillTime, PremiseOperator::Gt, 2.0);
        assert!(eval_with_state(premise, 5.0, -0.001), "∞ > 2 h");
        let premise = tank_premise(PremiseAttribute::FillTime, PremiseOperator::Lt, 2.0);
        assert!(!eval_with_state(premise, 5.0, -0.001), "∞ < 2 h is false");
    }

    #[test]
    fn apply_operator_uses_tolerance_by_relation() {
        assert!(apply_operator(10.0, PremiseOperator::Eq, 10.0005));
        assert!(apply_operator(10.0, PremiseOperator::Lt, 10.0));
        assert!(!apply_operator(10.0, PremiseOperator::Le, 10.0));
        assert!(apply_operator(10.0, PremiseOperator::Gt, 10.0));
        assert!(!apply_operator(10.0, PremiseOperator::Ge, 10.0));
    }

    #[test]
    fn apply_operator_exact_requires_true_clock_equality() {
        assert!(apply_operator_exact(5.0, PremiseOperator::Eq, 5.0));
        assert!(!apply_operator_exact(
            5.0,
            PremiseOperator::Eq,
            5.0 + 1.0e-9
        ));
        assert!(apply_operator_exact(5.0, PremiseOperator::Le, 5.0));
        assert!(!apply_operator_exact(5.0, PremiseOperator::Lt, 5.0));
    }

    #[test]
    fn link_status_mapping_matches_rule_encoding() {
        assert_eq!(link_status_as_f64(LinkStatus::Open), 1.0);
        assert_eq!(link_status_as_f64(LinkStatus::Active), 2.0);
        assert_eq!(link_status_as_f64(LinkStatus::Closed), 0.0);
        assert_eq!(link_status_as_f64(LinkStatus::TempClosed), 0.0);
    }
}
