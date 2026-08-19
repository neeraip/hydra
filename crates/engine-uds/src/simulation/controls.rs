//! Operational control (§9.1–§9.2): prioritised `IF`/`THEN` rules over
//! the model-semantics observable vocabulary, with conventional `AND`/`OR`
//! precedence, per-link pending-slot conflict resolution, curve, series,
//! and PID-valued actions, and named variables and expressions through
//! the §9.3 language. Premise comparisons and constant action values read
//! in the file's unit system; times compare in days.

use super::expression::Expression;
use crate::io::options::FlowUnits;
use crate::model::{CurveKind, LinkKind, Network};

/// A quantity a premise can observe.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Source {
    /// Current gage intensity (file rain-rate unit).
    GageIntensity(usize),
    /// Past rainfall depth over `n` completed hourly buckets (file
    /// rain-depth unit), `n` ≤ 48.
    GagePast(usize, u32),
    Vertex(usize, VertexAttr),
    Link(usize, LinkAttr),
    Sim(SimAttr),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VertexAttr {
    Depth,
    MaxDepth,
    Head,
    Volume,
    Inflow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum LinkAttr {
    Flow,
    Depth,
    Velocity,
    Status,
    Setting,
    TimeOpen,
    TimeClosed,
    FullFlow,
    FullDepth,
    Length,
    Slope,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SimAttr {
    /// Elapsed time (days).
    Time,
    /// Calendar date (civil days).
    Date,
    /// Clock time (fraction of a day).
    ClockTime,
    /// Day of week, Sunday = 1 (predecessor convention).
    Day,
    /// Month 1–12.
    Month,
    /// Day of year 1–366.
    DayOfYear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rel {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A premise's left-hand side.
#[derive(Debug, Clone)]
enum Lhs {
    Var(Source),
    /// A named §9.3 expression, by index into the compiled set.
    Expr(usize),
}

/// A premise's right-hand side.
#[derive(Debug, Clone)]
enum Rhs {
    Value(f64),
    Var(Source),
}

#[derive(Debug, Clone)]
struct Premise {
    lhs: Lhs,
    rel: Rel,
    rhs: Rhs,
    /// Whether the comparison carries the half-step time tolerance.
    timed: bool,
}

/// What an action assigns.
#[derive(Debug, Clone)]
enum ActionValue {
    Constant(f64),
    /// Curve looked up at the rule's last-compared premise value.
    Curve(usize),
    /// Series looked up at the current date (raw values).
    Series(usize),
    /// §9.2 PID controller with its error history.
    Pid {
        kp: f64,
        ki: f64,
        kd: f64,
    },
}

#[derive(Debug, Clone)]
struct Action {
    link: usize,
    value: ActionValue,
    /// Index of the owning rule.
    rule: usize,
    /// PID error history (e₁, e₂), advanced on evaluation (§9.2).
    e1: f64,
    e2: f64,
    /// The most recently evaluated modulated value.
    value_now: f64,
}

struct Rule {
    name: String,
    /// OR-groups of AND-chained premises (conventional precedence §9.1).
    premises: Vec<Vec<Premise>>,
    then_actions: Vec<usize>,
    else_actions: Vec<usize>,
    priority: f64,
}

/// The compiled §9 control system.
pub struct Controls {
    rules: Vec<Rule>,
    actions: Vec<Action>,
    /// Named variables: sources behind the §9.3 vocabulary.
    variables: Vec<Source>,
    /// Named expressions over the variables.
    expressions: Vec<Expression>,
    // File-unit factors (§9.1 boundary).
    cv_len: f64,
    cv_flow: f64,
    cv_rain: f64,
    cv_rain_depth: f64,
    cv_vol: f64,
    /// Control actions taken, as (elapsed s, link, setting, rule name);
    /// modulated actions excluded (§9.1).
    pub log: Vec<(f64, String, f64, String)>,
    /// Per-expression §9.3 domain-guard warnings already issued.
    expr_warned: Vec<bool>,
    /// Guard events pending collection by the session: expression names.
    pub guard_events: Vec<String>,
    /// The expression names, for the §9.3 warning text.
    expr_names: Vec<String>,
}

/// The observable state a rule evaluation reads.
pub struct ControlView<'a> {
    pub router: &'a crate::hydraulics::routing::Router,
    pub net: &'a Network,
    /// Current intensity (m/s) by gage; zero for unreferenced gages.
    pub gage_intensity: &'a dyn Fn(usize) -> f64,
    /// Past rainfall depth (m) over `n` completed hourly buckets.
    pub gage_past: &'a dyn Fn(usize, u32) -> f64,
    /// Current lateral inflow per vertex (m³/s), the INFLOW premise.
    pub laterals: &'a [f64],
    /// Raw value of series `si` at the current time, ends held (§9.1).
    pub series_value: &'a dyn Fn(usize) -> f64,
    /// Elapsed simulation time (s).
    pub elapsed: f64,
    /// Civil date (days) including the day fraction.
    pub date_days: f64,
    /// Routing step (s), for the half-step time tolerance and PID.
    pub dt: f64,
}

/// A compile failure, with the offending rule named.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlError(pub String);

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

const OBJECTS: [&str; 9] = [
    "GAGE",
    "NODE",
    "LINK",
    "CONDUIT",
    "PUMP",
    "ORIFICE",
    "WEIR",
    "OUTLET",
    "SIMULATION",
];

impl Controls {
    /// Compile the retained `[CONTROLS]` text against the model. Returns
    /// `None` when the model has no rules. `advisories` collects the
    /// §9.1 never-true premise warnings.
    pub fn compile(
        net: &Network,
        advisories: &mut Vec<String>,
    ) -> Result<Option<Controls>, ControlError> {
        let text = &net.controls;
        if text.rules.is_empty() {
            return Ok(None);
        }
        let us = net.options.flow_units.is_us();
        let mut c = Controls {
            rules: Vec::new(),
            actions: Vec::new(),
            variables: Vec::new(),
            expressions: Vec::new(),
            cv_len: if us { 0.3048 } else { 1.0 },
            cv_flow: match net.options.flow_units {
                FlowUnits::Cfs => 0.028_316_846_592,
                FlowUnits::Gpm => 6.309_019_64e-5,
                FlowUnits::Mgd => 0.043_812_636_4,
                FlowUnits::Cms => 1.0,
                FlowUnits::Lps => 1.0e-3,
                FlowUnits::Mld => 1.0 / 86.4,
            },
            cv_rain: if us { 0.0254 } else { 1.0e-3 } / 3600.0,
            cv_rain_depth: if us { 0.0254 } else { 1.0e-3 },
            cv_vol: if us { 0.028_316_846_592 } else { 1.0 },
            log: Vec::new(),
            expr_warned: Vec::new(),
            guard_events: Vec::new(),
            expr_names: Vec::new(),
        };

        // Named variables: `VARIABLE vname = object id attribute`.
        let mut var_names: Vec<String> = Vec::new();
        for line in &text.variables {
            let toks: Vec<&str> = line.split_whitespace().collect();
            if toks.len() < 5 || toks[2] != "=" {
                return Err(ControlError(format!("malformed VARIABLE line '{line}'")));
            }
            let src =
                parse_source(net, &toks[3..], advisories, "VARIABLE").map_err(ControlError)?;
            var_names.push(toks[1].to_ascii_lowercase());
            c.variables.push(src);
        }
        // Named expressions: `EXPRESSION ename = <math>`.
        let mut expr_names: Vec<String> = Vec::new();
        for line in &text.expressions {
            let rest = line
                .trim_start()
                .strip_prefix("EXPRESSION")
                .or_else(|| line.trim_start().strip_prefix("expression"))
                .unwrap_or(line)
                .trim_start();
            let Some((name, body)) = rest.split_once('=') else {
                return Err(ControlError(format!("malformed EXPRESSION line '{line}'")));
            };
            let name = name.trim().to_ascii_lowercase();
            let compiled =
                Expression::compile(body.trim(), |n| var_names.iter().position(|v| v == n))
                    .map_err(|e| ControlError(format!("expression {name}: {e}")))?;
            expr_names.push(name.clone());
            c.expr_names.push(name);
            c.expr_warned.push(false);
            c.expressions.push(compiled);
        }

        for (ri, rule) in text.rules.iter().enumerate() {
            let mut premises: Vec<Vec<Premise>> = vec![Vec::new()];
            let mut then_actions = Vec::new();
            let mut else_actions = Vec::new();
            let mut priority = 0.0;
            // Which clause list an `AND` continues.
            #[derive(PartialEq)]
            enum Phase {
                Premises,
                Then,
                Else,
            }
            let mut phase = Phase::Premises;
            let fail = |m: String| ControlError(format!("rule {}: {m}", rule.name));
            for line in &rule.lines {
                let toks: Vec<&str> = line.split_whitespace().collect();
                if toks.is_empty() {
                    continue;
                }
                let kw = toks[0].to_ascii_uppercase();
                match kw.as_str() {
                    "IF" => {
                        phase = Phase::Premises;
                        premises[0].push(
                            parse_premise(net, &c, &var_names, &expr_names, &toks[1..], advisories)
                                .map_err(fail)?,
                        );
                    }
                    "AND" => match phase {
                        Phase::Premises => {
                            let g = premises.len() - 1;
                            premises[g].push(
                                parse_premise(
                                    net,
                                    &c,
                                    &var_names,
                                    &expr_names,
                                    &toks[1..],
                                    advisories,
                                )
                                .map_err(fail)?,
                            );
                        }
                        Phase::Then => then_actions
                            .push(parse_action(net, &toks[1..], ri, &mut c).map_err(fail)?),
                        Phase::Else => else_actions
                            .push(parse_action(net, &toks[1..], ri, &mut c).map_err(fail)?),
                    },
                    "OR" => {
                        if phase != Phase::Premises {
                            return Err(fail("OR outside the premise clauses".into()));
                        }
                        premises.push(vec![parse_premise(
                            net,
                            &c,
                            &var_names,
                            &expr_names,
                            &toks[1..],
                            advisories,
                        )
                        .map_err(fail)?]);
                    }
                    "THEN" => {
                        phase = Phase::Then;
                        then_actions.push(parse_action(net, &toks[1..], ri, &mut c).map_err(fail)?);
                    }
                    "ELSE" => {
                        phase = Phase::Else;
                        else_actions.push(parse_action(net, &toks[1..], ri, &mut c).map_err(fail)?);
                    }
                    "PRIORITY" => {
                        priority = toks
                            .get(1)
                            .and_then(|t| t.parse().ok())
                            .ok_or_else(|| fail("malformed PRIORITY".into()))?;
                    }
                    other => {
                        return Err(fail(format!("unexpected clause keyword '{other}'")));
                    }
                }
            }
            if premises.iter().all(|g| g.is_empty()) {
                return Err(fail("no premises".into()));
            }
            if then_actions.is_empty() {
                return Err(fail("no THEN action".into()));
            }
            c.rules.push(Rule {
                name: rule.name.clone(),
                premises,
                then_actions,
                else_actions,
                priority,
            });
        }
        Ok(Some(c))
    }

    /// Evaluate every rule at the current state and return the
    /// per-link settings to apply — conflicts already resolved through
    /// the pending slot (§9.1): strictly higher priority replaces, ties
    /// keep the earlier rule.
    pub fn evaluate(&mut self, view: &ControlView) -> Vec<(usize, f64, usize)> {
        // Pending slot per link: (action index, priority).
        let mut pending: Vec<(usize, f64)> = Vec::new();
        let mut chosen: Vec<usize> = Vec::new();
        for ri in 0..self.rules.len() {
            // OR-groups of AND-chains, conventional precedence; the
            // last-compared premise leaves the (control, set-point) pair.
            let mut fired = false;
            let mut last_pair = (0.0, 0.0);
            let groups = self.rules[ri].premises.clone();
            for group in &groups {
                let mut all = true;
                for p in group {
                    let (ok, pair) = self.eval_premise(p, view);
                    last_pair = pair;
                    if !ok {
                        all = false;
                        break;
                    }
                }
                if all && !group.is_empty() {
                    fired = true;
                    break;
                }
            }
            let list = if fired {
                self.rules[ri].then_actions.clone()
            } else {
                self.rules[ri].else_actions.clone()
            };
            for ai in list {
                // Modulated values evaluate now, against the last pair.
                self.update_action_value(ai, view, last_pair);
                let link = self.actions[ai].link;
                let priority = self.rules[ri].priority;
                match pending
                    .iter_mut()
                    .find(|(a, _)| self.actions[*a].link == link)
                {
                    Some(slot) => {
                        if priority > slot.1 {
                            *slot = (ai, priority);
                        }
                    }
                    None => pending.push((ai, priority)),
                }
            }
        }
        for (ai, _) in pending {
            chosen.push(ai);
        }
        // Resolve to concrete settings.
        chosen
            .into_iter()
            .map(|ai| {
                let a = &self.actions[ai];
                let v = match &a.value {
                    ActionValue::Constant(v) => *v,
                    _ => a.value_now,
                };
                (a.link, v, ai)
            })
            .collect()
    }

    /// Record a fired (non-modulated) action in the §9.1 log.
    pub fn log_action(&mut self, t: f64, ai: usize, link_id: &str, value: f64) {
        let a = &self.actions[ai];
        if matches!(a.value, ActionValue::Constant(_)) {
            let rule = self.rules[a.rule].name.clone();
            self.log.push((t, link_id.to_string(), value, rule));
        }
    }

    /// Evaluate a modulated action's current value into `value_now`.
    fn update_action_value(&mut self, ai: usize, view: &ControlView, pair: (f64, f64)) {
        let (control, set_point) = pair;
        match self.actions[ai].value {
            ActionValue::Constant(_) => {}
            ActionValue::Curve(ci) => {
                self.actions[ai].value_now = lookup(&view.net.curves[ci].points, control);
            }
            ActionValue::Series(si) => {
                // Raw series value at the current time, ends held (§9.1).
                self.actions[ai].value_now = (view.series_value)(si);
            }
            ActionValue::Pid { kp, ki, kd } => {
                // §9.2: the velocity-form update on the normalised error,
                // added to the link's current target.
                let dt_min = view.dt / 60.0;
                let mut e0 = set_point - control;
                if e0.abs() > 1e-12 {
                    e0 /= if set_point != 0.0 { set_point } else { control };
                }
                let (mut e1, mut e2) = (self.actions[ai].e1, self.actions[ai].e2);
                // Reset a stuck controller's history (§9.2).
                if (e0 - e1).abs() < 1e-4 {
                    e1 = 0.0;
                    e2 = 0.0;
                }
                let p = e0 - e1;
                let i = if ki == 0.0 { 0.0 } else { e0 * dt_min / ki };
                let d = if dt_min > 0.0 {
                    kd * (e0 - 2.0 * e1 + e2) / dt_min
                } else {
                    0.0
                };
                let mut update = kp * (p + i + d);
                if update.abs() < 1e-4 {
                    update = 0.0;
                }
                let link = self.actions[ai].link;
                let current = view.router.setting(link).unwrap_or(1.0);
                let mut setting = current + update;
                if setting < 0.0 {
                    setting = 0.0;
                }
                let is_pump = matches!(view.net.links[link].kind, LinkKind::Pump { .. });
                if !is_pump && setting > 1.0 {
                    setting = 1.0;
                }
                self.actions[ai].e2 = e1;
                self.actions[ai].e1 = e0;
                self.actions[ai].value_now = setting;
            }
        }
    }

    fn eval_premise(&mut self, p: &Premise, view: &ControlView) -> (bool, (f64, f64)) {
        let lhs = match &p.lhs {
            Lhs::Var(s) => self.source_value(*s, view),
            Lhs::Expr(ei) => {
                let vars: Vec<f64> = self
                    .variables
                    .iter()
                    .map(|s| self.source_value(*s, view).unwrap_or(0.0))
                    .collect();
                let (v, guarded) = self.expressions[*ei].eval(&vars);
                if guarded && !self.expr_warned[*ei] {
                    self.expr_warned[*ei] = true;
                    self.guard_events.push(self.expr_names[*ei].clone());
                }
                Some(v)
            }
        };
        let rhs = match &p.rhs {
            Rhs::Value(v) => Some(*v),
            Rhs::Var(s) => self.source_value(*s, view),
        };
        // An inapplicable attribute evaluates the premise false (§9.1).
        let (Some(l), Some(r)) = (lhs, rhs) else {
            return (false, (0.0, 0.0));
        };
        let pair = (l, r);
        let ok = if p.timed {
            // Half-step tolerance on equality, in days (§9.1).
            let half = view.dt / 2.0 / 86_400.0;
            match p.rel {
                Rel::Eq => l >= r - half && l < r + half,
                Rel::Ne => l < r - half || l >= r + half,
                _ => compare(l, p.rel, r),
            }
        } else {
            compare(l, p.rel, r)
        };
        (ok, pair)
    }

    /// A source's value in the file's unit system; `None` when the
    /// attribute does not apply (§9.1).
    fn source_value(&self, s: Source, view: &ControlView) -> Option<f64> {
        let r = view.router;
        Some(match s {
            Source::GageIntensity(g) => (view.gage_intensity)(g) / self.cv_rain,
            Source::GagePast(g, n) => (view.gage_past)(g, n) / self.cv_rain_depth,
            Source::Vertex(v, attr) => match attr {
                VertexAttr::Depth => r.depth(v) / self.cv_len,
                VertexAttr::MaxDepth => r.vertex_max_depth(v) / self.cv_len,
                VertexAttr::Head => (r.vertex_invert(v) + r.depth(v)) / self.cv_len,
                VertexAttr::Volume => r.vertex_volume_now(v) / self.cv_vol,
                VertexAttr::Inflow => view.laterals.get(v).copied().unwrap_or(0.0) / self.cv_flow,
            },
            Source::Link(l, attr) => match attr {
                LinkAttr::Flow => r.flow(l, view.net) / self.cv_flow,
                LinkAttr::Depth => r.link_depth(l)? / self.cv_len,
                LinkAttr::Velocity => r.link_velocity(l)? / self.cv_len,
                LinkAttr::Status => {
                    let open = r.is_open(l)?;
                    let statusable = matches!(
                        view.net.links[l].kind,
                        LinkKind::Channel { .. } | LinkKind::Pump { .. }
                    );
                    if !statusable {
                        return None;
                    }
                    if open {
                        1.0
                    } else {
                        0.0
                    }
                }
                LinkAttr::Setting => {
                    let settable = matches!(
                        view.net.links[l].kind,
                        LinkKind::Pump { .. } | LinkKind::Orifice { .. } | LinkKind::Weir { .. }
                    );
                    if !settable {
                        return None;
                    }
                    r.setting(l)?
                }
                LinkAttr::TimeOpen => {
                    if !r.is_open(l)? {
                        return None;
                    }
                    r.time_in_status(l)? / 86_400.0
                }
                LinkAttr::TimeClosed => {
                    if r.is_open(l)? {
                        return None;
                    }
                    r.time_in_status(l)? / 86_400.0
                }
                LinkAttr::FullFlow => r.chan_full_attrs(l)?.0 / self.cv_flow,
                LinkAttr::FullDepth => r.chan_full_attrs(l)?.1 / self.cv_len,
                LinkAttr::Length => r.chan_full_attrs(l)?.2 / self.cv_len,
                LinkAttr::Slope => r.chan_full_attrs(l)?.3,
            },
            Source::Sim(attr) => {
                let clock = view.date_days.fract();
                match attr {
                    SimAttr::Time => view.elapsed / 86_400.0,
                    SimAttr::Date => view.date_days,
                    SimAttr::ClockTime => clock,
                    SimAttr::Day => f64::from(super::time::weekday(view.date_days as i64) + 1),
                    SimAttr::Month => {
                        f64::from(super::time::civil_from_days(view.date_days as i64).month)
                    }
                    SimAttr::DayOfYear => {
                        let d = super::time::civil_from_days(view.date_days as i64);
                        let jan1 = super::time::days_from_civil(crate::io::options::Date {
                            year: d.year,
                            month: 1,
                            day: 1,
                        });
                        (view.date_days as i64 - jan1 + 1) as f64
                    }
                }
            }
        })
    }
}

fn compare(l: f64, rel: Rel, r: f64) -> bool {
    match rel {
        Rel::Eq => l == r,
        Rel::Ne => l != r,
        Rel::Lt => l < r,
        Rel::Le => l <= r,
        Rel::Gt => l > r,
        Rel::Ge => l >= r,
    }
}

/// End-held linear interpolation.
fn lookup(points: &[(f64, f64)], x: f64) -> f64 {
    let Some(&(x0, y0)) = points.first() else {
        return 0.0;
    };
    if x <= x0 {
        return y0;
    }
    let (mut x1, mut y1) = (x0, y0);
    for &(x2, y2) in &points[1..] {
        if x <= x2 {
            return y1 + (y2 - y1) * (x - x1) / (x2 - x1);
        }
        (x1, y1) = (x2, y2);
    }
    y1
}

fn parse_source(
    net: &Network,
    toks: &[&str],
    advisories: &mut Vec<String>,
    context: &str,
) -> Result<Source, String> {
    if toks.len() < 2 {
        return Err("truncated premise variable".into());
    }
    let obj = toks[0].to_ascii_uppercase();
    if !OBJECTS.contains(&obj.as_str()) {
        return Err(format!("unknown object '{}'", toks[0]));
    }
    if obj == "SIMULATION" {
        let attr = toks[1].to_ascii_uppercase();
        return Ok(Source::Sim(match attr.as_str() {
            "TIME" => SimAttr::Time,
            "DATE" => SimAttr::Date,
            "CLOCKTIME" => SimAttr::ClockTime,
            "DAY" => SimAttr::Day,
            "MONTH" => SimAttr::Month,
            "DAYOFYEAR" => SimAttr::DayOfYear,
            _ => return Err(format!("unknown SIMULATION attribute '{}'", toks[1])),
        }));
    }
    if toks.len() < 3 {
        return Err("truncated premise variable".into());
    }
    let id = toks[1];
    let attr = toks[2].to_ascii_uppercase();
    if obj == "GAGE" {
        let g = net
            .gages
            .iter()
            .position(|x| x.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| format!("unknown gage '{id}'"))?;
        if attr == "INTENSITY" {
            return Ok(Source::GageIntensity(g));
        }
        let hours: u32 = attr.parse().map_err(|_| {
            format!("gage attribute '{attr}' is neither INTENSITY nor an hour count")
        })?;
        if hours == 0 || hours > 48 {
            return Err(format!("gage look-back {hours} h outside 1–48"));
        }
        return Ok(Source::GagePast(g, hours));
    }
    if obj == "NODE" {
        let v = net
            .vertices
            .iter()
            .position(|x| x.id.eq_ignore_ascii_case(id))
            .ok_or_else(|| format!("unknown vertex '{id}'"))?;
        return Ok(Source::Vertex(
            v,
            match attr.as_str() {
                "DEPTH" => VertexAttr::Depth,
                "MAXDEPTH" => VertexAttr::MaxDepth,
                "HEAD" => VertexAttr::Head,
                "VOLUME" => VertexAttr::Volume,
                "INFLOW" => VertexAttr::Inflow,
                _ => return Err(format!("unknown NODE attribute '{}'", toks[2])),
            },
        ));
    }
    // Link-flavoured objects.
    let l = net
        .links
        .iter()
        .position(|x| x.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("unknown link '{id}'"))?;
    let a = match attr.as_str() {
        "FLOW" => LinkAttr::Flow,
        "DEPTH" => LinkAttr::Depth,
        "VELOCITY" => LinkAttr::Velocity,
        "STATUS" => LinkAttr::Status,
        "SETTING" => LinkAttr::Setting,
        "TIMEOPEN" => LinkAttr::TimeOpen,
        "TIMECLOSED" => LinkAttr::TimeClosed,
        "FULLFLOW" => LinkAttr::FullFlow,
        "FULLDEPTH" => LinkAttr::FullDepth,
        "LENGTH" => LinkAttr::Length,
        "SLOPE" => LinkAttr::Slope,
        _ => return Err(format!("unknown link attribute '{}'", toks[2])),
    };
    // An attribute inapplicable to its object evaluates false at run
    // time; validation warns instead of leaving it silently inert (§9.1).
    let kind = &net.links[l].kind;
    let inapplicable = match a {
        LinkAttr::Status => !matches!(kind, LinkKind::Channel { .. } | LinkKind::Pump { .. }),
        LinkAttr::Setting => !matches!(
            kind,
            LinkKind::Pump { .. } | LinkKind::Orifice { .. } | LinkKind::Weir { .. }
        ),
        LinkAttr::Velocity
        | LinkAttr::FullFlow
        | LinkAttr::FullDepth
        | LinkAttr::Length
        | LinkAttr::Slope => !matches!(kind, LinkKind::Channel { .. }),
        _ => false,
    };
    if inapplicable {
        advisories.push(format!(
            "{context}: attribute {attr} never applies to link {id}; the premise can never hold"
        ));
    }
    Ok(Source::Link(l, a))
}

fn parse_premise(
    net: &Network,
    c: &Controls,
    var_names: &[String],
    expr_names: &[String],
    toks: &[&str],
    advisories: &mut Vec<String>,
) -> Result<Premise, String> {
    if toks.is_empty() {
        return Err("empty premise".into());
    }
    // LHS: named expression, named variable, or object-attribute triple.
    let first = toks[0].to_ascii_lowercase();
    let (lhs, consumed, lhs_attr_timed, lhs_attr) =
        if let Some(ei) = expr_names.iter().position(|n| *n == first) {
            (Lhs::Expr(ei), 1, false, None)
        } else if let Some(vi) = var_names.iter().position(|n| *n == first) {
            let src = c.variables[vi];
            (Lhs::Var(src), 1, source_is_timed(src), Some(src))
        } else {
            let src = parse_source(net, toks, advisories, "premise")?;
            let n = if matches!(src, Source::Sim(_)) { 2 } else { 3 };
            (Lhs::Var(src), n, source_is_timed(src), Some(src))
        };
    let rel_tok = toks.get(consumed).ok_or("premise missing relation")?;
    let rel = match *rel_tok {
        "=" => Rel::Eq,
        "<>" => Rel::Ne,
        "<" => Rel::Lt,
        "<=" => Rel::Le,
        ">" => Rel::Gt,
        ">=" => Rel::Ge,
        other => return Err(format!("unknown relation '{other}'")),
    };
    let rest = &toks[consumed + 1..];
    if rest.is_empty() {
        return Err("premise missing value".into());
    }
    // RHS: named variable, object reference, or literal value.
    let rhs_first = rest[0].to_ascii_lowercase();
    let rhs = if let Some(vi) = var_names.iter().position(|n| *n == rhs_first) {
        Rhs::Var(c.variables[vi])
    } else if OBJECTS.contains(&rest[0].to_ascii_uppercase().as_str()) {
        Rhs::Var(parse_source(net, rest, advisories, "premise")?)
    } else {
        Rhs::Value(parse_premise_value(rest[0], lhs_attr)?)
    };
    Ok(Premise {
        lhs,
        rel,
        rhs,
        timed: lhs_attr_timed,
    })
}

fn source_is_timed(s: Source) -> bool {
    matches!(
        s,
        Source::Sim(SimAttr::Time | SimAttr::ClockTime)
            | Source::Link(_, LinkAttr::TimeOpen | LinkAttr::TimeClosed)
    )
}

/// Parse a premise's literal per its attribute: times as decimal hours or
/// `hh:mm(:ss)` into days, dates as `mm/dd/yyyy` into civil days, day
/// names into 1–7, status words into 0/1, plain numbers otherwise.
fn parse_premise_value(tok: &str, attr: Option<Source>) -> Result<f64, String> {
    let upper = tok.to_ascii_uppercase();
    match attr {
        Some(Source::Sim(SimAttr::Time | SimAttr::ClockTime))
        | Some(Source::Link(_, LinkAttr::TimeOpen | LinkAttr::TimeClosed)) => {
            parse_hours(tok).map(|h| h / 24.0)
        }
        Some(Source::Sim(SimAttr::Date)) => {
            let parts: Vec<&str> = tok.split('/').collect();
            if parts.len() == 3 {
                let m: u32 = parts[0].parse().map_err(|_| bad(tok))?;
                let d: u32 = parts[1].parse().map_err(|_| bad(tok))?;
                let y: i32 = parts[2].parse().map_err(|_| bad(tok))?;
                Ok(super::time::days_from_civil(crate::io::options::Date {
                    year: y,
                    month: m,
                    day: d,
                }) as f64)
            } else {
                Err(bad(tok))
            }
        }
        Some(Source::Sim(SimAttr::Day)) => Ok(match upper.as_str() {
            "SUNDAY" | "SUN" => 1.0,
            "MONDAY" | "MON" => 2.0,
            "TUESDAY" | "TUE" => 3.0,
            "WEDNESDAY" | "WED" => 4.0,
            "THURSDAY" | "THU" => 5.0,
            "FRIDAY" | "FRI" => 6.0,
            "SATURDAY" | "SAT" => 7.0,
            _ => tok.parse().map_err(|_| bad(tok))?,
        }),
        Some(Source::Link(_, LinkAttr::Status)) => Ok(match upper.as_str() {
            "OPEN" | "ON" => 1.0,
            "CLOSED" | "OFF" => 0.0,
            _ => return Err(bad(tok)),
        }),
        _ => tok.parse().map_err(|_| bad(tok)),
    }
}

fn bad(tok: &str) -> String {
    format!("malformed value '{tok}'")
}

/// `hh:mm(:ss)` or decimal hours, to hours.
fn parse_hours(tok: &str) -> Result<f64, String> {
    if let Some((h, rest)) = tok.split_once(':') {
        let h: f64 = h.parse().map_err(|_| bad(tok))?;
        let (m, s) = match rest.split_once(':') {
            Some((m, s)) => (
                m.parse::<f64>().map_err(|_| bad(tok))?,
                s.parse::<f64>().map_err(|_| bad(tok))?,
            ),
            None => (rest.parse::<f64>().map_err(|_| bad(tok))?, 0.0),
        };
        Ok(h + m / 60.0 + s / 3600.0)
    } else {
        tok.parse().map_err(|_| bad(tok))
    }
}

/// `THEN <object> <id> <STATUS|SETTING> = <value…>` — returns the action
/// index.
fn parse_action(
    net: &Network,
    toks: &[&str],
    rule: usize,
    c: &mut Controls,
) -> Result<usize, String> {
    if toks.len() < 5 {
        return Err(format!("truncated action '{}'", toks.join(" ")));
    }
    let obj = toks[0].to_ascii_uppercase();
    if !matches!(
        obj.as_str(),
        "CONDUIT" | "PUMP" | "ORIFICE" | "WEIR" | "OUTLET" | "LINK"
    ) {
        return Err(format!("'{}' is not an actionable object", toks[0]));
    }
    let id = toks[1];
    let link = net
        .links
        .iter()
        .position(|x| x.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("unknown link '{id}'"))?;
    let attr = toks[2].to_ascii_uppercase();
    if toks[3] != "=" {
        return Err(format!("action missing '=' in '{}'", toks.join(" ")));
    }
    let value = match attr.as_str() {
        "STATUS" => match toks[4].to_ascii_uppercase().as_str() {
            "OPEN" | "ON" => ActionValue::Constant(1.0),
            "CLOSED" | "OFF" => ActionValue::Constant(0.0),
            other => return Err(format!("unknown status '{other}'")),
        },
        "SETTING" => match toks[4].to_ascii_uppercase().as_str() {
            "CURVE" => {
                let name = toks.get(5).ok_or("CURVE needs a name")?;
                let ci = net
                    .curves
                    .iter()
                    .position(|x| x.id.eq_ignore_ascii_case(name) && x.kind == CurveKind::Control)
                    .ok_or_else(|| format!("unknown control curve '{name}'"))?;
                ActionValue::Curve(ci)
            }
            "TIMESERIES" => {
                let name = toks.get(5).ok_or("TIMESERIES needs a name")?;
                let si = net
                    .timeseries
                    .iter()
                    .position(|x| x.id.eq_ignore_ascii_case(name))
                    .ok_or_else(|| format!("unknown series '{name}'"))?;
                ActionValue::Series(si)
            }
            "PID" => {
                if toks.len() < 8 {
                    return Err("PID needs three coefficients".into());
                }
                let kp: f64 = toks[5].parse().map_err(|_| bad(toks[5]))?;
                let ki: f64 = toks[6].parse().map_err(|_| bad(toks[6]))?;
                let kd: f64 = toks[7].parse().map_err(|_| bad(toks[7]))?;
                ActionValue::Pid { kp, ki, kd }
            }
            v => ActionValue::Constant(v.parse().map_err(|_| bad(toks[4]))?),
        },
        other => return Err(format!("unknown action attribute '{other}'")),
    };
    c.actions.push(Action {
        link,
        value,
        rule,
        e1: 0.0,
        e2: 0.0,
        value_now: 0.0,
    });
    Ok(c.actions.len() - 1)
}

// ── Checkpointing (§12.3) ────────────────────────────────────────────────────

impl Controls {
    /// Write the control system's state (§12.3).
    ///
    /// The rules and expressions are compiled from the model and rebuilt
    /// with it. What a run changes is each modulated action's error
    /// history, the log of what has been done, and the warn-once latches.
    pub fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_b, put_f, put_u};
        let Controls {
            // Parameters: the compiled model.
            rules: _,
            variables: _,
            expressions: _,
            cv_len: _,
            cv_flow: _,
            cv_rain: _,
            cv_rain_depth: _,
            cv_vol: _,
            expr_names: _,
            // State.
            actions,
            log,
            expr_warned,
            guard_events,
        } = self;
        put_u(w, actions.len() as u64)?;
        for a in actions {
            for v in [a.e1, a.e2, a.value_now] {
                put_f(w, v)?;
            }
        }
        put_u(w, log.len() as u64)?;
        for (at, link, setting, rule) in log {
            put_f(w, *at)?;
            for text in [link, rule] {
                put_u(w, text.len() as u64)?;
                w.write_all(text.as_bytes())?;
            }
            put_f(w, *setting)?;
        }
        put_u(w, expr_warned.len() as u64)?;
        for flag in expr_warned {
            put_b(w, *flag)?;
        }
        put_u(w, guard_events.len() as u64)?;
        for name in guard_events {
            put_u(w, name.len() as u64)?;
            w.write_all(name.as_bytes())?;
        }
        Ok(())
    }

    /// Read back what `checkpoint_put` wrote.
    pub fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        let n = r.u()? as usize;
        if n != self.actions.len() {
            return Err(format!(
                "checkpoint holds {n} control actions where this model has {}",
                self.actions.len()
            ));
        }
        for a in &mut self.actions {
            a.e1 = r.f()?;
            a.e2 = r.f()?;
            a.value_now = r.f()?;
        }
        let n = r.u()? as usize;
        self.log = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let at = r.f()?;
            let link = r.text()?;
            let rule = r.text()?;
            let setting = r.f()?;
            self.log.push((at, link, setting, rule));
        }
        let n = r.u()? as usize;
        if n != self.expr_warned.len() {
            return Err(format!(
                "checkpoint holds {n} expression latches where this model has {}",
                self.expr_warned.len()
            ));
        }
        for flag in self.expr_warned.iter_mut() {
            *flag = r.b()?;
        }
        let n = r.u()? as usize;
        self.guard_events = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            self.guard_events.push(r.text()?);
        }
        Ok(())
    }
}
