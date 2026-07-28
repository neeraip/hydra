//! Renderers (spec §4): pure functions from a document to a string, with
//! shared value-formatting helpers. Deterministic byte-for-byte for
//! identical documents; renderers cannot fail.

mod chart_svg;
mod csv;
mod html;
#[cfg(feature = "pdf")]
mod pdf;
mod txt;

pub use csv::render_csv;
pub use html::render_html;
#[cfg(feature = "pdf")]
pub use pdf::{render_pdf, PdfError};
pub use txt::render_txt;

use hydra_common::{Chart, ChartData, Column, Table, Value, ValueKind};

/// The mechanical table derivation of a chart (hydra-common spec §3.3),
/// used by renderers without graphics support (txt, csv): bar →
/// category/value rows; line → x column plus one column per series,
/// absent where a series lacks that x.
pub(crate) fn derive_chart_table(chart: &Chart) -> Table {
    match &chart.data {
        ChartData::Bar { categories, values } => Table {
            columns: vec![
                Column {
                    name: chart.x_label.clone(),
                    unit: chart.x_unit.clone(),
                    kind: ValueKind::Text,
                },
                Column {
                    name: chart.y_label.clone(),
                    unit: chart.y_unit.clone(),
                    kind: ValueKind::Number,
                },
            ],
            rows: categories
                .iter()
                .zip(values)
                .map(|(category, &value)| {
                    vec![
                        Value::Text {
                            value: category.clone(),
                        },
                        Value::Number { value, unit: None },
                    ]
                })
                .collect(),
        },
        ChartData::Line { series } => {
            let mut xs: Vec<f64> = series
                .iter()
                .flat_map(|s| s.points.iter().map(|p| p[0]))
                .collect();
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            xs.dedup_by(|a, b| (*a - *b).abs() < 1e-12);

            let mut columns = vec![Column {
                name: chart.x_label.clone(),
                unit: chart.x_unit.clone(),
                kind: ValueKind::Number,
            }];
            columns.extend(series.iter().map(|s| Column {
                name: s.name.clone(),
                unit: chart.y_unit.clone(),
                kind: ValueKind::Number,
            }));

            let rows = xs
                .iter()
                .map(|&x| {
                    let mut row = vec![Value::Number {
                        value: x,
                        unit: None,
                    }];
                    for s in series {
                        row.push(
                            s.points
                                .iter()
                                .find(|p| (p[0] - x).abs() < 1e-12)
                                .map(|p| Value::Number {
                                    value: p[1],
                                    unit: None,
                                })
                                .unwrap_or(Value::Absent),
                        );
                    }
                    row
                })
                .collect();
            Table { columns, rows }
        }
    }
}

/// Human number formatting (spec §4.1): up to 3 decimal places, trailing
/// zeros and trailing decimal point trimmed.
pub(crate) fn human_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let mut s = format!("{value:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    // -0.0004 trims to "-0" — normalise to "0".
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// Human rendering of a value with its unit (spec §4.1). Absent → em dash.
pub(crate) fn value_human(value: &Value) -> String {
    match value {
        Value::Number { value, unit } => match unit {
            Some(unit) => format!("{} {unit}", human_number(*value)),
            None => human_number(*value),
        },
        Value::Integer { value } => value.to_string(),
        Value::Boolean { value } => if *value { "yes" } else { "no" }.into(),
        Value::Text { value } | Value::Timestamp { value } => value.clone(),
        Value::Absent => "—".into(),
    }
}

/// Data rendering of a value for csv (spec §4.1): shortest round-trip
/// numbers, no unit (units live in the header/unit column). Absent → empty.
pub(crate) fn value_data(value: &Value) -> String {
    match value {
        Value::Number { value, .. } => value.to_string(),
        Value::Integer { value } => value.to_string(),
        Value::Boolean { value } => value.to_string(),
        Value::Text { value } | Value::Timestamp { value } => value.clone(),
        Value::Absent => String::new(),
    }
}

/// `Name (unit)` column header text (spec §4.2/§4.3).
pub(crate) fn column_header(name: &str, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{name} ({unit})"),
        None => name.into(),
    }
}

/// The unit text of a value, for the csv key-value unit column.
pub(crate) fn value_unit(value: &Value) -> &str {
    match value {
        Value::Number {
            unit: Some(unit), ..
        } => unit,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_numbers_trim_cleanly() {
        assert_eq!(human_number(1234.5), "1234.5");
        assert_eq!(human_number(0.042), "0.042");
        assert_eq!(human_number(7.0), "7");
        assert_eq!(human_number(1.2345), "1.234"); // ≤3 decimals (ties-to-even fmt)
        assert_eq!(human_number(-0.0004), "0");
        assert_eq!(human_number(f64::NAN), "NaN");
    }

    #[test]
    fn values_render_by_kind() {
        assert_eq!(
            value_human(&Value::Number {
                value: 2.5,
                unit: Some("m".into())
            }),
            "2.5 m"
        );
        assert_eq!(value_human(&Value::Boolean { value: true }), "yes");
        assert_eq!(value_human(&Value::Absent), "—");
        assert_eq!(value_data(&Value::Absent), "");
        // Data fidelity: shortest round-trip, not human trimming.
        assert_eq!(
            value_data(&Value::Number {
                value: 0.1234567,
                unit: None
            }),
            "0.1234567"
        );
    }
}
