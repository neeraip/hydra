use super::*;
use crate::simulation::contract::{FlowBalance, FlowBalanceSummary, MassBalance, PumpEnergy};

/// Per-node result at a single timestep.
///
/// All values are in the internal SI unit system.
#[derive(Debug, Clone)]
pub struct NodeResult {
    /// Total hydraulic head at the node (m above datum).
    pub head: f64,
    /// Gauge pressure at the node (m of water column = head − elevation).
    pub pressure: f64,
    /// Net demand delivered at or extracted from the node (m³/s).
    pub demand: f64,
}

/// Per-link result at a single timestep.
///
/// All values are in the internal SI unit system.
#[derive(Debug, Clone)]
pub struct LinkResult {
    /// Volumetric flow rate through the link (m³/s; positive = from-node → to-node).
    pub flow: f64,
    /// Mean flow velocity in the link (m/s).
    pub velocity: f64,
    /// Head loss across the link (m; positive = from-node head > to-node head).
    pub head_loss: f64,
    /// Dimensionless link status flag (0.0 = closed/inactive, 1.0 = open/active).
    pub status: f64,
}

impl Simulation {
    /// Query a single scalar result for a node at (or near) simulation time `t`.
    ///
    /// `t` snaps to the nearest recorded snapshot within a 0.5 s tolerance;
    /// times further than 0.5 s from any snapshot yield
    /// `Err(SessionError::NoResultsYet)`.
    ///
    /// Returns `Err(SessionError::UnknownId)` if `node_id` is not in the network
    /// and `Err(SessionError::NoResultsYet)` if no hydraulic snapshot was
    /// recorded at or near `t`. All returned values are in the internal SI unit
    /// system (head/pressure in m, demand in m³/s, quality in mg/L or h or %).
    pub fn get_node_result(
        &self,
        node_id: &str,
        quantity: NodeQuantity,
    ) -> Result<f64, SessionError> {
        let network = self.require_loaded_network()?;
        let node_index = self
            .node_index_by_id(node_id)
            .ok_or_else(|| SessionError::UnknownId(node_id.to_string()))?;
        let snapshot = self
            .current_instant
            .as_ref()
            .ok_or(SessionError::NoResultsYet)?;
        let node_state = &snapshot.node_states[node_index];
        let node = &network.nodes[node_index];
        let elevation = node.base.elevation;
        Ok(match quantity {
            NodeQuantity::Head => node_state.head,
            NodeQuantity::GaugePressure => {
                // For tanks, elevation has been adjusted by +min_level during
                // import (so that head = elevation + level works). Physical
                // elevation for pressure is elevation − min_level.
                let physical_elevation = match &node.kind {
                    NodeKind::Tank(tank) => elevation - tank.min_level,
                    _ => elevation,
                };
                node_state.head - physical_elevation
            }
            NodeQuantity::Demand => {
                // Junctions report total demand: consumption + emitter + leakage
                // (matches EPANET NodeDemand = DemandFlow + EmitterFlow + LeakageFlow).
                // Tanks and reservoirs report their net inflow (net_flow):
                // positive = inflow (filling), negative = outflow (supply).
                match &node.kind {
                    NodeKind::Junction(_) => {
                        node_state.demand_flow + node_state.emitter_flow + node_state.leakage_flow
                    }
                    NodeKind::Reservoir(_) | NodeKind::Tank(_) => node_state.net_flow,
                }
            }
            NodeQuantity::Quality => node_state.quality,
        })
    }

