//! Groundwater (§4.1): the two-zone aquifer under each subscribing
//! parcel — an unsaturated zone of uniform moisture over a saturated
//! zone — with the six per-area fluxes, the source-authoritative
//! constitutive relations, and the configurable lateral-discharge power
//! relation. States advance under the §3.5 embedded-pair integrator.

use crate::model::{Aquifer, GroundwaterLink};
use crate::simulation::expression::Expression;
use std::cell::Cell;

/// The §4.1 expression vocabulary, in slot order: HGW HSW HCB HGS KS K
/// THETA PHI FI FU A.
const GW_VOCAB: [&str; 11] = [
    "hgw", "hsw", "hcb", "hgs", "ks", "k", "theta", "phi", "fi", "fu", "a",
];

/// §3.5 tolerance on each integrated state (m or fraction).
const TOL: f64 = 1.0e-5;
/// Integrator step floor (s).
const FLOOR: f64 = 1.0e-3;
/// Clamp margin below porosity and total depth.
const XTOL: f64 = 0.001 * 0.3048;

/// One parcel's aquifer state.
pub struct GwState {
    // Static parameters, SI.
    porosity: f64,
    wilting: f64,
    field_capacity: f64,
    conductivity: f64,
    conduct_slope: f64,
    tension_slope: f64,
    upper_evap_frac: f64,
    lower_evap_depth: f64,
    lower_loss_coeff: f64,
    /// Ground minus bottom elevation (m).
    total_depth: f64,
    /// Threshold height above the bottom for lateral flow (m).
    h_star: f64,
    /// Fixed surface-water height above the bottom (m); `None` reads the
    /// live routed stage.
    h_sw_fixed: Option<f64>,
    a1: f64,
    b1: f64,
    a2: f64,
    b2: f64,
    a3: f64,
    /// The receiving vertex.
    pub vertex: usize,
    /// Bottom elevation (m), for staging the live vertex head.
    bottom_elev: f64,
    /// Custom relations (§4.1), compiled per §9.3: deep **replaces** the
    /// linear reservoir, lateral **adds to** the power relation.
    lateral_expr: Option<Expression>,
    deep_expr: Option<Expression>,
    /// Parcel area (m²), presented to expressions through `A`.
    area: f64,
    /// File-unit factors for expression edges (§14.6): metres per length
    /// unit, m/s per rain-rate unit, m² per land-area unit, and m/s per
    /// lateral-flow unit.
    cv_len: f64,
    cv_rain: f64,
    cv_area: f64,
    cv_gwq: f64,
    /// Once-per-expression §9.3 domain-guard warnings already issued.
    lateral_warned: bool,
    deep_warned: bool,
    /// Guard events pending collection by the session ("lateral"/"deep").
    pub guard_events: Vec<&'static str>,
    // State.
    /// Upper-zone moisture content.
    pub theta: f64,
    /// Saturated (lower) zone depth (m).
    pub lower_depth: f64,
    /// Lateral discharge from the last step (m/s over the parcel area).
    pub flow: f64,
    /// The §3.5 degraded flag for the last step.
    pub degraded: bool,
    // §11.1 subsurface-ledger accounts (m³), integrated per substep.
    pub infil_in: f64,
    pub evap_out: f64,
    pub perc_out: f64,
    pub lateral_out: f64,
    /// Stored water at start (m³).
    pub initial_storage: f64,
}

/// The per-step fluxes at a given state (m/s each).
struct Fluxes {
    upper_evap: f64,
    lower_evap: f64,
    upper_perc: f64,
    lower_loss: f64,
    gw_flow: f64,
}

