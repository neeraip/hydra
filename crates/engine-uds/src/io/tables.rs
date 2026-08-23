//! Data-object parsing (§2.9): curves, time series, and patterns.
//!
//! Records span lines sharing an identifier — the first line for an object
//! carries its type, continuations carry more data — per the predecessor's
//! readers. Curve points convert at import per the role's units (§14.6);
//! series values stay as written, their unit being the consumer's.

use std::collections::HashMap;

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::options::Date;
use super::survey::{Diagnostic, DiagnosticKind, TokenLine};
use crate::io::lex::FiniteParse;
use crate::model::{
    Curve, CurveKind, PatternKind, SeriesTime, TimePattern, TimeSeries, TimeSeriesPoint,
    TimeSeriesSource,
};

const CURVE_TYPES: &[&str] = &[
    "STORAGE",
    "DIVERSION",
    "TIDAL",
    "RATING",
    "CONTROL",
    "SHAPE",
    "WEIR",
    "PUMP1",
    "PUMP2",
    "PUMP3",
    "PUMP4",
    "PUMP5",
];

const CURVE_KINDS: &[CurveKind] = &[
    CurveKind::Storage,
    CurveKind::Diversion,
    CurveKind::Tidal,
    CurveKind::Rating,
    CurveKind::Control,
    CurveKind::Shape,
    CurveKind::WeirCoeff,
    CurveKind::Pump1,
    CurveKind::Pump2,
    CurveKind::Pump3,
    CurveKind::Pump4,
    CurveKind::Pump5,
];

fn err(line: usize, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic { line, kind }
}

fn bad(line: usize, token: &str) -> Diagnostic {
    err(
        line,
        DiagnosticKind::BadValue {
            token: token.to_string(),
        },
    )
}

/// Per-role point conversion (§14.6). `(x, y)` as the file carries them.
fn convert_point(kind: CurveKind, cv: &UnitConverter, x: f64, y: f64) -> (f64, f64) {
    match kind {
        CurveKind::Storage => (x * cv.len, y * cv.len * cv.len),
        CurveKind::Diversion => (x * cv.flow, y * cv.flow),
        // Hour of day → seconds; stage is a length.
        CurveKind::Tidal => (x * 3600.0, y * cv.len),
        CurveKind::Rating => (x * cv.len, y * cv.flow),
        // Controller variable and setting: as written.
        CurveKind::Control => (x, y),
        // Normalised.
        CurveKind::Shape => (x, y),
        CurveKind::WeirCoeff => (x * cv.len, y * cv.weir_coeff),
        CurveKind::Pump1 => (x * cv.len.powi(3), y * cv.flow),
        CurveKind::Pump2 | CurveKind::Pump4 => (x * cv.len, y * cv.flow),
        CurveKind::Pump3 | CurveKind::Pump5 => (x * cv.len, y * cv.flow),
    }
}

/// Parse a `[CURVES]` section.
pub(crate) fn parse_curves(
    lines: &[TokenLine<'_>],
    ids: &HashMap<String, usize>,
    cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Curve> {
    let mut curves: Vec<Option<Curve>> = vec![None; ids.len()];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.get(t[0].to_ascii_uppercase().as_str()) else {
            continue; // registration already diagnosed anything amiss
        };
        let mut k = 1;
        if curves[idx].is_none() {
            // First line for this curve: the type.
            let Some(m) = match_keyword(CURVE_TYPES, t[1]) else {
                diags.push(bad(l, t[1]));
                continue;
            };
            if !t[1].eq_ignore_ascii_case(CURVE_TYPES[m]) {
                diags.push(err(
                    l,
                    DiagnosticKind::PrefixMatched {
                        token: t[1].to_string(),
                        matched: CURVE_TYPES[m],
                    },
                ));
            }
            curves[idx] = Some(Curve {
                id: t[0].to_string(),
                kind: CURVE_KINDS[m],
                points: Vec::new(),
            });
            k = 2;
        }
        let curve = curves[idx].as_mut().expect("just ensured");
        // Points come in pairs; an unpaired trailing token is an error.
        while k < t.len() {
            if k + 1 >= t.len() {
                diags.push(err(l, DiagnosticKind::MissingItems));
                break;
            }
            let (Ok(x), Ok(y)) = (t[k].finite_f64(), t[k + 1].finite_f64()) else {
                diags.push(bad(l, t[k]));
                break;
            };
            let (x, y) = convert_point(curve.kind, cv, x, y);
            curve.points.push((x, y));
            k += 2;
        }
    }
    curves
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            c.unwrap_or_else(|| Curve {
                id: ids
                    .iter()
                    .find(|(_, &v)| v == i)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default(),
                kind: CurveKind::Storage,
                points: Vec::new(),
            })
        })
        .collect()
}

