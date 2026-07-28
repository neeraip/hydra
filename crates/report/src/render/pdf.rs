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
    s.push_str(
        "#set page(paper: \"a4\", margin: 2cm)\n\
         #set text(size: 10pt)\n\
         #set table(stroke: 0.5pt + luma(180), inset: 5pt)\n\
         #show heading.where(level: 1): set text(size: 17pt)\n\
         #show heading.where(level: 2): it => block(above: 1.4em, below: 0.7em)[#it]\n",
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
