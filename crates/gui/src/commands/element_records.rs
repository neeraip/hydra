//! The records an element carries, for whichever engine holds the model.
//!
//! The §4.5.2.3 shape: rows that belong to an element and have no
//! identity of their own — a water-distribution junction's demand
//! categories, a drainage vertex's dry-weather inflows.
//!
//! Neither of the two things this contract already had could hold them.
//! An attribute is one value under one label, so several rows have to be
//! flattened into one — which is why a junction's published demand is the
//! *sum* of its categories and its published pattern is the *first*
//! one's, and why writing that total refuses outright for a junction with
//! more than one. An element is identified by an id, and these rows have
//! no name.
//!
//! **A set's columns are described the way attributes are.** Same field
//! names, same value shapes, same referenced kinds — so a surface that can
//! draw an attribute row draws a record table without learning anything,
//! and the two descriptions cannot come to disagree.
//!
//! **A write replaces a whole set.** Adding a record is writing the set
//! with a row more. That keeps one validation pass — two dry-weather
//! inflows for the same constituent are a contradiction only the whole
//! set can be judged for — and makes the inverse the set that was there.

use hydra::common::{OptionKind, QuantityDescriptor};
use serde::Serialize;

use super::network_dto::{NetworkState, M3S_TO_LPS};
use super::projects::{app_data_dir, project_engine_key, validate_target_ids};

/// One column of a record set, described exactly as a §4.4 attribute is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordColumnDto {
    pub key: String,
    pub label: String,
    /// The value's shape and bounds (§3.2.1 vocabulary).
    pub kind: OptionKind,
    /// The §5 quantity for numeric cells; absent for text and for numbers
    /// that carry no unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
    /// The kinds whose elements this column may name (§4.5.1.1).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

/// One set of records attached to an element.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSetDto {
    /// Stable machine identifier — what a write is addressed by.
    pub key: String,
    /// What the engine calls this set.
    pub label: String,
    pub columns: Vec<RecordColumnDto>,
    /// The records, in the engine's own order, one value per column.
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Whether a write of this set may be offered. Advisory; the write is
    /// the authority. A set served read-only is not a failure — showing
    /// what is attached is worth doing whether or not it can be rewritten.
    pub editable: bool,
}

fn column(key: &str, label: &str, kind: OptionKind) -> RecordColumnDto {
    RecordColumnDto {
        key: key.to_string(),
        label: label.to_string(),
        kind,
        quantity: None,
        references: Vec::new(),
    }
}

fn number() -> OptionKind {
    OptionKind::Number {
        default: None,
        min: None,
        max: None,
    }
}

fn text() -> OptionKind {
    OptionKind::Text { default: None }
}

#[tauri::command(async)]
/// Every record set attached to one element (§4.5.2.3).
///
/// Empty for an element that carries none, and for an engine this build
/// cannot open — never an error, because a panel asking about an element
/// with nothing attached wants to draw nothing rather than a failure.
pub fn get_element_records(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    element_id: String,
) -> Result<Vec<RecordSetDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let net = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            Ok(uds_records(&net, &element_id))
        }
        "wds" => {
            let guard = state.0.lock();
            Ok(guard
                .wds_network()
                .map_or_else(Vec::new, |net| wds_records(net, &element_id)))
        }
        _ => Ok(Vec::new()),
    }
}

#[tauri::command(async)]
/// Replace one record set on one element.
///
/// The whole set every time: adding a record is writing it with a row
/// more, which keeps one validation pass and makes the inverse the set
/// that was there.
pub fn set_element_records(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    element_id: String,
    set: String,
    rows: Vec<Vec<serde_json::Value>>,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => super::mutations::mutate_uds(&app, &state, |network| {
            set_uds_records(network, &element_id, &set, &rows)
        }),
        "wds" => super::mutations::mutate_wds(&app, &state, |network| {
            set_wds_records(network, &element_id, &set, &rows)
        }),
        other => Err(format!("no editing surface for engine '{other}'")),
    }
}