/// Parse a `[TIMESERIES]` section, per the predecessor's date/time/value
/// state machine: a date token (recognised by parsing as one) anchors every
/// later time until the next date; times are decimal hours or clock strings.
pub(crate) fn parse_timeseries<'a>(
    lines: impl Iterator<Item = TokenLine<'a>>,
    ids: &HashMap<String, usize>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<TimeSeries> {
    let mut series: Vec<Option<TimeSeries>> = vec![None; ids.len()];
    let mut last_date: Vec<Option<Date>> = vec![None; ids.len()];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 3 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.get(t[0].to_ascii_uppercase().as_str()) else {
            continue;
        };
        if t[1].eq_ignore_ascii_case("FILE") {
            series[idx] = Some(TimeSeries {
                id: t[0].to_string(),
                source: TimeSeriesSource::External {
                    file: t[2].to_string(),
                },
            });
            continue;
        }
        let entry = series[idx].get_or_insert_with(|| TimeSeries {
            id: t[0].to_string(),
            source: TimeSeriesSource::Points(Vec::new()),
        });
        let TimeSeriesSource::Points(points) = &mut entry.source else {
            diags.push(bad(l, t[1]));
            continue;
        };
        let mut k = 1;
        while k < t.len() {
            // Optional date.
            if let Some(d) = super::options::parse_date_token(t[k]) {
                last_date[idx] = Some(d);
                k += 1;
            }
            // Time: decimal hours or a clock string.
            let Some(tok) = t.get(k) else {
                diags.push(err(l, DiagnosticKind::MissingItems));
                break;
            };
            let seconds = if let Ok(h) = tok.finite_f64() {
                h * 3600.0
            } else if let Some(s) = super::options::parse_clock_token(tok) {
                s
            } else {
                diags.push(bad(l, tok));
                break;
            };
            k += 1;
            let Some(vtok) = t.get(k) else {
                diags.push(err(l, DiagnosticKind::MissingItems));
                break;
            };
            let Ok(value) = vtok.finite_f64() else {
                diags.push(bad(l, vtok));
                break;
            };
            k += 1;
            let time = match last_date[idx] {
                Some(date) => SeriesTime::Absolute { date, seconds },
                None => SeriesTime::Elapsed(seconds),
            };
            points.push(TimeSeriesPoint { time, value });
        }
    }
    series
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            s.unwrap_or_else(|| TimeSeries {
                id: ids
                    .iter()
                    .find(|(_, &v)| v == i)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default(),
                source: TimeSeriesSource::Points(Vec::new()),
            })
        })
        .collect()
}

const PATTERN_TYPES: &[&str] = &["MONTHLY", "DAILY", "HOURLY", "WEEKEND"];
const PATTERN_KINDS: &[PatternKind] = &[
    PatternKind::Monthly,
    PatternKind::Daily,
    PatternKind::Hourly,
    PatternKind::Weekend,
];

