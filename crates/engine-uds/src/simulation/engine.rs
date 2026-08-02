//! The session (§12) over the §10.1 routing-period loop.
//!
//! Load parses and validates; run advances routing period by routing
//! period, assembling lateral inflows at each period's start — external
//! inflows and sanitary base flows evaluated at the step-start date,
//! series read with the §10.1 extension contract (inflows fall to zero
//! beyond their range, with a one-time warning; stages hold their ends) —
//! and servicing every reporting boundary passed. Event windows (§10.3)
//! freeze the network between events. Results are recorded at reporting
//! times and served by object identity (§12.2).

use std::collections::HashMap;

use super::time::{civil_from_days, days_from_civil, weekday};
use crate::hydraulics::routing::{Router, RouterRefusal, RoutingReport};
use crate::hydrology::groundwater::GwState;
use crate::hydrology::infiltration::InfilFactors;
use crate::hydrology::rdii::RdiiState;
use crate::hydrology::runoff::{Surface, SurfaceRefusal};
use crate::hydrology::snow::SnowClimate;
use crate::io::objects::parse_network;
use crate::io::survey::Diagnostic;
use crate::io::validate::{validate, ValidationDiagnostic};
use crate::model::{
    InflowKind, Network, OutfallStage, PatternKind, SeriesTime, TimeSeriesSource, VertexKind,
};

/// Truncation threshold for assembled lateral inflows (m³/s) — the
/// predecessor's flow tolerance, converted.
const FLOW_TOL: f64 = 2.832e-7;

/// Why a session could not be opened (§12.1: every entry point returns a
/// typed error rather than faulting).
#[derive(Debug)]
pub enum OpenError {
    /// The file was refused by parsing; the diagnostics say where.
    Parse(Vec<Diagnostic>),
    /// The model was refused by §14.7 validation.
    Validation(Vec<ValidationDiagnostic>),
    /// The router cannot serve this model yet.
    Routing(RouterRefusal),
    /// The surface compartment cannot serve this model yet.
    Surface(SurfaceRefusal),
    /// A control rule was refused at compile (§9.1).
    Controls(String),
    /// A transport configuration this stage does not evaluate yet (§8).
    Transport(String),
}

/// One recorded reporting boundary: every vertex depth and link flow, by
/// index into the model (§12.2 serves them by identity).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Simulation time (s from start).
    pub t: f64,
    /// Vertex depths (m).
    pub depths: Vec<f64>,
    /// Link flows (m³/s), in the user's orientation.
    pub flows: Vec<f64>,
}

/// A run-time notice: the engine reporting on the run as it happens.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeNotice {
    /// Simulation time (s from start).
    pub t: f64,
    /// What happened.
    pub message: String,
}

struct EventWindow {
    start: f64,
    end: f64,
}

/// A loaded, runnable model (§12.1).
pub struct Simulation {
    net: Network,
    router: Router,
    /// The §3 surface compartment, when the model has one.
    surface: Option<Surface>,
    /// §4.1 aquifers by parcel index.
    aquifers: Vec<(usize, GwState)>,
    /// §4.3 sewer-inflow convolutions.
    rdii: Vec<RdiiState>,
    /// Hydrology clock (s from start).
    hydro_t: f64,
    /// Bracketing hydrology laterals for §10.1 linear interpolation.
    hydro_prev: (f64, Vec<f64>),
    hydro_now: (f64, Vec<f64>),
    hydro_degraded_warned: bool,
    /// Epoch instant of simulation start (s).
    start_epoch: f64,
    /// Run duration (s).
    duration: f64,
    report_step: f64,
    next_report: f64,
    routing_period: f64,
    events: Vec<EventWindow>,
    vertex_by_id: HashMap<String, usize>,
    link_by_id: HashMap<String, usize>,
    /// Per-series exhaustion warnings already issued.
    series_warned: Vec<bool>,
    /// Per-vertex lateral overrides (§12.4 boundary forcing).
    lateral_override: HashMap<usize, f64>,
    /// The compiled §9 control system, when the model has rules.
    controls: Option<super::controls::Controls>,
    /// §8.4 network quality, when the model declares constituents.
    quality: Option<crate::transport::NetworkQuality>,
    /// §8.2–§8.3 surface quality, when parcels carry land uses.
    surface_quality: Option<crate::transport::SurfaceQuality>,
    /// Bracketing hydrology lateral mass rates `[p][v]` (unit·m³/s).
    hydro_mass_prev: Vec<Vec<f64>>,
    hydro_mass_now: Vec<Vec<f64>>,
    /// The next rule-evaluation boundary (s) under the rule-step option;
    /// zero rule step evaluates every routing step.
    next_rule_t: f64,
    /// Recorded reporting boundaries.
    pub snapshots: Vec<Snapshot>,
    /// Run-time notices, in time order.
    pub notices: Vec<RuntimeNotice>,
}

