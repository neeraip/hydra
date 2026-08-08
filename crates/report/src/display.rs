//! Display-family resolution (spec §4.0): re-express quantity-tagged
//! values in the family the reader chose, before rendering.
//!
//! A tagged number arrives in its quantity's SI display unit (hydra-common
//! spec §3.3). Resolution converts the value by the descriptor's affine
//! map, swaps the unit text for the family's label, and **drops the tag**:
//! the tag's meaning is "this value is in SI display units", which stops
//! being true the moment the value is converted. A resolved document is
//! therefore an ordinary pre-v1.7 document, and the renderers never learn
//! any of this happened — which is what keeps them byte-for-byte
//! deterministic in exactly the terms they always were.
//!
//! Resolution is deliberately a document transform rather than a renderer
//! parameter. Four renderers walk fragments; teaching each to convert
//! would be the same decision four times, and the pdf renderer's chart
//! pipeline would make it five. One pass, one place, and `render_*`
//! signatures stay untouched.
//!
//! What resolution does not touch, by spec: untagged values (engine-
//! authored display text, rendered as written), tags naming a key the
//! catalog does not declare (a producer defect — the value renders as
//! written rather than failing the document), and every kind of text —
//! including engine-authored text that happens to embed numbers, like
//! threshold band labels and narrative notes.

use hydra_common::{
    Chart, ChartData, Column, DisplayFamily, Fragment, FragmentItem, KeyValue, QuantityDescriptor,
    Table, Value,
};

use crate::document::{ReportDocument, Section};

/// How to display quantity-tagged values: which family, resolved against
/// which engine's catalog (spec §4.0).
#[derive(Debug, Clone, Copy)]
pub struct DisplaySettings<'a> {
    /// The display family the reader chose.
    pub family: DisplayFamily,
    /// The producing engine's quantity catalog (hydra-common spec §5),
    /// handed in by the application — this layer never looks one up.
    pub catalog: &'a [QuantityDescriptor],
}

impl<'a> DisplaySettings<'a> {
    fn descriptor(&self, key: &str) -> Option<&'a QuantityDescriptor> {
        self.catalog.iter().find(|q| q.key == key)
    }
}

/// Re-express every quantity-tagged value of `document` in the chosen
/// family and drop the tags (spec §4.0). Untagged values, unknown keys,
/// and all text pass through unchanged; `Si` converts nothing but still
/// drops tags, so a resolved document never claims a tag it might not
/// honour.
pub fn resolve_display(
    document: &ReportDocument,
    settings: &DisplaySettings<'_>,
) -> ReportDocument {
    ReportDocument {
        title: document.title.clone(),
        generated_at: document.generated_at.clone(),
        source: document.source.clone(),
        sections: document
            .sections
            .iter()
            .map(|section| match section {
                Section::Content(fragment) => {
                    Section::Content(resolve_fragment(fragment, settings))
                }
                other => other.clone(),
            })
            .collect(),
    }
}

fn resolve_fragment(fragment: &Fragment, settings: &DisplaySettings<'_>) -> Fragment {
    Fragment {
        title: fragment.title.clone(),
        items: fragment
            .items
            .iter()
            .map(|item| match item {
                FragmentItem::KeyValues { entries } => FragmentItem::KeyValues {
                    entries: entries
                        .iter()
                        .map(|kv| KeyValue {
                            label: kv.label.clone(),
                            value: resolve_value(kv.value.clone(), None, settings),
                        })
                        .collect(),
                },
                FragmentItem::Table { table } => FragmentItem::Table {
                    table: resolve_table(table, settings),
                },
                FragmentItem::Chart { chart } => FragmentItem::Chart {
                    chart: resolve_chart(chart, settings),
                },
                note @ FragmentItem::Note { .. } => note.clone(),
            })
            .collect(),
    }
}

