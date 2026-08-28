//! Reportable-output contract: block descriptors and the neutral fragment
//! model (spec §3).
//!
//! Everything here is *data*, deliberately free of presentation (no colors,
//! fonts, page geometry, or format hints) and free of engine knowledge. An
//! engine's catalog describes what it can produce; fragments carry the
//! materialized content of one block for one completed simulation; the
//! report layer renders fragments without knowing which engine made them.

use serde::{Deserialize, Serialize};

/// One non-fatal diagnostic a run produced, in the neutral shape every
/// engine's warnings take when they reach report production (spec §3.4.1).
///
/// A run raises these while it works — a solver that could not balance, an
/// element the engine had to constrain — and they are not recoverable from a
/// results file written for a legacy dialect, so they travel beside the
/// results rather than inside them.
///
/// The wire shape is `{ "code", "message", "elementId"?, "time"? }`. Both
/// optional fields default to absent when a stored record omits them.
///
/// **A list of these is not the same as no list.** An empty list means the
/// run was observed and raised nothing; no list at all means the run's
/// diagnostics are unknown, and a block built on them is then
/// [`BlockError::Unavailable`] rather than empty. See spec §3.4.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunDiagnostic {
    /// Stable machine identifier for the diagnostic's class, authored by the
    /// producing engine and opaque here. Stable once released, so a consumer
    /// may group, count, or filter on it without knowing what it means.
    pub code: String,
    /// What happened, for a person to read. A complete sentence: a consumer
    /// may show it standing alone rather than after a label.
    pub message: String,
    /// Identifier of the element the diagnostic names, absent when it names
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    /// Simulated time at which it was raised, in seconds, absent when it is
    /// not tied to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<f64>,
}

/// Descriptor of one block in an engine's catalog (spec §3.2).
///
/// `id` is namespaced by engine key (`wds.pressure-summary`) and **never
/// changes once released** — report templates reference it; removing or
/// repurposing an id is a compatibility break on par with a file-format
/// break.
///
/// Deliberately carries no result-class or prerequisite vocabulary: what a
/// block needs from a simulation is the producing engine's internal
/// concern, surfaced only through [`BlockError::Unavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDescriptor {
    /// Stable namespaced identifier: `<engine>.<name>`.
    pub id: &'static str,
    /// Default human-facing heading.
    pub title: &'static str,
    /// What this block contains, for the template-builder UI.
    pub summary: &'static str,
    /// Engine-authored grouping heading (spec §3.2). Blocks sharing the
    /// exact string belong together; group order is catalog order. Display
    /// text only — carries no semantics beyond equality.
    pub category: &'static str,
}

/// One selectable item of a [`OptionKind::Choice`] or
/// [`OptionKind::MultiChoice`] (spec §3.2.1).
///
/// `value` is what goes into the options object and is opaque here; `label`
/// is display text. Both are engine-authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceItem {
    /// Opaque value stored in the options object.
    pub value: String,
    /// Human-facing label for this item.
    pub label: String,
}

/// Shape and bounds of one describable block option (spec §3.2.1).
///
/// Bounds and defaults are advisory — they tell a UI what to offer, and are
/// never the validation authority. Production validates independently, so an
/// engine may accept a value no descriptor advertised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OptionKind {
    /// Real number, with optional inclusive bounds.
    Number {
        default: Option<f64>,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Whole number, with optional inclusive bounds.
    Integer {
        default: Option<i64>,
        min: Option<i64>,
        max: Option<i64>,
    },
    Boolean {
        default: Option<bool>,
    },
    Text {
        default: Option<String>,
    },
    /// Ordered list of reals — threshold edges and the like.
    NumberList {
        default: Option<Vec<f64>>,
        /// Fewest entries the block will accept, when it requires any.
        min_len: Option<usize>,
        /// Whether entries must strictly ascend.
        ascending: bool,
    },
    /// Exactly one of `items`.
    Choice {
        default: Option<String>,
        items: Vec<ChoiceItem>,
    },
    /// Any subset of `items`.
    MultiChoice {
        default: Option<Vec<String>>,
        items: Vec<ChoiceItem>,
    },
}

/// Description of one option a block accepts (spec §3.2.1).
///
/// Resolved by an engine **against a model**, because permissible values and
/// correct defaults are often properties of the model in hand — which
/// constituents exist, and what unit system the file declares. A consumer
/// displays what it is given and computes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDescriptor {
    /// Field name in the block's options object.
    pub key: String,
    /// Human-facing control label.
    pub label: String,
    /// One or two sentences explaining what the option does.
    pub help: String,
    /// Value shape and bounds.
    pub kind: OptionKind,
    /// Display unit text, or `None`. Display only — never a unit system.
    pub unit: Option<String>,
}

