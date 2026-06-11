use super::*;

impl Default for Simulation {
    fn default() -> Self {
        Self::create()
    }
}

impl Simulation {
    /// Allocate a new empty session (§8.3 `create()`).
    pub fn create() -> Self {
        Simulation {
            phase: Phase::Created,
            network: None,
            favad: None,
            solver_ctx: None,
            node_states: vec![],
            link_states: vec![],
            current_t: 0.0,
            next_report_t: 0.0,
            report_count: 0,
            hyd_snapshots: vec![],
            quality_state: None,
            quality_t: 0.0,
            accounting: None,
            warnings: vec![],
            neg_pressure_seen: vec![],
            analysis_begun: None,
            analysis_ended: None,
        }
    }

    /// Create a session from a network and validate/load it.
    ///
    /// This is a convenience for the common sequence:
    /// 1. `Simulation::create()`
    /// 2. `session.load(network)`
    pub fn from_network(network: Network) -> Result<Self, SessionError> {
        let mut session = Self::create();
        session.load(network)?;
        Ok(session)
    }

    /// Load and validate a network, preparing for simulation.
    /// Load and validate a network, preparing for simulation (§8.3 `load()`).
    ///
    /// Runs the §2.9 validation checks. Returns `SessionError::ValidationFailed`
    /// if any check fails. On success the session transitions to `Loaded`.
    pub fn load(&mut self, network: Network) -> Result<(), SessionError> {
        // Validate (§8.1.2 / §2.9).
        network.validate().map_err(SessionError::ValidationFailed)?;

        // Build FAVAD coefficients (§2.10).
        let favad = network.compute_favad();

        // Build solver context (§3.6 Phase 1 + 2).
        let solver_ctx = hydraulics::build_solver_context(&network, &favad)
            .map_err(SessionError::HydraulicSolve)?;

        // Initialise node states from static data.
        let node_states = init_node_states(&network);
        let link_states = init_link_states(&network);

        // Initialise accounting.
        let accounting = accounting::init_accounting(&network, &node_states);

        let options = &network.options;
        let next_report = options.report_start;

        self.network = Some(network);
        self.favad = Some(favad);
        self.solver_ctx = Some(solver_ctx);
        self.node_states = node_states;
        self.link_states = link_states;
        self.current_t = 0.0;
        self.next_report_t = next_report;
        self.hyd_snapshots = vec![];
        self.quality_state = None;
        self.quality_t = 0.0;
        self.accounting = Some(accounting);
        self.warnings = vec![];
        self.neg_pressure_seen = vec![false; self.node_states.len()];
        self.phase = Phase::Loaded;
        Ok(())
    }

    /// Run the complete extended-period hydraulic simulation (§8.3 `run_hydraulics()`).
    ///
    /// Requires the session to be in `Loaded` phase.
    pub fn run_hydraulics(&mut self) -> Result<(), SessionError> {
        self.require_phase(Phase::Loaded)?;
        self.analysis_begun = Some(SystemTime::now());
        loop {
            let dt = self.step_hydraulics()?;
            if dt == 0.0 {
                break;
            }
        }
        self.analysis_ended = Some(SystemTime::now());
        Ok(())
    }

    /// Run the full simulation to completion (hydraulics then quality).
    ///
    /// This is the easiest entry point for most users:
    /// 1. [`Simulation::load`]
    /// 2. `run()`
    /// 3. query results via [`Simulation::snapshot_times`],
    ///    [`Simulation::get_node_result`], and [`Simulation::get_link_result`].
    pub fn run(&mut self) -> Result<(), SessionError> {
        self.run_hydraulics()?;
        self.run_quality()
    }

