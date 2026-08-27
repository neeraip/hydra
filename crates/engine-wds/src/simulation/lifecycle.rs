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
            id_index: OnceLock::new(),
            node_states: vec![],
            link_states: vec![],
            current_t: 0.0,
            next_report_t: 0.0,
            report_count: 0,
            states_at_t: (vec![], vec![]),
            has_stepped: false,
            current_instant: None,
            instants_recorded: 0,
            quality_state: None,
            quality_t: 0.0,
            accounting: None,
            warnings: vec![],
            neg_pressure_seen: vec![],
            analysis_begun: None,
            analysis_ended: None,
            post_rejection_dt: None,
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

    /// Load and validate a network, preparing for simulation (§8.3 `load()`).
    ///
    /// Runs the §2.9 validation checks. Returns `SessionError::ValidationFailed`
    /// if any check fails. On success the session transitions to `Loaded`.
    pub fn load(&mut self, mut network: Network) -> Result<(), SessionError> {
        // Derive the pattern index here rather than trusting the caller's.
        //
        // §8.3 accepts a network built programmatically as readily as a parsed
        // one, and §8.5 requires a cache derived from a mutable property to be
        // refreshed when that property changes — this is one, over `patterns`.
        // A caller who assembles a network by hand, or edits `patterns` on a
        // parsed one, would otherwise reach the solver with an index that is
        // empty or stale, and a demand pattern that cannot be looked up is not
        // an error anywhere: `total_demand` falls back to a multiplier of 1,
        // so the pattern is silently dropped rather than loudly misapplied.
        //
        // Derived before validation, so the checks see what the solver will.
        network.build_pattern_index();

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
        // Discard any id → index maps cached for a previously loaded network.
        self.id_index = OnceLock::new();
        self.favad = Some(favad);
        self.solver_ctx = Some(solver_ctx);
        self.node_states = node_states;
        self.link_states = link_states;
        self.current_t = 0.0;
        self.next_report_t = next_report;
        self.states_at_t = (vec![], vec![]);
        self.has_stepped = false;
        self.current_instant = None;
        self.instants_recorded = 0;
        self.quality_state = None;
        self.quality_t = 0.0;
        self.accounting = Some(accounting);
        self.warnings = vec![];
        // A speed pattern's multipliers ARE the speed schedule and supersede
        // the pump's initial speed from the first step (spec §5.4); surface
        // the dead field once at load rather than silently ignoring it.
        if let Some(net) = self.network.as_ref() {
            for (k, link) in net.links.iter().enumerate() {
                if let LinkKind::Pump(pump) = &link.kind {
                    let init = link.base.initial_setting.unwrap_or(1.0);
                    if pump.speed_pattern.is_some() && init != 1.0 {
                        self.warnings.push(crate::simulation::contract::SimWarning {
                            t: 0.0,
                            kind: crate::simulation::contract::WarningKind::PumpSpeedPatternSupersedesSetting {
                                link_index: k,
                            },
                        });
                    }
                }
            }
        }
        self.neg_pressure_seen = vec![false; self.node_states.len()];
        self.post_rejection_dt = None;
        self.phase = Phase::Loaded;
        Ok(())
    }

    /// Run the complete extended-period hydraulic simulation (§8.3 `run_hydraulics()`).
    ///
    /// Requires the session to be in `Loaded` phase.
    pub fn run_hydraulics(&mut self) -> Result<(), SessionError> {
        self.require_phase(Phase::Loaded)?;
        self.analysis_begun = Some(crate::wall_clock::now());
        loop {
            let dt = self.step_hydraulics()?;
            if dt == 0.0 {
                break;
            }
        }
        self.analysis_ended = Some(crate::wall_clock::now());
        Ok(())
    }

    /// Run the full simulation to completion (§8.3 `run()`).
    ///
    /// Quality advances alongside the hydraulics rather than in a pass of its
    /// own, so this is one loop and the run is over when it returns.
    ///
    /// This is the easiest entry point for most users:
    /// 1. [`Simulation::load`]
    /// 2. `run()`
    /// 3. query the instant it holds via [`Simulation::current_time`],
    ///    [`Simulation::get_node_result`], and [`Simulation::get_link_result`].
    pub fn run(&mut self) -> Result<(), SessionError> {
        self.run_hydraulics()?;
        self.analysis_ended = Some(crate::wall_clock::now());
        self.phase = Phase::QualityDone;
        Ok(())
    }

    /// Advance the hydraulic simulation by one adaptive time step (§8.3 `step_hydraulics()`).
    ///
    /// Returns the duration of the step taken (s). Returns 0.0 when the
    /// simulation has reached its end time.
    pub fn step_hydraulics(&mut self) -> Result<f64, SessionError> {
        self.require_phase(Phase::Loaded)?;

        // Record the wall-clock start time on the first step call.
        if self.analysis_begun.is_none() {
            self.analysis_begun = Some(crate::wall_clock::now());
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
        // Past the termination check, this call takes a step. Recorded here
        // rather than inferred from the history, which does not receive an
        // instant on every step (§8.2).
        self.has_stepped = true;

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
                        // The pattern's multipliers ARE the speed schedule
                        // (spec §5.4): they replace the setting, never scale
                        // init_setting — matching EPANET's file semantics.
                        self.link_states[k].setting = factor;
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

        // Quality rides this step rather than replaying it later (§8.2).
        // Initialise on the first step so the instant at t=0 carries initial
        // quality, exactly as the replaced second pass wrote it into the
        // first snapshot.
        let quality_on = self
            .network
            .as_ref()
            .is_some_and(|n| n.options.quality_mode != QualityMode::None);
        if quality_on && self.quality_state.is_none() {
            let network = self
                .network
                .as_ref()
                .expect("invariant: network set in load()");
            let qs = quality::init_quality(network, &self.node_states, &self.link_states)
                .map_err(SessionError::QualityEngine)?;
            self.quality_state = Some(qs);
            self.quality_t = 0.0;
        }
        if quality_on {
            self.stamp_quality_onto_live_states();
            // The flow field quality advances over is the one solved at t,
            // before the tank advance moves it.
            self.states_at_t.0.clone_from(&self.node_states);
            self.states_at_t.1.clone_from(&self.link_states);
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

        // §5.2: after a period accepted only through error rejections, cap the
        // next attempt at twice the accepted interval — re-attempting the full
        // nominal step immediately after the error control has just shown it
        // too coarse only buys another round of rejections. Lapses on the
        // first rejection-free period.
        if let Some(prev) = self.post_rejection_dt {
            dt = dt.min(2.0 * prev);
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
        // §5.3: tank volumes are the only differential state, integrated with
        // a Heun predictor–corrector under a per-step local error estimate.
        // The trial below — rule evaluation, predictor, corrector — is
        // transactional: on rejection every effect is discarded and the
        // period is retried at half the interval, floored at DT_FLOOR.
        let level_err_tol = network.options.level_err_tol;
        let tank_indices: Vec<usize> = network
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| matches!(n.kind, NodeKind::Tank(_)).then_some(i))
            .collect();
        let correcting = level_err_tol > 0.0 && !tank_indices.is_empty();
        let pre_trial = correcting.then(|| (self.node_states.clone(), self.link_states.clone()));
        let v_pre: Vec<f64> = tank_indices
            .iter()
            .map(|&i| self.node_states[i].volume)
            .collect();
        let q_t: Vec<f64> = tank_indices
            .iter()
            .map(|&i| self.node_states[i].net_flow)
            .collect();

        const DT_FLOOR: f64 = 1.0;
        let mut step_overflow: f64;
        let mut rejections = 0u32;
        loop {
            step_overflow = 0.0;
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

            if !correcting {
                break;
            }

            // ── §5.3 corrector: re-solve at the predicted levels and average ──
            let v_star: Vec<f64> = tank_indices
                .iter()
                .map(|&i| self.node_states[i].volume)
                .collect();
            // Statuses as the predictor left them. The corrector's own solve
            // may reclassify a link under ../hydraulics §3.9 — a pump past
            // its maximum head, a check valve closing, a PRV changing mode —
            // and that makes its flows a sample of a different regime (§5.3
            // smoothness precondition). Simple controls and rules cannot do
            // this: they are applied outside the corrector.
            let status_before: Vec<LinkStatus> =
                self.link_states.iter().map(|l| l.status).collect();
            {
                let network = self
                    .network
                    .as_ref()
                    .expect("invariant: network set in load()");
                let favad = self.favad.as_ref().expect("invariant: favad set in load()");
                let solver_ctx = self
                    .solver_ctx
                    .as_mut()
                    .expect("invariant: solver_ctx set in load()");
                // The predictor already wrote V* into the tank heads, so this is
                // an ordinary solve warm-started from the period's opening state.
                // An Unbalanced result is not separately warned here: the next
                // period's opening solve starts from the same state and re-reports
                // (and halts, under UNBALANCED STOP) if it persists.
                hydraulics::solve_hydraulic_step(
                    network,
                    favad,
                    solver_ctx,
                    &mut self.node_states,
                    &mut self.link_states,
                    t,
                    controls::pswitch,
                )
                .map_err(SessionError::HydraulicSolve)?;
            }

            let switched = self
                .link_states
                .iter()
                .zip(&status_before)
                .any(|(l, before)| l.status != *before);

            if switched {
                // §5.3: the predictor is regime-consistent across the
                // interval; the trapezoid would average two different flow
                // regimes and integrate neither. Take V* — the same choice a
                // clamped tank gets — accept the step, and run no error test,
                // because halving cannot reduce a discontinuity. It would
                // retry at the same switch one interval later, and again,
                // down to the floor.
                for (j, &i) in tank_indices.iter().enumerate() {
                    let network = self
                        .network
                        .as_ref()
                        .expect("invariant: network set in load()");
                    let NodeKind::Tank(tank) = &network.nodes[i].kind else {
                        continue;
                    };
                    let level = tank.level_from_volume(v_star[j], &network.curves);
                    let head = tank.head_from_level(network.nodes[i].base.elevation, level);
                    let ns = &mut self.node_states[i];
                    ns.volume = v_star[j];
                    ns.level = level;
                    ns.head = head;
                }
                break;
            }

            let network = self
                .network
                .as_ref()
                .expect("invariant: network set in load()");
            let mut e_h_max = 0.0_f64;
            let mut worst_tank = tank_indices[0];
            let mut corrected: Vec<(usize, f64)> = Vec::with_capacity(tank_indices.len());
            for (j, &i) in tank_indices.iter().enumerate() {
                let NodeKind::Tank(tank) = &network.nodes[i].kind else {
                    continue;
                };
                // A tank clamped by boundary enforcement during the predictor
                // keeps its predictor value (§5.3): the trapezoid would average
                // across the clamp's flow discontinuity, and §5.2's Δt_tank term
                // already lands boundary crossings exactly.
                let v_raw = v_pre[j] + q_t[j] * dt;
                if (self.node_states[i].volume - v_raw).abs() > 1e-9 * v_raw.abs().max(1.0) {
                    continue;
                }
                let q_star = self.node_states[i].net_flow;
                let mut v_corr = v_pre[j] + 0.5 * dt * (q_t[j] + q_star);
                // Keep the correction inside the tank's physical band; a sliver
                // above v_max is real outflow on an overflow tank.
                let v_min = tank.volume_from_level(tank.min_level, &network.curves);
                let v_max = tank.volume_from_level(tank.max_level, &network.curves);
                if v_corr > v_max {
                    if tank.overflow {
                        step_overflow += v_corr - v_max;
                    }
                    v_corr = v_max;
                } else if v_corr < v_min {
                    v_corr = v_min;
                }
                // The estimate is the actual level difference between corrected
                // and predicted volumes — the spec's e_h through the local
                // surface area, exact in the tank's own geometry.
                let level_corr = tank.level_from_volume(v_corr, &network.curves);
                let level_star = tank.level_from_volume(v_star[j], &network.curves);
                let e_h = (level_corr - level_star).abs();
                if e_h > e_h_max {
                    e_h_max = e_h;
                    worst_tank = i;
                }
                corrected.push((i, v_corr));
            }

            if e_h_max <= level_err_tol || dt <= DT_FLOOR {
                // Accept: apply the corrected volumes.
                for (i, v_corr) in corrected {
                    let NodeKind::Tank(tank) = &network.nodes[i].kind else {
                        continue;
                    };
                    let level = tank.level_from_volume(v_corr, &network.curves);
                    let head = tank.head_from_level(network.nodes[i].base.elevation, level);
                    let ns = &mut self.node_states[i];
                    ns.volume = v_corr;
                    ns.level = level;
                    ns.head = head;
                }
                if e_h_max > level_err_tol {
                    // At the floor with the tolerance still exceeded: accepted
                    // with degraded accuracy, and said so (§5.3).
                    self.warnings.push(SimWarning {
                        t,
                        kind: WarningKind::TankLevelAccuracy {
                            node_index: worst_tank,
                        },
                    });
                }
                break;
            }

            // Reject: restore the complete pre-trial state and retry shorter.
            let (nodes, links) = pre_trial
                .as_ref()
                .expect("invariant: correcting implies a snapshot");
            self.node_states.clone_from(nodes);
            self.link_states.clone_from(links);
            dt = (dt / 2.0).max(DT_FLOOR);
            rejections += 1;
        }
        self.post_rejection_dt = (rejections > 0).then_some(dt);

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

        // Advance quality across the step just taken, now that the tank
        // advance has settled dt. The interval is clamped at the duration so
        // the final period matches what the replaced second pass produced,
        // where the last interval ran to the duration rather than past it.
        if quality_on {
            let duration = self
                .network
                .as_ref()
                .expect("invariant: network set in load()")
                .options
                .duration;
            let dt_q = (t + dt).min(duration) - t;
            if dt_q > 0.0 {
                let network = self
                    .network
                    .as_ref()
                    .expect("invariant: network set in load()");
                let (ns, ls) = (&self.states_at_t.0, &self.states_at_t.1);
                if let Some(qs) = self.quality_state.as_mut() {
                    quality::advance_quality(qs, network, ns, ls, dt_q, t);
                    self.quality_t = t + dt_q;
                }
            }
        }

        let new_t = t + dt;
        self.current_t = new_t;

        Ok(dt)
    }

    /// Complete the run. Quality advanced alongside hydraulics (§8.2), so
    /// there is nothing left to do but settle the phase.
    ///
    /// Retained for one release so callers driving the old two-phase
    /// lifecycle keep working; the spec's lifecycle is `run` / `step`.
    pub fn run_quality(&mut self) -> Result<(), SessionError> {
        self.require_phase(Phase::HydraulicsDone)?;
        self.analysis_ended = Some(crate::wall_clock::now());
        self.phase = Phase::QualityDone;
        Ok(())
    }

    /// Formerly one quality sub-cycle. Quality now advances inside
    /// `step_hydraulics`, so this only settles the phase and reports that
    /// there is nothing to advance.
    pub fn step_quality(&mut self) -> Result<f64, SessionError> {
        if self.phase != Phase::HydraulicsDone && self.phase != Phase::QualityDone {
            return Err(SessionError::InvalidPhase {
                expected: "HydraulicsDone".into(),
                actual: self.phase.name().to_string(),
            });
        }
        self.analysis_ended = Some(crate::wall_clock::now());
        self.phase = Phase::QualityDone;
        Ok(0.0)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestNetworkBuilder;
    use crate::SimulationOptions;

    /// Reservoir → J1, whose 10 L/s demand follows a pattern that doubles in
    /// the second of two hours. Parsed rather than built, so the network
    /// starts out exactly as a caller's would before they touch it.
    const PATTERNED_DEMAND: &[u8] = b"\
[JUNCTIONS]
J1  0  10

[RESERVOIRS]
R1  100

[PIPES]
P1  R1  J1  1000  12  100  0  Open

[PATTERNS]
WANTED  1.0  2.0

[DEMANDS]
J1  10  WANTED

[TIMES]
Duration  2:00
Hydraulic Timestep  1:00
Pattern Timestep  1:00
Report Timestep  1:00

[OPTIONS]
Units  LPS
Headloss  H-W

[END]
";

    /// The demand delivered at J1 at the instant the session holds.
    fn j1_demand(sess: &Simulation) -> f64 {
        sess.get_node_result("J1", crate::NodeQuantity::Demand)
            .expect("demand")
    }

    /// Demand at each reporting instant of a run, in order.
    ///
    /// A session holds one instant (§8.2), so a per-hour claim is made by
    /// walking the run and reading as each instant is recorded.
    fn j1_demand_by_instant(sess: &mut Simulation) -> Vec<f64> {
        let mut out = Vec::new();
        let mut last = None;
        loop {
            let dt = sess.step_hydraulics().expect("step");
            let t = sess.current_time();
            if t.is_some() && t != last {
                out.push(j1_demand(sess));
                last = t;
            }
            if dt == 0.0 {
                break;
            }
        }
        out
    }

    /// A caller's patterns must apply even when the caller never built the
    /// index they are looked up through.
    ///
    /// §8.3 accepts a programmatically built network, and one assembled by
    /// hand arrives with an empty `pattern_index` — nothing in the data model
    /// obliges a caller to fill it, and the crate's own test builder fills it
    /// for them, which is why nothing here saw this. A lookup that misses is
    /// an error at no layer: `total_demand` falls back to a multiplier of 1,
    /// so the pattern is silently discarded and the run quietly answers a
    /// different question.
    #[test]
    fn a_hand_built_networks_patterns_are_not_silently_discarded() {
        let mut network = crate::dialect::parse(PATTERNED_DEMAND).expect("parses");
        // What a caller who assembled this themselves would hand over.
        network.pattern_index = Default::default();

        let mut sess = Simulation::from_network(network).expect("loads");
        let demands = j1_demand_by_instant(&mut sess);

        assert!(demands.len() >= 2, "expected at least two instants");
        assert!(
            (demands[0] - 0.010).abs() < 1e-9,
            "first hour should draw the base demand, got {}",
            demands[0]
        );
        assert!(
            (demands[1] - 0.020).abs() < 1e-9,
            "second hour should follow the pattern, got {}",
            demands[1]
        );
    }

    /// Editing `patterns` before loading must not misapply them.
    ///
    /// The index maps an id to a *position*, so inserting or reordering
    /// leaves every entry after the edit pointing at a neighbour — which
    /// applies a real pattern, just the wrong one. §8.5 requires a cache
    /// derived from a mutable property to be refreshed when it changes, and
    /// this is where that refresh has to happen for a caller who edits the
    /// model the session is about to take ownership of.
    #[test]
    fn patterns_edited_before_load_are_not_misapplied() {
        let mut network = crate::dialect::parse(PATTERNED_DEMAND).expect("parses");
        // Insert ahead of the pattern in use. The stale index still says
        // WANTED is at position 0, which is now this one.
        network.patterns.insert(
            0,
            crate::Pattern {
                id: "INSERTED".into(),
                factors: vec![1.0, 9.0],
            },
        );

        let mut sess = Simulation::from_network(network).expect("loads");
        let demands = j1_demand_by_instant(&mut sess);

        let second_hour = demands[1];
        assert!(
            (second_hour - 0.020).abs() < 1e-9,
            "should follow WANTED (×2), got {second_hour}"
        );
    }

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

    #[test]
    fn run_completes_hydraulics_and_quality_and_exposes_results() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::Age)).expect("load");
        sess.run().expect("run");
        assert_eq!(sess.phase, Phase::QualityDone);

        assert_eq!(sess.current_time(), Some(4.0 * 3600.0));
        let head = sess
            .get_node_result("J1", crate::NodeQuantity::Head)
            .expect("head");
        assert!(head.is_finite() && head > 0.0, "head = {head}");
        // Age at the reservoir stays 0; downstream junction age is positive.
        let age = sess
            .get_node_result("J2", crate::NodeQuantity::Quality)
            .expect("age");
        assert!(age > 0.0, "age = {age}");
    }

    #[test]
    fn step_hydraulics_sentinel_reaches_duration_exactly() {
        let net = eps_network(QualityMode::None);
        let duration = net.options.duration;
        let mut sess = Simulation::from_network(net).expect("load");

        let mut total = 0.0;
        let mut steps = 0;
        loop {
            let dt = sess.step_hydraulics().expect("step_hydraulics");
            if dt == 0.0 {
                break;
            }
            total += dt;
            steps += 1;
            assert!(steps < 1000, "did not terminate");
        }
        assert!((total - duration).abs() < 1e-6, "total = {total}");
        assert_eq!(sess.phase, Phase::HydraulicsDone);

        // Stepping past completion is a phase error, not a silent no-op.
        let err = sess.step_hydraulics();
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
    }

    #[test]
    fn quality_reaches_the_duration_without_being_driven() {
        // Quality rides the hydraulic step (§8.2), so by the time the run
        // returns it has already advanced to the duration. Nothing remains
        // for a caller to drive.
        let net = eps_network(QualityMode::Age);
        let duration = net.options.duration;
        let mut sess = Simulation::from_network(net).expect("load");
        sess.run_hydraulics().expect("run_hydraulics");

        assert!(
            (sess.quality_t - duration).abs() < 1e-6,
            "quality_t = {}, duration = {duration}",
            sess.quality_t
        );
        assert!(
            sess.quality_state.is_some(),
            "quality initialised during the run"
        );
    }

    #[test]
    fn step_hydraulics_before_load_is_phase_error() {
        let mut sess = Simulation::create();
        let err = sess.step_hydraulics();
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
    }

    #[test]
    fn run_quality_before_hydraulics_is_phase_error() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::Age)).expect("load");
        let err = sess.run_quality();
        assert!(matches!(err, Err(SessionError::InvalidPhase { .. })));
        // The failed call must not have corrupted the phase.
        assert_eq!(sess.phase, Phase::Loaded);
    }

    #[test]
    fn reload_after_completed_run_resets_session() {
        let mut sess = Simulation::from_network(eps_network(QualityMode::Age)).expect("load");
        sess.run().expect("first run");
        assert!(sess.current_time().is_some());

        sess.load(eps_network(QualityMode::None)).expect("reload");
        assert_eq!(sess.phase, Phase::Loaded);
        assert!(
            sess.current_time().is_none(),
            "the held instant must be dropped on reload"
        );
        assert!(sess.warnings().is_empty(), "warnings must be reset");

        sess.run().expect("second run");
        assert_eq!(sess.phase, Phase::QualityDone);
    }

    #[test]
    fn load_invalid_network_fails_validation_and_stays_created() {
        let mut net = eps_network(QualityMode::None);
        // Corrupt a link endpoint: out-of-bounds node index fails validation.
        net.links[0].base.from_node = 999;

        let mut sess = Simulation::create();
        let err = sess.load(net);
        assert!(matches!(err, Err(SessionError::ValidationFailed(_))));
        assert_eq!(sess.phase, Phase::Created);
        // The session is still unusable until a valid load.
        assert!(matches!(
            sess.run_hydraulics(),
            Err(SessionError::InvalidPhase { .. })
        ));
    }

    /// §5.4: a speed pattern's multipliers ARE the speed schedule — they
    /// replace the pump's initial setting rather than scaling it (matching
    /// EPANET), and the superseded initial speed is surfaced once at load.
    #[test]
    fn speed_pattern_replaces_initial_setting_and_warns_at_load() {
        let inp = b"[TITLE]\nSpeed pattern supersedes SPEED\n\n\
            [JUNCTIONS]\nJ1  0  200\n\n\
            [RESERVOIRS]\nR1  0\n\n\
            [PUMPS]\nPU1  R1  J1  HEAD C1  SPEED 0.9  PATTERN SP1\n\n\
            [CURVES]\nC1  0  400\nC1  1000  200\n\n\
            [PATTERNS]\nSP1  1.0\n\n\
            [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n\
            [TIMES]\nDuration  1:00\nHydraulic Timestep  1:00\n\n[END]\n";
        let net = crate::dialect::parse(inp).expect("parse");
        let mut sess = Simulation::from_network(net).expect("load");

        // The dead SPEED field is reported once, at t=0, naming the pump.
        assert!(
            sess.warnings().iter().any(|w| matches!(
                w.kind,
                crate::simulation::contract::WarningKind::PumpSpeedPatternSupersedesSetting {
                    link_index: 0
                }
            )),
            "expected the superseded-speed warning at load"
        );

        // After the first step the live setting is the pattern value (1.0),
        // not init_setting × pattern (0.9).
        sess.step_hydraulics().expect("step");
        assert_eq!(sess.link_states[0].setting, 1.0);
    }

    /// A pump at the default initial speed (1.0) with a speed pattern is the
    /// normal authoring style and must not warn.
    #[test]
    fn speed_pattern_with_default_initial_speed_does_not_warn() {
        let inp = b"[TITLE]\nSpeed pattern, default SPEED\n\n\
            [JUNCTIONS]\nJ1  0  200\n\n\
            [RESERVOIRS]\nR1  0\n\n\
            [PUMPS]\nPU1  R1  J1  HEAD C1  PATTERN SP1\n\n\
            [CURVES]\nC1  0  400\nC1  1000  200\n\n\
            [PATTERNS]\nSP1  1.0  0.5\n\n\
            [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n\
            [TIMES]\nDuration  1:00\nHydraulic Timestep  1:00\n\n[END]\n";
        let net = crate::dialect::parse(inp).expect("parse");
        let sess = Simulation::from_network(net).expect("load");
        assert!(sess.warnings().is_empty());
    }
}
