//! The first of the two input passes (§14.2): register identifiers, count
//! objects, section the file, and report every lexical and structural
//! diagnostic — exhaustively, with no reporting cap (§14.7).
//!
//! Forward references are legal because this pass runs to completion before
//! any parameter is read: every identifier exists by the time parsing
//! begins. Identifier registration follows the predecessor's per-section
//! styles exactly:
//!
//! - **Every line registers, duplicates are errors** — gages, parcels,
//!   aquifers, the four vertex sections (one shared namespace), the five
//!   link sections (one shared namespace), constituents, land uses, streets.
//! - **First occurrence registers, repeats are the same object** — patterns,
//!   curves, time series, unit-hydrograph groups, snow packs,
//!   control-measure designs, inlet designs: their records span lines
//!   sharing an identifier, so a duplicate is structurally impossible.
//! - **Transects** register on `X1` lines (the identifier is the second
//!   token), duplicates errors; **controls** count `RULE` lines; **events**
//!   count lines.

use std::collections::HashMap;

use super::keywords::{match_section, Section};
use super::lex::{check_line_length, effective_content, tokenize, LexError};

/// Which identifier namespace an object registers in (§2.2's kinds, at the
/// granularity the file format distinguishes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Gage,
    Parcel,
    Aquifer,
    UnitHydrographGroup,
    Snowpack,
    /// One namespace across junctions, outfalls, storage, and dividers.
    Vertex,
    /// One namespace across channels, pumps, orifices, weirs, and outlets.
    Link,
    Constituent,
    LandUse,
    TimePattern,
    Curve,
    TimeSeries,
    Transect,
    ControlMeasure,
    Street,
    Inlet,
}

/// The vertex sub-kind a `[JUNCTIONS]`/`[OUTFALLS]`/`[STORAGE]`/`[DIVIDERS]`
/// line declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexKind {
    Junction,
    Outfall,
    Storage,
    Divider,
}

/// The link sub-kind a link-section line declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Channel,
    Pump,
    Orifice,
    Weir,
    Outlet,
}

/// A diagnostic from the survey pass. Severity is intrinsic to the kind:
/// [`DiagnosticKind::is_error`] partitions them.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    /// 1-based input line the diagnostic anchors to.
    pub line: usize,
    /// What happened.
    pub kind: DiagnosticKind,
}

/// The survey diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticKind {
    /// A recognised section header spelled non-canonically — accepted, per
    /// the §14.3 prefix rule, and reported so the typo is visible.
    NonCanonicalSectionHeader {
        /// The token as written.
        token: String,
        /// The section it matched.
        section: Section,
    },
    /// An unrecognised section header: the reader goes sectionless and
    /// discards until the next recognised header (§14.2).
    UnrecognisedSection {
        /// The offending header token.
        token: String,
        /// Data lines discarded before the next recognised header.
        lines_discarded: usize,
    },
    /// A lexical failure (§14.2).
    Lex(LexError),
    /// A duplicate identifier within a namespace (§14.2).
    DuplicateIdentifier {
        /// The namespace.
        kind: ObjectKind,
        /// The identifier as written.
        id: String,
    },
    /// An `[OPTIONS]` keyword matching nothing (§14.4).
    UnknownOption {
        /// The token as written.
        token: String,
    },
    /// An option value outside its keyword's accepted vocabulary or range.
    BadOptionValue {
        /// The option keyword.
        keyword: &'static str,
        /// The offending value token.
        token: String,
    },
    /// A keyword or enumerated value matched by prefix rather than equality
    /// (§14.3) — accepted, and reported so the typo is visible.
    PrefixMatched {
        /// The token as written.
        token: String,
        /// The keyword it matched.
        matched: &'static str,
    },
    /// A predecessor behaviour this engine substitutes (§14.4): the run
    /// notice naming what was requested.
    SubstitutedOption {
        /// The option keyword.
        keyword: &'static str,
        /// The requested value, as written.
        requested: String,
    },
    /// An option accepted and ignored, with the reason recorded in §14.4
    /// (the lengthening transform's retirement).
    IgnoredOption {
        /// The option keyword.
        keyword: &'static str,
    },
    /// The §14.4 interlock: a report step below the routing step is fatal.
    ReportStepBelowRoutingStep {
        /// Report step (s).
        report: f64,
        /// Routing step (s).
        routing: f64,
    },
}

