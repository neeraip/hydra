//! Administrative sections: `[CONTROLS]` retained as text (§9.1 compiles
//! it later), `[FILES]` interface declarations (§14.8), `[REPORT]`
//! selections (§14.5), and `[EVENTS]` windows (§10.3).

use super::keywords::match_keyword;
use super::objects::UnitConverter;
use super::options::{clock_or_hours_to_seconds, parse_date_token};
use super::survey::{Diagnostic, DiagnosticKind, ObjectKind, Survey, TokenLine};
use crate::model::{
    ControlRule, ControlText, EventWindow, FileMode, InterfaceFiles, ReportOptions, ReportSelection,
};

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

/// Parse a `[CONTROLS]` section into retained text. The clause keywords
/// match by prefix, `VARIABLE` and `EXPRESSION` lines tried first, exactly
/// as the predecessor reads them; clause *content* is kept as written.
pub(crate) fn parse_controls(
    lines: &[TokenLine],
    out: &mut ControlText,
    diags: &mut Vec<Diagnostic>,
) {
    const CLAUSES: &[&str] = &["RULE", "IF", "AND", "OR", "THEN", "ELSE", "PRIORITY"];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        if match_keyword(&["VARIABLE"], &t[0]).is_some() {
            out.variables.push(line.raw.clone());
            continue;
        }
        if match_keyword(&["EXPRESSION"], &t[0]).is_some() {
            out.expressions.push(line.raw.clone());
            continue;
        }
        let Some(k) = match_keyword(CLAUSES, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        if !t[0].eq_ignore_ascii_case(CLAUSES[k]) {
            diags.push(err(
                l,
                DiagnosticKind::PrefixMatched {
                    token: t[0].clone(),
                    matched: CLAUSES[k],
                },
            ));
        }
        if k == 0 {
            // A new rule; duplicate names are refused.
            if out.rules.iter().any(|r| r.name == t[1]) {
                diags.push(err(
                    l,
                    DiagnosticKind::DuplicateIdentifier {
                        kind: ObjectKind::ControlMeasure,
                        id: t[1].clone(),
                    },
                ));
                continue;
            }
            out.rules.push(ControlRule {
                name: t[1].clone(),
                lines: Vec::new(),
            });
            continue;
        }
        // A clause line belongs to the last-opened rule.
        let Some(rule) = out.rules.last_mut() else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        rule.lines.push(line.raw.clone());
    }
}

/// Parse a `[FILES]` section.
pub(crate) fn parse_files(
    lines: &[TokenLine],
    out: &mut InterfaceFiles,
    diags: &mut Vec<Diagnostic>,
) {
    const MODES: &[&str] = &["NO", "SCRATCH", "USE", "SAVE"];
    const KINDS: &[&str] = &[
        "RAINFALL", "RUNOFF", "HOTSTART", "RDII", "INFLOWS", "OUTFLOWS",
    ];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(m) = match_keyword(MODES, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        let Some(k) = match_keyword(KINDS, &t[1]) else {
            diags.push(bad(l, &t[1]));
            continue;
        };
        for (table, i, tok) in [(MODES, m, &t[0]), (KINDS, k, &t[1])] {
            if !tok.eq_ignore_ascii_case(table[i]) {
                diags.push(err(
                    l,
                    DiagnosticKind::PrefixMatched {
                        token: tok.clone(),
                        matched: table[i],
                    },
                ));
            }
        }
        // A mode and type without a name is the predecessor's silent no-op.
        if t.len() < 3 {
            continue;
        }
        let mode = [
            FileMode::No,
            FileMode::Scratch,
            FileMode::Use,
            FileMode::Save,
        ][m];
        let name = t[2].clone();
        match KINDS[k] {
            "RAINFALL" => out.rainfall = Some((mode, name)),
            "RUNOFF" => out.runoff = Some((mode, name)),
            "RDII" => out.rdii = Some((mode, name)),
            "HOTSTART" => match mode {
                // USE and SAVE are separate slots; other modes do nothing.
                FileMode::Use => out.hotstart_use = Some(name),
                FileMode::Save => out.hotstart_save = Some(name),
                _ => {}
            },
            "INFLOWS" => {
                if mode != FileMode::Use {
                    diags.push(bad(l, &t[0]));
                    continue;
                }
                out.inflows = Some(name);
            }
            _ => {
                if mode != FileMode::Save {
                    diags.push(bad(l, &t[0]));
                    continue;
                }
                out.outflows = Some(name);
            }
        }
    }
}