/// Resolve one value. `column_quantity` is the owning column's tag, which
/// applies to every number in the column that does not carry its own
/// (spec §3.3: a column's tag applies to its numbers).
///
/// Resolution preserves the *shape* the producer chose: unit text is
/// swapped where it exists and never invented where it does not. A table
/// cell typically carries no unit text because its column header does —
/// converting the number must not suddenly stamp "psi" onto every cell.
fn resolve_value(
    value: Value,
    column_quantity: Option<&str>,
    settings: &DisplaySettings<'_>,
) -> Value {
    let Value::Number {
        value: number,
        unit,
        quantity,
    } = value
    else {
        return value;
    };
    let key = quantity.as_deref().or(column_quantity);
    let Some(descriptor) = key.and_then(|k| settings.descriptor(k)) else {
        // Untagged, or a key the catalog does not declare: render as
        // written. The value keeps no tag either way — an unknown key is
        // a producer defect, and forwarding it would just move the defect
        // into the rendered output's metadata.
        return Value::Number {
            value: number,
            unit,
            quantity: None,
        };
    };
    Value::Number {
        value: descriptor.from_si(number, settings.family),
        unit: unit.map(|_| descriptor.label(settings.family).to_string()),
        quantity: None,
    }
}

fn resolve_table(table: &Table, settings: &DisplaySettings<'_>) -> Table {
    let column_tags: Vec<Option<String>> =
        table.columns.iter().map(|c| c.quantity.clone()).collect();
    Table {
        columns: table
            .columns
            .iter()
            .map(|column| {
                let resolved_unit = column
                    .quantity
                    .as_deref()
                    .and_then(|k| settings.descriptor(k))
                    .map(|d| d.label(settings.family).to_string())
                    .or_else(|| column.unit.clone());
                Column {
                    name: column.name.clone(),
                    unit: resolved_unit,
                    kind: column.kind,
                    quantity: None,
                }
            })
            .collect(),
        rows: table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .zip(column_tags.iter().chain(std::iter::repeat(&None)))
                    .map(|(value, column_tag)| {
                        resolve_value(value.clone(), column_tag.as_deref(), settings)
                    })
                    .collect()
            })
            .collect(),
    }
}