impl DiagnosticKind {
    /// Whether this diagnostic refuses the file (the predecessor's errors)
    /// rather than merely reporting on it (this engine's warnings).
    pub fn is_error(&self) -> bool {
        !matches!(
            self,
            DiagnosticKind::NonCanonicalSectionHeader { .. }
                | DiagnosticKind::PrefixMatched { .. }
                | DiagnosticKind::SubstitutedOption { .. }
                | DiagnosticKind::IgnoredOption { .. }
        )
    }
}

/// One tokenised data line retained for the parse pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLine {
    /// 1-based input line number.
    pub line: usize,
    /// The line's tokens, comment stripped.
    pub tokens: Vec<String>,
}

/// The survey result: identifiers, counts, and the sectioned, tokenised
/// lines the parse pass consumes.
#[derive(Debug, Default)]
pub struct Survey {
    /// Up to three `[TITLE]` lines, as written (further lines ignored).
    pub title: Vec<String>,
    /// Per-namespace identifier registries, id → registration order.
    pub ids: HashMap<ObjectKind, HashMap<String, usize>>,
    /// Vertex counts by sub-kind.
    pub vertex_counts: HashMap<VertexKind, usize>,
    /// Link counts by sub-kind.
    pub link_counts: HashMap<LinkKind, usize>,
    /// Count of control rules (`RULE` lines in `[CONTROLS]`).
    pub rule_count: usize,
    /// Count of `[EVENTS]` lines.
    pub event_count: usize,
    /// Data lines grouped by section, in file order, for the parse pass.
    pub sections: Vec<(Section, Vec<TokenLine>)>,
    /// Every diagnostic, exhaustively.
    pub diagnostics: Vec<Diagnostic>,
}

impl Survey {
    /// Number of registered identifiers in a namespace.
    pub fn count(&self, kind: ObjectKind) -> usize {
        self.ids.get(&kind).map_or(0, HashMap::len)
    }

    /// Whether any diagnostic refuses the file.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.kind.is_error())
    }
}

/// How a section registers identifiers in this pass.
enum Registration {
    /// First token registers; duplicate is an error.
    EveryLine(ObjectKind),
    /// First token registers on first occurrence; repeats are the object's
    /// continuation lines.
    FirstOccurrence(ObjectKind),
    /// `X1` lines register their second token; duplicate is an error.
    TransectX1,
    /// `RULE` lines count.
    ControlRule,
    /// Lines count.
    EventLine,
    /// No identifiers here.
    None,
}

fn registration(section: Section) -> Registration {
    use Registration as R;
    match section {
        Section::RainGages => R::EveryLine(ObjectKind::Gage),
        Section::Subcatchments => R::EveryLine(ObjectKind::Parcel),
        Section::Aquifers => R::EveryLine(ObjectKind::Aquifer),
        Section::Junctions | Section::Outfalls | Section::Storage | Section::Dividers => {
            R::EveryLine(ObjectKind::Vertex)
        }
        Section::Conduits
        | Section::Pumps
        | Section::Orifices
        | Section::Weirs
        | Section::Outlets => R::EveryLine(ObjectKind::Link),
        Section::Pollutants => R::EveryLine(ObjectKind::Constituent),
        Section::LandUses => R::EveryLine(ObjectKind::LandUse),
        Section::Streets => R::EveryLine(ObjectKind::Street),
        Section::Hydrographs => R::FirstOccurrence(ObjectKind::UnitHydrographGroup),
        Section::Snowpacks => R::FirstOccurrence(ObjectKind::Snowpack),
        Section::Patterns => R::FirstOccurrence(ObjectKind::TimePattern),
        Section::Curves => R::FirstOccurrence(ObjectKind::Curve),
        Section::TimeSeries => R::FirstOccurrence(ObjectKind::TimeSeries),
        Section::LidControls => R::FirstOccurrence(ObjectKind::ControlMeasure),
        Section::Inlets => R::FirstOccurrence(ObjectKind::Inlet),
        Section::Transects => R::TransectX1,
        Section::Controls => R::ControlRule,
        Section::Events => R::EventLine,
        _ => R::None,
    }
}

