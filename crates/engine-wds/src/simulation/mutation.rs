use super::*;
use crate::ActionValue;

impl Simulation {
    /// True until the first hydraulic step has been taken for the currently
    /// loaded network. Initial-state mutations made in this window are
    /// re-applied to the live states so they behave exactly like loading a
    /// network that had the mutated value from the start (§8.3 mutation
    /// semantics).
    fn before_first_hydraulic_step(&self) -> bool {
        self.phase == Phase::Loaded && self.current_t == 0.0 && !self.has_stepped
    }

    /// Modify a node property (§8.3 `set_node_property()`).
    ///
    /// The mutation takes effect on subsequent simulation behaviour per the
    /// §8.3 mutation semantics: `Elevation` refreshes the solver's
    /// elevation-derived precomputes (and, before the first hydraulic step,
    /// re-derives the node's initial head); `InitialQuality` is consumed at
    /// quality initialisation. Completed steps are never recomputed.
    ///
    /// Returns [`SessionError::InvalidPhase`] if no network is loaded, or
    /// [`SessionError::UnknownId`] if `node_id` does not exist.
    pub fn set_node_property(
        &mut self,
        node_id: &str,
        property: NodeProperty,
        value: f64,
    ) -> Result<(), SessionError> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: self.phase.name().to_string(),
            })?;
        let idx = network
            .nodes
            .iter()
            .position(|n| n.base.id == node_id)
            .ok_or_else(|| SessionError::UnknownId(node_id.to_string()))?;
        match property {
            NodeProperty::Elevation => network.nodes[idx].base.elevation = value,
            NodeProperty::InitialQuality => network.nodes[idx].base.initial_quality = value,
        }

        // Propagate to derived state (§8.3 mutation semantics).
        let network = self.network.as_ref().expect("invariant: network set above");
        match property {
            NodeProperty::Elevation => {
                // Refresh the solver's elevation snapshot and tank head limits.
                if let Some(ctx) = self.solver_ctx.as_mut() {
                    ctx.refresh_node_elevation(network, idx);
                }
                // Before the first step, re-derive the initial reservoir/tank
                // head from the new elevation (§2.4 rules). Mid-run, tank
                // level/volume are preserved and the next tank level update
                // re-derives the head from the new elevation.
                if self.before_first_hydraulic_step() {
                    self.node_states[idx] = init_node_state(network, idx);
                }
            }
            // Consumed at quality initialisation — no derived state to refresh.
            NodeProperty::InitialQuality => {}
        }
        Ok(())
    }

    /// Modify a link property (§8.3 `set_link_property()`).
    ///
    /// The mutation takes effect on subsequent simulation behaviour per the
    /// §8.3 mutation semantics: `Roughness` re-derives the pipe's head-loss
    /// resistance for the next solve (a silent no-op on non-pipe links);
    /// `InitialStatus`/`InitialSetting` re-derive the link's live state before
    /// the first hydraulic step, and mid-run apply to the live state under the
    /// same rules as a control action. Completed steps are never recomputed.
    pub fn set_link_property(
        &mut self,
        link_id: &str,
        property: LinkProperty,
        value: f64,
    ) -> Result<(), SessionError> {
        let network = self
            .network
            .as_mut()
            .ok_or_else(|| SessionError::InvalidPhase {
                expected: "Loaded".into(),
                actual: self.phase.name().to_string(),
            })?;
        let idx = network
            .links
            .iter()
            .position(|l| l.base.id == link_id)
            .ok_or_else(|| SessionError::UnknownId(link_id.to_string()))?;
        match property {
            LinkProperty::Roughness => {
                if let LinkKind::Pipe(p) = &mut network.links[idx].kind {
                    p.roughness = value;
                }
            }
            LinkProperty::InitialStatus => {
                network.links[idx].base.initial_status = if value < 0.5 {
                    LinkStatus::Closed
                } else {
                    LinkStatus::Open
                };
            }
            LinkProperty::InitialSetting => {
                network.links[idx].base.initial_setting = Some(value);
            }
        }

        // Propagate to derived state (§8.3 mutation semantics).
        let network = self.network.as_ref().expect("invariant: network set above");
        match property {
            LinkProperty::Roughness => {
                // Re-derive the pipe's resistance so the next solve uses the
                // new roughness. No-op for non-pipe links.
                if let Some(ctx) = self.solver_ctx.as_mut() {
                    ctx.refresh_pipe_resistance(network, idx);
                }
            }
            LinkProperty::InitialStatus | LinkProperty::InitialSetting => {
                if self.before_first_hydraulic_step() {
                    // Re-derive the live state (status, setting, and initial
                    // flow estimate) under the same §2.6 rules used at load.
                    self.link_states[idx] = init_link_state(network, idx);
                } else {
                    // Mid-run: apply to the live state as a control action
                    // (§4.2.3), effective from the next solve.
                    let action = match property {
                        LinkProperty::InitialStatus => {
                            ActionValue::Status(network.links[idx].base.initial_status)
                        }
                        _ => ActionValue::Setting(value),
                    };
                    controls::apply_link_actions(&mut self.link_states, &[(idx, action)], network);
                }
            }
        }
        Ok(())
    }

    // ── Peak demand cost convenience ──────────────────────────────────────────

    /// Return the total peak demand cost (§7.1).
    pub fn peak_demand_cost(&self) -> f64 {
        match (&self.accounting, &self.network) {
            (Some(acc), Some(network)) => accounting::peak_demand_cost(acc, network),
            _ => 0.0,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestNetworkBuilder;
    use crate::{NodeQuantity, Pump, SimulationOptions};

    /// Reservoir → J1 single-pipe network with a 4 h EPS horizon.
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
            .hw_pipe("P1", "R1", "J1", 100.0, 12.0, 100.0)
            .build()
            .0
    }

    /// Reservoir —pump→ J1 network (constant-horsepower pump).
    fn pump_network() -> Network {
        TestNetworkBuilder::new()
            .with_options(SimulationOptions {
                duration: 3600.0,
                hyd_step: 3600.0,
                report_step: 3600.0,
                report_start: 0.0,
                ..SimulationOptions::default()
            })
            .reservoir("R1", 50.0)
            .junction("J1", 80.0, 10.0)
            .const_hp_pump("PU1", "R1", "J1", 5.0)
            .build()
            .0
    }

    #[test]
    fn initial_quality_mutation_lands_only_before_the_run() {
        // Quality initialises at the first step now (§8.3 mutation
        // semantics), so the window for setting initial quality closes when
        // the run starts. It used to stay open until the separate quality
        // phase began, and a caller mutating between the two phases was in
        // time. Asserting both halves keeps that narrowing honest.
        let mut sess = Simulation::from_network(eps_network(QualityMode::Chemical)).expect("load");
        sess.set_node_property("R1", NodeProperty::InitialQuality, 5.0)
            .expect("set_node_property before the run");
        sess.run_hydraulics().expect("run_hydraulics");

        let quality = sess
            .get_node_result("J1", NodeQuantity::Quality)
            .expect("quality");
        approx::assert_relative_eq!(quality, 5.0, max_relative = 1e-6);

        // The same mutation once the run has begun does not reach quality.
        let mut late = Simulation::from_network(eps_network(QualityMode::Chemical)).expect("load");
        late.run_hydraulics().expect("run_hydraulics");
        late.set_node_property("R1", NodeProperty::InitialQuality, 5.0)
            .expect("accepted");
        let after = late
            .get_node_result("J1", NodeQuantity::Quality)
            .expect("quality");
        assert!(
            (after - 5.0).abs() > 1e-6,
            "a mutation after the run began must not change quality, got {after}"
        );
    }

    #[test]
    fn roughness_mutation_on_non_pipe_is_a_silent_noop() {
        let mut sess = Simulation::from_network(pump_network()).expect("load");
        let before = match &sess.network.as_ref().unwrap().links[0].kind {
            LinkKind::Pump(p) => p.clone(),
            _ => panic!("expected pump"),
        };
        // Roughness only applies to pipes; on a pump it must succeed but
        // change nothing.
        sess.set_link_property("PU1", LinkProperty::Roughness, 42.0)
            .expect("set_link_property");
        let after: &Pump = match &sess.network.as_ref().unwrap().links[0].kind {
            LinkKind::Pump(p) => p,
            _ => panic!("expected pump"),
        };
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }

    #[test]
    fn initial_status_mutation_thresholds_at_half() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::None)).expect("load");
        sess.set_link_property("P1", LinkProperty::InitialStatus, 0.49)
            .expect("set closed");
        assert_eq!(
            sess.network.as_ref().unwrap().links[0].base.initial_status,
            LinkStatus::Closed
        );
        sess.set_link_property("P1", LinkProperty::InitialStatus, 0.5)
            .expect("set open");
        assert_eq!(
            sess.network.as_ref().unwrap().links[0].base.initial_status,
            LinkStatus::Open
        );
    }

    #[test]
    fn mutation_between_hydraulic_steps_is_accepted() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::None)).expect("load");
        let dt = sess.step_hydraulics().expect("first step");
        assert!(dt > 0.0);

        sess.set_link_property("P1", LinkProperty::Roughness, 50.0)
            .expect("mutation mid-run");
        if let LinkKind::Pipe(p) = &sess.network.as_ref().unwrap().links[0].kind {
            approx::assert_abs_diff_eq!(p.roughness, 50.0, epsilon = 1e-12);
        } else {
            panic!("expected pipe");
        }

        // The session keeps stepping to completion after the mutation.
        loop {
            if sess.step_hydraulics().expect("step") == 0.0 {
                break;
            }
        }
        assert_eq!(sess.phase, Phase::HydraulicsDone);
    }

    /// Reservoir → J1 single-pipe network (quality off) with configurable
    /// reservoir elevation and pipe roughness, for mutation-equivalence tests.
    fn eps_network_with(reservoir_elevation: f64, roughness: f64) -> Network {
        TestNetworkBuilder::new()
            .with_options(SimulationOptions {
                duration: 4.0 * 3600.0,
                hyd_step: 3600.0,
                report_step: 3600.0,
                report_start: 0.0,
                ..SimulationOptions::default()
            })
            .reservoir("R1", reservoir_elevation)
            .junction("J1", 0.0, 10.0)
            .hw_pipe("P1", "R1", "J1", 100.0, 12.0, roughness)
            .build()
            .0
    }

    /// Run hydraulics and return (J1 head, P1 flow) at time `t`.
    /// Heads and flows at the instant a session is holding.
    fn head_and_flow(sess: &Simulation) -> (f64, f64) {
        let head = sess
            .get_node_result("J1", NodeQuantity::Head)
            .expect("head");
        let flow = sess
            .get_link_result("P1", crate::LinkQuantity::Flow)
            .expect("flow");
        (head, flow)
    }

    /// Step several runs together, calling `check` at every instant.
    ///
    /// A session holds one instant (§8.2), so a claim about every reporting
    /// time is made by walking the runs in lockstep rather than by asking a
    /// finished session for its history. This compares at every hydraulic
    /// step, which is more than the reporting instants the old form saw.
    fn in_lockstep(sessions: &mut [&mut Simulation], mut check: impl FnMut(&[&mut Simulation])) {
        let mut instants = 0usize;
        loop {
            let mut dt = 0.0;
            for sess in sessions.iter_mut() {
                dt = sess.step_hydraulics().expect("step");
            }
            check(sessions);
            instants += 1;
            if dt == 0.0 {
                break;
            }
        }
        assert!(instants > 1, "the runs took no steps to compare");
    }

    #[test]
    fn roughness_mutation_before_run_matches_freshly_loaded_network() {
        // §8.3 mutation semantics: mutating roughness before the first step
        // must produce the same results as loading a network that had the
        // mutated roughness from the start — and must differ from the
        // unmutated baseline (i.e. the mutation is not a stored-data no-op).
        let mut mutated =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load mutated");
        mutated
            .set_link_property("P1", LinkProperty::Roughness, 50.0)
            .expect("set roughness");
        let mut fresh =
            Simulation::from_network(eps_network_with(100.0, 50.0)).expect("load fresh");
        let mut baseline =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load baseline");

        in_lockstep(&mut [&mut mutated, &mut fresh, &mut baseline], |s| {
            let (h_mut, q_mut) = head_and_flow(s[0]);
            let (h_fresh, q_fresh) = head_and_flow(s[1]);
            approx::assert_relative_eq!(h_mut, h_fresh, max_relative = 1e-12);
            approx::assert_relative_eq!(q_mut, q_fresh, max_relative = 1e-12);

            // Rougher pipe (lower Hazen-Williams C) → more head loss → lower
            // downstream head than the baseline.
            let (h_base, _) = head_and_flow(s[2]);
            assert!(
                h_mut < h_base - 1e-6,
                "mutation had no effect: h_mut = {h_mut}, h_base = {h_base}"
            );
        });
    }

    #[test]
    fn initial_status_closed_before_run_yields_zero_flow_and_matches_fresh_load() {
        let mut mutated =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load mutated");
        mutated
            .set_link_property("P1", LinkProperty::InitialStatus, 0.0)
            .expect("set closed");
        // Same network loaded with the pipe closed from the start.
        let mut closed_net = eps_network_with(100.0, 100.0);
        closed_net.links[0].base.initial_status = LinkStatus::Closed;
        let mut fresh = Simulation::from_network(closed_net).expect("load fresh");

        in_lockstep(&mut [&mut mutated, &mut fresh], |s| {
            let (h_mut, q_mut) = head_and_flow(s[0]);
            let (h_fresh, q_fresh) = head_and_flow(s[1]);
            let t = s[0].current_time().unwrap_or(f64::NAN);
            assert_eq!(q_mut, 0.0, "closed pipe must carry zero flow at t = {t}");
            assert_eq!(q_fresh, 0.0);
            approx::assert_relative_eq!(h_mut, h_fresh, max_relative = 1e-12);

            let status = s[0]
                .get_link_result("P1", crate::LinkQuantity::Status)
                .expect("status");
            assert_eq!(status, 0.0);
        });
    }

    #[test]
    fn roughness_mutation_mid_run_applies_from_next_step() {
        // §8.3 mutation semantics: a mid-run roughness mutation applies from
        // the next hydraulic solve; already-recorded steps are unaffected.
        let mut sess = Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load");
        let mut baseline =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load baseline");
        let mut fresh =
            Simulation::from_network(eps_network_with(100.0, 50.0)).expect("load fresh");

        // A session holds one instant, so each comparison is made while the
        // runs are standing on it rather than afterwards.
        let dt = sess.step_hydraulics().expect("first step");
        assert!(dt > 0.0);
        baseline.step_hydraulics().expect("baseline first step");
        fresh.step_hydraulics().expect("fresh first step");

        // The step solved before the mutation matches the baseline.
        let (h0, _) = head_and_flow(&sess);
        let (h0_base, _) = head_and_flow(&baseline);
        approx::assert_relative_eq!(h0, h0_base, max_relative = 1e-12);

        sess.set_link_property("P1", LinkProperty::Roughness, 50.0)
            .expect("mutate mid-run");

        // Steps solved after the mutation match a network that always had the
        // new roughness (the network is steady, so per-time comparison holds).
        let mut h1;
        loop {
            let dt = sess.step_hydraulics().expect("step");
            fresh.step_hydraulics().expect("fresh step");
            h1 = head_and_flow(&sess).0;
            let (h1_fresh, _) = head_and_flow(&fresh);
            approx::assert_relative_eq!(h1, h1_fresh, max_relative = 1e-12);
            if dt == 0.0 {
                break;
            }
        }
        assert!(
            h1 < h0 - 1e-6,
            "mid-run mutation had no effect: h0 = {h0}, h1 = {h1}"
        );
    }

    #[test]
    fn initial_status_mutation_mid_run_applies_as_status_change() {
        let mut sess = Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load");
        let dt = sess.step_hydraulics().expect("first step");
        assert!(dt > 0.0);

        // Before the mutation the pipe carried the junction demand …
        let (_, q0) = head_and_flow(&sess);
        assert!(q0 > 0.0, "expected positive pre-mutation flow, got {q0}");

        sess.set_link_property("P1", LinkProperty::InitialStatus, 0.0)
            .expect("close mid-run");
        loop {
            if sess.step_hydraulics().expect("step") == 0.0 {
                break;
            }
        }

        // … afterwards it is closed and carries none.
        let (_, q1) = head_and_flow(&sess);
        assert_eq!(q1, 0.0, "closed pipe must carry zero flow after mutation");
        let status = sess
            .get_link_result("P1", crate::LinkQuantity::Status)
            .expect("status");
        assert_eq!(status, 0.0);
    }

    #[test]
    fn elevation_mutation_before_run_matches_freshly_loaded_network() {
        // §8.3 mutation semantics: reservoir elevation mutated before the
        // first step behaves like loading with that elevation from the start.
        // The setter operates in internal units, so the target value is read
        // from an already-converted network rather than passed in user units.
        let fresh_net = eps_network_with(120.0, 100.0);
        let target_elevation = fresh_net.nodes[0].base.elevation;

        let mut mutated =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load mutated");
        mutated
            .set_node_property("R1", NodeProperty::Elevation, target_elevation)
            .expect("set elevation");
        let mut fresh = Simulation::from_network(fresh_net).expect("load fresh");
        let mut baseline =
            Simulation::from_network(eps_network_with(100.0, 100.0)).expect("load baseline");

        in_lockstep(&mut [&mut mutated, &mut fresh, &mut baseline], |s| {
            let (h_mut, q_mut) = head_and_flow(s[0]);
            let (h_fresh, q_fresh) = head_and_flow(s[1]);
            approx::assert_relative_eq!(h_mut, h_fresh, max_relative = 1e-12);
            approx::assert_relative_eq!(q_mut, q_fresh, max_relative = 1e-12);

            let (h_base, _) = head_and_flow(s[2]);
            assert!(
                h_mut > h_base + 1e-6,
                "elevation mutation had no effect: h_mut = {h_mut}, h_base = {h_base}"
            );
        });
    }

    #[test]
    fn elevation_mutation_mid_run_raises_reservoir_head_from_next_step() {
        let net = eps_network_with(100.0, 100.0);
        let original_elevation = net.nodes[0].base.elevation;
        let target_elevation = eps_network_with(120.0, 100.0).nodes[0].base.elevation;

        let mut sess = Simulation::from_network(net).expect("load");
        let dt = sess.step_hydraulics().expect("first step");
        assert!(dt > 0.0);

        // The instant already recorded keeps the original elevation …
        let r0 = sess
            .get_node_result("R1", NodeQuantity::Head)
            .expect("head at the first instant");
        approx::assert_relative_eq!(r0, original_elevation, max_relative = 1e-12);

        sess.set_node_property("R1", NodeProperty::Elevation, target_elevation)
            .expect("mutate mid-run");
        loop {
            if sess.step_hydraulics().expect("step") == 0.0 {
                break;
            }
        }

        // … and every instant from the next step on carries the new one.
        let r1 = sess
            .get_node_result("R1", NodeQuantity::Head)
            .expect("head at the final instant");
        approx::assert_relative_eq!(r1, target_elevation, max_relative = 1e-12);
    }

    #[test]
    fn set_link_property_unknown_id_and_unloaded_session_error() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::None)).expect("load");
        let err = sess.set_link_property("ZZZZ", LinkProperty::Roughness, 1.0);
        assert!(matches!(err, Err(SessionError::UnknownId(_))));

        let mut empty = Simulation::create();
        let err = empty.set_link_property("P1", LinkProperty::Roughness, 1.0);
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
    }

    #[test]
    fn peak_demand_cost_zero_before_load() {
        let sess = Simulation::create();
        assert_eq!(sess.peak_demand_cost(), 0.0);
    }
}