/// Parse a `[REPORT]` section.
pub(crate) fn parse_report(
    lines: &[TokenLine],
    s: &Survey,
    out: &mut ReportOptions,
    diags: &mut Vec<Diagnostic>,
) {
    // Prefix table in the predecessor's order (its `SUBCATCH`/`NODE`/`LINK`
    // entries are abbreviations); canonical spellings carry the warn layer.
    // `NODE` precedes any `NODESTATS`-shaped token in the walk, so the
    // deprecated directive is recognised by full comparison first (§14.3).
    const PREFIXES: &[&str] = &[
        "DISABLED",
        "INPUT",
        "SUBCATCH",
        "NODE",
        "LINK",
        "CONTINUITY",
        "FLOWSTATS",
        "CONTROLS",
        "AVERAGES",
    ];
    const CANONICAL: &[&str] = &[
        "DISABLED",
        "INPUT",
        "SUBCATCHMENTS",
        "NODES",
        "LINKS",
        "CONTINUITY",
        "FLOWSTATS",
        "CONTROLS",
        "AVERAGES",
    ];
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 2 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        // NODESTATS: recognised in its intended meaning — ignored and
        // warned as deprecated — where the predecessor's prefix walk would
        // misread it as a NODE list (§14.3).
        if t[0].eq_ignore_ascii_case("NODESTATS") {
            diags.push(err(
                l,
                DiagnosticKind::IgnoredOption {
                    keyword: "NODESTATS",
                },
            ));
            continue;
        }
        let Some(k) = match_keyword(PREFIXES, &t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        if !t[0].eq_ignore_ascii_case(CANONICAL[k]) {
            diags.push(err(
                l,
                DiagnosticKind::PrefixMatched {
                    token: t[0].clone(),
                    matched: CANONICAL[k],
                },
            ));
        }
        // Yes/no directives.
        if !(2..=4).contains(&k) {
            let Some(v) = match_keyword(&["NO", "YES"], &t[1]) else {
                diags.push(bad(l, &t[1]));
                continue;
            };
            let v = v == 1;
            match k {
                0 => out.disabled = v,
                1 => out.input = v,
                5 => out.continuity = v,
                6 => out.flow_stats = v,
                7 => out.control_actions = v,
                _ => out.averages = v,
            }
            continue;
        }
        // List directives: ALL and NONE compare by full string, so an
        // identifier like ALLNODES stays an identifier (§14.3).
        let kind = [ObjectKind::Parcel, ObjectKind::Vertex, ObjectKind::Link][k - 2];
        let slot = match k {
            2 => &mut out.parcels,
            3 => &mut out.vertices,
            _ => &mut out.links,
        };
        if t[1].eq_ignore_ascii_case("NONE") {
            *slot = ReportSelection::None;
            continue;
        }
        if t[1].eq_ignore_ascii_case("ALL") {
            *slot = ReportSelection::All;
            continue;
        }
        let mut ids = match std::mem::take(slot) {
            ReportSelection::Ids(v) => v,
            _ => Vec::new(),
        };
        let mut ok = true;
        for tok in &t[1..] {
            match s.ids.get(&kind).and_then(|m| m.get(tok)) {
                Some(&i) => {
                    if !ids.contains(&i) {
                        ids.push(i);
                    }
                }
                None => {
                    diags.push(err(
                        l,
                        DiagnosticKind::UnresolvedReference { id: tok.clone() },
                    ));
                    ok = false;
                    break;
                }
            }
        }
        *slot = ReportSelection::Ids(ids);
        if !ok {
            continue;
        }
    }
}

