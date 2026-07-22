use crate::{LinkKind, LinkState, LinkStatus, Network, NodeKind, NodeState, TriggerType};

/// Tolerance ε_t for time-trigger comparisons (§4.1).
///
/// Simulation time is real-valued; the adaptive stepper lands on trigger
/// times only up to floating-point rounding, which this tolerance absorbs.
/// Far smaller than any achievable time step, so a trigger fires on exactly
/// one step per occurrence.
pub(crate) const TIME_TRIGGER_TOL: f64 = 1.0e-6;

/// Resolve the effective (status, setting) pair for a control action,
/// matching EPANET's `controldata()` (input3.c) derivation:
///
///   - Numeric setting on pump/pipe: `0.0` -> Closed, `>0` -> Open
///   - Numeric setting on valve: status = Active (EPANET default)
///   - OPEN on pump: setting = 1.0
///   - CLOSED on pump: setting = 0.0
pub(crate) fn resolve_control_action(
    action_status: Option<LinkStatus>,
    action_setting: Option<f64>,
    is_pump_or_pipe: bool,
    is_pump: bool,
    is_valve: bool,
) -> (Option<LinkStatus>, Option<f64>) {
    let mut status = action_status;
    let mut setting = action_setting;

    // OPEN/CLOSED keywords on pumps imply a setting value.
    if is_pump {
        match status {
            Some(LinkStatus::Open) if setting.is_none() => setting = Some(1.0),
            Some(LinkStatus::Closed) if setting.is_none() => setting = Some(0.0),
            _ => {}
        }
    }

    // Numeric settings on pumps/pipes imply a status.
    if is_pump_or_pipe {
        if let Some(v) = setting {
            if status.is_none() {
                status = Some(if v == 0.0 {
                    LinkStatus::Closed
                } else {
                    LinkStatus::Open
                });
            }
        }
    }

    // Numeric settings on valves: EPANET defaults status to ACTIVE
    // (controldata() initialises `status = ACTIVE` and never overrides it
    // for valves with numeric settings).
    if is_valve && setting.is_some() && status.is_none() {
        status = Some(LinkStatus::Active);
    }

    (status, setting)
}

/// Applies §4.1 simple controls to `link_states` in index order.
///
/// Each enabled control is tested against time `t`. When the trigger fires
/// and the resulting action differs from the current link state, the state is
/// updated. If multiple controls target the same link and both fire in the
/// same step, the last one in index order wins (§4.1 step 3).
///
/// Returns `true` if at least one link status or setting changed.
pub(crate) fn apply_simple_controls(
    network: &Network,
    node_states: &[NodeState],
    link_states: &mut [LinkState],
    t: f64,
) -> bool {
    let clock_start = network.options.start_clocktime;
    let mut any_changed = false;

    for ctrl in &network.controls {
        if !ctrl.enabled {
            continue;
        }

        let fires = match ctrl.trigger_type {
            // §4.1: time triggers compare within ε_t rather than exactly —
            // any accumulated floating-point error in `t` would otherwise
            // silently prevent the control from ever firing.
            TriggerType::Timer => ctrl
                .trigger_time
                .is_some_and(|tt| (t - tt).abs() <= TIME_TRIGGER_TOL),

            TriggerType::TimeOfDay => ctrl.trigger_time.is_some_and(|tt| {
                // Circular distance covers rounding on either side of midnight.
                let d = (t + clock_start - tt).rem_euclid(86400.0);
                d.min(86400.0 - d) <= TIME_TRIGGER_TOL
            }),

            TriggerType::HiLevel | TriggerType::LowLevel => {
                let (node_idx, grade) = match (ctrl.trigger_node, ctrl.trigger_grade) {
                    (Some(n), Some(g)) => (n, g),
                    _ => continue,
                };
                if node_idx < 1 || node_idx > network.nodes.len() {
                    continue;
                }
                let node_state = &node_states[node_idx - 1];
                let node = &network.nodes[node_idx - 1];

                if let NodeKind::Tank(tank) = &node.kind {
                    // EPANET controls(): both volumes are computed via the
                    // same tankvolume(head) function, ensuring consistency.
                    // v1 = tankvolume(NodeHead); v2 = tankvolume(Grade).
                    let bottom = tank.bottom_elevation(node.base.elevation);
                    let level_current = node_state.head - bottom;
                    let level_at_grade = grade - bottom;
                    let v_current = tank.volume_from_level(level_current, &network.curves);
                    let v_grade = tank.volume_from_level(level_at_grade, &network.curves);
                    let vplus = node_state.net_flow.abs();
                    match ctrl.trigger_type {
                        TriggerType::HiLevel => v_current >= v_grade - vplus,
                        TriggerType::LowLevel => v_current <= v_grade + vplus,
                        _ => unreachable!(),
                    }
                } else {
                    match ctrl.trigger_type {
                        TriggerType::HiLevel => node_state.head >= grade,
                        TriggerType::LowLevel => node_state.head <= grade,
                        _ => unreachable!(),
                    }
                }
            }
        };

        if !fires {
            continue;
        }

        let link_idx = ctrl.link;
        if link_idx < 1 || link_idx > link_states.len() {
            continue;
        }
        let link_state = &mut link_states[link_idx - 1];

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

        // EPANET controls(): for valves (link->Type > PIPE), always overwrites
        // LinkSetting with control->Setting. When that is MISSING (no numeric
        // value), we use NaN as the sentinel — the output writer maps NaN → 0.
        let eff_setting = if is_valve {
            Some(eff_setting.unwrap_or(f64::NAN))
        } else {
            eff_setting
        };

        if let Some(new_status) = eff_status {
            if new_status != link_state.status {
                link_state.status = new_status;
                any_changed = true;
            }
        }
        if let Some(new_setting) = eff_setting {
            // NaN-aware comparison: NaN == NaN should not be a change.
            let changed = if new_setting.is_nan() {
                !link_state.setting.is_nan()
            } else {
                new_setting != link_state.setting
            };
            if changed {
                link_state.setting = new_setting;
                any_changed = true;
            }
        }
    }

    any_changed
}