/// Kind of a [`Value`], used in column descriptors (spec §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKind {
    Number,
    Integer,
    Boolean,
    Text,
    Timestamp,
}

/// One typed value inside a fragment (spec §3.3). Unit strings are display
/// text — a structured unit system in this layer is an explicit non-goal
/// (spec §1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Value {
    Number {
        value: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        /// Quantity key from the producing engine's catalog (spec §3.3,
        /// v1.7). When present, `value` is in that quantity's SI display
        /// unit and `unit` is its SI label; a consumer holding the catalog
        /// may re-express both in a chosen display family. Absent, the
        /// value renders as written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quantity: Option<String>,
    },
    Integer {
        value: i64,
    },
    Boolean {
        value: bool,
    },
    Text {
        value: String,
    },
    /// RFC 3339 timestamp text.
    Timestamp {
        value: String,
    },
    /// Explicitly missing (rendered as a gap, not zero).
    Absent,
}

impl Value {
    /// The [`ValueKind`] this value belongs to; `None` for [`Value::Absent`],
    /// which is valid under any column kind.
    pub fn kind(&self) -> Option<ValueKind> {
        match self {
            Value::Number { .. } => Some(ValueKind::Number),
            Value::Integer { .. } => Some(ValueKind::Integer),
            Value::Boolean { .. } => Some(ValueKind::Boolean),
            Value::Text { .. } => Some(ValueKind::Text),
            Value::Timestamp { .. } => Some(ValueKind::Timestamp),
            Value::Absent => None,
        }
    }
}

/// One (label, value) pair in a key-value list (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub label: String,
    pub value: Value,
}

/// Column descriptor of a table (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    /// Unit display text applying to every value in this column.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub kind: ValueKind,
    /// Quantity key applying to every number in this column (spec §3.3,
    /// v1.7): values are in the quantity's SI display unit and `unit` is
    /// its SI label, re-expressible by a consumer holding the catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
}

/// Column descriptors plus row-major values (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

/// One named series of (x, y) points in x order (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSeries {
    pub name: String,
    pub points: Vec<[f64; 2]>,
}

/// Chart data (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChartData {
    /// Parallel category labels and values (distributions, rankings).
    /// Single-series in this revision.
    Bar {
        categories: Vec<String>,
        values: Vec<f64>,
    },
    /// One or more named series over a continuous x axis (time series).
    Line { series: Vec<LineSeries> },
}

/// A declarative chart (spec §3.3): data plus axis labels only — engines
/// describe *what* is charted, never colors, geometry, or layout. Every
/// chart is table-derivable so it never gates information behind a
/// graphics-capable format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub x_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_unit: Option<String>,
    /// Quantity key for the x coordinates (spec §3.3, v1.7): tagged
    /// coordinates are in the quantity's SI display unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_quantity: Option<String>,
    pub y_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_unit: Option<String>,
    /// Quantity key for the y values (spec §3.3, v1.7), as `x_quantity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_quantity: Option<String>,
    pub data: ChartData,
}

/// One item of a fragment (spec §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FragmentItem {
    KeyValues {
        entries: Vec<KeyValue>,
    },
    Table {
        table: Table,
    },
    /// Plain-text paragraph for caveats and methodological remarks.
    Note {
        text: String,
    },
    /// A declarative chart; renderers without graphics support present
    /// its mechanical table derivation instead.
    Chart {
        chart: Chart,
    },
}

/// The materialized content of one block for one completed simulation
/// (spec §3.1): a titled sequence of items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub title: String,
    pub items: Vec<FragmentItem>,
}

/// Failure producing a block (spec §3.4). The report layer decides how an
/// unavailable or failed block renders (placeholder, omission) — the
/// engine never does, and the contract carries no engine vocabulary for
/// *why* beyond the engine-authored reason text.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockError {
    /// The id is not in this engine's catalog.
    UnknownBlock { id: String },
    /// The block does not apply to this run — an expected condition, not a
    /// fault. `reason` is engine-authored human-readable text.
    Unavailable { reason: String },
    /// Reading or deriving from the simulation artifacts failed.
    Failed { message: String },
}

impl std::fmt::Display for BlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockError::UnknownBlock { id } => write!(f, "unknown report block: {id:?}"),
            BlockError::Unavailable { reason } => {
                write!(f, "report block unavailable for this run: {reason}")
            }
            BlockError::Failed { message } => write!(f, "report block failed: {message}"),
        }
    }
}

