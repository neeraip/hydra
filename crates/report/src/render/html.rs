//! HTML renderer (spec §4.4): one self-contained document — semantic
//! markup, inline CSS only, no external resources, no scripts. Neutral
//! styling that prints acceptably; doubles as the GUI's live preview.

use hydra_common::{Fragment, FragmentItem, Table, ValueKind};

use super::{chart_svg::chart_svg, column_header, value_human};
use crate::document::{ReportDocument, Section};

const STYLE: &str = "\
:root { color-scheme: light; }
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
       margin: 2.5rem auto; padding: 0 1.5rem;
       color: #1a222c; line-height: 1.5; }
header { border-bottom: 2px solid #1a222c; margin-bottom: 1.5rem; padding-bottom: 0.75rem; }
h1 { font-size: 1.5rem; margin: 0 0 0.5rem; }
h2 { font-size: 1.05rem; margin: 2rem 0 0.6rem; border-bottom: 1px solid #c9d1d9;
     padding-bottom: 0.25rem; }
.provenance { font-size: 0.85rem; color: #555f6b; }
.provenance span { margin-right: 1.25rem; }
dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.15rem 1rem; margin: 0.6rem 0; }
dt { color: #555f6b; }
dd { margin: 0; }
table { border-collapse: collapse; margin: 0.6rem 0; width: 100%; font-size: 0.9rem; }
th, td { border: 1px solid #c9d1d9; padding: 0.3rem 0.55rem; text-align: left; }
th { background: #f2f4f7; }
td.num, th.num { text-align: right; font-variant-numeric: tabular-nums; }
p.note { font-size: 0.85rem; color: #555f6b; }
figure.chart { margin: 0.8rem 0; }
figure.chart svg { max-width: 100%; height: auto; }
p.placeholder { font-style: italic; color: #8a6d1f; background: #fdf6e3;
                border: 1px solid #e8dcb5; border-radius: 4px; padding: 0.4rem 0.6rem; }
@media print { body { margin: 0; } }
";

/// Render the document as a self-contained HTML page (spec §4.4).
pub fn render_html(doc: &ReportDocument) -> String {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", esc(&doc.title)));
    out.push_str(&format!("<style>\n{STYLE}</style>\n</head>\n<body>\n"));

    out.push_str("<header>\n");
    out.push_str(&format!("<h1>{}</h1>\n", esc(&doc.title)));
    if doc.generated_at.is_some() || !doc.source.is_empty() {
        out.push_str("<div class=\"provenance\">");
        for (label, value) in &doc.source {
            out.push_str(&format!("<span>{}: {}</span>", esc(label), esc(value)));
        }
        if let Some(generated_at) = &doc.generated_at {
            out.push_str(&format!("<span>Generated: {}</span>", esc(generated_at)));
        }
        out.push_str("</div>\n");
    }
    out.push_str("</header>\n");

    for section in &doc.sections {
        out.push_str(&format!("<h2>{}</h2>\n", esc(section.title())));
        match section {
            Section::Content(fragment) => render_fragment(&mut out, fragment),
            Section::Unavailable { reason, .. } => {
                out.push_str(&format!(
                    "<p class=\"placeholder\">Not available: {}</p>\n",
                    esc(reason)
                ));
            }
            Section::Failed { message, .. } => {
                out.push_str(&format!(
                    "<p class=\"placeholder\">Failed: {}</p>\n",
                    esc(message)
                ));
            }
        }
    }

    out.push_str("</body>\n</html>\n");
    out
}

fn render_fragment(out: &mut String, fragment: &Fragment) {
    for item in &fragment.items {
        match item {
            FragmentItem::KeyValues { entries } => {
                out.push_str("<dl>\n");
                for entry in entries {
                    out.push_str(&format!(
                        "<dt>{}</dt><dd>{}</dd>\n",
                        esc(&entry.label),
                        esc(&value_human(&entry.value))
                    ));
                }
                out.push_str("</dl>\n");
            }
            FragmentItem::Table { table } => render_table(out, table),
            FragmentItem::Note { text } => {
                out.push_str(&format!("<p class=\"note\">{}</p>\n", esc(text)));
            }
            FragmentItem::Chart { chart } => {
                // The generator escapes all embedded text itself.
                out.push_str("<figure class=\"chart\">\n");
                out.push_str(&chart_svg(chart));
                out.push_str("</figure>\n");
            }
        }
    }
}

fn render_table(out: &mut String, table: &Table) {
    let class = |kind: ValueKind| {
        if matches!(kind, ValueKind::Number | ValueKind::Integer) {
            " class=\"num\""
        } else {
            ""
        }
    };
    out.push_str("<table>\n<thead><tr>");
    for column in &table.columns {
        out.push_str(&format!(
            "<th{}>{}</th>",
            class(column.kind),
            esc(&column_header(&column.name, column.unit.as_deref()))
        ));
    }
    out.push_str("</tr></thead>\n<tbody>\n");
    for row in &table.rows {
        out.push_str("<tr>");
        for (i, value) in row.iter().enumerate() {
            let kind_class = table
                .columns
                .get(i)
                .map(|c| class(c.kind))
                .unwrap_or_default();
            out.push_str(&format!(
                "<td{kind_class}>{}</td>",
                esc(&value_human(value))
            ));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n");
}

/// Escape text content for HTML (also safe inside attribute values).
fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_common::{KeyValue, Value};

    #[test]
    fn escapes_all_text_content() {
        let doc = ReportDocument {
            title: "<Anytown> & \"Co\"".into(),
            generated_at: None,
            source: vec![],
            sections: vec![Section::Content(Fragment {
                title: "S<script>".into(),
                items: vec![FragmentItem::KeyValues {
                    entries: vec![KeyValue {
                        label: "a<b".into(),
                        value: Value::Text {
                            value: "x&y".into(),
                        },
                    }],
                }],
            })],
        };
        let html = render_html(&doc);
        assert!(html.contains("&lt;Anytown&gt; &amp; &quot;Co&quot;"));
        assert!(html.contains("S&lt;script&gt;"));
        assert!(html.contains("<dt>a&lt;b</dt><dd>x&amp;y</dd>"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn is_self_contained_and_scriptless() {
        let doc = ReportDocument {
            title: "T".into(),
            generated_at: Some("2026-07-28T12:00:00Z".into()),
            source: vec![("Project".into(), "Anytown".into())],
            sections: vec![Section::Unavailable {
                title: "P".into(),
                reason: "r".into(),
            }],
        };
        let html = render_html(&doc);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("class=\"placeholder\""));
        for forbidden in ["<script", "http://", "https://", "src=", "@import"] {
            assert!(!html.contains(forbidden), "forbidden: {forbidden}");
        }
    }
}
