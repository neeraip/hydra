//! PDF renderer (spec §4.5): typesets the document via an embedded Typst
//! engine. Feature-gated (`pdf`) — heavyweight dependency. All user text
//! passes through Typst string context, so content can never inject
//! markup; no creation timestamp is embedded, preserving determinism.

use std::fmt::Write as _;

use hydra_common::{Fragment, FragmentItem, Table, ValueKind};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt as _, World};

use super::chart_svg::chart_svg;
use super::value_human;
use crate::document::{ReportDocument, Section};

/// Typesetting failure (spec §4.5): the joined Typst diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfError {
    pub message: String,
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pdf rendering failed: {}", self.message)
    }
}

impl std::error::Error for PdfError {}

/// Page margin, in points. Named because the tests need to tell the body of a
/// page from its margins — the page number lives in the bottom one — and a
/// second copy of the number would stop matching the preamble the moment
/// either moved.
#[cfg(test)]
const PAGE_MARGIN_PT: f64 = 2.0 * 28.346_456_692_913_385;

/// Render the document as PDF bytes (spec §4.5).
pub fn render_pdf(doc: &ReportDocument) -> Result<Vec<u8>, PdfError> {
    let (source, charts) = typst_source(doc);
    let world = ReportWorld::new(source, charts);
    let compiled: typst_layout::PagedDocument =
        typst::compile(&world).output.map_err(|diagnostics| {
            let message = diagnostics
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            PdfError { message }
        })?;
    typst_pdf::pdf(&compiled, &typst_pdf::PdfOptions::default()).map_err(|diagnostics| {
        let message = diagnostics
            .iter()
            .map(|d| d.message.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        PdfError { message }
    })
}

// ── Typst source generation ───────────────────────────────────────────────────

/// Escape text into a Typst string literal (double-quoted): user content
/// only ever appears inside string context.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn typst_source(doc: &ReportDocument) -> (String, Vec<String>) {
    let mut charts: Vec<String> = Vec::new();
    let mut s = String::new();
    // Running header: the title, and what the report was produced from, on
    // every page but the first. A page that gets separated from its report
    // then still says which report and which run it belongs to — the default
    // title ("Simulation Report") identifies nothing on its own, so the
    // provenance is the half that does the work.
    //
    // Suppressed on page 1, which already carries both as its title block;
    // repeating them a centimetre above themselves reads as a mistake.
    let header_provenance = doc
        .source
        .iter()
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    let _ = writeln!(
        s,
        "#set page(paper: \"a4\", margin: 2cm, numbering: \"1 / 1\", header: context {{\n\
         \x20 if counter(page).get().first() > 1 {{\n\
         \x20   set text(size: 8pt, fill: luma(120))\n\
         \x20   grid(columns: (1fr, auto), align: (left, right), [#{}], [#{}])\n\
         \x20   v(2pt)\n\
         \x20   line(length: 100%, stroke: 0.4pt + luma(200))\n\
         \x20 }}\n\
         }})",
        quoted(&doc.title),
        quoted(&header_provenance),
    );
    s.push_str(
        "\
         #set text(size: 10pt)\n\
         #set table(stroke: 0.5pt + luma(180), inset: 5pt)\n\
         #show heading.where(level: 1): set text(size: 17pt)\n\
         #show heading.where(level: 2): it => block(above: 1.4em, below: 0.7em, sticky: true)[#it]\n",
    );

    let _ = writeln!(s, "= #{}", quoted(&doc.title));
    let mut provenance: Vec<String> = doc
        .source
        .iter()
        .map(|(label, value)| format!("{label}: {value}"))
        .collect();
    if let Some(generated_at) = &doc.generated_at {
        provenance.push(format!("Generated: {generated_at}"));
    }
    if !provenance.is_empty() {
        let _ = writeln!(
            s,
            "#text(size: 8.5pt, fill: luma(90))[#{}]\n#v(0.5em)\n#line(length: 100%, stroke: 0.5pt + luma(150))",
            quoted(&provenance.join("   ·   "))
        );
    }

    for section in &doc.sections {
        let _ = writeln!(s, "== #{}", quoted(section.title()));
        match section {
            Section::Content(fragment) => emit_fragment(&mut s, fragment, &mut charts),
            Section::Unavailable { reason, .. } => {
                emit_placeholder(&mut s, &format!("Not available: {reason}"))
            }
            Section::Failed { message, .. } => {
                emit_placeholder(&mut s, &format!("Failed: {message}"));
            }
        }
    }

    (s, charts)
}