/// Parse an `[EVENTS]` section.
pub(crate) fn parse_events(
    lines: &[TokenLine],
    _cv: &UnitConverter,
    diags: &mut Vec<Diagnostic>,
) -> Vec<EventWindow> {
    let mut out = Vec::new();
    for line in lines {
        let t = &line.tokens;
        let l = line.line;
        if t.len() < 4 {
            diags.push(err(l, DiagnosticKind::MissingItems));
            continue;
        }
        let Some(start_date) = parse_date_token(&t[0]) else {
            diags.push(bad(l, &t[0]));
            continue;
        };
        let Some(start_time) = clock_or_hours_to_seconds(&t[1]) else {
            diags.push(bad(l, &t[1]));
            continue;
        };
        let Some(end_date) = parse_date_token(&t[2]) else {
            diags.push(bad(l, &t[2]));
            continue;
        };
        let Some(end_time) = clock_or_hours_to_seconds(&t[3]) else {
            diags.push(bad(l, &t[3]));
            continue;
        };
        if (start_date, start_time) >= (end_date, end_time) {
            diags.push(bad(l, &t[2]));
            continue;
        }
        out.push(EventWindow {
            start_date,
            start_time,
            end_date,
            end_time,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::io::objects::parse_network;
    use crate::io::options::Date;
    use crate::model::ReportSelection;

    const FIXTURE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  3
J2  99   3

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  J2  400  0.01  0  0
C2  J2  O1  400  0.01  0  0

[XSECTIONS]
C1  CIRCULAR  1  0  0  0
C2  CIRCULAR  1  0  0  0

[CONTROLS]
VARIABLE  D1 = NODE J1 DEPTH
RULE  R1
IF    NODE J1 DEPTH > 2
THEN  CONDUIT C1 STATUS = CLOSED
PRIORITY  2
RULE  R2
IF    SIMULATION TIME > 1:00
THEN  CONDUIT C2 STATUS = OPEN

[FILES]
USE   HOTSTART  in.hsf
SAVE  HOTSTART  out.hsf
SAVE  OUTFLOWS  routing.txt

[REPORT]
INPUT  YES
NODESTATS  YES
NODES  J1
NODES  J2
LINKS  ALL

[EVENTS]
01/01/2024  00:00  01/02/2024  12:00

[COORDINATES]
J1  10.5  20.5
J2  30.0  40.0
";

    fn net_ok() -> crate::model::Network {
        let (net, diags) = parse_network(FIXTURE);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "{:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        net
    }

    #[test]
    fn rules_retain_their_clause_text() {
        let net = net_ok();
        assert_eq!(net.controls.variables, vec!["VARIABLE  D1 = NODE J1 DEPTH"]);
        assert_eq!(net.controls.rules.len(), 2);
        let r1 = &net.controls.rules[0];
        assert_eq!(r1.name, "R1");
        assert_eq!(
            r1.lines,
            vec![
                "IF    NODE J1 DEPTH > 2",
                "THEN  CONDUIT C1 STATUS = CLOSED",
                "PRIORITY  2",
            ]
        );
        assert_eq!(net.controls.rules[1].lines.len(), 2);
    }

    #[test]
    fn hotstart_use_and_save_are_separate_slots() {
        let net = net_ok();
        let f = &net.interface_files;
        assert_eq!(f.hotstart_use.as_deref(), Some("in.hsf"));
        assert_eq!(f.hotstart_save.as_deref(), Some("out.hsf"));
        assert_eq!(f.outflows.as_deref(), Some("routing.txt"));
        assert_eq!(f.rainfall, None);
    }

    #[test]
    fn report_lists_accumulate_and_nodestats_is_ignored() {
        let (net, diags) = parse_network(FIXTURE);
        assert!(net.report.input);
        // Defaults hold where unwritten.
        assert!(net.report.continuity);
        assert!(!net.report.averages);
        // Two NODES lines accumulate; LINKS ALL replaces.
        assert_eq!(net.report.vertices, ReportSelection::Ids(vec![0, 1]));
        assert_eq!(net.report.links, ReportSelection::All);
        assert_eq!(net.report.parcels, ReportSelection::None);
        // NODESTATS was recognised, ignored, and noticed — not read as a
        // NODE list holding an identifier "YES".
        assert!(diags.iter().any(|d| matches!(
            &d.kind,
            crate::io::survey::DiagnosticKind::IgnoredOption { keyword }
                if *keyword == "NODESTATS"
        )));
    }

    #[test]
    fn events_and_display_sections_are_kept() {
        let net = net_ok();
        assert_eq!(net.events.len(), 1);
        let e = &net.events[0];
        assert_eq!(
            e.start_date,
            Date {
                year: 2024,
                month: 1,
                day: 1
            }
        );
        assert!((e.end_time - 12.0 * 3600.0).abs() < 1e-9);
        // The display section survives verbatim under its canonical header.
        assert_eq!(net.display.len(), 1);
        assert_eq!(net.display[0].header, "[COORDINATES]");
        assert_eq!(
            net.display[0].lines,
            vec!["J1  10.5  20.5", "J2  30.0  40.0"]
        );
    }
}