    /// Return a link result quantity at the specified simulation time (§8.2.1).
    ///
    /// `t` snaps to the nearest recorded snapshot within a 0.5 s tolerance;
    /// times further than 0.5 s from any snapshot yield
    /// `Err(SessionError::NoResultsYet)`.
    pub fn get_link_result(
        &self,
        link_id: &str,
        quantity: LinkQuantity,
    ) -> Result<f64, SessionError> {
        let network = self.require_loaded_network()?;
        let link_index = self
            .link_index_by_id(link_id)
            .ok_or_else(|| SessionError::UnknownId(link_id.to_string()))?;
        let snapshot = self
            .current_instant
            .as_ref()
            .ok_or(SessionError::NoResultsYet)?;
        let link_state = &snapshot.link_states[link_index];
        let link = &network.links[link_index];

        // EPANET forces flow to zero for closed links at output time
        // (Status <= CLOSED means XHEAD, TEMPCLOSED, CLOSED).
        let is_closed = matches!(
            link_state.status,
            LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
        );

        Ok(match quantity {
            LinkQuantity::Flow => {
                if is_closed {
                    0.0
                } else {
                    link_state.flow
                }
            }
            LinkQuantity::MeanVelocity => {
                if let LinkKind::Pipe(pipe) = &link.kind {
                    let area = std::f64::consts::PI * (pipe.diameter / 2.0).powi(2);
                    if area > 0.0 {
                        link_state.flow.abs() / area
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            LinkQuantity::UnitHeadLoss => {
                if let LinkKind::Pipe(pipe) = &link.kind {
                    let from_node_index = link.base.from_idx();
                    let to_node_index = link.base.to_idx();
                    let head_drop = (snapshot.node_states[from_node_index].head
                        - snapshot.node_states[to_node_index].head)
                        .abs();
                    if pipe.length > 0.0 {
                        head_drop / pipe.length
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            LinkQuantity::FrictionFactor => {
                // Every pipe, whatever head-loss formula ran (§8.2). The
                // quantity is back-computed from the head loss the solve
                // actually produced, so under Hazen-Williams or Chezy-Manning
                // it reports the equivalent Darcy-Weisbach factor that would
                // reproduce the observed loss.
                //
                // This used to return 0 unless the formula was Darcy-Weisbach,
                // and Hazen-Williams is the default, so the column was zero
                // for most models. EPANET has no such gate: `output.c`'s
                // FRICTION case tests only `Type <= PIPE` and
                // `ABS(flow) > TINY` before applying the same inversion, and
                // this is a file format we call EPANET-compatible. Zero
                // belongs to non-pipes and negligible flow, where the
                // inversion is undefined, not to formulas.
                if let LinkKind::Pipe(pipe) = &link.kind {
                    let from_node_index = link.base.from_idx();
                    let to_node_index = link.base.to_idx();
                    let head_drop = (snapshot.node_states[from_node_index].head
                        - snapshot.node_states[to_node_index].head)
                        .abs();
                    // f = dh * D * 2g / (L * v²) where v = Q/A.
                    // Guard: flows below Q_CLOSED (1e-6 m³/s) are
                    // within solver convergence noise — treat as zero.
                    const Q_CLOSED: f64 = 1.0e-6;
                    let area = std::f64::consts::PI * (pipe.diameter / 2.0).powi(2);
                    let velocity = if area > 0.0 && link_state.flow.abs() >= Q_CLOSED {
                        link_state.flow.abs() / area
                    } else {
                        0.0
                    };
                    if velocity > 0.0 && pipe.length > 0.0 {
                        // Friction factor from the DW definition:
                        //   f = Δh · D · 2g / (L · v²)
                        // G_DW is 9.81 m/s² (SI); result is dimensionless.
                        let two_g = 2.0 * crate::hydraulics::G_DW;
                        (head_drop * pipe.diameter * two_g) / (pipe.length * velocity * velocity)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            LinkQuantity::Quality => link_state.quality,
            LinkQuantity::Status => link_status_to_f64(link_state.status),
            LinkQuantity::Setting => link_state.setting,
        })
    }

    /// How many reporting instants the run has recorded so far.
    ///
    /// The session holds only the most recent of them (§8.2 Retention). This
    /// is what a result stream compares against its own count to notice it has
    /// fallen behind, and what a caller uses to know how long a run was
    /// without keeping the run.
    pub fn instants_recorded(&self) -> u64 {
        self.instants_recorded
    }

    /// The reporting time the session currently holds, if it has recorded one.
    ///
    /// A session keeps one instant and accumulates no history (§8.2
    /// Retention), so there is no list of times to return. A caller wanting
    /// every reporting time reads them from the serialized results, where the
    /// reader publishes them.
    pub fn current_time(&self) -> Option<f64> {
        self.current_instant.as_ref().map(|inst| inst.t)
    }

    /// Return all node results at a given simulation time, indexed by position.
    ///
    /// `t` snaps to the nearest recorded snapshot within a 0.5 s tolerance;
    /// times further than 0.5 s from any snapshot yield
    /// `Err(SessionError::NoResultsYet)`.
    ///
    /// Returns one `NodeResult` per node in the same order as `node_ids()`.
    /// Uses direct index access — O(N) with no string lookups.
    pub fn all_node_results(&self) -> Result<Vec<NodeResult>, SessionError> {
        let network = self.require_loaded_network()?;
        let snapshot = self
            .current_instant
            .as_ref()
            .ok_or(SessionError::NoResultsYet)?;

        let mut results = Vec::with_capacity(network.nodes.len());
        for (i, ns) in snapshot.node_states.iter().enumerate() {
            let node = &network.nodes[i];
            let elevation = node.base.elevation;

            let pressure = {
                let physical_elevation = match &node.kind {
                    NodeKind::Tank(tank) => elevation - tank.min_level,
                    _ => elevation,
                };
                ns.head - physical_elevation
            };

            let demand = match &node.kind {
                NodeKind::Junction(_) => ns.demand_flow + ns.emitter_flow + ns.leakage_flow,
                NodeKind::Reservoir(_) | NodeKind::Tank(_) => ns.net_flow,
            };

            results.push(NodeResult {
                head: ns.head,
                pressure,
                demand,
            });
        }
        Ok(results)
    }

    /// Return all link results at a given simulation time, indexed by position.
    ///
    /// `t` snaps to the nearest recorded snapshot within a 0.5 s tolerance;
    /// times further than 0.5 s from any snapshot yield
    /// `Err(SessionError::NoResultsYet)`.
    ///
    /// Returns one `LinkResult` per link in the same order as `link_ids()`.
    /// Uses direct index access — O(L) with no string lookups.
    pub fn all_link_results(&self) -> Result<Vec<LinkResult>, SessionError> {
        let network = self.require_loaded_network()?;
        let snapshot = self
            .current_instant
            .as_ref()
            .ok_or(SessionError::NoResultsYet)?;

        let mut results = Vec::with_capacity(network.links.len());
        for (i, ls) in snapshot.link_states.iter().enumerate() {
            let link = &network.links[i];

            let is_closed = matches!(
                ls.status,
                LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
            );

            let flow = if is_closed { 0.0 } else { ls.flow };

            let velocity = if let LinkKind::Pipe(pipe) = &link.kind {
                let area = std::f64::consts::PI * (pipe.diameter / 2.0).powi(2);
                if area > 0.0 {
                    ls.flow.abs() / area
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let head_loss = if let LinkKind::Pipe(pipe) = &link.kind {
                let from_idx = link.base.from_idx();
                let to_idx = link.base.to_idx();
                let head_drop =
                    (snapshot.node_states[from_idx].head - snapshot.node_states[to_idx].head).abs();
                if pipe.length > 0.0 {
                    head_drop / pipe.length
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let status = link_status_to_f64(ls.status);

            results.push(LinkResult {
                flow,
                velocity,
                head_loss,
                status,
            });
        }
        Ok(results)
    }

    /// Return the node IDs in the order they were indexed at load time.
    pub fn node_ids(&self) -> Vec<&str> {
        match &self.network {
            Some(n) => n.nodes.iter().map(|nd| nd.base.id.as_str()).collect(),
            None => vec![],
        }
    }

    /// Return the link IDs in the order they were indexed at load time.
    pub fn link_ids(&self) -> Vec<&str> {
        match &self.network {
            Some(n) => n.links.iter().map(|lk| lk.base.id.as_str()).collect(),
            None => vec![],
        }
    }

    /// Return the pump link IDs in the order they appear in the network.
    pub fn pump_ids(&self) -> Vec<&str> {
        match &self.network {
            Some(n) => n
                .links
                .iter()
                .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
                .map(|l| l.base.id.as_str())
                .collect(),
            None => vec![],
        }
    }

    /// Return the declared `FlowUnits` of the loaded network.
    pub fn flow_units(&self) -> Option<FlowUnits> {
        self.network.as_ref().map(|n| n.options.flow_units)
    }

    /// Return energy statistics for a pump link (§8.2.1).
    pub fn get_pump_energy(&self, pump_id: &str) -> Result<&PumpEnergy, SessionError> {
        let network = self.require_loaded_network()?;
        let link_index = self
            .link_index_by_id(pump_id)
            .ok_or_else(|| SessionError::UnknownId(pump_id.to_string()))?;
        // Verify it is a pump.
        if !matches!(network.links[link_index].kind, LinkKind::Pump(_)) {
            return Err(SessionError::UnknownId(pump_id.to_string()));
        }
        let accounting_state =
            self.accounting
                .as_ref()
                .ok_or_else(|| SessionError::InvalidPhase {
                    expected: "HydraulicsDone".into(),
                    actual: self.phase.name().to_string(),
                })?;
        Ok(&accounting_state.pump_energy[link_index])
    }

    /// Return the global mass balance from the quality engine (§8.2.1).
    pub fn get_mass_balance(&self) -> Result<&MassBalance, SessionError> {
        let qs = self
            .quality_state
            .as_ref()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "QualityDone".into(),
                actual: self.phase.name().to_string(),
            })?;
        Ok(&qs.mass_balance)
    }

    /// Return the global volumetric flow balance from accounting (§8.2.1).
    pub fn get_flow_balance(&self) -> Result<&FlowBalance, SessionError> {
        let acc = self
            .accounting
            .as_ref()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: self.phase.name().to_string(),
            })?;
        Ok(&acc.flow_balance)
    }

    /// Return the total tank volume at the end of the simulation (m³).
    ///
    /// Sums `NodeState::volume` from the live state vector for all tank nodes.
    pub fn final_tank_volume(&self) -> Result<f64, SessionError> {
        let network = self.require_loaded_network()?;
        let vol: f64 = network
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                if matches!(n.kind, NodeKind::Tank(_)) {
                    Some(self.node_states[i].volume)
                } else {
                    None
                }
            })
            .sum();
        Ok(vol)
    }

    /// Return the complete flow balance summary with derived values.
    pub fn flow_balance_summary(&self) -> Result<FlowBalanceSummary, SessionError> {
        let fb = self.get_flow_balance()?;
        let final_vol = self.final_tank_volume()?;
        Ok(fb.summarize(final_vol))
    }

    /// Borrow the list of simulation warnings.
    pub fn warnings(&self) -> &[SimWarning] {
        &self.warnings
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestNetworkBuilder;
    use crate::SimulationOptions;

    /// Reservoir → J1 → J2 two-pipe network with a 4 h EPS horizon.
    fn eps_network(quality_mode: QualityMode) -> Network {
        TestNetworkBuilder::new()
            .with_options(SimulationOptions {
                duration: 4.0 * 3600.0,
                hyd_step: 3600.0,
                qual_step: 300.0,
                report_step: 3600.0,
                report_start: 0.0,
                quality_mode,
                ..SimulationOptions::default()
            })
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 10.0)
            .junction("J2", 0.0, 5.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "J2", 1000.0, 8.0, 100.0)
            .build()
            .0
    }

    /// Reservoir → J1 ← T1 network with a tank, 4 h EPS horizon.
    fn tank_network() -> Network {
        TestNetworkBuilder::new()
            .with_options(SimulationOptions {
                duration: 4.0 * 3600.0,
                hyd_step: 3600.0,
                report_step: 3600.0,
                report_start: 0.0,
                ..SimulationOptions::default()
            })
            .reservoir("R1", 100.0)
            .junction("J1", 0.0, 10.0)
            .tank("T1", 50.0, 10.0, 0.0, 20.0, 40.0)
            .hw_pipe("P1", "R1", "J1", 1000.0, 12.0, 100.0)
            .hw_pipe("P2", "J1", "T1", 1000.0, 8.0, 100.0)
            .build()
            .0
    }

    fn run_session(net: Network) -> Simulation {
        let mut sess = Simulation::from_network(net).expect("load");
        sess.run().expect("run");
        sess
    }

    #[test]
    fn queries_before_any_step_report_no_snapshot() {
        let sess = Simulation::from_network(eps_network(QualityMode::None)).expect("load");
        let err = sess.get_node_result("J1", NodeQuantity::Head);
        assert!(matches!(err, Err(SessionError::NoResultsYet)));
        let err = sess.all_node_results();
        assert!(matches!(err, Err(SessionError::NoResultsYet)));
    }

    #[test]
    fn all_node_results_agree_with_scalar_queries() {
        let sess = run_session(eps_network(QualityMode::None));
        let all = sess.all_node_results().expect("all_node_results_at");
        let ids = sess.node_ids();
        assert_eq!(all.len(), ids.len());
        for (i, id) in ids.iter().enumerate() {
            let head = sess.get_node_result(id, NodeQuantity::Head).unwrap();
            let pressure = sess
                .get_node_result(id, NodeQuantity::GaugePressure)
                .unwrap();
            let demand = sess.get_node_result(id, NodeQuantity::Demand).unwrap();
            approx::assert_abs_diff_eq!(all[i].head, head, epsilon = 1e-12);
            approx::assert_abs_diff_eq!(all[i].pressure, pressure, epsilon = 1e-12);
            approx::assert_abs_diff_eq!(all[i].demand, demand, epsilon = 1e-12);
        }
    }

    #[test]
    fn all_link_results_agree_with_scalar_queries() {
        let sess = run_session(eps_network(QualityMode::None));
        let all = sess.all_link_results().expect("all_link_results_at");
        let ids = sess.link_ids();
        assert_eq!(all.len(), ids.len());
        for (i, id) in ids.iter().enumerate() {
            let flow = sess.get_link_result(id, LinkQuantity::Flow).unwrap();
            let velocity = sess
                .get_link_result(id, LinkQuantity::MeanVelocity)
                .unwrap();
            let status = sess.get_link_result(id, LinkQuantity::Status).unwrap();
            approx::assert_abs_diff_eq!(all[i].flow, flow, epsilon = 1e-12);
            approx::assert_abs_diff_eq!(all[i].velocity, velocity, epsilon = 1e-12);
            approx::assert_abs_diff_eq!(all[i].status, status, epsilon = 1e-12);
        }
    }

    #[test]
    fn reporting_instants_ascend_and_cover_the_full_horizon() {
        // A session holds one instant, so the series is observed by walking
        // the run rather than by asking it for a list afterwards.
        let mut sess = Simulation::from_network(eps_network(QualityMode::None)).expect("load");
        let mut times = Vec::new();
        loop {
            let dt = sess.step_hydraulics().expect("step");
            if let Some(t) = sess.current_time() {
                if times.last() != Some(&t) {
                    times.push(t);
                }
            }
            if dt == 0.0 {
                break;
            }
        }
        assert!(!times.is_empty());
        assert_eq!(times[0], 0.0);
        assert!(times.windows(2).all(|w| w[0] < w[1]), "times = {times:?}");
        assert_eq!(*times.last().unwrap(), 4.0 * 3600.0);
    }

    #[test]
    fn mass_balance_needs_a_run_not_a_second_pass() {
        let net = eps_network(QualityMode::Age);
        let mut sess = Simulation::from_network(net).expect("load");
        assert!(matches!(
            sess.get_mass_balance(),
            Err(SessionError::InvalidPhase { .. })
        ));
        sess.run_hydraulics().expect("run_hydraulics");
        // Quality rides the run, so its mass balance exists as soon as the
        // run does. It used to require a second pass first.
        let mb = sess.get_mass_balance().expect("mass balance after the run");
        assert!(mb.init.is_finite());
    }

    #[test]
    fn tank_volume_and_flow_balance_summary_available_after_run() {
        let sess = run_session(tank_network());
        let vol = sess.final_tank_volume().expect("final_tank_volume");
        assert!(vol > 0.0, "vol = {vol}");
        let summary = sess.flow_balance_summary().expect("flow_balance_summary");
        assert!(summary.total_inflow >= 0.0);

        // The no-tank network reports zero final tank volume.
        let sess2 = run_session(eps_network(QualityMode::None));
        assert_eq!(sess2.final_tank_volume().expect("volume"), 0.0);
    }
}