fn vertex_kind(section: Section) -> Option<VertexKind> {
    match section {
        Section::Junctions => Some(VertexKind::Junction),
        Section::Outfalls => Some(VertexKind::Outfall),
        Section::Storage => Some(VertexKind::Storage),
        Section::Dividers => Some(VertexKind::Divider),
        _ => None,
    }
}

fn link_kind(section: Section) -> Option<LinkKind> {
    match section {
        Section::Conduits => Some(LinkKind::Channel),
        Section::Pumps => Some(LinkKind::Pump),
        Section::Orifices => Some(LinkKind::Orifice),
        Section::Weirs => Some(LinkKind::Weir),
        Section::Outlets => Some(LinkKind::Outlet),
        _ => None,
    }
}

/// Run the survey pass over the whole input.
pub fn survey(input: &str) -> Survey {
    let mut s = Survey::default();
    // The reader's state: a recognised section, or None (start of file, or
    // sectionless after an unrecognised header).
    let mut current: Option<Section> = None;
    // A pending unrecognised-header diagnostic accumulating its discard
    // count until the next recognised header or end of input.
    let mut pending_unrecognised: Option<(usize, String, usize)> = None;

    for (idx, raw) in input.lines().enumerate() {
        let line_no = idx + 1;

        if let Err(e) = check_line_length(raw) {
            s.diagnostics.push(Diagnostic {
                line: line_no,
                kind: DiagnosticKind::Lex(e),
            });
            continue;
        }
        let content = effective_content(raw);
        let tokens = match tokenize(content) {
            Ok(t) => t,
            Err(e) => {
                s.diagnostics.push(Diagnostic {
                    line: line_no,
                    kind: DiagnosticKind::Lex(e),
                });
                continue;
            }
        };
        let Some(first) = tokens.first() else {
            continue; // blank or comment-only
        };

        // ── Section header ───────────────────────────────────────────────
        if first.starts_with('[') {
            flush_unrecognised(&mut pending_unrecognised, &mut s);
            match match_section(first) {
                Some(m) => {
                    if !m.canonical {
                        s.diagnostics.push(Diagnostic {
                            line: line_no,
                            kind: DiagnosticKind::NonCanonicalSectionHeader {
                                token: (*first).to_string(),
                                section: m.section,
                            },
                        });
                    }
                    current = Some(m.section);
                    s.sections.push((m.section, Vec::new()));
                }
                None => {
                    current = None;
                    pending_unrecognised = Some((line_no, (*first).to_string(), 0));
                }
            }
            continue;
        }

        // ── Data line ────────────────────────────────────────────────────
        let Some(section) = current else {
            if let Some((_, _, count)) = pending_unrecognised.as_mut() {
                *count += 1;
            }
            continue; // sectionless: discarded (§14.2)
        };

        if section == Section::Title {
            if s.title.len() < 3 {
                s.title.push(content.trim().to_string());
            }
            continue;
        }

        register(&mut s, section, &tokens, line_no);

        if let Some((_, lines)) = s.sections.last_mut() {
            lines.push(TokenLine {
                line: line_no,
                tokens: tokens.iter().map(|t| (*t).to_string()).collect(),
            });
        }
    }
    flush_unrecognised(&mut pending_unrecognised, &mut s);
    s
}

fn flush_unrecognised(pending: &mut Option<(usize, String, usize)>, s: &mut Survey) {
    if let Some((line, token, lines_discarded)) = pending.take() {
        s.diagnostics.push(Diagnostic {
            line,
            kind: DiagnosticKind::UnrecognisedSection {
                token,
                lines_discarded,
            },
        });
    }
}

