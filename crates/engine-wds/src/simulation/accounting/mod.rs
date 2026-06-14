// accounting — §7 of crates/simulation/spec.md
//
// Accumulates pump energy and cost statistics (§7.1) and computes the
// volumetric flow balance ratio (§7.2).
//
// Parallelism (∥): per-pump energy accumulation is independent per pump.

use crate::{LinkState, Network, NodeKind, NodeState};

mod energy;
mod flow_balance;

pub(crate) use energy::PrecomputedPumpPower;

// PumpEnergy and FlowBalance are defined in crate::io so the output
// writers in that crate can reference them without a circular dependency.
pub(crate) use crate::io::{FlowBalance, PumpEnergy};

/// Mutable accounting state updated after every hydraulic step (§7).
#[derive(Debug, Clone)]
pub(crate) struct AccountingState {
    /// Per-link pump energy; indexed parallel to `network.links`.
    /// Non-pump entries are default-zero and must not be read.
    pub pump_energy: Vec<PumpEnergy>,
    /// Running peak of total simultaneous electrical demand (kW) across all pumps (§7.1).
    pub peak_demand_kw: f64,
    /// Volumetric flow balance (§7.2).
    pub flow_balance: FlowBalance,
}

/// Initialise an `AccountingState` at the start of a simulation (t = 0).
///
/// Records the initial total tank volume for the balance ratio denominator.
pub(crate) fn init_accounting(network: &Network, node_states: &[NodeState]) -> AccountingState {
    let initial_tank_volume: f64 = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            if matches!(n.kind, NodeKind::Tank(_)) {
                Some(node_states[i].volume)
            } else {
                None
            }
        })
        .sum();

    AccountingState {
        pump_energy: vec![PumpEnergy::default(); network.links.len()],
        peak_demand_kw: 0.0,
        flow_balance: FlowBalance {
            total_inflow: 0.0,
            total_outflow: 0.0,
            demand_deficit: 0.0,
            initial_tank_volume,
        },
    }
}

/// Pre-compute pump power and efficiency from current heads/flows (§7.1).
///
/// Must be called BEFORE tank-level updates so that the head difference across
/// each pump reflects the hydraulic solve, not the post-advance tank levels.
/// This mirrors EPANET's `getallpumpsenergy()` → `timestep()` → `addenergy()`
/// ordering where `CurrentPower` is captured before `tanklevels()` modifies
/// node heads.
pub(crate) fn precompute_pump_powers(
    network: &Network,
    node_states: &[NodeState],
    link_states: &[LinkState],
) -> Vec<PrecomputedPumpPower> {
    energy::precompute_pump_powers(network, node_states, link_states)
}

/// Accumulate accounting statistics for a single hydraulic step of duration
/// `dt` seconds at simulation time `t` (§7.1 and §7.2).
///
/// `overflow_volume` is the total overflow volume (m³) accumulated
/// across all tanks during this step's tank-level update (§5.3).  It is
/// added to `total_outflow` in the flow balance (§7.2).
///
/// `pump_powers` are the pre-computed power/efficiency values captured BEFORE
/// tank levels were advanced (via `precompute_pump_powers`).  This matches
/// EPANET's ordering: compute power → advance tanks / evaluate rules → accumulate.
pub(crate) fn accumulate_step(
    state: &mut AccountingState,
    network: &Network,
    node_states: &[NodeState],
    pump_powers: &[PrecomputedPumpPower],
    dt: f64,
    t: f64,
    overflow_volume: f64,
) {
    energy::accumulate_pump_energy(state, network, pump_powers, dt, t);
    flow_balance::accumulate_flow_balance(
        &mut state.flow_balance,
        network,
        node_states,
        dt,
        t,
        overflow_volume,
    );
}