impl GwState {
    /// Build from a parcel's groundwater link and its aquifer, applying
    /// the initial-condition overrides; `area` is the parcel area (m²)
    /// and `us_units` the file's unit system, both serving the §4.1
    /// custom expressions. Compilation failures are returned as text.
    pub fn build(
        gw: &GroundwaterLink,
        aq: &Aquifer,
        vertex_invert: f64,
        area: f64,
        us_units: bool,
    ) -> Result<GwState, String> {
        let resolve = |n: &str| GW_VOCAB.iter().position(|v| *v == n);
        let compile = |label: &str, text: &Option<String>| -> Result<Option<Expression>, String> {
            match text {
                Some(t) => Expression::compile(t, resolve)
                    .map(Some)
                    .map_err(|e| format!("custom {label} groundwater relation: {e}")),
                None => Ok(None),
            }
        };
        let lateral_expr = compile("lateral", &gw.lateral_expression)?;
        let deep_expr = compile("deep-percolation", &gw.deep_expression)?;
        let bottom = gw.bottom_elev.unwrap_or(aq.bottom_elev);
        let water_table = gw.water_table_elev.unwrap_or(aq.water_table_elev);
        let total_depth = (gw.surface_elev - bottom).max(0.0);
        let h_star = match gw.threshold_elev {
            Some(e) => e - bottom,
            None => vertex_invert - bottom,
        };
        let mut state = GwState {
            porosity: aq.porosity,
            wilting: aq.wilting_point,
            field_capacity: aq.field_capacity,
            conductivity: aq.conductivity,
            conduct_slope: aq.conductivity_slope,
            tension_slope: aq.tension_slope,
            upper_evap_frac: aq.upper_evap_frac,
            lower_evap_depth: aq.lower_evap_depth,
            lower_loss_coeff: aq.lower_loss_coeff,
            total_depth,
            h_star,
            h_sw_fixed: if gw.fixed_surface_depth > 0.0 {
                Some(gw.fixed_surface_depth + vertex_invert - bottom)
            } else {
                None
            },
            a1: gw.a1,
            b1: gw.b1,
            a2: gw.a2,
            b2: gw.b2,
            a3: gw.a3,
            vertex: gw.vertex,
            bottom_elev: bottom,
            lateral_expr,
            deep_expr,
            area,
            cv_len: if us_units { 0.3048 } else { 1.0 },
            cv_rain: if us_units { 0.0254 } else { 0.001 } / 3600.0,
            cv_area: if us_units { 4_046.856_422_4 } else { 10_000.0 },
            cv_gwq: if us_units {
                0.028_316_846_592 / 4_046.856_422_4
            } else {
                1.0e-4
            },
            lateral_warned: false,
            deep_warned: false,
            guard_events: Vec::new(),
            theta: gw.upper_moisture.unwrap_or(aq.upper_moisture),
            lower_depth: (water_table - bottom).clamp(0.0, total_depth),
            flow: 0.0,
            degraded: false,
            infil_in: 0.0,
            evap_out: 0.0,
            perc_out: 0.0,
            lateral_out: 0.0,
            initial_storage: 0.0,
        };
        state.initial_storage = state.stored_volume();
        Ok(state)
    }

    /// Stored water volume (m³): saturated zone at porosity plus the
    /// unsaturated zone at its moisture content (§11.1).
    pub fn stored_volume(&self) -> f64 {
        (self.lower_depth * self.porosity + (self.total_depth - self.lower_depth) * self.theta)
            * self.area
    }

    /// The §14.8 hotstart state: (moisture, water-table elevation m,
    /// lateral flow m/s, unsaturated acceptance m).
    pub fn hotstart_get(&self) -> (f64, f64, f64, f64) {
        (
            self.theta,
            self.bottom_elev + self.lower_depth,
            self.flow,
            (self.total_depth - self.lower_depth) * (self.porosity - self.theta),
        )
    }

    /// Restore the §14.8 hotstart state.
    pub fn hotstart_set(&mut self, theta: f64, table_elev: f64, flow: f64) {
        self.theta = theta.clamp(self.wilting, self.porosity);
        self.lower_depth = (table_elev - self.bottom_elev).clamp(0.0, self.total_depth);
        self.flow = flow;
    }

    /// The water-table elevation (m), for the §14.9 records.
    pub fn table_elevation(&self) -> f64 {
        self.bottom_elev + self.lower_depth
    }