fn register(s: &mut Survey, section: Section, tokens: &[&str], line_no: usize) {
    match registration(section) {
        Registration::EveryLine(kind) => {
            add_id(s, kind, tokens[0], line_no, true);
            if let Some(vk) = vertex_kind(section) {
                *s.vertex_counts.entry(vk).or_insert(0) += 1;
            }
            if let Some(lk) = link_kind(section) {
                *s.link_counts.entry(lk).or_insert(0) += 1;
            }
        }
        Registration::FirstOccurrence(kind) => {
            add_id(s, kind, tokens[0], line_no, false);
        }
        Registration::TransectX1 => {
            if tokens[0].eq_ignore_ascii_case("X1") {
                if let Some(id) = tokens.get(1) {
                    add_id(s, ObjectKind::Transect, id, line_no, true);
                }
            }
        }
        Registration::ControlRule => {
            if tokens[0].eq_ignore_ascii_case("RULE") {
                s.rule_count += 1;
            }
        }
        Registration::EventLine => {
            s.event_count += 1;
        }
        Registration::None => {}
    }
}

fn add_id(s: &mut Survey, kind: ObjectKind, id: &str, line_no: usize, duplicate_is_error: bool) {
    let registry = s.ids.entry(kind).or_default();
    if registry.contains_key(id) {
        if duplicate_is_error {
            s.diagnostics.push(Diagnostic {
                line: line_no,
                kind: DiagnosticKind::DuplicateIdentifier {
                    kind,
                    id: id.to_string(),
                },
            });
        }
        return;
    }
    let index = registry.len();
    registry.insert(id.to_string(), index);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
[TITLE]
A survey fixture
line two
line three
line four is ignored

[JUNCTIONS]
;;ID   Invert  MaxDepth
J1     100.0   3.0
J2     99.0    3.0

[OUTFALLS]
O1     95.0    FREE

[CONDUITS]
C1  J1  J2  120  0.013  0 0
C2  J2  O1  100  0.013  0 0

[PUMPS]
P1  J2  O1  PCURVE

[PATTERNS]
DWF  HOURLY  1.0 1.0 1.0 1.0 1.0 1.0
DWF  HOURLY  1.0 1.0 1.0 1.0 1.0 1.0

[CURVES]
PCURVE  PUMP1  0 0.1
PCURVE          1 0.2

[TRANSECTS]
NC  0.05 0.05 0.04
X1  T1  4  0 10
GR  2 0  1 3  1 7  2 10

[CONTROLS]
RULE R1
IF NODE J1 DEPTH > 2
THEN PUMP P1 STATUS = ON

[EVENTS]
01/01/2026 00:00  01/02/2026 00:00
";

    fn run(input: &str) -> Survey {
        survey(input)
    }

    #[test]
    fn the_fixture_surveys_clean() {
        let s = run(FIXTURE);
        assert!(!s.has_errors(), "{:?}", s.diagnostics);
    }

    #[test]
    fn title_retains_exactly_three_lines() {
        let s = run(FIXTURE);
        assert_eq!(s.title, vec!["A survey fixture", "line two", "line three"]);
    }

    #[test]
    fn vertices_share_one_namespace_with_per_kind_counts() {
        let s = run(FIXTURE);
        assert_eq!(s.count(ObjectKind::Vertex), 3);
        assert_eq!(s.vertex_counts[&VertexKind::Junction], 2);
        assert_eq!(s.vertex_counts[&VertexKind::Outfall], 1);
    }

    #[test]
    fn links_share_one_namespace_with_per_kind_counts() {
        let s = run(FIXTURE);
        assert_eq!(s.count(ObjectKind::Link), 3);
        assert_eq!(s.link_counts[&LinkKind::Channel], 2);
        assert_eq!(s.link_counts[&LinkKind::Pump], 1);
    }

    #[test]
    fn forward_references_are_no_concern_of_this_pass() {
        // C1 references J2 before... actually after; P1 references PCURVE
        // defined later in the file. The survey registers everything without
        // resolving anything.
        let s = run(FIXTURE);
        assert_eq!(s.count(ObjectKind::Curve), 1);
        assert!(!s.has_errors());
    }

    #[test]
    fn run_length_sections_register_once_without_duplicate_errors() {
        let s = run(FIXTURE);
        assert_eq!(s.count(ObjectKind::TimePattern), 1);
        assert_eq!(s.count(ObjectKind::Curve), 1);
    }

    #[test]
    fn transects_register_on_x1_lines_only() {
        let s = run(FIXTURE);
        assert_eq!(s.count(ObjectKind::Transect), 1);
        let ids = &s.ids[&ObjectKind::Transect];
        assert!(ids.contains_key("T1"));
        // NC and GR tokens must not have registered.
        assert!(!ids.contains_key("NC"));
    }

    #[test]
    fn rules_and_events_count() {
        let s = run(FIXTURE);
        assert_eq!(s.rule_count, 1);
        assert_eq!(s.event_count, 1);
    }

    #[test]
    fn a_duplicate_vertex_across_kinds_is_an_error() {
        // One namespace: an outfall reusing a junction id is a duplicate.
        let s = run("[JUNCTIONS]\nJ1 100 3\n[OUTFALLS]\nJ1 95 FREE\n");
        assert!(s.has_errors());
        assert!(s.diagnostics.iter().any(|d| matches!(
            &d.kind,
            DiagnosticKind::DuplicateIdentifier { kind: ObjectKind::Vertex, id } if id == "J1"
        )));
    }

    #[test]
    fn an_unrecognised_section_discards_until_the_next_header() {
        let s = run("[NOSUCH]\ndata 1\ndata 2\ndata 3\n[JUNCTIONS]\nJ1 100 3\n");
        let unrec = s
            .diagnostics
            .iter()
            .find_map(|d| match &d.kind {
                DiagnosticKind::UnrecognisedSection {
                    token,
                    lines_discarded,
                } => Some((d.line, token.clone(), *lines_discarded)),
                _ => None,
            })
            .expect("unrecognised-section diagnostic");
        assert_eq!(unrec, (1, "[NOSUCH]".to_string(), 3));
        // The junction after the recovery header still registered.
        assert_eq!(s.count(ObjectKind::Vertex), 1);
        assert!(s.has_errors(), "an unrecognised section refuses the file");
    }

    #[test]
    fn a_non_canonical_header_is_accepted_and_warned() {
        let s = run("[JUNC]\nJ1 100 3\n");
        assert_eq!(s.count(ObjectKind::Vertex), 1);
        let warn = s
            .diagnostics
            .iter()
            .find(|d| {
                matches!(
                    &d.kind,
                    DiagnosticKind::NonCanonicalSectionHeader {
                        section: Section::Junctions,
                        ..
                    }
                )
            })
            .expect("non-canonical warning");
        assert!(!warn.kind.is_error(), "accepted, not refused");
        assert!(!s.has_errors());
    }

    #[test]
    fn lexical_failures_carry_their_line_numbers() {
        let long = "x".repeat(2000);
        let input = format!("[JUNCTIONS]\nJ1 100 3\n{long}\n");
        let s = run(&input);
        assert!(s
            .diagnostics
            .iter()
            .any(|d| d.line == 3
                && matches!(&d.kind, DiagnosticKind::Lex(LexError::LineTooLong { .. }))));
    }

    #[test]
    fn sections_retain_their_data_lines_in_order_for_the_parse_pass() {
        let s = run(FIXTURE);
        let junctions = s
            .sections
            .iter()
            .find(|(sec, _)| *sec == Section::Junctions)
            .map(|(_, lines)| lines)
            .expect("junction section retained");
        assert_eq!(junctions.len(), 2);
        assert_eq!(junctions[0].tokens, vec!["J1", "100.0", "3.0"]);
    }
}
