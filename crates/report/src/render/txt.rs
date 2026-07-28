//! Plain-text renderer (spec §4.2): human-readable, diffable, fixed-width
//! tables.

use hydra_common::{Fragment, FragmentItem, Table, ValueKind};

use super::{column_header, derive_chart_table, value_human};
use crate::document::{ReportDocument, Section};

/// Render the document as plain text (spec §4.2).
pub fn render_txt(doc: &ReportDocument) -> String {
    let mut out = String::new();

    out.push_str(&doc.title);
    out.push('\n');
    out.push_str(&"=".repeat(doc.title.chars().count()));
    out.push('\n');
    if let Some(generated_at) = &doc.generated_at {
        out.push_str(&format!("Generated: {generated_at}\n"));
    }
    for (label, value) in &doc.source {
        out.push_str(&format!("{label}: {value}\n"));
    }

    for section in &doc.sections {
        out.push('\n');
        let title = section.title();
        out.push_str(title);
        out.push('\n');
        out.push_str(&"-".repeat(title.chars().count()));
        out.push('\n');
        match section {
            Section::Content(fragment) => render_fragment(&mut out, fragment),
            Section::Unavailable { reason, .. } => {
                out.push_str(&format!("[not available: {reason}]\n"));
            }
            Section::Failed { message, .. } => {
                out.push_str(&format!("[failed: {message}]\n"));
            }
        }
    }

    out
}

fn render_fragment(out: &mut String, fragment: &Fragment) {
    for (i, item) in fragment.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match item {
            FragmentItem::KeyValues { entries } => {
                let width = entries
                    .iter()
                    .map(|e| e.label.chars().count())
                    .max()
                    .unwrap_or(0);
                for entry in entries {
                    let pad = width - entry.label.chars().count();
                    out.push_str(&format!(
                        "{}:{} {}\n",
                        entry.label,
                        " ".repeat(pad),
                        value_human(&entry.value)
                    ));
                }
            }
            FragmentItem::Table { table } => render_table(out, table),
            FragmentItem::Note { text } => {
                out.push_str(text);
                out.push('\n');
            }
            FragmentItem::Chart { chart } => render_table(out, &derive_chart_table(chart)),
        }
    }
}

fn render_table(out: &mut String, table: &Table) {
    let headers: Vec<String> = table
        .columns
        .iter()
        .map(|c| column_header(&c.name, c.unit.as_deref()))
        .collect();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();

    let rows: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(value_human).collect())
        .collect();
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell.chars().count());
            }
        }
    }

    let numeric: Vec<bool> = table
        .columns
        .iter()
        .map(|c| matches!(c.kind, ValueKind::Number | ValueKind::Integer))
        .collect();

    let emit_row = |out: &mut String, cells: &[String]| {
        let last = cells.len().saturating_sub(1);
        for (i, cell) in cells.iter().enumerate() {
            let pad = widths[i] - cell.chars().count();
            // Right-align numeric columns; left-align text. The final
            // column never carries trailing padding.
            if numeric.get(i).copied().unwrap_or(false) {
                out.push_str(&" ".repeat(pad));
                out.push_str(cell);
            } else {
                out.push_str(cell);
                if i != last {
                    out.push_str(&" ".repeat(pad));
                }
            }
            if i != last {
                out.push_str("  ");
            }
        }
        out.push('\n');
    };

    emit_row(out, &headers);
    out.push_str(
        &widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  "),
    );
    out.push('\n');
    for row in &rows {
        emit_row(out, row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_common::{Column, KeyValue, Value};

    fn doc() -> ReportDocument {
        ReportDocument {
            title: "Anytown Report".into(),
            generated_at: Some("2026-07-28T12:00:00Z".into()),
            source: vec![("Project".into(), "Anytown".into())],
            sections: vec![
                Section::Content(Fragment {
                    title: "Summary".into(),
                    items: vec![
                        FragmentItem::KeyValues {
                            entries: vec![
                                KeyValue {
                                    label: "Junctions".into(),
                                    value: Value::Integer { value: 42 },
                                },
                                KeyValue {
                                    label: "Max pressure".into(),
                                    value: Value::Number {
                                        value: 51.25,
                                        unit: Some("m".into()),
                                    },
                                },
                            ],
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
                                        value: "Pressure".into(),
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
    fn renders_the_expected_layout() {
        let txt = render_txt(&doc());
        let expected = "\
Anytown Report
==============
Generated: 2026-07-28T12:00:00Z
Project: Anytown

Summary
-------
Junctions:    42
Max pressure: 51.25 m

Quantity  Max (m)
--------  -------
Pressure    51.25

Pump Energy
-----------
[not available: the network has no pumps]
";
        assert_eq!(txt, expected);
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(render_txt(&doc()), render_txt(&doc()));
    }
}