impl Simulation {
    /// Load a model from its input text: parse, validate (§14.7 mutations
    /// applied), and build the router. Warning-class diagnostics from
    /// both passes are returned alongside the session.
    pub fn open(
        input: &str,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        let (mut net, diags) = parse_network(input);
        if diags.iter().any(|d| d.kind.is_error()) {
            return Err(OpenError::Parse(diags));
        }
        let findings = validate(&mut net);
        if findings.iter().any(|f| f.kind.is_error()) {
            return Err(OpenError::Validation(findings));
        }
        let router = Router::build(&net).map_err(OpenError::Routing)?;
        let start_epoch_for_surface =
            days_from_civil(net.options.start_date) as f64 * 86_400.0 + net.options.start_time;
        let surface = Surface::build(&net, start_epoch_for_surface).map_err(OpenError::Surface)?;
        let mut aquifers = Vec::new();
        for (pi, p) in net.parcels.iter().enumerate() {
            if let Some(gw) = &p.groundwater {
                let invert = net.vertices[gw.vertex].invert;
                let state = GwState::build(
                    gw,
                    &net.aquifers[gw.aquifer],
                    invert,
                    p.area,
                    net.options.flow_units.is_us(),
                )
                .map_err(|e| {
                    OpenError::Surface(SurfaceRefusal::Incomplete(format!("{}: {e}", p.id)))
                })?;
                aquifers.push((pi, state));
            }
        }
        let rdii = RdiiState::build_all(&net);

        let start_epoch =
            days_from_civil(net.options.start_date) as f64 * 86_400.0 + net.options.start_time;
        let end_epoch =
            days_from_civil(net.options.end_date) as f64 * 86_400.0 + net.options.end_time;
        let duration = (end_epoch - start_epoch).max(0.0);
        let report_step = net.options.report_step.max(1.0);
        let next_report = match net.options.report_start {
            Some((d, s)) => {
                (days_from_civil(d) as f64 * 86_400.0 + s - start_epoch).max(report_step)
            }
            None => report_step,
        };

        // Event windows in run time, sorted, overlaps clipped to the next
        // start (§10.3).
        let mut events: Vec<EventWindow> = net
            .events
            .iter()
            .map(|e| EventWindow {
                start: days_from_civil(e.start_date) as f64 * 86_400.0 + e.start_time - start_epoch,
                end: days_from_civil(e.end_date) as f64 * 86_400.0 + e.end_time - start_epoch,
            })
            .collect();
        events.sort_by(|a, b| a.start.total_cmp(&b.start));
        for i in 0..events.len().saturating_sub(1) {
            let next_start = events[i + 1].start;
            if events[i].end > next_start {
                events[i].end = next_start;
            }
        }

        let vertex_by_id = net
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id.clone(), i))
            .collect();
        let link_by_id = net
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        let n_series = net.timeseries.len();
        let routing_period = net.options.routing_step.max(0.5);

        // §8: this stage evaluates network transport only; surface
        // accumulation-mobilisation and treatment refuse, typed.
        let quality = if net.constituents.is_empty() {
            None
        } else {
            Some(
                crate::transport::NetworkQuality::build(&router, &net)
                    .map_err(OpenError::Transport)?,
            )
        };
        let surface_quality = if net.constituents.is_empty() || net.parcels.is_empty() {
            None
        } else {
            Some(crate::transport::SurfaceQuality::build(&net))
        };

        // §9.1: compile the control rules; never-true premises warn.
        let mut rule_advisories = Vec::new();
        let controls = super::controls::Controls::compile(&net, &mut rule_advisories)
            .map_err(|e| OpenError::Controls(e.0))?;
        let mut notices: Vec<RuntimeNotice> = rule_advisories
            .into_iter()
            .map(|m| RuntimeNotice { t: 0.0, message: m })
            .collect();
        let _ = &mut notices;

        let nv = net.vertices.len();
        Ok((
            Simulation {
                router,
                surface,
                aquifers,
                rdii,
                hydro_t: 0.0,
                hydro_prev: (0.0, vec![0.0; nv]),
                hydro_now: (0.0, vec![0.0; nv]),
                hydro_degraded_warned: false,
                start_epoch,
                duration,
                report_step,
                next_report,
                routing_period,
                events,
                vertex_by_id,
                link_by_id,
                series_warned: vec![false; n_series],
                lateral_override: HashMap::new(),
                controls,
                quality,
                surface_quality,
                hydro_mass_prev: Vec::new(),
                hydro_mass_now: Vec::new(),
                next_rule_t: 0.0,
                snapshots: Vec::new(),
                notices,
                net,
            },
            diags,
            findings,
        ))
    }

    /// Current simulation time (s from start).
    pub fn time(&self) -> f64 {
        self.router.time()
    }

    /// The run duration (s).
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// The routing ledger to date.
    pub fn report(&self) -> &RoutingReport {
        &self.router.report
    }

    /// Depth at a vertex, by identity (m).
    pub fn depth(&self, id: &str) -> Option<f64> {
        self.vertex_by_id.get(id).map(|&v| self.router.depth(v))
    }

    /// Flow in a link, by identity (m³/s), in the user's orientation.
    pub fn flow(&self, id: &str) -> Option<f64> {
        self.link_by_id
            .get(id)
            .map(|&l| self.router.flow(l, &self.net))
    }

    /// Override the lateral inflow at a vertex (§12.4 boundary forcing);
    /// `None` restores the model's own forcing.
    pub fn set_lateral_inflow(&mut self, id: &str, q: Option<f64>) -> bool {
        let Some(&v) = self.vertex_by_id.get(id) else {
            return false;
        };
        match q {
            Some(q) => {
                self.lateral_override.insert(v, q);
            }
            None => {
                self.lateral_override.remove(&v);
            }
        }
        true
    }

    /// Advance the §3 surface compartment in whole hydrology steps until
    /// it covers `period_end` (§10.1): the wet step while precipitation
    /// or ponded water exists, the dry step otherwise, truncated at gage
    /// recording boundaries.
    fn advance_hydrology(&mut self, period_end: f64) {
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let nv = self.net.vertices.len();
        while self.hydro_t < period_end - 1e-9 {
            let epoch = self.start_epoch + self.hydro_t;
            let wet = surface.is_wet(epoch) || self.rdii.iter().any(|r| r.flow > 0.0);
            let mut dt = if wet {
                self.net.options.wet_step
            } else {
                self.net.options.dry_step
            };
            if let Some(b) = surface.next_gage_boundary(epoch) {
                let to_boundary = b - epoch;
                if to_boundary > 1e-9 {
                    dt = dt.min(to_boundary);
                }
            }
            let month = {
                let epoch_days = ((self.start_epoch + self.hydro_t) / 86_400.0).floor() as i64;
                civil_from_days(epoch_days).month
            };
            let m = (month - 1) as usize;
            let evap = self.evaporation_rate(month);
            let rain_factor = self.net.climate.adjust_rainfall[m];
            let fac = InfilFactors {
                conductivity: self.net.climate.adjust_conductivity[m],
                recovery: 1.0,
            };
            let dry_only = self.net.climate.evaporate_dry_only;
            let snow_cl = self.snow_climate(m);
            surface.step(
                epoch,
                dt,
                evap,
                dry_only,
                rain_factor,
                fac,
                snow_cl.as_ref(),
            );
            // §4.1: each aquifer advances on the same clock, reading the
            // routed stage lagged one step (§10.1), its discharge joining
            // the vertex laterals.
            let mut lats = surface.vertex_laterals(nv);
            let np = self.net.constituents.len();
            let mut mass = vec![vec![0.0; lats.len()]; np];
            // §8.2–§8.3: surface quality advances on the same clock; the
            // runoff and drain streams join the lateral mass at their
            // parcels' concentrations.
            if let Some(sq) = &mut self.surface_quality {
                let day = (self.start_epoch + self.hydro_t + dt) / 86_400.0;
                let doy = {
                    let d = civil_from_days(day as i64);
                    let jan1 = crate::io::options::Date {
                        year: d.year,
                        month: 1,
                        day: 1,
                    };
                    (day as i64 - days_from_civil(jan1) + 1) as u32
                };
                let (ss, se) = (self.net.options.sweep_start, self.net.options.sweep_end);
                let in_season = if ss <= se {
                    doy >= ss && doy <= se
                } else {
                    doy >= ss || doy <= se
                };
                let qsteps: Vec<_> = (0..self.net.parcels.len())
                    .map(|pi| surface.qstep(pi))
                    .collect();
                let net = &self.net;
                let start_epoch = self.start_epoch;
                let t_now = self.hydro_t + dt;
                let sv = move |si: usize| series_value_pure(net, start_epoch, si, t_now, false);
                sq.step(net, &qsteps, day, in_season, &sv);
                for (pi, parcel) in self.net.parcels.iter().enumerate() {
                    let q_out = surface.parcel_runoff(pi);
                    if let crate::model::ParcelOutlet::Vertex(v) = parcel.outlet {
                        for (ci, row) in mass.iter_mut().enumerate() {
                            row[v] += sq.conc[pi][ci] * q_out;
                        }
                    }
                    // Control-measure drains carry the parent parcel's
                    // concentration, less any drain removal (§8.1).
                    for &(v, qd) in surface.lid_drains(pi) {
                        for (ci, row) in mass.iter_mut().enumerate() {
                            row[v] += sq.conc[pi][ci] * qd;
                        }
                    }
                }
            }
            for (pi, gw) in &mut self.aquifers {
                let (infil, evap_used) = surface.parcel_infil_evap(*pi);
                let p = &self.net.parcels[*pi];
                let frac_perv = 1.0 - p.frac_imperv;
                let max_evap = evap * frac_perv;
                let stage = self.net.vertices[gw.vertex].invert + self.router.depth(gw.vertex);
                let q = gw.step(dt, infil, evap_used, max_evap, stage);
                lats[gw.vertex] += q * p.area;
                // §8.1: subsurface inflow at its constant concentration.
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[ci][gw.vertex] += (q * p.area).max(0.0) * c.c_groundwater;
                }
                // §9.3: a domain-guarded custom relation announces itself
                // once without changing any result.
                for which in gw.guard_events.drain(..) {
                    self.notices.push(RuntimeNotice {
                        t: self.hydro_t,
                        message: format!(
                            "{}: the custom {which} groundwater relation was domain-guarded \
                             to zero at least once (§9.3)",
                            p.id
                        ),
                    });
                }
            }
            // §4.3: RDII convolutions on the same clock, with the monthly
            // rainfall adjustment applied during preprocessing (§3.1).
            for r in &mut self.rdii {
                let rain = r
                    .gage(&self.net)
                    .map_or(0.0, |g| surface.gage_rate(g, epoch))
                    * rain_factor;
                let q = r.step(&self.net, rain, month, dt);
                lats[r.vertex] += q;
                // §8.1: sewer inflow at its constant concentration.
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[ci][r.vertex] += q * c.c_rdii;
                }
            }
            self.hydro_t += dt;
            self.hydro_prev = std::mem::replace(&mut self.hydro_now, (self.hydro_t, lats));
            self.hydro_mass_prev = std::mem::replace(&mut self.hydro_mass_now, mass);
            if surface.degraded && !self.hydro_degraded_warned {
                self.hydro_degraded_warned = true;
                self.notices.push(RuntimeNotice {
                    t: self.hydro_t,
                    message: "surface integration ran at its floor below tolerance (§3.5)"
                        .to_string(),
                });
            }
        }
        self.surface = Some(surface);
    }

    /// The §4.2 snow climate for this step, when the model melts snow:
    /// air temperature from its series with the monthly offset, wind from
    /// the monthly averages, and the seasonal melt sweep.
    fn snow_climate(&mut self, month_index: usize) -> Option<SnowClimate> {
        use crate::model::{TemperatureSource, WindSource};
        let sm = self.net.climate.snowmelt.clone()?;
        let t = self.hydro_t;
        let ta = match &self.net.climate.temperature {
            Some(TemperatureSource::Series(ts)) => {
                let ts = *ts;
                self.series_value(ts, t, true)
            }
            _ => return None,
        } + self.net.climate.adjust_temperature[month_index];
        let wind = match &self.net.climate.wind {
            WindSource::Monthly(w) => w[month_index],
            WindSource::File => 0.0,
        };
        // Day of year for the seasonal sweep.
        let epoch_days = ((self.start_epoch + t) / 86_400.0).floor() as i64;
        let date = civil_from_days(epoch_days);
        let jan1 = crate::io::options::Date {
            year: date.year,
            month: 1,
            day: 1,
        };
        let day = (epoch_days - days_from_civil(jan1) + 1) as f64;
        Some(SnowClimate {
            ta,
            wind,
            snow_temp: sm.snow_temp,
            ati_weight: sm.ati_weight,
            rnm: sm.negative_melt_ratio,
            elevation: sm.elevation,
            season: (0.017_261_5 * (day - 81.0)).sin(),
            adc_impervious: self.net.climate.adc_impervious,
            adc_pervious: self.net.climate.adc_pervious,
        })
    }

    /// The potential surface evaporation rate (m/s) for a month, from the
    /// §3.1 sources this stage evaluates, plus the monthly adjustment.
    fn evaporation_rate(&self, month: u32) -> f64 {
        use crate::model::EvaporationSource;
        let m = (month - 1) as usize;
        let base = match &self.net.climate.evaporation {
            EvaporationSource::Constant(e) => *e,
            EvaporationSource::Monthly(ms) => ms[m],
            // Series, temperature, and climate-file evaporation join with
            // the §4 climate state.
            _ => 0.0,
        };
        (base + self.net.climate.adjust_evaporation[m]).max(0.0)
    }

    /// Whether run time `t` lies inside an event window; with no events
    /// declared, everything does.
    fn in_event(&self, t: f64) -> bool {
        if self.events.is_empty() {
            return true;
        }
        self.events.iter().any(|e| t >= e.start && t < e.end)
    }

    /// Advance one routing period (§10.1): assemble forcing at the
    /// period's start, update dynamic boundaries, route — or freeze,
    /// between events — and service any reporting boundary passed.
    /// Returns false once the run is complete.
    pub fn step(&mut self) -> bool {
        let t = self.router.time();
        if t >= self.duration - 1e-9 {
            return false;
        }
        let period_end = (t + self.routing_period).min(self.duration);

        if self.in_event(t) {
            self.update_boundary_stages(t);
            self.advance_hydrology(period_end);
            let (base, base_mass) = self.assemble_lateral(t);
            // §10.1: hydrology outputs interpolate linearly to routing
            // times between the bracketing hydrology results.
            let (t0, l0) = (self.hydro_prev.0, self.hydro_prev.1.clone());
            let (t1, l1) = (self.hydro_now.0, self.hydro_now.1.clone());
            let interp = move |tt: f64, lat: &mut [f64]| {
                let f = if t1 > t0 {
                    ((tt - t0) / (t1 - t0)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                for (i, l) in lat.iter_mut().enumerate() {
                    *l = base[i] + l0[i] + f * (l1[i] - l0[i]);
                }
            };
            if self.controls.is_some() || self.quality.is_some() {
                // §9.1: rules evaluate at every routing step — or on the
                // fixed rule-step clock, whose boundaries the stepper
                // lands on — before the step's trials begin; §8.4 quality
                // updates after each accepted step.
                let rule_step = self.net.options.rule_step;
                let np = self.net.constituents.len();
                let nv = self.net.vertices.len();
                let (mt0, mt1) = (self.hydro_prev.0, self.hydro_now.0);
                let mut lat = vec![0.0; nv];
                let mut mass = vec![vec![0.0; nv]; np];
                while self.router.time() < period_end - 1e-9 {
                    let tt = self.router.time();
                    interp(tt, &mut lat);
                    if self.controls.is_some() {
                        let mut cap = period_end;
                        if rule_step > 0.0 {
                            if tt + 1e-9 >= self.next_rule_t {
                                self.apply_controls(tt, &lat);
                                self.next_rule_t = ((tt / rule_step).floor() + 1.0) * rule_step;
                            }
                            cap = cap.min(self.next_rule_t);
                        } else {
                            self.apply_controls(tt, &lat);
                        }
                        self.router.step_once(cap, &lat);
                    } else {
                        self.router.step_once(period_end, &lat);
                    }
                    if let Some(mut q) = self.quality.take() {
                        // §8.1 lateral mass: period-start base plus the
                        // hydrology terms interpolated like their flows.
                        let f = if mt1 > mt0 {
                            ((tt - mt0) / (mt1 - mt0)).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        for p in 0..np {
                            for v in 0..nv {
                                let m0 = self.hydro_mass_prev.get(p).map_or(0.0, |x| x[v]);
                                let m1 = self.hydro_mass_now.get(p).map_or(0.0, |x| x[v]);
                                mass[p][v] = base_mass[p][v] + m0 + f * (m1 - m0);
                            }
                        }
                        q.update(&self.router, &self.net, &lat, &mass, self.router.last_dt());
                        self.quality = Some(q);
                    }
                }
            } else {
                self.router.advance(period_end, &interp);
            }
        } else {
            // Between events the network state freezes and no lateral
            // inflows apply (§10.3).
            self.router.skip_to(period_end);
        }

        while self.next_report <= self.router.time() + 1e-9 {
            self.snapshots.push(Snapshot {
                t: self.next_report,
                depths: (0..self.net.vertices.len())
                    .map(|v| self.router.depth(v))
                    .collect(),
                flows: (0..self.net.links.len())
                    .map(|l| self.router.flow(l, &self.net))
                    .collect(),
            });
            self.next_report += self.report_step;
        }
        true
    }

    /// Run to completion.
    pub fn run(&mut self) {
        while self.step() {}
    }

    /// The calendar decomposition of run time `t`: (month 1–12, weekday
    /// Sunday = 0, hour 0–23, seconds past midnight).
    fn calendar(&self, t: f64) -> (u32, u32, u32, f64) {
        let epoch = self.start_epoch + t;
        let days = (epoch / 86_400.0).floor() as i64;
        let secs = epoch - days as f64 * 86_400.0;
        let date = civil_from_days(days);
        (date.month, weekday(days), (secs / 3600.0) as u32, secs)
    }

    /// A pattern's factor at run time `t`; a missing slot is 1.
    fn pattern_factor(&self, pattern: Option<usize>, t: f64) -> f64 {
        let Some(p) = pattern else {
            return 1.0;
        };
        let pat = &self.net.patterns[p];
        let (month, wday, hour, _) = self.calendar(t);
        let idx = match pat.kind {
            PatternKind::Monthly => (month - 1) as usize,
            PatternKind::Daily => wday as usize,
            PatternKind::Hourly => hour as usize,
            PatternKind::Weekend => {
                if wday == 0 || wday == 6 {
                    hour as usize
                } else {
                    return 1.0;
                }
            }
        };
        pat.factors.get(idx).copied().unwrap_or(1.0)
    }

    /// Evaluate the §9 rules at the current state and apply the winning
    /// per-link settings; fired constant actions land in the action log.
    fn apply_controls(&mut self, t: f64, lat: &[f64]) {
        let Some(mut controls) = self.controls.take() else {
            return;
        };
        let epoch = self.start_epoch + t;
        let (month, _, _, _) = self.calendar(t);
        let rain_factor = self.net.climate.adjust_rainfall[(month - 1) as usize];
        let surface = self.surface.as_ref();
        let net = &self.net;
        let start_epoch = self.start_epoch;
        let gage_intensity =
            move |g: usize| surface.map_or(0.0, |s| s.gage_rate(g, epoch)) * rain_factor;
        let gage_past = move |g: usize, n: u32| {
            surface.map_or(0.0, |s| s.gage_past_depth(g, epoch, n)) * rain_factor
        };
        let series_value = move |si: usize| series_value_pure(net, start_epoch, si, t, true);
        let applied = controls.evaluate(&super::controls::ControlView {
            router: &self.router,
            net: &self.net,
            gage_intensity: &gage_intensity,
            gage_past: &gage_past,
            laterals: lat,
            series_value: &series_value,
            elapsed: t,
            date_days: (self.start_epoch + t) / 86_400.0,
            dt: self.router.last_dt(),
        });
        for (li, v, ai) in applied {
            if self.router.set_setting(li, v) == Some(true) {
                controls.log_action(t, ai, &self.net.links[li].id, v);
            }
        }
        self.controls = Some(controls);
    }

    /// The §9.1 control-action log: (time s, link, setting, rule).
    pub fn control_actions(&self) -> &[(f64, String, f64, String)] {
        self.controls.as_ref().map_or(&[], |c| &c.log)
    }

    /// Constituent index by identity.
    fn constituent_index(&self, pollutant: &str) -> Option<usize> {
        self.net.constituents.iter().position(|c| c.id == pollutant)
    }

    /// Concentration of `pollutant` at vertex `id`, in its declared unit
    /// (§8.4, §12.2).
    pub fn node_concentration(&self, id: &str, pollutant: &str) -> Option<f64> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        let v = *self.vertex_by_id.get(id)?;
        Some(q.c_vertex[p][v])
    }

    /// Concentration of `pollutant` in link `id` (§8.4, §12.2).
    pub fn link_concentration(&self, id: &str, pollutant: &str) -> Option<f64> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        let l = *self.link_by_id.get(id)?;
        q.link_concentration(&self.router, p, l)
    }

    /// The §8 mass ledger for `pollutant`: (admitted, discharged,
    /// reacted, final storage), each in the declared unit times m³.
    pub fn quality_ledger(&self, pollutant: &str) -> Option<(f64, f64, f64, f64)> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        Some((
            q.inflow_mass[p],
            q.outfall_mass[p],
            q.reacted[p],
            q.final_storage[p],
        ))
    }

    /// A series value at run time `t` under the §10.1 extension contract.
    /// `hold_ends` holds the first/last value outside the range (stages);
    /// otherwise the value falls to zero, with a one-time warning when the
    /// series ends before the run does.
    fn series_value(&mut self, si: usize, t: f64, hold_ends: bool) -> f64 {
        let TimeSeriesSource::Points(points) = &self.net.timeseries[si].source else {
            // External-file series need the caller to supply bytes; the
            // model carries only the name (§2.9). Absent data reads as an
            // empty series.
            return 0.0;
        };
        if points.is_empty() {
            return 0.0;
        }
        let epoch = self.start_epoch + t;
        let at = |st: &SeriesTime| -> f64 {
            match st {
                SeriesTime::Elapsed(s) => *s,
                SeriesTime::Absolute { date, seconds } => {
                    days_from_civil(*date) as f64 * 86_400.0 + seconds - self.start_epoch
                }
            }
        };
        let x = match points[0].time {
            SeriesTime::Elapsed(_) => t,
            SeriesTime::Absolute { .. } => epoch - self.start_epoch,
        };
        let first = at(&points[0].time);
        let last = at(&points[points.len() - 1].time);
        if x < first {
            return if hold_ends { points[0].value } else { 0.0 };
        }
        if x > last {
            if hold_ends {
                return points[points.len() - 1].value;
            }
            if !self.series_warned[si] {
                self.series_warned[si] = true;
                self.notices.push(RuntimeNotice {
                    t,
                    message: format!(
                        "series {} exhausted before the run ends; its consumers read zero",
                        self.net.timeseries[si].id
                    ),
                });
            }
            return 0.0;
        }
        // Linear interpolation between bracketing points.
        for w in points.windows(2) {
            let (t0, t1) = (at(&w[0].time), at(&w[1].time));
            if x <= t1 {
                if t1 <= t0 {
                    return w[1].value;
                }
                let f = (x - t0) / (t1 - t0);
                return w[0].value + f * (w[1].value - w[0].value);
            }
        }
        points[points.len() - 1].value
    }

    /// Assemble the lateral inflow vector at a period start (§10.1):
    /// external inflows and sanitary base flows, evaluated at the
    /// step-start date, near-zero values truncated; §12.4 overrides win.
    fn assemble_lateral(&mut self, t: f64) -> (Vec<f64>, Vec<Vec<f64>>) {
        let nv = self.net.vertices.len();
        let np = self.net.constituents.len();
        let mut lat = vec![0.0; nv];
        let mut ext_flow = vec![0.0; nv];
        let mut dwf_flow = vec![0.0; nv];
        let mut mass = vec![vec![0.0; nv]; np];

        for i in 0..self.net.inflows.len() {
            let inflow = &self.net.inflows[i];
            if inflow.kind != InflowKind::Flow {
                continue;
            }
            let (vertex, series, scale, baseline, base_pattern) = (
                inflow.vertex,
                inflow.series,
                inflow.scale,
                inflow.baseline,
                inflow.base_pattern,
            );
            let mut q = baseline * self.pattern_factor(base_pattern, t);
            if let Some(si) = series {
                q += self.series_value(si, t, false) * scale;
            }
            lat[vertex] += q;
            ext_flow[vertex] += q;
        }

        for d in 0..self.net.dry_weather.len() {
            let dwf = &self.net.dry_weather[d];
            if dwf.constituent.is_some() {
                continue;
            }
            let (vertex, average, patterns) = (dwf.vertex, dwf.average, dwf.patterns);
            let mut q = average;
            for p in patterns {
                q *= self.pattern_factor(p, t);
            }
            lat[vertex] += q;
            dwf_flow[vertex] += q;
        }

        // §8.1 mass sources riding those flows: constituent inflows as a
        // concentration on the external flow or a flow-free mass rate,
        // sanitary flow at its global and per-vertex concentrations.
        if np > 0 {
            for i in 0..self.net.inflows.len() {
                let inflow = &self.net.inflows[i];
                let Some(ci) = inflow.constituent else {
                    continue;
                };
                let (vertex, series, scale, baseline, base_pattern, kind, units_factor) = (
                    inflow.vertex,
                    inflow.series,
                    inflow.scale,
                    inflow.baseline,
                    inflow.base_pattern,
                    inflow.kind,
                    inflow.units_factor,
                );
                let mut v = baseline * self.pattern_factor(base_pattern, t);
                if let Some(si) = series {
                    v += self.series_value(si, t, false) * scale;
                }
                match kind {
                    InflowKind::Concentration => {
                        mass[ci][vertex] += v * ext_flow[vertex].max(0.0);
                    }
                    InflowKind::Mass => {
                        mass[ci][vertex] += v * units_factor;
                    }
                    InflowKind::Flow => {}
                }
            }
            for d in 0..self.net.dry_weather.len() {
                let dwf = &self.net.dry_weather[d];
                let Some(ci) = dwf.constituent else {
                    continue;
                };
                let (vertex, average, patterns) = (dwf.vertex, dwf.average, dwf.patterns);
                let mut c = average;
                for p in patterns {
                    c *= self.pattern_factor(p, t);
                }
                mass[ci][vertex] += c * dwf_flow[vertex];
            }
            for (ci, c) in self.net.constituents.iter().enumerate() {
                if c.c_dwf != 0.0 {
                    for v in 0..nv {
                        mass[ci][v] += c.c_dwf * dwf_flow[v];
                    }
                }
            }
        }

        for (&v, &q) in &self.lateral_override {
            lat[v] = q;
        }
        for l in &mut lat {
            if l.abs() < FLOW_TOL {
                *l = 0.0;
            }
        }
        (lat, mass)
    }

    /// Update tidal and series outfall stages for the period (§2.6):
    /// tides indexed by clock time (§14.7), series holding their ends.
    fn update_boundary_stages(&mut self, t: f64) {
        for vi in 0..self.net.vertices.len() {
            let VertexKind::Outfall { stage, .. } = &self.net.vertices[vi].kind else {
                continue;
            };
            match stage {
                OutfallStage::Tidal { curve } => {
                    let (_, _, _, secs) = self.calendar(t);
                    let elev = tidal_stage(&self.net.curves[*curve].points, secs);
                    self.router.set_outfall_stage(vi, elev);
                }
                OutfallStage::Series { series } => {
                    let elev = self.series_value(*series, t, true);
                    self.router.set_outfall_stage(vi, elev);
                }
                _ => {}
            }
        }
    }
}

