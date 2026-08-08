//! Golden-file tests for the txt / csv / html renderers.
//!
//! The spec makes byte-identical output a promise (`document.rs`: "identical
//! inputs must yield byte-identical output"), and `hydra report
//! --no-timestamp` exists so reports can be diffed and reproduced. The unit
//! tests assert determinism *within a run* — `render(x) == render(x)` — which
//! catches hash ordering and stray clocks but not regression: a change to the
//! txt layout or the CSV quoting rules passes every one of them while silently
//! changing the format under every existing consumer.
//!
//! These pin the actual bytes, so such a change has to arrive as a visible
//! diff in review rather than as a surprise downstream.
//!
//! # Updating
//!
//! When a format change is intended, regenerate and read the diff:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p hydra-report --test golden
//! ```
//!
//! Review what moved before committing. A golden updated without reading it
//! is worse than no golden at all — it converts a caught regression into a
//! rubber stamp.

use std::path::PathBuf;

use hydra_common::{
    Chart, ChartData, Column, Fragment, FragmentItem, KeyValue, LineSeries, Table, Value, ValueKind,
};
use hydra_report::{render_csv, render_html, render_txt, ReportDocument, Section};

/// One document exercising what the three renderers disagree about.
///
/// Deliberately includes the cases that are easy to break and invisible in a
/// smoke test: every `Value` variant including `Absent` (a gap, never zero),
/// text needing CSV quoting and HTML escaping, a column with and without a
/// unit, a chart (graphical in html, a table derivation elsewhere), and the
/// two non-content sections.
fn fixture() -> ReportDocument {
    ReportDocument {
        title: "Golden Report".into(),
        // None, not a timestamp: a clock in a golden file makes it fail once
        // per run rather than once per regression.
        generated_at: None,
        source: vec![
            ("Model".into(), "network.inp".into()),
            ("Results".into(), "output.out".into()),
        ],
        sections: vec![
            Section::Content(Fragment {
                title: "Run Summary".into(),
                items: vec![
                    FragmentItem::KeyValues {
                        entries: vec![
                            KeyValue {
                                label: "Junctions".into(),
                                value: Value::Integer { value: 4 },
                            },
                            KeyValue {
                                label: "Duration".into(),
                                value: Value::Number {
                                    value: 24.0,
                                    unit: Some("h".into()),
                                    quantity: None,
                                },
                            },
                            KeyValue {
                                label: "Quality".into(),
                                value: Value::Boolean { value: true },
                            },
                            KeyValue {
                                label: "Started".into(),
                                value: Value::Timestamp {
                                    value: "2026-01-01T00:00:00Z".into(),
                                },
                            },
                            KeyValue {
                                label: "Not measured".into(),
                                value: Value::Absent,
                            },
                        ],
                    },
                    FragmentItem::Note {
                        text: "Values are <reported> at full precision; \
                               commas, \"quotes\" & ampersands are deliberate."
                            .into(),
                    },
                ],
            }),
            Section::Content(Fragment {
                title: "Result Extremes".into(),
                items: vec![FragmentItem::Table {
                    table: Table {
                        columns: vec![
                            Column {
                                name: "Node".into(),
                                unit: None,
                                kind: ValueKind::Text,
                                quantity: None,
                            },
                            Column {
                                name: "Pressure".into(),
                                unit: Some("m".into()),
                                kind: ValueKind::Number,
                                quantity: None,
                            },
                            Column {
                                name: "Reported".into(),
                                unit: None,
                                kind: ValueKind::Boolean,
                                quantity: None,
                            },
                        ],
                        rows: vec![
                            vec![
                                Value::Text {
                                    value: "J1, the \"first\" node".into(),
                                },
                                Value::Number {
                                    value: 31.845_123_456_789,
                                    unit: Some("m".into()),
                                    quantity: None,
                                },
                                Value::Boolean { value: true },
                            ],
                            vec![
                                Value::Text {
                                    value: "J2 <tank & inlet>".into(),
                                },
                                Value::Absent,
                                Value::Boolean { value: false },
                            ],
                            // KNOWN DEFECT, pinned deliberately: the txt
                            // renderer emits an embedded newline raw into a
                            // fixed-width column layout, so this row splits
                            // across two lines and its remaining columns stop
                            // lining up under their headers. csv quotes the
                            // field per RFC 4180 and html collapses it, so
                            // both are fine. The golden records what txt does
                            // today so that fixing it shows up as an
                            // intentional diff rather than a surprise — the
                            // file is a record of current behaviour, not an
                            // endorsement of it.
                            vec![
                                Value::Text {
                                    value: "line\nbreak".into(),
                                },
                                Value::Number {
                                    value: -0.5,
                                    unit: Some("m".into()),
                                    quantity: None,
                                },
                                Value::Absent,
                            ],
                        ],
                    },
                }],
            }),
            Section::Content(Fragment {
                title: "Pressure Distribution".into(),
                items: vec![
                    FragmentItem::Chart {
                        chart: Chart {
                            x_label: "Band".into(),
                            x_unit: Some("m".into()),
                            x_quantity: None,
                            y_label: "Junctions".into(),
                            y_unit: None,
                            y_quantity: None,
                            data: ChartData::Bar {
                                categories: vec!["< 20".into(), "20–40".into(), "> 40".into()],
                                values: vec![1.0, 2.0, 1.0],
                            },
                        },
                    },
                    FragmentItem::Chart {
                        chart: Chart {
                            x_label: "Time".into(),
                            x_unit: Some("h".into()),
                            x_quantity: None,
                            y_label: "Head".into(),
                            y_unit: Some("m".into()),
                            y_quantity: None,
                            data: ChartData::Line {
                                series: vec![LineSeries {
                                    name: "T1".into(),
                                    points: vec![[0.0, 100.0], [12.0, 98.25], [24.0, 101.5]],
                                }],
                            },
                        },
                    },
                ],
            }),
            Section::Unavailable {
                title: "Pump Energy".into(),
                reason: "the network has no pumps".into(),
            },
            Section::Failed {
                title: "wds.nope".into(),
                message: "unknown report block: \"wds.nope\"".into(),
            },
        ],
    }
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Compare against the stored bytes, or rewrite them under `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read golden {}: {e}\n\
             If this is a new golden, create it with:\n    \
             UPDATE_GOLDEN=1 cargo test -p hydra-report --test golden",
            path.display()
        )
    });
    assert_eq!(
        actual, expected,
        "\n{} no longer matches its golden file.\n\
         If the change is intended, regenerate and READ the diff:\n    \
         UPDATE_GOLDEN=1 cargo test -p hydra-report --test golden\n",
        name
    );
}

#[test]
fn txt_output_matches_its_golden() {
    assert_golden("report.txt", &render_txt(&fixture()));
}

#[test]
fn csv_output_matches_its_golden() {
    assert_golden("report.csv", &render_csv(&fixture()));
}

#[test]
fn html_output_matches_its_golden() {
    assert_golden("report.html", &render_html(&fixture()));
}

/// The goldens are only meaningful if the fixture still reaches the code
/// paths they were written for. A refactor that quietly drops a variant from
/// `fixture()` would leave three passing tests guarding less than they claim.
#[test]
fn the_fixture_still_covers_every_shape_it_was_written_for() {
    let doc = fixture();

    let (content, unavailable, failed) =
        doc.sections.iter().fold((0, 0, 0), |(c, u, f), s| match s {
            Section::Content(_) => (c + 1, u, f),
            Section::Unavailable { .. } => (c, u + 1, f),
            Section::Failed { .. } => (c, u, f + 1),
        });
    assert!(content >= 3, "content sections");
    assert_eq!(unavailable, 1, "an unavailable section");
    assert_eq!(failed, 1, "a failed section");

    let items: Vec<&FragmentItem> = doc
        .sections
        .iter()
        .filter_map(|s| match s {
            Section::Content(f) => Some(&f.items),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(items
        .iter()
        .any(|i| matches!(i, FragmentItem::KeyValues { .. })));
    assert!(items
        .iter()
        .any(|i| matches!(i, FragmentItem::Table { .. })));
    assert!(items.iter().any(|i| matches!(i, FragmentItem::Note { .. })));
    assert!(items.iter().any(|i| matches!(
        i,
        FragmentItem::Chart {
            chart: Chart {
                data: ChartData::Bar { .. },
                ..
            }
        }
    )));
    assert!(items.iter().any(|i| matches!(
        i,
        FragmentItem::Chart {
            chart: Chart {
                data: ChartData::Line { .. },
                ..
            }
        }
    )));

    // Every Value variant, Absent included — it must render as a gap, and a
    // renderer that printed 0.0 instead would be caught by the goldens only
    // while a case remains here.
    let values: Vec<&Value> = items
        .iter()
        .flat_map(|i| -> Vec<&Value> {
            match i {
                FragmentItem::KeyValues { entries } => entries.iter().map(|e| &e.value).collect(),
                FragmentItem::Table { table } => table.rows.iter().flatten().collect(),
                _ => vec![],
            }
        })
        .collect();
    for expected in [
        Value::Number {
            value: 0.0,
            unit: None,
            quantity: None,
        },
        Value::Integer { value: 0 },
        Value::Boolean { value: false },
        Value::Text {
            value: String::new(),
        },
        Value::Timestamp {
            value: String::new(),
        },
        Value::Absent,
    ] {
        assert!(
            values
                .iter()
                .any(|v| std::mem::discriminant(*v) == std::mem::discriminant(&expected)),
            "fixture no longer contains a {expected:?} — the goldens stopped covering it"
        );
    }

    // Characters each renderer has to treat specially.
    let text = format!("{doc:?}");
    for needle in ["\"", ",", "&", "<", "\\n"] {
        assert!(
            text.contains(needle),
            "fixture no longer contains {needle:?}, which the renderers must escape or quote"
        );
    }
}