fn emit_fragment(s: &mut String, fragment: &Fragment, charts: &mut Vec<String>) {
    for item in &fragment.items {
        match item {
            FragmentItem::KeyValues { entries } => {
                s.push_str("#grid(columns: (auto, 1fr), column-gutter: 14pt, row-gutter: 5pt,\n");
                for entry in entries {
                    let _ = writeln!(
                        s,
                        "  [#text(fill: luma(90))[#{}]], [#{}],",
                        quoted(&entry.label),
                        quoted(&value_human(&entry.value))
                    );
                }
                s.push_str(")\n");
            }
            FragmentItem::Table { table } => emit_table(s, table),
            FragmentItem::Note { text } => {
                let _ = writeln!(s, "#text(size: 9pt, fill: luma(90))[#{}]", quoted(text));
            }
            FragmentItem::Chart { chart } => {
                let index = charts.len();
                charts.push(chart_svg(chart));
                let _ = writeln!(s, "#image(\"chart-{index}.svg\", width: 100%)");
            }
        }
    }
}

fn emit_table(s: &mut String, table: &Table) {
    let aligns: Vec<&str> = table
        .columns
        .iter()
        .map(|c| {
            if matches!(c.kind, ValueKind::Number | ValueKind::Integer) {
                "right"
            } else {
                "left"
            }
        })
        .collect();
    let _ = writeln!(
        s,
        "#table(columns: {}, align: ({}),",
        table.columns.len(),
        aligns.join(", ")
    );
    s.push_str("  table.header(");
    for column in &table.columns {
        let header = super::column_header(&column.name, column.unit.as_deref());
        let _ = write!(s, "[#strong[#{}]], ", quoted(&header));
    }
    s.push_str("),\n");
    for row in &table.rows {
        s.push_str("  ");
        for value in row {
            let _ = write!(s, "[#{}], ", quoted(&value_human(value)));
        }
        s.push('\n');
    }
    s.push_str(")\n");
}

fn emit_placeholder(s: &mut String, text: &str) {
    let _ = writeln!(
        s,
        "#block(fill: rgb(\"fdf6e3\"), stroke: 0.5pt + rgb(\"e8dcb5\"), inset: 7pt, radius: 3pt, width: 100%)[#emph[#{}]]",
        quoted(text)
    );
}

// ── Minimal Typst world ───────────────────────────────────────────────────────

/// A self-contained compilation world: one detached source, embedded
/// fonts, chart SVGs as virtual files, no filesystem, no packages, no
/// clock.
struct ReportWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
    /// `chart-<index>.svg` bytes, indexed by chart order of appearance.
    charts: Vec<Bytes>,
}