/// A cell that has to be a finite number.
fn cell_number(row: &[serde_json::Value], at: usize, what: &str) -> Result<f64, String> {
    row.get(at)
        .and_then(serde_json::Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{what} has to be a number"))
}

/// A cell that is text, empty read as absent.
fn cell_text(row: &[serde_json::Value], at: usize) -> Option<String> {
    let s = row.get(at).and_then(serde_json::Value::as_str)?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn wrong_width(got: usize, want: usize) -> String {
    format!("every record takes {want} values, got {got}")
}

// ── Water distribution ───────────────────────────────────────────────────────

/// The demand categories of a junction (§4.5.2.3).
///
/// The record set that made this section necessary. A junction may carry
/// several, each with its own base demand and pattern, and the attribute
/// schema can only publish their sum and the first one's pattern — so
/// before this, a junction with two categories read as one and could not
/// be edited at all.
pub(crate) fn wds_records(network: &hydra::Network, element_id: &str) -> Vec<RecordSetDto> {
    let Some(node) = network.nodes.iter().find(|n| n.base.id == element_id) else {
        return Vec::new();
    };
    let hydra::NodeKind::Junction(j) = &node.kind else {
        return Vec::new();
    };
    vec![RecordSetDto {
        key: "demands".to_string(),
        label: "Demand categories".to_string(),
        columns: vec![
            RecordColumnDto {
                quantity: super::results::wds_quantity("demand"),
                ..column("baseDemand", "Base demand", number())
            },
            RecordColumnDto {
                references: vec!["pattern".to_string()],
                ..column("pattern", "Pattern", text())
            },
            column("name", "Category", text()),
        ],
        rows: j
            .demands
            .iter()
            .map(|d| {
                vec![
                    serde_json::json!(d.base_demand * M3S_TO_LPS),
                    serde_json::json!(d.pattern.clone().unwrap_or_default()),
                    serde_json::json!(d.name.clone().unwrap_or_default()),
                ]
            })
            .collect(),
        editable: true,
    }]
}

pub(crate) fn set_wds_records(
    network: &mut hydra::Network,
    element_id: &str,
    set: &str,
    rows: &[Vec<serde_json::Value>],
) -> Result<(), String> {
    if set != "demands" {
        return Err(format!("no record set '{set}'"));
    }
    let demands = rows
        .iter()
        .map(|row| {
            if row.len() != 3 {
                return Err(wrong_width(row.len(), 3));
            }
            Ok(hydra::DemandCategory {
                // Litres per second on the way out, so litres per second
                // on the way back — the column declared the quantity and
                // this is the inverse of what the read applied.
                base_demand: cell_number(row, 0, "a base demand")? / M3S_TO_LPS,
                pattern: cell_text(row, 1),
                name: cell_text(row, 2),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let node = network
        .nodes
        .iter_mut()
        .find(|n| n.base.id == element_id)
        .ok_or_else(|| format!("no element '{element_id}'"))?;
    let hydra::NodeKind::Junction(j) = &mut node.kind else {
        return Err(format!("'{element_id}' carries no demand categories"));
    };
    j.demands = demands;
    Ok(())
}

// ── Urban drainage ───────────────────────────────────────────────────────────

/// The four pattern slots a sanitary inflow is modulated by, in the
/// order the file writes them. Positional, and named so a reader does not
/// have to count columns to know which is which.
const DWF_SLOTS: [(&str, &str); 4] = [
    ("pattern1", "Monthly"),
    ("pattern2", "Daily"),
    ("pattern3", "Hourly"),
    ("pattern4", "Weekend"),
];

/// The three surfaces of a snow pack (§4.5.2.3).
///
/// The parameter set that fits this shape best, and the reason it needed
/// one: a pack is three identical records — plowable, impervious,
/// pervious — each seven melt parameters, and any of the three may be
/// absent. The catalog could only ever publish how many were defined,
/// which is a count of something nobody could then read.
///
/// The surface a row is about is its first column and is not editable:
/// there are exactly three, they are not interchangeable, and a set that
/// let one be renamed would let a pack have two pervious surfaces.
fn snowpack_records(net: &hydra::uds::model::Network, id: &str) -> Option<RecordSetDto> {
    let pack = net
        .snowpacks
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))?;
    let mut columns = vec![column("surface", "Surface", text())];
    for (key, label, quantity) in [
        ("dhMin", "Minimum melt", None),
        ("dhMax", "Maximum melt", None),
        ("tBase", "Base temperature", Some("temperature")),
        ("fwFrac", "Free-water capacity", None),
        ("initDepth", "Initial depth", Some("depth")),
        ("initFreeWater", "Initial free water", Some("depth")),
        ("fullCoverDepth", "Depth at full cover", Some("depth")),
    ] {
        columns.push(RecordColumnDto {
            quantity: quantity.and_then(super::uds_results::quantity_descriptor),
            ..column(key, label, number())
        });
    }
    let row = |name: &str, s: &hydra::uds::model::SnowSurface| {
        vec![
            serde_json::json!(name),
            serde_json::json!(s.dh_min),
            serde_json::json!(s.dh_max),
            serde_json::json!(s.t_base),
            serde_json::json!(s.fw_frac),
            serde_json::json!(s.init_depth),
            serde_json::json!(s.init_free_water),
            // The plowable surface is always fully covered, so it has no
            // such depth — null rather than a zero, which would read as
            // a surface that is bare at any depth.
            match s.full_cover_depth {
                Some(d) => serde_json::json!(d),
                None => serde_json::Value::Null,
            },
        ]
    };
    Some(RecordSetDto {
        key: "surfaces".to_string(),
        label: "Snow surfaces".to_string(),
        columns,
        rows: [
            ("Plowable", pack.plowable.as_ref()),
            ("Impervious", pack.impervious.as_ref()),
            ("Pervious", pack.pervious.as_ref()),
        ]
        .into_iter()
        .filter_map(|(name, s)| s.map(|s| row(name, s)))
        .collect(),
        // Read-only for now: writing one means deciding what an absent
        // surface becomes when a row is added, and the three are not
        // interchangeable. Served so a pack can be read at all (§4.5.2.3).
        editable: false,
    })
}

/// The dry-weather inflows attached to a vertex (§4.5.2.3).
///
/// One row per constituent — the flow inflow, and one per pollutant — so
/// a vertex with a flow and a TSS inflow shows two. Flattening these onto
/// the vertex could only ever have shown the first.
pub(crate) fn uds_records(net: &hydra::uds::model::Network, element_id: &str) -> Vec<RecordSetDto> {
    let Some(vertex) = net
        .vertices
        .iter()
        .position(|v| v.id.eq_ignore_ascii_case(element_id))
    else {
        return snowpack_records(net, element_id).into_iter().collect();
    };
    let mut columns = vec![
        column("constituent", "Constituent", text()),
        RecordColumnDto {
            quantity: super::uds_results::quantity_descriptor("flow"),
            ..column("average", "Average", number())
        },
    ];
    for (key, label) in DWF_SLOTS {
        columns.push(RecordColumnDto {
            references: vec!["pattern".to_string()],
            ..column(key, label, text())
        });
    }
    let pattern_id = |slot: Option<usize>| {
        slot.and_then(|i| net.patterns.get(i))
            .map_or_else(String::new, |p| p.id.clone())
    };
    vec![RecordSetDto {
        key: "dryWeather".to_string(),
        label: "Dry weather inflow".to_string(),
        columns,
        rows: net
            .dry_weather
            .iter()
            .filter(|d| d.vertex == vertex)
            .map(|d| {
                let mut row = vec![
                    // The flow inflow's constituent is absent in the
                    // model, and "" would read as an unnamed pollutant.
                    serde_json::json!(d
                        .constituent
                        .and_then(|i| net.constituents.get(i))
                        .map_or_else(|| "Flow".to_string(), |p| p.id.clone())),
                    serde_json::json!(d.average),
                ];
                row.extend(d.patterns.iter().map(|s| serde_json::json!(pattern_id(*s))));
                row
            })
            .collect(),
        // Read-only for now: an average's unit and a constituent's
        // identity both depend on which pollutant the row is about, and
        // the write that took them would have to resolve a name to an
        // index the model keys by. Served so it can be read (§4.5.2.3).
        editable: false,
    }]
}

pub(crate) fn set_uds_records(
    _net: &mut hydra::uds::model::Network,
    _element_id: &str,
    set: &str,
    _rows: &[Vec<serde_json::Value>],
) -> Result<(), String> {
    Err(format!("'{set}' cannot be edited here yet"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    fn wds_model() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("fixture")
    }

    fn demands(network: &hydra::Network, id: &str) -> RecordSetDto {
        wds_records(network, id)
            .into_iter()
            .find(|s| s.key == "demands")
            .expect("a demands set")
    }

    /// The defect this section exists for. A junction with two demand
    /// categories published their *sum* as one attribute and the *first*
    /// one's pattern as another, so the second category was invisible —
    /// and the write refused outright rather than distribute a total
    /// nobody could distribute.
    #[test]
    fn a_junction_reports_every_demand_category_not_their_sum() {
        let mut network = wds_model();
        let node = network
            .nodes
            .iter_mut()
            .find(|n| n.base.id == "J1")
            .expect("J1");
        let hydra::NodeKind::Junction(j) = &mut node.kind else {
            panic!("J1 is a junction");
        };
        j.demands = vec![
            hydra::DemandCategory {
                base_demand: 0.010,
                pattern: Some("P1".into()),
                name: Some("Residential".into()),
            },
            hydra::DemandCategory {
                base_demand: 0.005,
                pattern: None,
                name: None,
            },
        ];

        let set = demands(&network, "J1");
        assert_eq!(set.rows.len(), 2, "both categories have to be reported");
        // Litres per second, which is what the column's quantity declares.
        assert_eq!(set.rows[0][0].as_f64(), Some(10.0));
        assert_eq!(set.rows[0][1].as_str(), Some("P1"));
        assert_eq!(set.rows[0][2].as_str(), Some("Residential"));
        // A category with no pattern reads as empty, not as a name.
        assert_eq!(set.rows[1][1].as_str(), Some(""));
    }

    /// The columns are described the way attributes are, which is what
    /// lets one renderer draw both. A pattern column that did not declare
    /// what it references would be a box to type a name into.
    #[test]
    fn a_pattern_column_says_which_kind_it_names() {
        let network = wds_model();
        let set = demands(&network, "J1");
        let pattern = set
            .columns
            .iter()
            .find(|c| c.key == "pattern")
            .expect("a pattern column");
        assert_eq!(pattern.references, ["pattern"]);
        let base = set
            .columns
            .iter()
            .find(|c| c.key == "baseDemand")
            .expect("a base demand column");
        assert!(base.quantity.is_some(), "a demand carries a unit");
    }

    /// A write replaces the set, so adding and removing a record is
    /// writing one row more or fewer — and the units invert.
    #[test]
    fn writing_a_set_replaces_it_whole_and_converts_back() {
        let mut network = wds_model();
        set_wds_records(
            &mut network,
            "J1",
            "demands",
            &[
                vec![
                    serde_json::json!(10.0),
                    serde_json::json!("P1"),
                    serde_json::json!("Residential"),
                ],
                vec![
                    serde_json::json!(2.5),
                    serde_json::json!(""),
                    serde_json::json!(""),
                ],
            ],
        )
        .expect("write");

        let node = network.nodes.iter().find(|n| n.base.id == "J1").unwrap();
        let hydra::NodeKind::Junction(j) = &node.kind else {
            panic!("J1 is a junction");
        };
        assert_eq!(j.demands.len(), 2);
        // 10 L/s is 0.010 m³/s in the model, which is the conversion a
        // round trip would have cancelled.
        assert!((j.demands[0].base_demand - 0.010).abs() < 1e-12);
        assert_eq!(j.demands[0].pattern.as_deref(), Some("P1"));
        // An emptied reference is absent, not a category named "".
        assert_eq!(j.demands[1].pattern, None);
        assert_eq!(j.demands[1].name, None);

        // Removing every record leaves a junction with no demand, which
        // is a thing a model may say.
        set_wds_records(&mut network, "J1", "demands", &[]).expect("empty");
        let node = network.nodes.iter().find(|n| n.base.id == "J1").unwrap();
        let hydra::NodeKind::Junction(j) = &node.kind else {
            panic!()
        };
        assert!(j.demands.is_empty());
    }

    #[test]
    fn a_row_of_the_wrong_width_is_refused_and_changes_nothing() {
        let mut network = wds_model();
        let before = demands(&network, "J1").rows;
        let err = set_wds_records(
            &mut network,
            "J1",
            "demands",
            &[vec![serde_json::json!(1.0)]],
        )
        .expect_err("short row");
        assert!(err.contains("3 values"), "{err}");
        assert_eq!(demands(&network, "J1").rows, before);

        assert!(set_wds_records(&mut network, "J1", "nope", &[]).is_err());
        assert!(set_wds_records(&mut network, "NOPE", "demands", &[]).is_err());
    }

    /// The parameter set the record shape was needed for. A pack is
    /// three identical surfaces, any of which may be absent, and the
    /// catalog could only ever publish how many were defined — a count
    /// of something nobody could then read.
    #[test]
    fn a_snow_pack_reports_a_row_per_surface_it_has() {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\n\
                     [JUNCTIONS]\nJ1 10 3 0 0 0\n\
                     [SNOWPACKS]\n\
                     SP1 PLOWABLE 0.001 0.002 0.0 0.1 0.0 0.0 0.0\n\
                     SP1 IMPERVIOUS 0.001 0.002 0.0 0.1 0.0 0.0 0.5\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let sets = uds_records(&net, "SP1");
        let set = sets.first().expect("a surfaces set");
        assert_eq!(set.key, "surfaces");
        // Two of the three, because the file defined two — not three
        // rows with one full of zeros.
        assert_eq!(set.rows.len(), 2);
        assert_eq!(set.rows[0][0].as_str(), Some("Plowable"));
        assert_eq!(set.rows[1][0].as_str(), Some("Impervious"));
        // The plowable surface is always fully covered, so it carries no
        // such depth — null rather than a zero, which would read as a
        // surface bare at any depth.
        assert!(set.rows[0].last().expect("a cell").is_null());
        assert!(set.rows[1].last().expect("a cell").is_f64());

        assert!(uds_records(&net, "NOPE").is_empty());
    }

    /// An element that carries no records of a kind reports none rather
    /// than an empty set with columns, so a panel draws nothing.
    #[test]
    fn an_element_with_nothing_attached_reports_nothing() {
        let network = wds_model();
        assert!(wds_records(&network, "R1").is_empty(), "a reservoir");
        assert!(wds_records(&network, "P1").is_empty(), "a pipe");
        assert!(wds_records(&network, "NOPE").is_empty());
    }
}