    /// The maximum infiltration volume the unsaturated zone can accept
    /// next step, as a depth over the pervious fraction (m).
    pub fn max_infil_depth(&self, frac_perv: f64) -> f64 {
        if frac_perv <= 0.0 {
            return f64::MAX;
        }
        (self.total_depth - self.lower_depth) * (self.porosity - self.theta) / frac_perv
    }

    /// Advance one hydrology step. `infil` is the surface infiltration
    /// rate over the whole parcel area (m/s), `evap_used` the pervious
    /// surface evaporation already exerted (m/s over the parcel),
    /// `max_evap` the potential rate scaled to the pervious fraction, and
    /// `stage_elev` the receiving vertex's water-surface elevation (m).
    /// Returns the lateral discharge (m/s over the parcel area, positive
    /// toward the vertex).
    pub fn step(
        &mut self,
        dt: f64,
        infil: f64,
        evap_used: f64,
        max_evap: f64,
        stage_elev: f64,
        evap_pattern_factor: f64,
    ) -> f64 {
        // A micro-thin aquifer (under the clamp margin) cannot hold the
        // §4.1 state split; it contributes nothing rather than faulting.
        if self.total_depth <= XTOL {
            return 0.0;
        }
        let avail_evap = (max_evap - evap_used).max(0.0);
        let h_sw = self
            .h_sw_fixed
            .unwrap_or((stage_elev - self.bottom_elev).max(0.0));

        // Per-step flux caps (§4.1): percolation by drainable volume,
        // outflow by lower-zone storage, inflow by unsaturated
        // acceptance.
        let v_upper =
            ((self.total_depth - self.lower_depth) * (self.theta - self.field_capacity)).max(0.0);
        let max_perc = v_upper / dt;
        let max_flow_pos = self.lower_depth * self.porosity / dt;
        let max_flow_neg =
            -((self.total_depth - self.lower_depth) * (self.porosity - self.theta) / dt);

        // §9.3 domain guards observed during flux evaluation, folded
        // into once-per-expression warnings after the step.
        let lateral_guard = Cell::new(false);
        let deep_guard = Cell::new(false);
        // Expression variables in vocabulary order, at the §14.6 file-unit
        // boundary.
        let expr_vars = |theta: f64, lower: f64, upper_perc: f64| -> [f64; 11] {
            let hydcon = self.conductivity * ((theta - self.porosity) * self.conduct_slope).exp();
            [
                lower / self.cv_len,
                h_sw / self.cv_len,
                self.h_star / self.cv_len,
                self.total_depth / self.cv_len,
                self.conductivity / self.cv_rain,
                hydcon / self.cv_rain,
                theta,
                self.porosity,
                infil / self.cv_rain,
                upper_perc / self.cv_rain,
                self.area / self.cv_area,
            ]
        };

        let fluxes = |theta: f64, lower: f64| -> Fluxes {
            let lower = lower.clamp(0.0, self.total_depth);
            let upper_depth = self.total_depth - lower;

            // Evapotranspiration: none during surface infiltration, none
            // below the wilting point; the lower share is complementary
            // and scaled by water-table reach into the cutoff depth.
            // §4.1: the upper share is optionally monthly-patterned; the
            // lower share is complementary to the *patterned* fraction.
            let upper_frac = (self.upper_evap_frac * evap_pattern_factor).min(1.0);
            let (mut upper_evap, mut lower_evap) = (0.0, 0.0);
            if infil <= 0.0 {
                if theta > self.wilting {
                    upper_evap = (upper_frac * max_evap).min(avail_evap);
                }
                if self.lower_evap_depth > 0.0 {
                    let frac = ((self.lower_evap_depth - upper_depth) / self.lower_evap_depth)
                        .clamp(0.0, 1.0);
                    lower_evap =
                        (frac * (1.0 - upper_frac) * max_evap).min(avail_evap - upper_evap);
                }
            }

            // Percolation: the exponential conductivity with the
            // suction-gradient factor the manual omits (§4.1).
            let mut upper_perc = 0.0;
            if upper_depth > 0.0 && theta > self.field_capacity {
                let hydcon =
                    self.conductivity * ((theta - self.porosity) * self.conduct_slope).exp();
                let dhdz =
                    1.0 + self.tension_slope * 2.0 * (theta - self.field_capacity) / upper_depth;
                upper_perc = (hydcon * dhdz).min(max_perc);
            }

            // Deep percolation: the linear reservoir, unless a custom
            // relation replaces it (§4.1), reading in file rain units.
            let lower_loss = match &self.deep_expr {
                Some(e) => {
                    let (v, guarded) = e.eval(&expr_vars(theta, lower, upper_perc));
                    if guarded {
                        deep_guard.set(true);
                    }
                    v * self.cv_rain
                }
                None => self.lower_loss_coeff * lower / self.total_depth,
            }
            .min(lower / dt);

            // Lateral discharge: the power relation, zero at or below the
            // threshold; negative flow zeroed when the interaction term
            // is in use (§4.1).
            let mut gw_flow = 0.0;
            if lower > self.h_star {
                let t1 = if self.b1 == 0.0 {
                    self.a1
                } else {
                    self.a1 * (lower - self.h_star).powf(self.b1)
                };
                let t2 = if self.b2 == 0.0 {
                    self.a2
                } else if h_sw > self.h_star {
                    self.a2 * (h_sw - self.h_star).powf(self.b2)
                } else {
                    0.0
                };
                let t3 = self.a3 * lower * h_sw;
                gw_flow = t1 - t2 + t3;
                if gw_flow < 0.0 && self.a3 != 0.0 {
                    gw_flow = 0.0;
                }
            }
            // A custom lateral relation adds to the power relation —
            // reading in the lateral-coefficient basis — and the caps
            // bound the combined flux (§4.1).
            if let Some(e) = &self.lateral_expr {
                let (v, guarded) = e.eval(&expr_vars(theta, lower, upper_perc));
                if guarded {
                    lateral_guard.set(true);
                }
                gw_flow += v * self.cv_gwq;
            }
            gw_flow = gw_flow.clamp(max_flow_neg, max_flow_pos);
            Fluxes {
                upper_evap,
                lower_evap,
                upper_perc,
                lower_loss,
                gw_flow,
            }
        };

        // The coupled pair under the §3.5 integrator.
        let deriv = |theta: f64, lower: f64| -> (f64, f64) {
            let f = fluxes(theta, lower);
            let q_upper = infil - f.upper_evap - f.upper_perc;
            let q_lower = f.upper_perc - f.lower_loss - f.lower_evap - f.gw_flow;
            let d_upper = self.total_depth - lower;
            let d_theta = if d_upper > 0.0 {
                q_upper / d_upper
            } else {
                0.0
            };
            let deficit = self.porosity - theta;
            let d_lower = if deficit > 0.0 {
                q_lower / deficit
            } else {
                0.0
            };
            (d_theta, d_lower)
        };

        self.degraded = false;
        let (mut t, mut h) = (0.0, dt);
        let (mut th, mut lo) = (self.theta, self.lower_depth);
        while t < dt - 1e-12 {
            h = h.min(dt - t);
            let (th_new, lo_new, err) = cash_karp2(th, lo, h, &deriv);
            if err <= TOL || h <= FLOOR {
                if err > TOL {
                    self.degraded = true;
                }
                // §11.1: integrate the ledger accounts at the substep's
                // starting fluxes.
                let fl = fluxes(th, lo);
                self.infil_in += infil * self.area * h;
                self.evap_out += (fl.upper_evap + fl.lower_evap) * self.area * h;
                self.perc_out += fl.lower_loss * self.area * h;
                self.lateral_out += fl.gw_flow * self.area * h;
                t += h;
                th = th_new;
                lo = lo_new;
                let grow = if err > 0.0 {
                    0.9 * (TOL / err).powf(0.2)
                } else {
                    5.0
                };
                h *= grow.clamp(0.1, 5.0);
            } else {
                let shrink = 0.9 * (TOL / err).powf(0.25);
                h = (h * shrink.clamp(0.1, 5.0)).max(FLOOR);
            }
        }

        // Clamp per §4.1: θ within [wilting, porosity), the water table
        // jumping to the surface at saturation.
        th = th.max(self.wilting);
        if th >= self.porosity {
            th = self.porosity - 1e-6;
            lo = self.total_depth - XTOL;
        }
        lo = lo.clamp(0.0, self.total_depth - XTOL);
        self.theta = th;
        self.lower_depth = lo;
        let f = fluxes(th, lo);
        self.flow = f.gw_flow;
        // The first domain-guarded evaluation of each expression warns
        // once (§9.3).
        if lateral_guard.get() && !self.lateral_warned {
            self.lateral_warned = true;
            self.guard_events.push("lateral");
        }
        if deep_guard.get() && !self.deep_warned {
            self.deep_warned = true;
            self.guard_events.push("deep-percolation");
        }
        self.flow
    }
}