impl ReportWorld {
    fn new(source_text: String, charts: Vec<String>) -> Self {
        let fonts: Vec<Font> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();
        let book = FontBook::from_fonts(&fonts);
        ReportWorld {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            source: Source::detached(source_text),
            charts: charts
                .into_iter()
                .map(|svg| Bytes::new(svg.into_bytes()))
                .collect(),
        }
    }
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        // Serve chart SVGs referenced as `chart-<index>.svg`.
        let name = id
            .vpath()
            .get_without_slash()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if let Some(index) = name
            .strip_prefix("chart-")
            .and_then(|rest| rest.strip_suffix(".svg"))
            .and_then(|digits| digits.parse::<usize>().ok())
        {
            if let Some(bytes) = self.charts.get(index) {
                return Ok(bytes.clone());
            }
        }
        Err(FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_common::{Column, KeyValue, Value};

    fn doc() -> ReportDocument {
        ReportDocument {
            title: "Anytown \"Q3\" Report".into(),
            generated_at: Some("2026-07-28T12:00:00Z".into()),
            source: vec![("Project".into(), "Anytown".into())],
            sections: vec![
                Section::Content(Fragment {
                    title: "Summary".into(),
                    items: vec![
                        FragmentItem::KeyValues {
                            entries: vec![KeyValue {
                                label: "Junctions".into(),
                                value: Value::Integer { value: 42 },
                            }],
                        },
                        FragmentItem::Table {
                            table: Table {
                                columns: vec![
                                    Column {
                                        name: "Quantity".into(),
                                        unit: None,
                                        kind: ValueKind::Text,
                                    },
                                    Column {
                                        name: "Max".into(),
                                        unit: Some("m".into()),
                                        kind: ValueKind::Number,
                                    },
                                ],
                                rows: vec![vec![
                                    Value::Text {
                                        value: "# not markup [see]".into(),
                                    },
                                    Value::Number {
                                        value: 51.25,
                                        unit: None,
                                    },
                                ]],
                            },
                        },
                    ],
                }),
                Section::Unavailable {
                    title: "Pump Energy".into(),
                    reason: "the network has no pumps".into(),
                },
            ],
        }
    }

    /// Every text run on a page, with its absolute y, so a test can ask what
    /// landed at the bottom. Groups carry their own coordinate space, so their
    /// origin has to be folded in.
    fn page_text(
        frame: &typst::layout::Frame,
        origin: typst::layout::Point,
    ) -> Vec<(f64, f64, String)> {
        let mut out = Vec::new();
        for (pos, item) in frame.items() {
            let at = origin + *pos;
            match item {
                typst::layout::FrameItem::Text(text) => {
                    out.push((at.y.to_pt(), at.x.to_pt(), text.text.to_string()))
                }
                typst::layout::FrameItem::Group(group) => out.extend(page_text(&group.frame, at)),
                _ => {}
            }
        }
        out
    }

    /// The bottom-most line of a page's BODY, reassembled from its runs.
    ///
    /// Margins excluded, because the page number now sits in the bottom one:
    /// including it would make the last line of every page the page number,
    /// and the stranded-heading test below would pass without ever looking at
    /// a heading.
    ///
    /// Whole lines, not runs: a run can be a single glyph, and asking whether
    /// a heading "contains" one matches a table cell holding the digit 3.
    fn bottom_line(frame: &typst::layout::Frame) -> String {
        let body_bottom = frame.height().to_pt() - PAGE_MARGIN_PT;
        let runs: Vec<(f64, f64, String)> = page_text(frame, typst::layout::Point::zero())
            .into_iter()
            .filter(|(y, _, _)| *y <= body_bottom)
            .collect();
        let Some(bottom) = runs
            .iter()
            .map(|(y, _, _)| *y)
            .fold(None, |acc: Option<f64>, y| {
                Some(acc.map_or(y, |a| a.max(y)))
            })
        else {
            return String::new();
        };
        let mut line: Vec<&(f64, f64, String)> = runs
            .iter()
            // One line's runs share a baseline; the tolerance absorbs the
            // sub-point differences between glyph runs of one line.
            .filter(|(y, _, _)| (y - bottom).abs() < 0.5)
            .collect();
        line.sort_by(|a, b| a.1.total_cmp(&b.1));
        line.iter()
            .map(|(_, _, t)| t.as_str())
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// A document long enough to span pages, for the pagination tests. Uniform
    /// sections, so a break can fall anywhere in the rhythm.
    fn long_document() -> ReportDocument {
        // Enough sections of enough length to force several page breaks, so
        // that without the keep-with-next some heading would land last.
        let sections: Vec<Section> = (1..=14)
            .map(|n| {
                Section::Content(Fragment {
                    title: format!("Section {n:02}"),
                    items: vec![FragmentItem::Table {
                        table: Table {
                            columns: vec![
                                Column {
                                    name: "Node".into(),
                                    unit: None,
                                    kind: ValueKind::Text,
                                },
                                Column {
                                    name: "Value".into(),
                                    unit: None,
                                    kind: ValueKind::Number,
                                },
                            ],
                            rows: (0..6)
                                .map(|r| {
                                    vec![
                                        Value::Text {
                                            value: format!("J{r}"),
                                        },
                                        Value::Number {
                                            value: f64::from(r),
                                            unit: None,
                                        },
                                    ]
                                })
                                .collect(),
                        },
                    }],
                })
            })
            .collect();
        ReportDocument {
            title: "Pagination".into(),
            generated_at: None,
            source: vec![("Model".into(), "anytown.inp".into())],
            sections,
        }
    }

    /// Spec §4.5: a running header identifies the report on every page but
    /// the first.
    ///
    /// "Pagination" and "anytown.inp" appear in the body only on page 1 — the
    /// title block — so finding either on a later page is the header and
    /// nothing else.
    #[test]
    fn pages_after_the_first_carry_a_running_header() {
        let doc = long_document();
        let (source, charts) = typst_source(&doc);
        let world = ReportWorld::new(source, charts);
        let compiled: typst_layout::PagedDocument =
            typst::compile(&world).output.expect("compiles");
        assert!(compiled.pages().len() > 1, "fixture no longer spans pages");

        let whole_page = |page: &typst_layout::Page| -> String {
            page_text(&page.frame, typst::layout::Point::zero())
                .iter()
                .map(|(_, _, t)| t.as_str())
                .collect()
        };

        // Page 1 carries the title once, as its heading — not twice.
        let first = whole_page(&compiled.pages()[0]);
        assert_eq!(
            first.matches("Pagination").count(),
            1,
            "page 1 repeats the title: the header should be suppressed there"
        );

        for (index, page) in compiled.pages().iter().enumerate().skip(1) {
            let text = whole_page(page);
            assert!(
                text.contains("Pagination"),
                "page {} carries no running header",
                index + 1
            );
            assert!(
                text.contains("anytown.inp"),
                "page {} header omits the provenance",
                index + 1
            );
        }
    }

    /// Spec §4.5: every page carries its number and the total.
    #[test]
    fn every_page_is_numbered() {
        let doc = long_document();
        let (source, charts) = typst_source(&doc);
        let world = ReportWorld::new(source, charts);
        let compiled: typst_layout::PagedDocument =
            typst::compile(&world).output.expect("compiles");
        let total = compiled.pages().len();
        assert!(total > 1, "fixture no longer spans pages");

        for (index, page) in compiled.pages().iter().enumerate() {
            // The number lives in the bottom margin, which `bottom_line`
            // deliberately excludes — so look at the whole page here.
            let runs = page_text(&page.frame, typst::layout::Point::zero());
            let text: String = {
                let mut sorted = runs.clone();
                sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
                sorted.iter().map(|(_, _, t)| t.as_str()).collect()
            };
            let expected = format!("{} / {total}", index + 1);
            assert!(
                text.contains(&expected),
                "page {} does not carry {expected:?}",
                index + 1,
            );
        }
    }

    /// Spec §4.5: a section heading is never the last thing on a page.
    ///
    /// Typst already guarantees this — its heading show-set rule sets
    /// `block.sticky`, and this passes with or without the explicit `sticky`
    /// in our own show rule. The test is here for the guarantee, not for our
    /// spelling of it: it would catch a Typst upgrade that changed the
    /// default, or a rewrite of the heading rule that wrapped the heading in
    /// a block that defeated it, which is the failure Typst's own docs warn
    /// custom heading show rules about.
    ///
    /// Checked against the laid-out pages rather than the preamble string,
    /// which would only restate the implementation.
    #[test]
    fn a_section_heading_is_never_stranded_at_the_foot_of_a_page() {
        let doc = long_document();
        let titles: Vec<String> = (1..=14).map(|n| format!("Section {n:02}")).collect();

        let (source, charts) = typst_source(&doc);
        let world = ReportWorld::new(source, charts);
        let compiled: typst_layout::PagedDocument =
            typst::compile(&world).output.expect("compiles");

        // Without breaks there is nothing to strand, so a single-page render
        // would make every assertion below vacuous.
        assert!(
            compiled.pages().len() > 1,
            "fixture no longer spans pages: {} page(s)",
            compiled.pages().len()
        );

        for (index, page) in compiled.pages().iter().enumerate() {
            let last = bottom_line(&page.frame);
            assert!(
                !titles.contains(&last),
                "page {} ends with section heading {last:?}",
                index + 1,
            );
        }
    }

    #[test]
    fn renders_a_valid_deterministic_pdf() {
        let first = render_pdf(&doc()).expect("render pdf");
        assert!(first.starts_with(b"%PDF-"), "not a PDF header");
        assert!(first.len() > 1_000, "implausibly small PDF");
        let second = render_pdf(&doc()).expect("render pdf again");
        assert_eq!(first, second, "pdf rendering must be deterministic");
    }

    #[test]
    fn renders_charts_via_virtual_svg_files() {
        use hydra_common::{Chart, ChartData};
        let mut with_chart = doc();
        with_chart.sections.push(Section::Content(Fragment {
            title: "Distribution".into(),
            items: vec![FragmentItem::Chart {
                chart: Chart {
                    x_label: "Minimum pressure".into(),
                    x_unit: Some("m".into()),
                    y_label: "Junctions".into(),
                    y_unit: None,
                    data: ChartData::Bar {
                        categories: vec!["0 – 14".into(), "14 – 28".into()],
                        values: vec![3.0, 7.0],
                    },
                },
            }],
        }));
        let pdf = render_pdf(&with_chart).expect("render pdf with chart");
        assert!(pdf.starts_with(b"%PDF-"));
        let plain = render_pdf(&doc()).expect("render pdf without chart");
        assert!(
            pdf.len() > plain.len(),
            "chart page must add content to the pdf"
        );
    }

    #[test]
    fn quoted_escapes_typst_string_syntax() {
        assert_eq!(quoted("plain"), "\"plain\"");
        assert_eq!(quoted("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quoted("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(quoted("line\nbreak"), "\"line\\nbreak\"");
    }
}
