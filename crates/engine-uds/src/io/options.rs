//! `[OPTIONS]` parsing (§14.4).
//!
//! The full predecessor vocabulary is accepted, with every default, because
//! an omitted keyword is part of what a file means. Options fall into the
//! §14.4 classes: mapped; substituted with a run notice (the reduced routing
//! forms, the damping and surcharge selections, the retired transforms);
//! and accepted-and-inert exactly as in the predecessor.
//!
//! Values are parsed in the file's own units and converted once all lines
//! are read, since the unit system is itself an option and order is
//! arbitrary. Time-of-day and step values round to the nearest second where
//! the predecessor's do; timestamps carry no +1 ms guard (§14.4).

use super::keywords::match_keyword;
use super::survey::{Diagnostic, DiagnosticKind, TokenLine};
use crate::io::lex::FiniteParse;
// The option types are model vocabulary (format-blind extraction,
// phase 2); this re-export keeps the historical `io::options::` paths
// alive until the interop lift re-points every consumer.
pub use crate::model::options::*;

/// ft → m, exact: the predecessor's US path performs no length conversion
/// (it computes in feet), so the factor is this engine's own and is exact
/// per §1.7.
const FT: f64 = 0.3048;

/// The option keywords, in the predecessor's table order.
const OPTION_WORDS: &[&str] = &[
    "FLOW_UNITS",
    "INFILTRATION",
    "FLOW_ROUTING",
    "START_DATE",
    "START_TIME",
    "END_DATE",
    "END_TIME",
    "REPORT_START_DATE",
    "REPORT_START_TIME",
    "SWEEP_START",
    "SWEEP_END",
    "DRY_DAYS",
    "WET_STEP",
    "DRY_STEP",
    "ROUTING_STEP",
    "RULE_STEP",
    "REPORT_STEP",
    "ALLOW_PONDING",
    "INERTIAL_DAMPING",
    "SLOPE_WEIGHTING",
    "VARIABLE_STEP",
    "NORMAL_FLOW_LIMITED",
    "LENGTHENING_STEP",
    "MIN_SURFAREA",
    "COMPATIBILITY",
    "SKIP_STEADY_STATE",
    "TEMPDIR",
    "IGNORE_RAINFALL",
    "FORCE_MAIN_EQUATION",
    "LINK_OFFSETS",
    "MIN_SLOPE",
    "IGNORE_SNOWMELT",
    "IGNORE_GROUNDWATER",
    "IGNORE_ROUTING",
    "IGNORE_QUALITY",
    "MAX_TRIALS",
    "HEAD_TOLERANCE",
    "SYS_FLOW_TOL",
    "LAT_FLOW_TOL",
    "IGNORE_RDII",
    "MINIMUM_STEP",
    "THREADS",
    "SURCHARGE_METHOD",
    "IGNORE_2D",
];

