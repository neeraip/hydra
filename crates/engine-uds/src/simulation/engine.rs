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

        Ok((
            Simulation {
                router,
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
                snapshots: Vec::new(),
                notices: Vec::new(),
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
            let lateral = self.assemble_lateral(t);
            self.router
                .advance(period_end, &move |_t, lat: &mut [f64]| {
                    lat.copy_from_slice(&lateral);
                });
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
    fn assemble_lateral(&mut self, t: f64) -> Vec<f64> {
        let nv = self.net.vertices.len();
        let mut lat = vec![0.0; nv];

        for i in 0..self.net.inflows.len() {
            let inflow = &self.net.inflows[i];
            if inflow.kind != InflowKind::Flow {
                continue; // quality inflows join with §8 transport
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
        }

        for (&v, &q) in &self.lateral_override {
            lat[v] = q;
        }
        for l in &mut lat {
            if l.abs() < FLOW_TOL {
                *l = 0.0;
            }
        }
        lat
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