/// Re-evaluates simple controls triggered by junction heads (§3.8 pswitch).
///
/// Called from the Newton-Raphson solver after convergence and after
/// valve/link status checks. Only level-based controls whose trigger node is a
/// junction are tested.
///
/// Returns `true` if at least one link status or setting changed.
pub(crate) fn pswitch(
    network: &Network,
    node_states: &[NodeState],
    statuses: &mut [LinkStatus],
    settings: &mut [f64],
) -> bool {
    let mut any_changed = false;

    for ctrl in &network.controls {
        if !ctrl.enabled {
            continue;
        }

        let fires = match ctrl.trigger_type {
            TriggerType::HiLevel | TriggerType::LowLevel => {
                let (node_idx_1, grade) = match (ctrl.trigger_node, ctrl.trigger_grade) {
                    (Some(n), Some(g)) => (n, g),
                    _ => continue,
                };
                if node_idx_1 < 1 || node_idx_1 > network.nodes.len() {
                    continue;
                }
                let node = &network.nodes[node_idx_1 - 1];
                if !matches!(node.kind, NodeKind::Junction(_)) {
                    continue;
                }
                let head = node_states[node_idx_1 - 1].head;
                let htol = network.options.head_tol;
                match ctrl.trigger_type {
                    TriggerType::LowLevel => head <= grade + htol,
                    TriggerType::HiLevel => head >= grade - htol,
                    _ => unreachable!(),
                }
            }
            _ => continue,
        };

        if !fires {
            continue;
        }

        let link_idx = ctrl.link;
        if link_idx < 1 || link_idx > statuses.len() {
            continue;
        }
        let link_index = link_idx - 1;
        let link = &network.links[link_index];
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

        if let Some(new_status) = eff_status {
            if new_status != statuses[link_index] {
                statuses[link_index] = new_status;
                any_changed = true;
            }
        }
        if let Some(new_setting) = eff_setting {
            if new_setting != settings[link_index] {
                settings[link_index] = new_setting;
                any_changed = true;
            }
        }
    }

    any_changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_control_action_sets_pump_open_and_closed_settings() {
        assert_eq!(
            resolve_control_action(Some(LinkStatus::Open), None, true, true, false),
            (Some(LinkStatus::Open), Some(1.0))
        );
        assert_eq!(
            resolve_control_action(Some(LinkStatus::Closed), None, true, true, false),
            (Some(LinkStatus::Closed), Some(0.0))
        );
    }

    #[test]
    fn resolve_control_action_infers_status_from_numeric_setting() {
        assert_eq!(
            resolve_control_action(None, Some(0.0), true, false, false),
            (Some(LinkStatus::Closed), Some(0.0))
        );
        assert_eq!(
            resolve_control_action(None, Some(2.5), true, false, false),
            (Some(LinkStatus::Open), Some(2.5))
        );
    }

    #[test]
    fn resolve_control_action_defaults_valve_setting_to_active() {
        assert_eq!(
            resolve_control_action(None, Some(35.0), false, false, true),
            (Some(LinkStatus::Active), Some(35.0))
        );
    }

    fn timer_control_network(
        trigger_type: TriggerType,
        trigger_time: f64,
    ) -> (Network, Vec<NodeState>, Vec<LinkState>) {
        use crate::test_support::TestNetworkBuilder;
        use crate::SimpleControl;

        let (mut net, ns, ls) = TestNetworkBuilder::new()
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 10.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .build();
        net.controls.push(SimpleControl {
            link: 1,
            trigger_type,
            trigger_time: Some(trigger_time),
            trigger_node: None,
            trigger_grade: None,
            action_status: Some(LinkStatus::Closed),
            action_setting: None,
            enabled: true,
        });
        (net, ns, ls)
    }

    #[test]
    fn timer_control_fires_within_time_tolerance() {
        // §4.1: time triggers compare within ε_t, so a non-integral simulation
        // time carrying floating-point rounding still fires the control.
        let (net, ns, mut ls) = timer_control_network(TriggerType::Timer, 7200.0);
        assert!(apply_simple_controls(&net, &ns, &mut ls, 7200.0 + 2.0e-7));
        assert_eq!(ls[0].status, LinkStatus::Closed);

        // Outside the tolerance the trigger must not fire.
        let (net, ns, mut ls) = timer_control_network(TriggerType::Timer, 7200.0);
        assert!(!apply_simple_controls(&net, &ns, &mut ls, 7200.5));
        assert_eq!(ls[0].status, LinkStatus::Open);
    }

    #[test]
    fn time_of_day_control_fires_within_circular_tolerance() {
        // Rounding on either side of the wall-clock trigger fires exactly once.
        let (net, ns, mut ls) = timer_control_network(TriggerType::TimeOfDay, 3600.0);
        assert!(apply_simple_controls(
            &net,
            &ns,
            &mut ls,
            86400.0 + 3600.0 - 3.0e-7
        ));
        assert_eq!(ls[0].status, LinkStatus::Closed);

        // Midnight trigger: a time fractionally before midnight is within the
        // circular distance of trigger_time = 0.
        let (net, ns, mut ls) = timer_control_network(TriggerType::TimeOfDay, 0.0);
        assert!(apply_simple_controls(&net, &ns, &mut ls, 86400.0 - 5.0e-7));
        assert_eq!(ls[0].status, LinkStatus::Closed);
    }
}