fn resolve_chart(chart: &Chart, settings: &DisplaySettings<'_>) -> Chart {
    let x = chart
        .x_quantity
        .as_deref()
        .and_then(|k| settings.descriptor(k));
    let y = chart
        .y_quantity
        .as_deref()
        .and_then(|k| settings.descriptor(k));
    let data = match &chart.data {
        ChartData::Bar { categories, values } => ChartData::Bar {
            categories: categories.clone(),
            values: match y {
                Some(d) => values
                    .iter()
                    .map(|&v| d.from_si(v, settings.family))
                    .collect(),
                None => values.clone(),
            },
        },
        ChartData::Line { series } => ChartData::Line {
            series: series
                .iter()
                .map(|s| hydra_common::LineSeries {
                    name: s.name.clone(),
                    points: s
                        .points
                        .iter()
                        .map(|&[px, py]| {
                            [
                                x.map_or(px, |d| d.from_si(px, settings.family)),
                                y.map_or(py, |d| d.from_si(py, settings.family)),
                            ]
                        })
                        .collect(),
                })
                .collect(),
        },
    };
    Chart {
        x_label: chart.x_label.clone(),
        x_unit: x
            .map(|d| d.label(settings.family).to_string())
            .or_else(|| chart.x_unit.clone()),
        x_quantity: None,
        y_label: chart.y_label.clone(),
        y_unit: y
            .map(|d| d.label(settings.family).to_string())
            .or_else(|| chart.y_unit.clone()),
        y_quantity: None,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ReportDocument;
    use hydra_common::LineSeries;

    // The scale is exact in binary (×2) so equality assertions stay
    // honest — the real catalog's factors are the engines' business.
    const PRESSURE: QuantityDescriptor = QuantityDescriptor {
        key: "pressure",
        si_label: "m",
        us_label: "psi",
        si_to_us_scale: 2.0,
        si_to_us_offset: 0.0,
        si_decimals: 1,
        us_decimals: 1,
    };

    fn settings(family: DisplayFamily) -> DisplaySettings<'static> {
        DisplaySettings {
            family,
            catalog: &[PRESSURE],
        }
    }

    fn doc_with(items: Vec<FragmentItem>) -> ReportDocument {
        ReportDocument {
            title: "T".into(),
            generated_at: None,
            source: vec![],
            sections: vec![Section::Content(Fragment {
                title: "F".into(),
                items,
            })],
        }
    }

    fn first_items(doc: &ReportDocument) -> &[FragmentItem] {
        match &doc.sections[0] {
            Section::Content(f) => &f.items,
            _ => panic!("content section"),
        }
    }

    fn tagged(value: f64, unit: Option<&str>, quantity: &str) -> Value {
        Value::Number {
            value,
            unit: unit.map(Into::into),
            quantity: Some(quantity.into()),
        }
    }

    /// The point of the mechanism: a tagged SI value reads in the family
    /// the reader chose, converted by the descriptor and wearing its label.
    #[test]
    fn a_tagged_value_converts_under_us() {
        let doc = doc_with(vec![FragmentItem::KeyValues {
            entries: vec![KeyValue {
                label: "Min pressure".into(),
                value: tagged(10.0, Some("m"), "pressure"),
            }],
        }]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Us));
        let FragmentItem::KeyValues { entries } = &first_items(&resolved)[0] else {
            panic!("key values");
        };
        assert_eq!(
            entries[0].value,
            Value::Number {
                value: 20.0,
                unit: Some("psi".into()),
                quantity: None,
            }
        );
    }

    /// Si converts nothing but still drops the tag: a resolved document
    /// never claims a tag the resolution might not have honoured.
    #[test]
    fn si_is_identity_except_the_tag() {
        let doc = doc_with(vec![FragmentItem::KeyValues {
            entries: vec![KeyValue {
                label: "Min pressure".into(),
                value: tagged(10.0, Some("m"), "pressure"),
            }],
        }]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Si));
        let FragmentItem::KeyValues { entries } = &first_items(&resolved)[0] else {
            panic!("key values");
        };
        assert_eq!(
            entries[0].value,
            Value::Number {
                value: 10.0,
                unit: Some("m".into()),
                quantity: None,
            }
        );
    }

    /// A key the catalog does not declare is a producer defect: the value
    /// renders as written rather than failing the fragment (spec §3.3).
    #[test]
    fn an_unknown_key_renders_as_written() {
        let doc = doc_with(vec![FragmentItem::KeyValues {
            entries: vec![KeyValue {
                label: "X".into(),
                value: tagged(10.0, Some("m"), "no-such-quantity"),
            }],
        }]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Us));
        let FragmentItem::KeyValues { entries } = &first_items(&resolved)[0] else {
            panic!("key values");
        };
        assert_eq!(
            entries[0].value,
            Value::Number {
                value: 10.0,
                unit: Some("m".into()),
                quantity: None,
            }
        );
    }

    /// Resolution preserves the shape the producer chose: a tagged value
    /// with no unit text converts without gaining any. Table cells rely on
    /// this — their column header carries the unit.
    #[test]
    fn conversion_never_invents_unit_text() {
        let doc = doc_with(vec![FragmentItem::KeyValues {
            entries: vec![KeyValue {
                label: "X".into(),
                value: tagged(10.0, None, "pressure"),
            }],
        }]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Us));
        let FragmentItem::KeyValues { entries } = &first_items(&resolved)[0] else {
            panic!("key values");
        };
        assert_eq!(
            entries[0].value,
            Value::Number {
                value: 20.0,
                unit: None,
                quantity: None,
            }
        );
    }

    /// A column's tag converts every number under it and re-labels the
    /// header; untagged columns beside it are untouched (spec §3.3: tags
    /// are per-value facts, not a fragment-wide mode).
    #[test]
    fn a_column_tag_governs_its_cells_and_header() {
        let doc = doc_with(vec![FragmentItem::Table {
            table: Table {
                columns: vec![
                    Column {
                        name: "Junction".into(),
                        unit: None,
                        kind: hydra_common::ValueKind::Text,
                        quantity: None,
                    },
                    Column {
                        name: "Min pressure".into(),
                        unit: Some("m".into()),
                        kind: hydra_common::ValueKind::Number,
                        quantity: Some("pressure".into()),
                    },
                ],
                rows: vec![vec![
                    Value::Text { value: "J1".into() },
                    Value::Number {
                        value: 10.0,
                        unit: None,
                        quantity: None,
                    },
                ]],
            },
        }]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Us));
        let FragmentItem::Table { table } = &first_items(&resolved)[0] else {
            panic!("table");
        };
        assert_eq!(table.columns[1].unit.as_deref(), Some("psi"));
        assert_eq!(table.columns[1].quantity, None);
        assert_eq!(
            table.rows[0][1],
            Value::Number {
                value: 20.0,
                unit: None,
                quantity: None,
            }
        );
        assert_eq!(table.rows[0][0], Value::Text { value: "J1".into() });
    }

    /// Chart axes: a tagged y converts bar values and re-labels the axis;
    /// line points convert per tagged axis.
    #[test]
    fn chart_axes_convert_by_their_tags() {
        let doc = doc_with(vec![
            FragmentItem::Chart {
                chart: Chart {
                    x_label: "Band".into(),
                    x_unit: None,
                    x_quantity: None,
                    y_label: "Pressure".into(),
                    y_unit: Some("m".into()),
                    y_quantity: Some("pressure".into()),
                    data: ChartData::Bar {
                        categories: vec!["a".into()],
                        values: vec![10.0],
                    },
                },
            },
            FragmentItem::Chart {
                chart: Chart {
                    x_label: "Time".into(),
                    x_unit: Some("h".into()),
                    x_quantity: None,
                    y_label: "Pressure".into(),
                    y_unit: Some("m".into()),
                    y_quantity: Some("pressure".into()),
                    data: ChartData::Line {
                        series: vec![LineSeries {
                            name: "T1".into(),
                            points: vec![[1.0, 10.0]],
                        }],
                    },
                },
            },
        ]);
        let resolved = resolve_display(&doc, &settings(DisplayFamily::Us));
        let items = first_items(&resolved);
        let FragmentItem::Chart { chart: bar } = &items[0] else {
            panic!("bar");
        };
        assert_eq!(bar.y_unit.as_deref(), Some("psi"));
        assert_eq!(bar.y_quantity, None);
        let ChartData::Bar { values, .. } = &bar.data else {
            panic!("bar data");
        };
        assert_eq!(values[0], 20.0);
        let FragmentItem::Chart { chart: line } = &items[1] else {
            panic!("line");
        };
        let ChartData::Line { series } = &line.data else {
            panic!("line data");
        };
        // x untagged: hours pass through. y tagged: m → psi.
        assert_eq!(series[0].points[0], [1.0, 20.0]);
        assert_eq!(line.x_unit.as_deref(), Some("h"));
    }

    /// End to end: the resolved document renders with converted numbers,
    /// through the untouched renderer.
    #[test]
    fn a_resolved_document_renders_converted() {
        let doc = doc_with(vec![FragmentItem::KeyValues {
            entries: vec![KeyValue {
                label: "Min pressure".into(),
                value: tagged(10.0, Some("m"), "pressure"),
            }],
        }]);
        let txt = crate::render_txt(&resolve_display(&doc, &settings(DisplayFamily::Us)));
        assert!(txt.contains("20 psi"), "{txt}");
        let txt_si = crate::render_txt(&resolve_display(&doc, &settings(DisplayFamily::Si)));
        assert!(txt_si.contains("10 m"), "{txt_si}");
    }
}