/// Parse an `[OPTIONS]` section's lines into resolved [`AnalysisOptions`],
/// appending diagnostics. Unit-dependent values convert after all lines are
/// read, since `FLOW_UNITS` may appear anywhere.
pub fn parse_options(
    lines: &[TokenLine<'_>],
    diagnostics: &mut Vec<Diagnostic>,
) -> AnalysisOptions {
    let mut o = AnalysisOptions::default();
    // Raw user-unit captures, converted in finalize.
    let mut head_tol_user: Option<f64> = None;
    let mut min_surfarea_user: Option<f64> = None;

    for line in lines {
        let Some(key_tok) = line.tokens.first() else {
            continue;
        };
        let Some(value) = line.tokens.get(1).copied() else {
            continue; // a keyword with no value configures nothing
        };
        let Some(ki) = match_keyword(OPTION_WORDS, key_tok) else {
            diagnostics.push(err(
                line.line,
                DiagnosticKind::UnknownOption {
                    token: key_tok.to_string(),
                },
            ));
            continue;
        };
        let keyword = OPTION_WORDS[ki];
        if !key_tok.eq_ignore_ascii_case(keyword) {
            diagnostics.push(err(
                line.line,
                DiagnosticKind::PrefixMatched {
                    token: key_tok.to_string(),
                    matched: keyword,
                },
            ));
        }

        let l = line.line;
        match keyword {
            "FLOW_UNITS" => {
                if let Some(v) = enum_value(
                    &["CFS", "GPM", "MGD", "CMS", "LPS", "MLD"],
                    keyword,
                    value,
                    l,
                    diagnostics,
                ) {
                    o.flow_units = [
                        FlowUnits::Cfs,
                        FlowUnits::Gpm,
                        FlowUnits::Mgd,
                        FlowUnits::Cms,
                        FlowUnits::Lps,
                        FlowUnits::Mld,
                    ][v];
                }
            }
            "INFILTRATION" => {
                if let Some(v) = enum_value(
                    &[
                        "HORTON",
                        "MODIFIED_HORTON",
                        "GREEN_AMPT",
                        "MODIFIED_GREEN_AMPT",
                        "CURVE_NUMBER",
                    ],
                    keyword,
                    value,
                    l,
                    diagnostics,
                ) {
                    o.infiltration = [
                        InfiltrationModel::Horton,
                        InfiltrationModel::ModifiedHorton,
                        InfiltrationModel::GreenAmpt,
                        InfiltrationModel::ModifiedGreenAmpt,
                        InfiltrationModel::CurveNumber,
                    ][v];
                }
            }
            "FLOW_ROUTING" => parse_routing(&mut o, value, l, diagnostics),
            "START_DATE" => set_date(&mut o.start_date, keyword, value, l, diagnostics),
            "END_DATE" => set_date(&mut o.end_date, keyword, value, l, diagnostics),
            "START_TIME" => set_time(&mut o.start_time, keyword, value, l, diagnostics),
            "END_TIME" => set_time(&mut o.end_time, keyword, value, l, diagnostics),
            "REPORT_START_DATE" => {
                let mut d = o.start_date;
                set_date(&mut d, keyword, value, l, diagnostics);
                o.report_start = Some((d, o.report_start.map_or(0.0, |(_, t)| t)));
            }
            "REPORT_START_TIME" => {
                let mut t = 0.0;
                set_time(&mut t, keyword, value, l, diagnostics);
                o.report_start = Some((o.report_start.map_or(o.start_date, |(d, _)| d), t));
            }
            "SWEEP_START" => set_day_of_year(&mut o.sweep_start, keyword, value, l, diagnostics),
            "SWEEP_END" => set_day_of_year(&mut o.sweep_end, keyword, value, l, diagnostics),
            "DRY_DAYS" => set_number(&mut o.dry_days, keyword, value, l, diagnostics, |v| {
                v >= 0.0
            }),
            "WET_STEP" => set_positive_step(&mut o.wet_step, keyword, value, l, diagnostics),
            "DRY_STEP" => set_positive_step(&mut o.dry_step, keyword, value, l, diagnostics),
            "REPORT_STEP" => set_positive_step(&mut o.report_step, keyword, value, l, diagnostics),
            "RULE_STEP" => set_step(&mut o.rule_step, keyword, value, l, diagnostics),
            "ROUTING_STEP" => {
                // Plain seconds or a clock string, per the predecessor.
                if let Some(v) = seconds_or_clock(value) {
                    if v > 0.0 {
                        o.routing_step = v;
                    } else {
                        diagnostics.push(err(l, bad(keyword, value)));
                    }
                } else {
                    diagnostics.push(err(l, bad(keyword, value)));
                }
            }
            "MINIMUM_STEP" => set_number(
                &mut o.min_routing_step,
                keyword,
                value,
                l,
                diagnostics,
                |v| v >= 0.0,
            ),
            "VARIABLE_STEP" => {
                set_number(&mut o.courant_factor, keyword, value, l, diagnostics, |v| {
                    (0.0..=2.0).contains(&v)
                });
                // The predecessor has no measure of integration error, so
                // this keyword is the whole of its stepping and zero means
                // its step never moves. Here it names the Courant term
                // alone, and §6.5's error test still governs — which can
                // cut the step severalfold below the one that was asked
                // for. Two mechanisms, so two controls, and the one that
                // is about to bind is not the one the model named.
                if o.courant_factor == 0.0 && o.routing_err_tol > 0.0 {
                    diagnostics.push(warn(
                        l,
                        DiagnosticKind::SubstitutedOption {
                            keyword,
                            requested: value.to_string(),
                            used: "the Courant term is off, but the \u{a7}6.5 error                                    test still sizes the step; set the session's                                    routing error tolerance to zero for a step the                                    model alone decides",
                        },
                    ));
                }
            }
            "ALLOW_PONDING" => set_bool(&mut o.allow_ponding, keyword, value, l, diagnostics),
            "IGNORE_RAINFALL" => set_bool(&mut o.ignore_rainfall, keyword, value, l, diagnostics),
            "IGNORE_SNOWMELT" => set_bool(&mut o.ignore_snowmelt, keyword, value, l, diagnostics),
            "IGNORE_GROUNDWATER" => {
                set_bool(&mut o.ignore_groundwater, keyword, value, l, diagnostics)
            }
            "IGNORE_RDII" => set_bool(&mut o.ignore_rdii, keyword, value, l, diagnostics),
            "IGNORE_ROUTING" => set_bool(&mut o.ignore_routing, keyword, value, l, diagnostics),
            "IGNORE_2D" => set_bool(&mut o.ignore_overland, keyword, value, l, diagnostics),
            "IGNORE_QUALITY" => set_bool(&mut o.ignore_quality, keyword, value, l, diagnostics),
            "LINK_OFFSETS" => {
                if let Some(v) = enum_value(&["DEPTH", "ELEVATION"], keyword, value, l, diagnostics)
                {
                    o.link_offsets = [LinkOffsets::Depth, LinkOffsets::Elevation][v];
                }
            }
            "FORCE_MAIN_EQUATION" => {
                if let Some(v) = enum_value(&["H-W", "D-W"], keyword, value, l, diagnostics) {
                    o.force_main = [
                        ForceMainEquation::HazenWilliams,
                        ForceMainEquation::DarcyWeisbach,
                    ][v];
                }
            }
            "NORMAL_FLOW_LIMITED" => {
                if let Some(v) = enum_value(
                    &["SLOPE", "FROUDE", "BOTH", "NONE"],
                    keyword,
                    value,
                    l,
                    diagnostics,
                ) {
                    o.normal_flow = [
                        NormalFlowCriteria::Slope,
                        NormalFlowCriteria::Froude,
                        NormalFlowCriteria::Both,
                        NormalFlowCriteria::None,
                    ][v];
                }
            }
            "INERTIAL_DAMPING" => {
                // Every value maps to the §6.3 taper; a value that differs
                // from the taper's behaviour carries the substitution notice.
                if let Some(v) =
                    enum_value(&["NONE", "PARTIAL", "FULL"], keyword, value, l, diagnostics)
                {
                    if v != 1 {
                        diagnostics.push(warn(
                            l,
                            DiagnosticKind::SubstitutedOption {
                                keyword,
                                requested: value.to_string(),
                                used: "the run damps inertia on the \u{a7}6.3 taper",
                            },
                        ));
                    }
                }
            }
            "SURCHARGE_METHOD" => {
                // Both values map to the §6.2 slot closure; EXTRAN is a
                // substitution.
                if let Some(v) = enum_value(&["EXTRAN", "SLOT"], keyword, value, l, diagnostics) {
                    if v == 0 {
                        diagnostics.push(warn(
                            l,
                            DiagnosticKind::SubstitutedOption {
                                keyword,
                                requested: value.to_string(),
                                used: "the run surcharges through the \u{a7}6.2 slot, \
                                       which can put peak depths well below the \
                                       predecessor's closure",
                            },
                        ));
                    }
                }
            }
            "SKIP_STEADY_STATE" => {
                let mut requested = false;
                set_bool(&mut requested, keyword, value, l, diagnostics);
                if requested {
                    // §10.3: the skip is not carried; quiescent growth is
                    // the sanctioned economy in its place.
                    diagnostics.push(warn(
                        l,
                        DiagnosticKind::SubstitutedOption {
                            keyword,
                            requested: value.to_string(),
                            used: "the run keeps stepping, growing the step while \
                                   the network is quiescent instead",
                        },
                    ));
                }
            }
            "LENGTHENING_STEP" => {
                // Accepted and ignored: §6.5 retired the transform.
                if seconds_or_clock(value).is_none() {
                    diagnostics.push(err(l, bad(keyword, value)));
                } else if seconds_or_clock(value).unwrap_or(0.0) > 0.0 {
                    diagnostics.push(warn(l, DiagnosticKind::IgnoredOption { keyword }));
                }
            }
            "MAX_TRIALS" => {
                let mut v = 0.0;
                set_number(&mut v, keyword, value, l, diagnostics, |x| {
                    x >= 0.0 && x.fract() == 0.0
                });
                if v > 0.0 {
                    o.max_trials = v as u32;
                }
            }
            "HEAD_TOLERANCE" => {
                let mut v = 0.0;
                set_number(&mut v, keyword, value, l, diagnostics, |x| x >= 0.0);
                if v > 0.0 {
                    head_tol_user = Some(v);
                }
            }
            "MIN_SURFAREA" => {
                let mut v = 0.0;
                set_number(&mut v, keyword, value, l, diagnostics, |x| x >= 0.0);
                if v > 0.0 {
                    min_surfarea_user = Some(v);
                }
            }
            "MIN_SLOPE" => {
                // Entered as a percentage, stored as a fraction, range
                // [0, 100) as the predecessor validates it.
                let mut v = 0.0;
                set_number(&mut v, keyword, value, l, diagnostics, |x| {
                    (0.0..100.0).contains(&x)
                });
                o.min_slope = v / 100.0;
            }
            "THREADS" => {
                let mut v = 1.0;
                set_number(&mut v, keyword, value, l, diagnostics, |x| {
                    x >= 0.0 && x.fract() == 0.0
                });
                o.threads = (v as u32).max(1);
            }
            "TEMPDIR" => o.temp_dir = Some(value.to_string()),
            // Accepted and inert, as in the predecessor (§14.4).
            "SLOPE_WEIGHTING" | "COMPATIBILITY" | "SYS_FLOW_TOL" | "LAT_FLOW_TOL" => {}
            // A keyword in OPTION_WORDS without a handler is a programming
            // gap; surface it as a diagnostic rather than faulting on user
            // input.
            other => diagnostics.push(Diagnostic {
                line: l,
                kind: DiagnosticKind::BadValue {
                    token: other.to_string(),
                },
            }),
        }
    }

    // ── Unit conversion of the length-valued options ─────────────────────
    if let Some(v) = head_tol_user {
        o.head_tol = if o.flow_units.is_us() { v * FT } else { v };
    }
    if let Some(v) = min_surfarea_user {
        o.min_surface_area = if o.flow_units.is_us() { v * FT * FT } else { v };
    }

    // ── The §14.4 interlocks ─────────────────────────────────────────────
    // Adjustments first, so the fatal check judges the effective steps.
    o.dry_step = o.dry_step.max(o.wet_step);
    o.routing_step = o.routing_step.min(o.wet_step);
    if o.report_step < o.routing_step {
        diagnostics.push(err(
            lines.last().map_or(0, |l| l.line),
            DiagnosticKind::ReportStepBelowRoutingStep {
                report: o.report_step,
                routing: o.routing_step,
            },
        ));
    }

    o
}

fn err(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}
fn warn(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}
fn bad(keyword: &'static str, token: &str) -> DiagnosticKind {
    DiagnosticKind::BadOptionValue {
        keyword,
        token: token.to_string(),
    }
}

/// Match `value` against an enumerated vocabulary with the §14.3 rule,
/// warning on prefix matches, erroring on no match.
fn enum_value(
    table: &[&'static str],
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<usize> {
    match match_keyword(table, value) {
        Some(i) => {
            if !value.eq_ignore_ascii_case(table[i]) {
                diagnostics.push(warn(
                    line,
                    DiagnosticKind::PrefixMatched {
                        token: value.to_string(),
                        matched: table[i],
                    },
                ));
            }
            Some(i)
        }
        None => {
            diagnostics.push(err(line, bad(keyword, value)));
            None
        }
    }
}

fn parse_routing(
    o: &mut AnalysisOptions,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const MODERN: &[&str] = &["NONE", "STEADY", "KINWAVE", "XKINWAVE", "DYNWAVE"];
    const LEGACY: &[&str] = &["NONE", "NF", "KW", "EKW", "DW"];
    // The legacy alias table is consulted only when no modern name matches,
    // and maps positionally (§14.4).
    let idx = match match_keyword(MODERN, value) {
        Some(i) => {
            if !value.eq_ignore_ascii_case(MODERN[i]) {
                diagnostics.push(warn(
                    line,
                    DiagnosticKind::PrefixMatched {
                        token: value.to_string(),
                        matched: MODERN[i],
                    },
                ));
            }
            Some(i)
        }
        None => match_keyword(LEGACY, value).filter(|_| {
            // Legacy aliases are short; prefix-with-trailing on them is a
            // modern-name miss like any other, so require exactness.
            LEGACY.iter().any(|w| value.eq_ignore_ascii_case(w))
        }),
    };
    let Some(idx) = idx else {
        diagnostics.push(err(line, bad("FLOW_ROUTING", value)));
        return;
    };
    match idx {
        // NONE: ignore-routing, model stays dynamic (§14.4).
        0 => o.ignore_routing = true,
        1..=3 => {
            o.routing_request = if idx == 1 {
                RoutingRequest::Steady
            } else {
                RoutingRequest::KinematicWave
            };
            diagnostics.push(warn(
                line,
                DiagnosticKind::SubstitutedOption {
                    keyword: "FLOW_ROUTING",
                    requested: value.to_string(),
                    used: "the run routes with the dynamic-wave solver",
                },
            ));
        }
        _ => o.routing_request = RoutingRequest::DynamicWave,
    }
}

fn set_bool(
    target: &mut bool,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(v) = enum_value(&["NO", "YES"], keyword, value, line, diagnostics) {
        *target = v == 1;
    }
}

fn set_number(
    target: &mut f64,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
    valid: impl Fn(f64) -> bool,
) {
    match value.finite_f64() {
        Ok(v) if valid(v) => *target = v,
        _ => diagnostics.push(err(line, bad(keyword, value))),
    }
}

/// A clock string (`H:M` or `H:M:S`) or decimal hours, to seconds, rounded
/// to the nearest second (§14.4).
fn clock_to_seconds(value: &str) -> Option<f64> {
    if value.contains(':') {
        let mut parts = value.split(':');
        let h: f64 = parts.next()?.parse().ok()?;
        let m: f64 = parts.next()?.parse().ok()?;
        let s: f64 = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0.0,
        };
        if parts.next().is_some() || !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&s) {
            return None;
        }
        Some((h * 3600.0 + m * 60.0 + s).round())
    } else {
        let hours: f64 = value.parse().ok()?;
        Some((hours * 3600.0).round())
    }
}

/// Plain seconds, or a clock string (the `ROUTING_STEP` grammar).
fn seconds_or_clock(value: &str) -> Option<f64> {
    if value.contains(':') {
        clock_to_seconds(value)
    } else {
        value.finite_f64().ok()
    }
}

fn set_step(
    target: &mut f64,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match clock_to_seconds(value) {
        Some(v) if v >= 0.0 => *target = v,
        _ => diagnostics.push(err(line, bad(keyword, value))),
    }
}

/// A step that must be strictly positive: the predecessor refuses zero
/// wet/dry/report steps (only the rule step may be 0), and a zero step
/// would stall the hydrology clock.
fn set_positive_step(
    target: &mut f64,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match clock_to_seconds(value) {
        Some(v) if v > 0.0 => *target = v,
        _ => diagnostics.push(err(line, bad(keyword, value))),
    }
}

fn set_time(
    target: &mut f64,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match clock_to_seconds(value) {
        Some(v) if (0.0..86400.0).contains(&v) => *target = v,
        _ => diagnostics.push(err(line, bad(keyword, value))),
    }
}

/// A date token, for consumers whose grammars embed dates (series lines).
pub(crate) fn parse_date_token(token: &str) -> Option<Date> {
    parse_date(token)
}

/// Decimal hours or a clock string, to seconds, rounded — the gage
/// recording-interval grammar.
pub(crate) fn clock_or_hours_to_seconds(token: &str) -> Option<f64> {
    clock_to_seconds(token)
}

/// A clock-string token (`H:M` or `H:M:S`) to seconds; decimal tokens are
/// the caller's to interpret.
pub(crate) fn parse_clock_token(token: &str) -> Option<f64> {
    if token.contains(':') {
        clock_to_seconds(token)
    } else {
        None
    }
}

const MONTHS: &[&str] = &[
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// `M/D/Y` with `/` or `-` separators, three-letter month names accepted.
fn parse_date(value: &str) -> Option<Date> {
    let parts: Vec<&str> = value.split(['/', '-']).collect();
    if parts.len() != 3 {
        return None;
    }
    let month = match parts[0].parse::<u32>() {
        Ok(m) => m,
        Err(_) => {
            let up = parts[0].to_ascii_uppercase();
            (MONTHS.iter().position(|m| up.starts_with(m))? + 1) as u32
        }
    };
    let day: u32 = parts[1].parse().ok()?;
    let year: i32 = parts[2].parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = if month == 2 && leap {
        29
    } else {
        *DAYS_IN_MONTH.get(month.checked_sub(1)? as usize)?
    };
    if day == 0 || day > max_day {
        return None;
    }
    Some(Date { year, month, day })
}

fn set_date(
    target: &mut Date,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match parse_date(value) {
        Some(d) => *target = d,
        None => diagnostics.push(err(line, bad(keyword, value))),
    }
}

/// `M/D` to day-of-year (non-leap, as the predecessor's arbitrary 1947).
fn set_day_of_year(
    target: &mut u32,
    keyword: &'static str,
    value: &str,
    line: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let parse = || -> Option<u32> {
        let (m, d) = value.split_once('/')?;
        let m: u32 = m.parse().ok()?;
        let d: u32 = d.parse().ok()?;
        if !(1..=12).contains(&m) || d == 0 || d > DAYS_IN_MONTH[(m - 1) as usize] {
            return None;
        }
        Some(DAYS_IN_MONTH[..(m - 1) as usize].iter().sum::<u32>() + d)
    };
    match parse() {
        Some(doy) => *target = doy,
        None => diagnostics.push(err(line, bad(keyword, value))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<TokenLine<'_>> {
        text.lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(i, l)| TokenLine {
                line: i + 1,
                tokens: l.split_whitespace().collect(),
                raw: l.trim_end(),
            })
            .collect()
    }

    fn parse(text: &str) -> (AnalysisOptions, Vec<Diagnostic>) {
        let mut diags = Vec::new();
        let o = parse_options(&lines(text), &mut diags);
        (o, diags)
    }

    fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
        diags.iter().filter(|d| d.kind.is_error()).collect()
    }

    #[test]
    fn an_empty_section_yields_every_default() {
        let (o, diags) = parse("");
        assert_eq!(o, AnalysisOptions::default());
        assert!(diags.is_empty());
        assert_eq!(o.max_trials, 8);
        assert_eq!(o.head_tol, 1.524e-3);
        assert_eq!(o.min_surface_area, 1.167);
        assert_eq!(o.courant_factor, 0.75);
    }

    #[test]
    fn units_infiltration_and_dates_parse() {
        let (o, diags) = parse(
            "FLOW_UNITS LPS\nINFILTRATION GREEN_AMPT\nSTART_DATE 7/1/2026\n\
             START_TIME 6:30\nEND_DATE JUL-2-2026\nEND_TIME 12.5",
        );
        assert!(errors(&diags).is_empty(), "{diags:?}");
        assert_eq!(o.flow_units, FlowUnits::Lps);
        assert_eq!(o.infiltration, InfiltrationModel::GreenAmpt);
        assert_eq!(
            o.start_date,
            Date {
                year: 2026,
                month: 7,
                day: 1
            }
        );
        assert_eq!(o.start_time, 6.0 * 3600.0 + 30.0 * 60.0);
        assert_eq!(o.end_date.day, 2, "month-name date");
        assert_eq!(o.end_time, 12.5 * 3600.0);
    }

    #[test]
    fn steps_accept_clock_strings_and_routing_accepts_plain_seconds() {
        let (o, diags) =
            parse("WET_STEP 0:05:00\nDRY_STEP 24:00:00\nROUTING_STEP 30\nREPORT_STEP 0:15:00");
        assert!(errors(&diags).is_empty(), "{diags:?}");
        assert_eq!(o.wet_step, 300.0);
        // The dry step parsed 86400 but... see interlock test for clamping.
        assert_eq!(o.routing_step, 30.0);
        assert_eq!(o.report_step, 900.0);
    }

    #[test]
    fn reduced_routing_forms_substitute_with_notice() {
        for (req, expect) in [
            ("STEADY", RoutingRequest::Steady),
            ("KINWAVE", RoutingRequest::KinematicWave),
            ("XKINWAVE", RoutingRequest::KinematicWave),
            ("KW", RoutingRequest::KinematicWave),
            ("NF", RoutingRequest::Steady),
        ] {
            let (o, diags) = parse(&format!("FLOW_ROUTING {req}"));
            assert_eq!(o.routing_request, expect, "{req}");
            assert!(
                diags.iter().any(|d| matches!(
                    &d.kind,
                    DiagnosticKind::SubstitutedOption {
                        keyword: "FLOW_ROUTING",
                        ..
                    }
                )),
                "{req} must carry the substitution notice"
            );
        }
        let (o, diags) = parse("FLOW_ROUTING DYNWAVE");
        assert_eq!(o.routing_request, RoutingRequest::DynamicWave);
        assert!(diags.is_empty(), "the solver's own form needs no notice");
    }

    #[test]
    fn flow_routing_none_means_ignore_routing() {
        let (o, diags) = parse("FLOW_ROUTING NONE");
        assert!(o.ignore_routing);
        assert_eq!(o.routing_request, RoutingRequest::DynamicWave);
        assert!(diags.is_empty());
    }

    #[test]
    fn lengthening_is_accepted_and_ignored_with_a_warning() {
        let (_, diags) = parse("LENGTHENING_STEP 10");
        let d = &diags[0];
        assert!(matches!(
            d.kind,
            DiagnosticKind::IgnoredOption {
                keyword: "LENGTHENING_STEP"
            }
        ));
        assert!(!d.kind.is_error());
        // Zero means "off" there too — no warning to give.
        let (_, diags) = parse("LENGTHENING_STEP 0");
        assert!(diags.is_empty());
    }

    #[test]
    fn extran_and_damping_overrides_substitute_partial_and_slot_silently() {
        let (_, diags) = parse("SURCHARGE_METHOD EXTRAN\nINERTIAL_DAMPING FULL");
        assert_eq!(
            diags
                .iter()
                .filter(|d| matches!(d.kind, DiagnosticKind::SubstitutedOption { .. }))
                .count(),
            2
        );
        let (_, diags) = parse("SURCHARGE_METHOD SLOT\nINERTIAL_DAMPING PARTIAL");
        assert!(diags.is_empty(), "identical behaviour needs no notice");
    }

    #[test]
    fn head_tolerance_and_min_surfarea_convert_by_the_units_option_in_any_order() {
        // Unit selection AFTER the value it governs: conversion must still
        // apply, because order is arbitrary (§14.4).
        let (o, _) = parse("HEAD_TOLERANCE 0.005\nFLOW_UNITS CFS");
        assert!(
            (o.head_tol - 0.005 * 0.3048).abs() < 1e-12,
            "{}",
            o.head_tol
        );
        let (o, _) = parse("HEAD_TOLERANCE 0.005\nFLOW_UNITS CMS");
        assert_eq!(o.head_tol, 0.005, "SI file values pass through");
        let (o, _) = parse("MIN_SURFAREA 12.566\nFLOW_UNITS GPM");
        assert!((o.min_surface_area - 12.566 * 0.3048 * 0.3048).abs() < 1e-9);
    }

    #[test]
    fn the_interlocks_apply() {
        // Report below routing: fatal.
        let (_, diags) = parse("ROUTING_STEP 60\nREPORT_STEP 0:00:30");
        assert!(diags
            .iter()
            .any(|d| matches!(d.kind, DiagnosticKind::ReportStepBelowRoutingStep { .. })));
        // Dry raised to wet; routing clamped to wet.
        let (o, diags) = parse("WET_STEP 0:10:00\nDRY_STEP 0:05:00\nROUTING_STEP 1200");
        assert!(errors(&diags).is_empty(), "{diags:?}");
        assert_eq!(o.dry_step, 600.0, "dry raised to wet");
        assert_eq!(o.routing_step, 600.0, "routing clamped to wet");
    }

    #[test]
    fn sweep_days_parse_as_day_of_year() {
        let (o, diags) = parse("SWEEP_START 3/1\nSWEEP_END 11/30");
        assert!(errors(&diags).is_empty());
        assert_eq!(o.sweep_start, 31 + 28 + 1);
        assert_eq!(o.sweep_end, 334);
    }

    #[test]
    fn unknown_keywords_and_bad_values_are_errors() {
        let (_, diags) = parse("NO_SUCH_OPTION 1\nFLOW_UNITS FURLONGS");
        assert_eq!(errors(&diags).len(), 2);
    }

    #[test]
    fn prefix_matched_keywords_and_values_warn_without_refusing() {
        let (o, diags) = parse("FLOW_UNITSX CFS\nFLOW_ROUTING DYNWAVEXYZ");
        assert!(errors(&diags).is_empty(), "{diags:?}");
        assert_eq!(o.routing_request, RoutingRequest::DynamicWave);
        assert_eq!(
            diags
                .iter()
                .filter(|d| matches!(d.kind, DiagnosticKind::PrefixMatched { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn inert_options_parse_and_do_nothing() {
        let (o, diags) =
            parse("SLOPE_WEIGHTING NO\nCOMPATIBILITY 5\nSYS_FLOW_TOL 5\nLAT_FLOW_TOL 5");
        assert!(diags.is_empty());
        assert_eq!(o, AnalysisOptions::default());
    }
}