    /// Advance the hydraulic simulation by one adaptive time step (§8.3 `step_hydraulics()`).
    ///
    /// Returns the duration of the step taken (s). Returns 0.0 when the
    /// simulation has reached its end time.
    pub fn step_hydraulics(&mut self) -> Result<f64, SessionError> {
        self.require_phase(Phase::Loaded)?;

        // Record the wall-clock start time on the first step call.
        if self.analysis_begun.is_none() {
            self.analysis_begun = Some(SystemTime::now());
        }

        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let t = self.current_t;
        let duration = network.options.duration;

        if t > duration {
            self.phase = Phase::HydraulicsDone;
            return Ok(0.0);
        }

        // Apply pump speed patterns: setting = init_setting × pattern_factor.
        // Done before simple controls so controls can override (matches EPANET).
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        for (k, link) in network.links.iter().enumerate() {
            if let LinkKind::Pump(pump) = &link.kind {
                if let Some(ref pat_id) = pump.speed_pattern {
                    if let Some(pat) = network.pattern_by_id(pat_id) {
                        let factor = pat.eval(
                            t,
                            network.options.pattern_step,
                            network.options.pattern_start,
                        );
                        self.link_states[k].setting =
                            link.base.initial_setting.unwrap_or(1.0) * factor;
                    }
                }
            }
        }

        // Apply simple controls (§4.1 — evaluated once before the solve).
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let _changed =
            controls::apply_simple_controls(network, &self.node_states, &mut self.link_states, t);

        // Solve (§3). Rule-based controls are evaluated AFTER the solve,
        // within the time-step computation — see the rule sub-step loop below.
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let favad = self.favad.as_ref().expect("invariant: favad set in load()");
        let solver_context = self
            .solver_ctx
            .as_mut()
            .expect("invariant: solver_ctx set in load()");
        let result = hydraulics::solve_hydraulic_step(
            network,
            favad,
            solver_context,
            &mut self.node_states,
            &mut self.link_states,
            t,
            controls::pswitch,
        )
        .map_err(SessionError::HydraulicSolve)?;

        if result == SolveResult::Unbalanced {
            self.warnings.push(SimWarning {
                t,
                kind: WarningKind::UnbalancedHydraulics,
            });
            // EPANET: Haltflag — if ExtraIter == -1, terminate after this step.
            if network.options.extra_iter < 0 {
                self.maybe_record_snapshot(t);
                self.phase = Phase::HydraulicsDone;
                return Ok(0.0);
            }
        }

        // Emit pressure warnings for junctions in DDA mode.
        // EPANET: only for junctions where head < elevation AND demand > 0.
        // Deduplicated per node — only the first occurrence is recorded.
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        for (i, node) in network.nodes.iter().enumerate() {
            if !self.neg_pressure_seen[i]
                && matches!(node.kind, NodeKind::Junction(_))
                && self.node_states[i].head < node.base.elevation
                && self.node_states[i].demand_flow > 0.0
            {
                self.neg_pressure_seen[i] = true;
                self.warnings.push(SimWarning {
                    t,
                    kind: WarningKind::NegativePressure { node_index: i },
                });
            }
        }

        // Emit pump out-of-range warnings (EPANET writehydwarn flag=4).
        // EPANET checks: status >= OPEN, flow > setting*Qmax or flow < 0.
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let ctx = self
            .solver_ctx
            .as_ref()
            .expect("invariant: solver_ctx set in load()");
        for (k, link) in network.links.iter().enumerate() {
            if let LinkKind::Pump(_) = &link.kind {
                let link_state = &self.link_states[k];
                if matches!(link_state.status, LinkStatus::Open | LinkStatus::Active) {
                    let qmax = ctx.pump_qmax(k);
                    if link_state.flow > link_state.setting * qmax || link_state.flow < 0.0 {
                        self.warnings.push(SimWarning {
                            t,
                            kind: WarningKind::PumpXHead { link_index: k },
                        });
                    }
                }
            }
        }

        // Record snapshot at t AFTER solve, BEFORE tank advance.
        // This matches EPANET's output ordering: solve → output → advance.
        self.maybe_record_snapshot(t);

        // Compute adaptive Δt AFTER solve (§5.2) so current flows are used
        // for the control timestep prediction (§5.2.1).
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let mut dt = timestep::adaptive_timestep(t, network, &self.node_states);

        // Shorten timestep for approaching simple controls (§5.2.1).
        let dt_control =
            timestep::control_timestep(t, network, &self.node_states, &self.link_states);
        if dt_control < dt {
            dt = dt_control;
        }

        if dt == 0.0 {
            // Final step: solved and recorded at t=duration, no advance needed.
            // EPANET (nexthyd): when Dur == 0, still accumulates energy with
            // dt normalised to 1 hour (3600 s).  For non-zero duration, this
            // is the last step so no further energy accumulation is needed
            // (integral was accumulated in all previous steps).
            if duration == 0.0 {
                let network = self
                    .network
                    .as_ref()
                    .expect("invariant: network set in load()");
                let pump_powers = accounting::precompute_pump_powers(
                    network,
                    &self.node_states,
                    &self.link_states,
                );
                let accounting = self
                    .accounting
                    .as_mut()
                    .expect("invariant: accounting set in load()");
                accounting::accumulate_step(
                    accounting,
                    network,
                    &self.node_states,
                    &pump_powers,
                    3600.0,
                    t,
                    0.0,
                );
            }
            self.phase = Phase::HydraulicsDone;
            return Ok(0.0);
        }

        // ── Rule sub-step loop (§4.2.1) ──────────────────────────────────
        // Advance tank levels in sub-steps, evaluating rule-based controls at
        // each sub-step.  If a rule fires (any action changes a link state),
        // the hydraulic period is shortened to the elapsed sub-step time.
        // When no rules exist, advance tanks by the full dt in one step.
        //
        // Pre-compute pump powers BEFORE tank levels are advanced, matching
        // EPANET's getallpumpsenergy() → timestep() → addenergy() ordering.
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let pump_powers =
            accounting::precompute_pump_powers(network, &self.node_states, &self.link_states);
        let mut step_overflow: f64 = 0.0;
        if !network.rules.is_empty() {
            let rule_step = network.options.rule_timestep;
            let mut elapsed = 0.0;

            // First sub-step aligned to even multiples of rule_step from t=0
            // (§4.2.1): δ = rule_step − (t mod rule_step), may be < rule_step.
            let first_dt = {
                let rem = t % rule_step;
                let d = rule_step - rem;
                if d <= 0.0 || d > rule_step {
                    rule_step
                } else {
                    d
                }
            };
            let mut dt1 = first_dt.min(dt);
            if dt1 == 0.0 {
                dt1 = rule_step.min(dt);
            }

            loop {
                // Advance tank levels by sub-step.
                let updates = timestep::update_tank_levels(network, &self.node_states, dt1);
                for u in &updates {
                    let node_state = &mut self.node_states[u.node_index];
                    node_state.head = u.new_head;
                    node_state.level = u.new_level;
                    node_state.volume = u.new_volume;
                    step_overflow += u.overflow_volume;
                }
                elapsed += dt1;

                // Evaluate rules at the sub-stepped time (t + elapsed).
                let sub_t = t + elapsed;
                if let Some((actions, _then_fired)) =
                    controls::eval_rules(network, &self.node_states, &self.link_states, sub_t)
                {
                    let any_changed =
                        controls::apply_link_actions(&mut self.link_states, &actions, network);
                    if any_changed {
                        // Rule fired — shorten the hydraulic period to elapsed.
                        dt = elapsed;
                        break;
                    }
                }

                // Update remaining time.
                let remaining = dt - elapsed;
                if remaining <= 0.0 {
                    break;
                }
                dt1 = rule_step.min(remaining);
            }
        } else {
            // No rules — advance tanks by the full dt in one step.
            let updates = timestep::update_tank_levels(network, &self.node_states, dt);
            for u in &updates {
                let node_state = &mut self.node_states[u.node_index];
                node_state.head = u.new_head;
                node_state.level = u.new_level;
                node_state.volume = u.new_volume;
                step_overflow += u.overflow_volume;
            }
        }

        // Accumulate accounting (uses the possibly-shortened dt and pre-computed pump powers).
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let accounting = self
            .accounting
            .as_mut()
            .expect("invariant: accounting set in load()");
        accounting::accumulate_step(
            accounting,
            network,
            &self.node_states,
            &pump_powers,
            dt,
            t,
            step_overflow,
        );

        let new_t = t + dt;
        self.current_t = new_t;

        Ok(dt)
    }

