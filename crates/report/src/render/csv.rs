//! CSV renderer (spec §4.3): RFC 4180 quoting, `\n` line endings, full
//! numeric precision. Sections are separated by one blank line and
//! introduced by a `#`-prefixed title row.

use hydra_common::{Fragment, FragmentItem, Table};

use super::{column_header, derive_chart_table, value_data, value_unit};
use crate::document::{ReportDocument, Section};

/// Render the document as CSV (spec §4.3).
pub fn render_csv(doc: &ReportDocument) -> String {
    let mut out = String::new();

    push_row(&mut out, &[format!("# {}", doc.title)]);
    if let Some(generated_at) = &doc.generated_at {
        push_row(&mut out, &["Generated".into(), generated_at.clone()]);
    }
    for (label, value) in &doc.source {
        push_row(&mut out, &[label.clone(), value.clone()]);
    }

    for section in &doc.sections {
        out.push('\n');
        push_row(&mut out, &[format!("# {}", section.title())]);
        match section {
            Section::Content(fragment) => render_fragment(&mut out, fragment),
            Section::Unavailable { reason, .. } => {
                push_row(&mut out, &[format!("[not available: {reason}]")]);
            }
            Section::Failed { message, .. } => {
                push_row(&mut out, &[format!("[failed: {message}]")]);
            }
        }
    }

    out
}

fn render_fragment(out: &mut String, fragment: &Fragment) {
    for item in &fragment.items {
        match item {
            FragmentItem::KeyValues { entries } => {
                for entry in entries {
                    push_row(
                        out,
                        &[
                            entry.label.clone(),
                            value_data(&entry.value),
                            value_unit(&entry.value).into(),
                        ],
                    );
                }
            }
            FragmentItem::Table { table } => render_table(out, table),
            FragmentItem::Note { text } => push_row(out, std::slice::from_ref(text)),
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
    push_row(out, &headers);
    for row in &table.rows {
        let cells: Vec<String> = row.iter().map(value_data).collect();
        push_row(out, &cells);
    }
}

/// RFC 4180: quote fields containing `"`, `,`, CR, or LF; double quotes.
fn field(text: &str) -> String {
    if text.contains(['"', ',', '\r', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.into()
    }
}

fn push_row(out: &mut String, cells: &[String]) {
    let row = cells.iter().map(|c| field(c)).collect::<Vec<_>>().join(",");
    out.push_str(&row);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_common::{Column, KeyValue, Value, ValueKind};

    #[test]
    fn quotes_per_rfc_4180() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn renders_sections_tables_and_full_precision() {
        let doc = ReportDocument {
            title: "T, with comma".into(),
            generated_at: None,
            source: vec![],
            sections: vec![Section::Content(Fragment {
                title: "S".into(),
                items: vec![
                    FragmentItem::KeyValues {
                        entries: vec![KeyValue {
                            label: "Max".into(),
                            value: Value::Number {
                                value: 0.1234567,
                                unit: Some("m".into()),
                            },
                        }],
                    },
                    FragmentItem::Table {
                        table: Table {
                            columns: vec![Column {
                                name: "Flow".into(),
                                unit: Some("LPS".into()),
                                kind: ValueKind::Number,
                            }],
                            rows: vec![vec![Value::Number {
                                value: 1.5,
                                unit: None,
                            }]],
                        },
                    },
                ],
            })],
        };
        let expected = "\
\"# T, with comma\"

# S
Max,0.1234567,m
Flow (LPS)
1.5
";
        assert_eq!(render_csv(&doc), expected);
    }
}