/// Parse a `[PATTERNS]` section: type on the first line for a pattern,
/// factors accumulating across lines, capped at 24 as the predecessor caps
/// them.
pub(crate) fn parse_patterns(
    lines: &[TokenLine<'_>],
    ids: &HashMap<String, usize>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<TimePattern> {
    let mut pats: Vec<Option<TimePattern>> = vec![None; ids.len()];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(&idx) = ids.get(t[0].to_ascii_uppercase().as_str()) else {
            continue;
        };
        let mut k = 1;
        if pats[idx].is_none() {
            let Some(m) = match_keyword(PATTERN_TYPES, t[1]) else {
                diags.push(bad(l, t[1]));
                continue;
            };
            if !t[1].eq_ignore_ascii_case(PATTERN_TYPES[m]) {
                diags.push(err(
                    l,
                    DiagnosticKind::PrefixMatched {
                        token: t[1].to_string(),
                        matched: PATTERN_TYPES[m],
                    },
                ));
            }
            pats[idx] = Some(TimePattern {
                id: t[0].to_string(),
                kind: PATTERN_KINDS[m],
                factors: Vec::new(),
            });
            k = 2;
        }
        let pat = pats[idx].as_mut().expect("just ensured");
        while k < t.len() && pat.factors.len() < 24 {
            let Ok(v) = t[k].finite_f64() else {
                diags.push(bad(l, t[k]));
                break;
            };
            pat.factors.push(v);
            k += 1;
        }
    }
    pats.into_iter()
        .enumerate()
        .map(|(i, p)| {
            p.unwrap_or_else(|| TimePattern {
                id: ids
                    .iter()
                    .find(|(_, &v)| v == i)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_default(),
                kind: PatternKind::Hourly,
                factors: Vec::new(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::io::objects::parse_network;
    use crate::model::{CurveKind, PatternKind, SeriesTime, TimeSeriesSource};

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  3

[CURVES]
PC  PUMP2  0  0
PC         1  2.5
SC  STORAGE  0 100  5 400

[TIMESERIES]
TS1  0    0.1  0.25  0.4
TS1  0.5  0.6
TS2  7/1/2026  0:15  1.0  0:30  2.0
TS3  FILE  rain.dat

[PATTERNS]
DWF  HOURLY  0.5 0.6 0.7 0.8 0.9 1.0
DWF          1.1 1.2 1.3 1.4 1.5 1.6
";

    #[test]
    fn curves_span_lines_and_convert_per_role() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        let pc = net.curves.iter().find(|c| c.id == "PC").unwrap();
        assert_eq!(pc.kind, CurveKind::Pump2);
        assert_eq!(pc.points.len(), 2);
        // Depth ft → m; flow cfs → m³/s.
        assert!((pc.points[1].0 - 0.3048).abs() < 1e-12);
        assert!((pc.points[1].1 - 2.5 * 0.028316846592).abs() < 1e-12);
        let sc = net.curves.iter().find(|c| c.id == "SC").unwrap();
        assert_eq!(sc.kind, CurveKind::Storage);
        assert!((sc.points[1].0 - 5.0 * 0.3048).abs() < 1e-12);
        assert!((sc.points[1].1 - 400.0 * 0.3048 * 0.3048).abs() < 1e-9);
    }

    #[test]
    fn elapsed_series_parse_multiple_points_per_line() {
        let (net, _) = parse_network(FIXTURE);
        let ts = net.timeseries.iter().find(|s| s.id == "TS1").unwrap();
        let TimeSeriesSource::Points(p) = &ts.source else {
            panic!()
        };
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].time, SeriesTime::Elapsed(0.0));
        assert_eq!(p[1].time, SeriesTime::Elapsed(0.25 * 3600.0));
        assert_eq!(p[2].time, SeriesTime::Elapsed(0.5 * 3600.0));
        assert_eq!(p[2].value, 0.6);
    }

    #[test]
    fn a_date_anchors_subsequent_times() {
        let (net, _) = parse_network(FIXTURE);
        let ts = net.timeseries.iter().find(|s| s.id == "TS2").unwrap();
        let TimeSeriesSource::Points(p) = &ts.source else {
            panic!()
        };
        let SeriesTime::Absolute { date, seconds } = p[0].time else {
            panic!()
        };
        assert_eq!((date.year, date.month, date.day), (2026, 7, 1));
        assert_eq!(seconds, 900.0);
        let SeriesTime::Absolute { seconds, .. } = p[1].time else {
            panic!()
        };
        assert_eq!(seconds, 1800.0);
    }

    #[test]
    fn a_file_series_records_its_reference() {
        let (net, _) = parse_network(FIXTURE);
        let ts = net.timeseries.iter().find(|s| s.id == "TS3").unwrap();
        assert_eq!(
            ts.source,
            TimeSeriesSource::External {
                file: "rain.dat".into()
            }
        );
    }

    #[test]
    fn patterns_accumulate_across_lines() {
        let (net, _) = parse_network(FIXTURE);
        let p = net.patterns.iter().find(|p| p.id == "DWF").unwrap();
        assert_eq!(p.kind, PatternKind::Hourly);
        assert_eq!(p.factors.len(), 12);
        assert_eq!(p.factors[11], 1.6);
    }

    /// The bulk retention (§14.3 concatenation preserved): a second
    /// `[TIMESERIES]` block chains after the first in file order, its
    /// date anchor carrying across the section break exactly as it did
    /// when the blocks were physically concatenated, and a bad line in
    /// the second block reports its true file line.
    #[test]
    fn timeseries_blocks_chain_with_their_anchor_and_line_numbers() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[TIMESERIES]
TS1  01/02/2020  0:00  1.0
TS1  6:00  2.0

[JUNCTIONS]
J1  100  3

[TIMESERIES]
TS1  12:00  3.0
TS1  bogus
TS2  0:30  9.0
";
        let (net, diags) = parse_network(inp);
        let points = |id: &str| -> Vec<crate::model::TimeSeriesPoint> {
            match &net
                .timeseries
                .iter()
                .find(|s| s.id == id)
                .expect("series")
                .source
            {
                TimeSeriesSource::Points(p) => p.clone(),
                other => panic!("not inline: {other:?}"),
            }
        };
        let ts1 = points("TS1");
        assert_eq!(ts1.len(), 3);
        // The third point still rides the first block's date anchor.
        match ts1[2].time {
            SeriesTime::Absolute { date, seconds } => {
                assert_eq!((date.month, date.day), (1, 2));
                assert_eq!(seconds, 12.0 * 3600.0);
            }
            other => panic!("anchor lost across the block break: {other:?}"),
        }
        // The bad line reports the file's own line number, 13.
        assert!(
            diags.iter().any(|d| d.line == 13),
            "no diagnostic on line 13: {diags:?}"
        );
        assert_eq!(points("TS2").len(), 1);
    }
}