/// The Cash–Karp 4(5) embedded step on the coupled pair; the error is the
/// larger of the two states'.
fn cash_karp2(x: f64, y: f64, h: f64, f: &dyn Fn(f64, f64) -> (f64, f64)) -> (f64, f64, f64) {
    let k1 = f(x, y);
    let k2 = f(x + h * 0.2 * k1.0, y + h * 0.2 * k1.1);
    let k3 = f(
        x + h * (0.075 * k1.0 + 0.225 * k2.0),
        y + h * (0.075 * k1.1 + 0.225 * k2.1),
    );
    let k4 = f(
        x + h * (0.3 * k1.0 - 0.9 * k2.0 + 1.2 * k3.0),
        y + h * (0.3 * k1.1 - 0.9 * k2.1 + 1.2 * k3.1),
    );
    let k5 = f(
        x + h * (-11.0 / 54.0 * k1.0 + 2.5 * k2.0 - 70.0 / 27.0 * k3.0 + 35.0 / 27.0 * k4.0),
        y + h * (-11.0 / 54.0 * k1.1 + 2.5 * k2.1 - 70.0 / 27.0 * k3.1 + 35.0 / 27.0 * k4.1),
    );
    let k6 = f(
        x + h
            * (1631.0 / 55_296.0 * k1.0
                + 175.0 / 512.0 * k2.0
                + 575.0 / 13_824.0 * k3.0
                + 44_275.0 / 110_592.0 * k4.0
                + 253.0 / 4096.0 * k5.0),
        y + h
            * (1631.0 / 55_296.0 * k1.1
                + 175.0 / 512.0 * k2.1
                + 575.0 / 13_824.0 * k3.1
                + 44_275.0 / 110_592.0 * k4.1
                + 253.0 / 4096.0 * k5.1),
    );
    let fifth = |a: f64, k1: f64, k3: f64, k4: f64, k6: f64| {
        a + h * (37.0 / 378.0 * k1 + 250.0 / 621.0 * k3 + 125.0 / 594.0 * k4 + 512.0 / 1771.0 * k6)
    };
    let fourth = |a: f64, k1: f64, k3: f64, k4: f64, k5: f64, k6: f64| {
        a + h
            * (2825.0 / 27_648.0 * k1
                + 18_575.0 / 48_384.0 * k3
                + 13_525.0 / 55_296.0 * k4
                + 277.0 / 14_336.0 * k5
                + 0.25 * k6)
    };
    let x5 = fifth(x, k1.0, k3.0, k4.0, k6.0);
    let y5 = fifth(y, k1.1, k3.1, k4.1, k6.1);
    let x4 = fourth(x, k1.0, k3.0, k4.0, k5.0, k6.0);
    let y4 = fourth(y, k1.1, k3.1, k4.1, k5.1, k6.1);
    (x5, y5, (x5 - x4).abs().max((y5 - y4).abs()))
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl GwState {
    /// Write this state (§12.3). Everything but the compiled expressions and the vertex it drains to, which the model rebuilds identically.
    ///
    /// Exhaustive by design: a field added here fails to compile until it
    /// is written or declared a parameter the model rebuilds.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        #[allow(unused_imports)]
        use crate::simulation::checkpoint::{put_b, put_f, put_fs, put_u};
        let GwState {
            porosity,
            wilting,
            field_capacity,
            conductivity,
            conduct_slope,
            tension_slope,
            upper_evap_frac,
            lower_evap_depth,
            lower_loss_coeff,
            total_depth,
            h_star,
            h_sw_fixed,
            a1,
            b1,
            a2,
            b2,
            a3,
            vertex: _,
            bottom_elev,
            lateral_expr: _,
            deep_expr: _,
            area,
            cv_len,
            cv_rain,
            cv_area,
            cv_gwq,
            lateral_warned,
            deep_warned,
            guard_events: _,
            theta,
            lower_depth,
            flow,
            degraded,
            infil_in,
            evap_out,
            perc_out,
            lateral_out,
            initial_storage,
        } = self;
        put_f(w, *porosity)?;
        put_f(w, *wilting)?;
        put_f(w, *field_capacity)?;
        put_f(w, *conductivity)?;
        put_f(w, *conduct_slope)?;
        put_f(w, *tension_slope)?;
        put_f(w, *upper_evap_frac)?;
        put_f(w, *lower_evap_depth)?;
        put_f(w, *lower_loss_coeff)?;
        put_f(w, *total_depth)?;
        put_f(w, *h_star)?;
        put_b(w, h_sw_fixed.is_some())?;
        put_f(w, h_sw_fixed.unwrap_or(0.0))?;
        put_f(w, *a1)?;
        put_f(w, *b1)?;
        put_f(w, *a2)?;
        put_f(w, *b2)?;
        put_f(w, *a3)?;
        put_f(w, *bottom_elev)?;
        put_f(w, *area)?;
        put_f(w, *cv_len)?;
        put_f(w, *cv_rain)?;
        put_f(w, *cv_area)?;
        put_f(w, *cv_gwq)?;
        put_b(w, *lateral_warned)?;
        put_b(w, *deep_warned)?;
        put_f(w, *theta)?;
        put_f(w, *lower_depth)?;
        put_f(w, *flow)?;
        put_b(w, *degraded)?;
        put_f(w, *infil_in)?;
        put_f(w, *evap_out)?;
        put_f(w, *perc_out)?;
        put_f(w, *lateral_out)?;
        put_f(w, *initial_storage)?;
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.porosity = r.f()?;
        self.wilting = r.f()?;
        self.field_capacity = r.f()?;
        self.conductivity = r.f()?;
        self.conduct_slope = r.f()?;
        self.tension_slope = r.f()?;
        self.upper_evap_frac = r.f()?;
        self.lower_evap_depth = r.f()?;
        self.lower_loss_coeff = r.f()?;
        self.total_depth = r.f()?;
        self.h_star = r.f()?;
        let has = r.b()?;
        let v = r.f()?;
        self.h_sw_fixed = has.then_some(v);
        self.a1 = r.f()?;
        self.b1 = r.f()?;
        self.a2 = r.f()?;
        self.b2 = r.f()?;
        self.a3 = r.f()?;
        self.bottom_elev = r.f()?;
        self.area = r.f()?;
        self.cv_len = r.f()?;
        self.cv_rain = r.f()?;
        self.cv_area = r.f()?;
        self.cv_gwq = r.f()?;
        self.lateral_warned = r.b()?;
        self.deep_warned = r.b()?;
        self.theta = r.f()?;
        self.lower_depth = r.f()?;
        self.flow = r.f()?;
        self.degraded = r.b()?;
        self.infil_in = r.f()?;
        self.evap_out = r.f()?;
        self.perc_out = r.f()?;
        self.lateral_out = r.f()?;
        self.initial_storage = r.f()?;
        Ok(())
    }
}