/// A tidal curve's stage at a clock time (s past midnight): linear in the
/// curve — whose abscissae import converted to seconds — wrapping over
/// the day.
fn tidal_stage(points: &[(f64, f64)], secs: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if points.len() == 1 {
        return points[0].1;
    }
    let secs = secs.rem_euclid(86_400.0);
    for w in points.windows(2) {
        if secs >= w[0].0 && secs <= w[1].0 {
            let f = (secs - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
            return w[0].1 + f * (w[1].1 - w[0].1);
        }
    }
    // Wrap: between the last point and the first, one day on.
    let (t0, v0) = points[points.len() - 1];
    let (t1, v1) = (points[0].0 + 86_400.0, points[0].1);
    if secs >= t0 && t1 > t0 {
        let f = (secs - t0) / (t1 - t0);
        return v0 + f * (v1 - v0);
    }
    points[0].1
}

/// A raw series value at run time `t`, warning-free: ends held for the
/// §9.1 rule-driven contract, zero beyond the range for §8.2 external
/// accumulation (§10.1).
fn series_value_pure(net: &Network, start_epoch: f64, si: usize, t: f64, hold_ends: bool) -> f64 {
    let TimeSeriesSource::Points(points) = &net.timeseries[si].source else {
        return 0.0;
    };
    if points.is_empty() {
        return 0.0;
    }
    let at = |st: &SeriesTime| -> f64 {
        match st {
            SeriesTime::Elapsed(s) => *s,
            SeriesTime::Absolute { date, seconds } => {
                days_from_civil(*date) as f64 * 86_400.0 + seconds - start_epoch
            }
        }
    };
    let x = t;
    if x <= at(&points[0].time) {
        return if hold_ends { points[0].value } else { 0.0 };
    }
    if x >= at(&points[points.len() - 1].time) {
        return if hold_ends {
            points[points.len() - 1].value
        } else {
            0.0
        };
    }
    for w in points.windows(2) {
        let (t0, t1) = (at(&w[0].time), at(&w[1].time));
        if x <= t1 {
            if t1 <= t0 {
                return w[1].value;
            }
            return w[0].value + (w[1].value - w[0].value) * (x - t0) / (t1 - t0);
        }
    }
    points[points.len() - 1].value
}
