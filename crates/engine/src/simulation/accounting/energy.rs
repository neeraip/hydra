use crate::{LinkKind, LinkState, Network, NodeState};

use super::AccountingState;

/// Gravitational acceleration (m/s²) for hydraulic power calculations.
const G: f64 = 9.81;
/// Density of fresh water (kg/m³); scaled by `specific_gravity`.
const RHO_WATER: f64 = 1000.0;
/// Conversion factor from W to kW: 1 kW = 1000 W.
pub(crate) const K_POWER: f64 = 1.0e-3;

/// Minimum pump flow used in energy calculations (m³/s).
/// Prevents division by zero when evaluating efficiency or kWh/flow.
const Q_MIN_ENERGY: f64 = 1.0e-6;

/// Minimum pump efficiency after clamping (fraction).
pub(super) const ETA_MIN: f64 = 0.01;

/// Pre-computed pump power and efficiency for a single pump, captured before
/// tank-level updates so that `accumulate_step` uses the same head/flow state
/// that was current at the time of the hydraulic solve (matching EPANET's
/// `getallpumpsenergy` → `timestep` → `addenergy` ordering).
#[derive(Debug, Clone)]
pub(crate) struct PrecomputedPumpPower {
    pub link_index: usize,
    pub w_elec_kw: f64,
    pub q_guard: f64,
    pub eta: f64,
    pub is_online: bool,
}

pub(super) fn precompute_pump_powers(
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
) -> Vec<PrecomputedPumpPower> {
    let opts = &network.options;
    let rho_g = rho_times_g(opts.specific_gravity);

    network
        .links
        .iter()
        .enumerate()
        .filter_map(|(li, link)| {
            let pump = match &link.kind {
                LinkKind::Pump(p) => p,
                _ => return None,
            };
            let ls = &link_states[li];
            if matches!(ls.status, crate::LinkStatus::Closed) {
                return None;
            }
            let omega: f64 = ls.setting.max(0.0);
            let h_from = node_states[link.base.from_idx()].head;
            let h_to = node_states[link.base.to_idx()].head;
            let delta_h = (h_to - h_from).max(0.0);
            let q_raw = ls.flow;
            let q_guard = q_raw.max(Q_MIN_ENERGY);
            let w_hyd = rho_g * q_guard * delta_h;
            let eta = compute_efficiency(pump, network, q_guard, omega, opts.energy_efficiency);
            let w_elec_kw = (w_hyd / eta) * K_POWER;
            Some(PrecomputedPumpPower {
                link_index: li,
                w_elec_kw,
                q_guard,
                eta,
                is_online: q_raw > 0.0,
            })
        })
        .collect()
}

pub(super) fn accumulate_pump_energy(
    state: &mut AccountingState,
    network: &Network,
    pump_powers: &[PrecomputedPumpPower],
    dt: f64,
    t: f64,
) {
    let mut total_kw: f64 = 0.0;
    for pp in pump_powers {
        let pump = match &network.links[pp.link_index].kind {
            LinkKind::Pump(p) => p,
            _ => continue,
        };
        let price = effective_price(pump, network, t);
        let cost = pp.w_elec_kw * dt / 3600.0 * price;

        let pe = &mut state.pump_energy[pp.link_index];
        let delta_kwh = pp.w_elec_kw * dt / 3600.0;
        pe.kwh += delta_kwh;
        pe.kwh_per_flow += (pp.w_elec_kw / pp.q_guard) * dt;
        pe.total_cost += cost;
        if pp.w_elec_kw > pe.max_kw {
            pe.max_kw = pp.w_elec_kw;
        }
        if pp.is_online {
            pe.time_online += dt;
            pe.efficiency_sum += pp.eta * dt;
        }
        total_kw += pp.w_elec_kw;
    }
    if total_kw > state.peak_demand_kw {
        state.peak_demand_kw = total_kw;
    }
}

pub(super) fn peak_demand_cost(state: &AccountingState, network: &Network) -> f64 {
    network.options.peak_demand_charge * state.peak_demand_kw
}

/// ρ·g product in SI units (kg/m³ × m/s² = N/m³), scaled by specific gravity.
/// The result in N/m³ gives hydraulic power in W when multiplied by flow (m³/s)
/// and head gain (m): W = N/m³ × m³/s × m = N·m/s = W.
pub(super) fn rho_times_g(specific_gravity: f64) -> f64 {
    specific_gravity * RHO_WATER * G
}

pub(super) fn compute_efficiency(
    pump: &crate::Pump,
    network: &Network,
    q: f64,
    omega: f64,
    global_efficiency: f64,
) -> f64 {
    let default_eta = if pump.default_efficiency > 0.0 {
        pump.default_efficiency
    } else {
        global_efficiency
    };

    if let Some(ref curve_id) = pump.efficiency_curve {
        if let Some(curve) = network.curves.iter().find(|c| c.id == *curve_id) {
            let q_adj = if omega > 0.0 { q / omega } else { q };
            let eta_1_pct = curve.eval(q_adj);
            let eta_pct = if (omega - 1.0).abs() < f64::EPSILON * 4.0 {
                eta_1_pct
            } else {
                100.0 - (100.0 - eta_1_pct) / omega.powf(0.1)
            };
            return (eta_pct / 100.0).clamp(ETA_MIN, 1.0);
        }
    }

    default_eta.clamp(ETA_MIN, 1.0)
}

