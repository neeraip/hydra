use crate::{
    LinkState, LinkStatus, LogicOp, Network, NodeKind, NodeState, Premise, PremiseAttribute,
    PremiseObject, PremiseOperator,
};

/// Specific weight of water at standard conditions (N/m³).
const GAMMA_WATER: f64 = 9810.0;

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
                    if let NodeKind::Tank(tank) = &node.kind {
                        let v_max = tank.volume_from_level(tank.max_level, &network.curves);
                        let q_net = node_state.net_flow;
                        if q_net > 0.0 {
                            (v_max - node_state.volume) / q_net
                        } else {
                            f64::INFINITY
                        }
                    } else {
                        f64::INFINITY
                    }
                }
                PremiseAttribute::DrainTime => {
                    if let NodeKind::Tank(tank) = &node.kind {
                        let v_min = tank.volume_from_level(tank.min_level, &network.curves);
                        let q_net = node_state.net_flow;
                        if q_net < 0.0 {
                            (node_state.volume - v_min) / (-q_net)
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
                    GAMMA_WATER
                        * network.options.specific_gravity
                        * flow
                        * head_gain
                        * KW_PER_W
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