impl std::error::Error for BlockError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape is read by a TypeScript frontend and by files written
    /// by earlier builds, so both directions are pinned here. The other half
    /// of this invariant is asserted in `hooks/issues.test.ts`; neither test
    /// alone would catch the two sides drifting apart.
    #[test]
    fn run_diagnostic_omits_what_it_does_not_have() {
        let d = RunDiagnostic {
            code: "unbalanced-hydraulics".into(),
            message: "Hydraulic equations were not fully balanced at 1:00:00".into(),
            element_id: None,
            time: Some(3600.0),
        };
        let json = serde_json::to_string(&d).expect("serialise");
        assert!(!json.contains("elementId"), "absent means absent: {json}");
        assert!(json.contains(r#""time":3600.0"#), "{json}");
    }

    /// A file written before diagnostics carried a time still reads, which is
    /// what lets an existing project keep the warnings it already had.
    #[test]
    fn a_run_diagnostic_without_a_time_still_reads() {
        let d: RunDiagnostic = serde_json::from_str(
            r#"{"code":"negative-pressure","message":"Negative pressure at J1","elementId":"J1"}"#,
        )
        .expect("deserialise");
        assert_eq!(Some("J1"), d.element_id.as_deref());
        assert_eq!(None, d.time);
    }

    #[test]
    fn value_serde_wire_shape_is_stable() {
        // The JSON shape is a compatibility surface (templates, IPC):
        // internally tagged with camelCase type names.
        let v = Value::Number {
            value: 1.5,
            unit: Some("m".into()),
            quantity: None,
        };
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"type":"number","value":1.5,"unit":"m"}"#
        );
        assert_eq!(
            serde_json::to_string(&Value::Absent).unwrap(),
            r#"{"type":"absent"}"#
        );
    }

    /// The v1.7 tag is additive on the wire: absent means absent from the
    /// JSON (the pre-v1.7 shape, byte for byte), and pre-v1.7 JSON still
    /// deserializes. Both directions matter — saved templates and IPC
    /// peers were written against the old shape.
    #[test]
    fn quantity_tags_are_additive_on_the_wire() {
        let tagged = Value::Number {
            value: 51.25,
            unit: Some("m".into()),
            quantity: Some("pressure".into()),
        };
        assert_eq!(
            serde_json::to_string(&tagged).unwrap(),
            r#"{"type":"number","value":51.25,"unit":"m","quantity":"pressure"}"#
        );
        let old: Value =
            serde_json::from_str(r#"{"type":"number","value":1.5,"unit":"m"}"#).unwrap();
        assert_eq!(
            old,
            Value::Number {
                value: 1.5,
                unit: Some("m".into()),
                quantity: None,
            }
        );
    }

    #[test]
    fn fragment_round_trips_through_json() {
        let fragment = Fragment {
            title: "Run Summary".into(),
            items: vec![
                FragmentItem::KeyValues {
                    entries: vec![KeyValue {
                        label: "Junctions".into(),
                        value: Value::Integer { value: 42 },
                    }],
                },
                FragmentItem::Table {
                    table: Table {
                        columns: vec![Column {
                            name: "Quantity".into(),
                            unit: None,
                            kind: ValueKind::Text,
                            quantity: None,
                        }],
                        rows: vec![vec![Value::Text {
                            value: "Pressure".into(),
                        }]],
                    },
                },
                FragmentItem::Note {
                    text: "Sampled.".into(),
                },
            ],
        };
        let json = serde_json::to_string(&fragment).unwrap();
        let back: Fragment = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fragment);
    }

    #[test]
    fn chart_serde_wire_shape_is_stable() {
        let chart = Chart {
            x_label: "Minimum pressure".into(),
            x_unit: Some("m".into()),
            x_quantity: None,
            y_label: "Junctions".into(),
            y_unit: None,
            y_quantity: None,
            data: ChartData::Bar {
                categories: vec!["0 – 14".into()],
                values: vec![3.0],
            },
        };
        assert_eq!(
            serde_json::to_string(&FragmentItem::Chart {
                chart: chart.clone()
            })
            .unwrap(),
            r#"{"type":"chart","chart":{"xLabel":"Minimum pressure","xUnit":"m","yLabel":"Junctions","data":{"type":"bar","categories":["0 – 14"],"values":[3.0]}}}"#
        );
        let json = serde_json::to_string(&chart).unwrap();
        assert_eq!(serde_json::from_str::<Chart>(&json).unwrap(), chart);
    }

    #[test]
    fn value_kind_mapping() {
        assert_eq!(Value::Integer { value: 1 }.kind(), Some(ValueKind::Integer));
        assert_eq!(Value::Absent.kind(), None);
    }

    #[test]
    fn block_error_messages_are_descriptive() {
        let e = BlockError::Unavailable {
            reason: "the run has no water-quality results".into(),
        };
        assert!(e.to_string().contains("no water-quality results"));
        let e = BlockError::UnknownBlock {
            id: "wds.nope".into(),
        };
        assert!(e.to_string().contains("wds.nope"));
    }
}