pub(super) fn effective_price(pump: &crate::Pump, network: &Network, t: f64) -> f64 {
    let opts = &network.options;
    let base = match pump.energy_price {
        Some(p) if p > 0.0 => p,
        _ => opts.energy_price,
    };
    let pattern_id = pump
        .price_pattern
        .as_deref()
        .or(opts.energy_price_pattern.as_deref());
    let multiplier = pattern_id
        .and_then(|id| network.pattern_by_id(id))
        .map_or(1.0, |pat| {
            pat.eval(t, opts.pattern_step, opts.pattern_start)
        });
    base * multiplier
}

#[cfg(test)]
mod tests {
    use super::AccountingState;
    use super::*;
    use crate::io::{FlowBalance, PumpEnergy};
    use crate::{
        Curve, CurveKind, CurvePoint, DemandCategory, Junction, Link, LinkBase, LinkState,
        LinkStatus, Network, Node, NodeBase, NodeKind, NodeState, Pattern, Pump, PumpCurveType,
        Reservoir, SimulationOptions,
    };
    use std::collections::HashMap;

    fn pump_network() -> Network {
        let mut pattern_index = HashMap::new();
        pattern_index.insert("global".to_string(), 0);
        pattern_index.insert("pump".to_string(), 1);

        let options = SimulationOptions {
            energy_price: 2.0,
            energy_price_pattern: Some("global".to_string()),
            energy_efficiency: 0.7,
            pattern_step: 3600.0,
            pattern_start: 0.0,
            ..Default::default()
        };

        Network {
            title: vec![],
            options,
            patterns: vec![
                Pattern {
                    id: "global".to_string(),
                    factors: vec![1.2],
                },
                Pattern {
                    id: "pump".to_string(),
                    factors: vec![1.5],
                },
            ],
            curves: vec![Curve {
                id: "eff".to_string(),
                kind: CurveKind::PumpEfficiency,
                points: vec![
                    CurvePoint { x: 0.0, y: 60.0 },
                    CurvePoint { x: 10.0, y: 80.0 },
                ],
            }],
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
                            base_demand: 0.0,
                            pattern: None,
                            name: None,
                        }],
                        emitter_coeff: 0.0,
                        emitter_exp: 0.5,
                    }),
                    source: None,
                },
            ],
            links: vec![Link {
                base: LinkBase {
                    id: "PU1".to_string(),
                    index: 1,
                    from_node: 1,
                    to_node: 2,
                    initial_status: LinkStatus::Open,
                    initial_setting: Some(1.0),
                },
                kind: LinkKind::Pump(Pump {
                    curve_type: PumpCurveType::PowerFunction,
                    head_curve: None,
                    power: None,
                    efficiency_curve: Some("eff".to_string()),
                    default_efficiency: 0.8,
                    speed_pattern: None,
                    energy_price: Some(3.0),
                    price_pattern: Some("pump".to_string()),
                }),
            }],
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
    fn effective_price_prefers_pump_specific_price_and_pattern() {
        let network = pump_network();
        let pump = match &network.links[0].kind {
            LinkKind::Pump(pump) => pump,
            _ => panic!("expected pump link"),
        };

        assert_eq!(effective_price(pump, &network, 0.0), 4.5);
    }

    #[test]
    fn precompute_pump_powers_clamps_zero_flow_and_marks_offline() {
        let network = pump_network();
        let node_states = vec![
            NodeState {
                head: 10.0,
                ..NodeState::default()
            },
            NodeState {
                head: 25.0,
                ..NodeState::default()
            },
        ];
        let link_states = vec![LinkState {
            flow: 0.0,
            status: LinkStatus::Open,
            setting: 1.0,
            ..LinkState::default()
        }];

        let pump_powers = precompute_pump_powers(&network, &node_states, &link_states);

        assert_eq!(pump_powers.len(), 1);
        assert!((pump_powers[0].q_guard - Q_MIN_ENERGY).abs() < 1e-15);
        assert!(!pump_powers[0].is_online);
        assert!(pump_powers[0].w_elec_kw > 0.0);
    }

    #[test]
    fn accumulate_pump_energy_updates_peak_and_online_stats() {
        let network = pump_network();
        let mut state = AccountingState {
            pump_energy: vec![PumpEnergy::default(); network.links.len()],
            peak_demand_kw: 0.0,
            flow_balance: FlowBalance {
                total_inflow: 0.0,
                total_outflow: 0.0,
                demand_deficit: 0.0,
                initial_tank_volume: 0.0,
            },
        };
        let pump_powers = vec![PrecomputedPumpPower {
            link_index: 0,
            w_elec_kw: 12.0,
            q_guard: 3.0,
            eta: 0.75,
            is_online: true,
        }];

        accumulate_pump_energy(&mut state, &network, &pump_powers, 1800.0, 0.0);

        let pump_energy = &state.pump_energy[0];
        assert!((pump_energy.kwh - 6.0).abs() < 1e-12);
        assert!((pump_energy.kwh_per_flow - 7200.0).abs() < 1e-12);
        assert!((pump_energy.total_cost - 27.0).abs() < 1e-12);
        assert_eq!(pump_energy.max_kw, 12.0);
        assert_eq!(pump_energy.time_online, 1800.0);
        assert_eq!(pump_energy.efficiency_sum, 1350.0);
        assert_eq!(state.peak_demand_kw, 12.0);
    }
}