    /// Run the complete quality simulation (§8.3 `run_quality()`).
    ///
    /// Requires hydraulics to be done.
    pub fn run_quality(&mut self) -> Result<(), SessionError> {
        self.require_phase(Phase::HydraulicsDone)?;
        // Initialise quality state.
        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        if network.options.quality_mode == QualityMode::None {
            self.analysis_ended = Some(SystemTime::now());
            self.phase = Phase::QualityDone;
            return Ok(());
        }
        // Use first snapshot states for initialisation.
        let (init_ns, init_ls) = self.first_snapshot_states();
        let qs = quality::init_quality(network, init_ns, init_ls)
            .map_err(SessionError::QualityEngine)?;

        // Write initial quality (node_conc and avgqual) into the first snapshot (t=0).
        // For Trace mode this ensures the trace node reports 100 % at t=0.
        if let Some(snap0) = self.hyd_snapshots.first_mut() {
            for (i, ns) in snap0.node_states.iter_mut().enumerate() {
                ns.quality = qs.node_conc[i];
            }
            for (k, ls) in snap0.link_states.iter_mut().enumerate() {
                ls.quality = quality::avg_link_quality(
                    &qs,
                    k,
                    network.links[k].base.from_idx(),
                    network.links[k].base.to_idx(),
                );
            }
        }

        self.quality_state = Some(qs);
        self.quality_t = 0.0;
        loop {
            let dt = self.step_quality()?;
            if dt == 0.0 {
                break;
            }
        }
        self.analysis_ended = Some(SystemTime::now());
        Ok(())
    }