/// Returns the peak demand cost (§7.1).
///
/// `peak_demand_cost = peak_demand_charge × peak_demand_kw`
pub(crate) fn peak_demand_cost(state: &AccountingState, network: &Network) -> f64 {
    energy::peak_demand_cost(state, network)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DemandCategory, Junction, Link, LinkBase, LinkKind, LinkStatus, Node, NodeBase, NodeKind,
        NodeState, Pipe, Pump, PumpCurveType, SimulationOptions,
    };

    fn minimal_network() -> Network {
        Network {
            title: vec![],
            options: SimulationOptions::default(),
            patterns: vec![],
            curves: vec![],
            nodes: vec![Node {
                base: NodeBase {
                    id: "J1".into(),
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
                    id: "P1".into(),
                    index: 1,
                    from_node: 1,
                    to_node: 1,
                    initial_status: LinkStatus::Open,
                    initial_setting: Some(1.0),
                },
                kind: LinkKind::Pipe(Pipe {
                    length: 1000.0,
                    diameter: 0.3,
                    roughness: 100.0,
                    minor_loss: 0.0,
                    check_valve: false,
                    bulk_coeff: None,
                    wall_coeff: None,
                    leak_coeff_1: 0.0,
                    leak_coeff_2: 0.0,
                }),
            }],
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

    #[test]
    fn k_power_is_1e_minus_3() {
        assert!((energy::K_POWER - 1.0e-3).abs() < 1e-15);
    }

    #[test]
    fn rho_g_approx() {
        let rg = energy::rho_times_g(1.0);
        assert!((rg - 9810.0_f64).abs() < 1.0);
    }

    #[test]
    fn efficiency_clamps_to_eta_min() {
        let pump = Pump {
            curve_type: PumpCurveType::PowerFunction,
            head_curve: None,
            power: None,
            efficiency_curve: None,
            default_efficiency: 0.0,
            speed_pattern: None,
            energy_price: None,
            price_pattern: None,
        };
        let net = minimal_network();
        let eta = energy::compute_efficiency(&pump, &net, 0.05, 1.0, 0.0);
        assert!((eta - energy::ETA_MIN).abs() < 1e-15);
    }

    #[test]
    fn efficiency_uses_global_when_no_curve_and_no_default() {
        let pump = Pump {
            curve_type: PumpCurveType::PowerFunction,
            head_curve: None,
            power: None,
            efficiency_curve: None,
            default_efficiency: 0.0,
            speed_pattern: None,
            energy_price: None,
            price_pattern: None,
        };
        let net = minimal_network();
        let eta = energy::compute_efficiency(&pump, &net, 0.05, 1.0, 0.5);
        assert!((eta - 0.5).abs() < 1e-15);
    }

    #[test]
    fn energy_kwh_internal_units() {
        // Q = 0.05 m³/s, H = 20 m → hydraulic power = 9810 × 0.05 × 20 = 9810 W
        // At η=0.75: electrical power = 13080 W → 13.08 kWh over 1 h
        let rg = energy::rho_times_g(1.0); // 9810.0 N/m³
        let q = 0.05_f64; // m³/s
        let dh = 20.0_f64; // m
        let eta = 0.75_f64;
        let dt = 3600.0_f64;
        let w_hyd = rg * q * dh;
        let w_elec = w_hyd / eta;
        let kwh = w_elec * energy::K_POWER * dt / 3600.0;
        assert!((kwh - 13.08).abs() < 0.1, "kwh = {kwh}");
    }

    #[test]
    fn balance_ratio_unity_no_storage_change() {
        let fb = FlowBalance {
            total_inflow: 100.0,
            total_outflow: 100.0,
            demand_deficit: 0.0,
            initial_tank_volume: 50.0,
        };
        let ratio = fb.balance_ratio(50.0);
        assert!((ratio - 1.0).abs() < 1e-12);
    }

    #[test]
    fn balance_ratio_filling_tank() {
        let fb = FlowBalance {
            total_inflow: 110.0,
            total_outflow: 100.0,
            demand_deficit: 0.0,
            initial_tank_volume: 50.0,
        };
        let ratio = fb.balance_ratio(60.0);
        assert!((ratio - 1.0).abs() < 1e-12, "ratio = {ratio}");
    }

    #[test]
    fn balance_ratio_draining_tank() {
        let fb = FlowBalance {
            total_inflow: 100.0,
            total_outflow: 110.0,
            demand_deficit: 0.0,
            initial_tank_volume: 50.0,
        };
        let ratio = fb.balance_ratio(40.0);
        assert!((ratio - 1.0).abs() < 1e-12, "ratio = {ratio}");
    }

    #[test]
    fn init_accounting_sums_tank_volumes() {
        let network = minimal_network();
        let node_states: Vec<NodeState> = vec![NodeState::default(); network.nodes.len()];
        let acc = init_accounting(&network, &node_states);
        assert_eq!(acc.pump_energy.len(), network.links.len());
    }

    #[test]
    fn effective_price_uses_global_when_pump_has_none() {
        let pump = Pump {
            curve_type: PumpCurveType::PowerFunction,
            head_curve: None,
            power: None,
            efficiency_curve: None,
            default_efficiency: 0.0,
            speed_pattern: None,
            energy_price: Some(0.20),
            price_pattern: None,
        };
        let mut net = minimal_network();
        net.options.energy_price = 0.12;
        let price = energy::effective_price(&pump, &net, 0.0);
        assert!((price - 0.20).abs() < 1e-15);
    }
}