    /// Advance the quality simulation by one hydraulic time step's worth of
    /// sub-steps (§8.3 `step_quality()`).
    ///
    /// Returns the hydraulic duration advanced (s). Returns 0.0 at end.
    pub fn step_quality(&mut self) -> Result<f64, SessionError> {
        if self.phase != Phase::HydraulicsDone && self.phase != Phase::QualityDone {
            return Err(SessionError::InvalidPhase {
                expected: "HydraulicsDone".into(),
                actual: self.phase.name().to_string(),
            });
        }

        // Lazy-initialise quality state on the first call so that step_quality()
        // can be used directly (e.g. in a CLI progress loop) without requiring a
        // prior run_quality() call.  run_quality() already initialises explicitly
        // before its own loop, so quality_state will be Some when reached there
        // and this block is skipped.
        if self.quality_state.is_none() {
            let network = self
                .network
                .as_ref()
                .expect("invariant: network set in load()");
            if network.options.quality_mode == QualityMode::None {
                self.analysis_ended = Some(SystemTime::now());
                self.phase = Phase::QualityDone;
                return Ok(0.0);
            }
            let (init_ns, init_ls) = self.first_snapshot_states();
            let qs = quality::init_quality(network, init_ns, init_ls)
                .map_err(SessionError::QualityEngine)?;
            if let Some(snap0) = self.hyd_snapshots.first_mut() {
                for (i, ns) in snap0.node_states.iter_mut().enumerate() {
                    ns.quality = qs.node_conc[i];
                }
                for (k, ls) in snap0.link_states.iter_mut().enumerate() {
                    ls.quality = quality::avg_link_quality(
                        &qs,
                        k,
                        network.links[k].base.from_idx(),
                        network.links[k].base.to_idx(),
                    );
                }
            }
            self.quality_state = Some(qs);
            self.quality_t = 0.0;
        }

        let network = self
            .network
            .as_ref()
            .expect("invariant: network set in load()");
        let duration = network.options.duration;
        let qt = self.quality_t;
        if qt >= duration {
            self.analysis_ended = Some(SystemTime::now());
            self.phase = Phase::QualityDone;
            return Ok(0.0);
        }

        // Find the snapshot at qt — this gives the flow field for this period.
        let snap_idx = self.find_snapshot_index_at(qt);
        let snap_idx = match snap_idx {
            Some(idx) => idx,
            None => {
                self.phase = Phase::QualityDone;
                return Ok(0.0);
            }
        };

        // dt_h = time from this snapshot to the next one (or end of simulation).
        // Quality results are written to the NEXT snapshot because EPANET
        // reports initial quality at t=0 and the quality after transport at
        // subsequent report times.
        let next_snap_idx = snap_idx + 1;
        let next_t = if next_snap_idx < self.hyd_snapshots.len() {
            self.hyd_snapshots[next_snap_idx].t
        } else {
            duration
        };
        let dt_h = next_t - qt;
        if dt_h <= 0.0 {
            self.phase = Phase::QualityDone;
            return Ok(0.0);
        }

        // Borrow node/link states from the snapshot without cloning.
        // NLL ensures these shared borrows end after advance_quality returns,
        // allowing the mutable write-back to next_snap_idx below.
        let node_states = &self.hyd_snapshots[snap_idx].node_states;
        let link_states = &self.hyd_snapshots[snap_idx].link_states;

        if let Some(qs) = self.quality_state.as_mut() {
            quality::advance_quality(qs, network, node_states, link_states, dt_h, qt);
            // Write-back quality to the NEXT snapshot (the one at t=next_t).
            // Quality at snap[0] (t=0) keeps its initial values.
            if next_snap_idx < self.hyd_snapshots.len() {
                let snap = &mut self.hyd_snapshots[next_snap_idx];
                for (i, ns) in snap.node_states.iter_mut().enumerate() {
                    ns.quality = qs.node_conc[i];
                }
                for (k, ls) in snap.link_states.iter_mut().enumerate() {
                    ls.quality = quality::avg_link_quality(
                        qs,
                        k,
                        network.links[k].base.from_idx(),
                        network.links[k].base.to_idx(),
                    );
                    ls.reaction_rate = qs.pipe_rate_coeff[k];
                }
            }
            self.quality_t = qt + dt_h;
        }

        if self.quality_t >= duration {
            self.phase = Phase::QualityDone;
        }
        Ok(dt_h)
    }
}
