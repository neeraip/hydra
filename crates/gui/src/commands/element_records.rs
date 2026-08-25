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
    /// How many rows this set may hold (§4.5.2.3), where the engine
    /// knows a limit. Absent is the ordinary case — a junction may have
    /// any number of demand categories.
    ///
    /// It says how many rows may exist, never which ones: a pack that is
    /// not yet full still refuses a second surface named like the first.
    /// Without it a full set could only be offered a row and then refuse
    /// it, which is a button that never works.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<usize>,
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

/// A yes/no cell, in the words the rest of the drainage rows use.
fn yes_no() -> OptionKind {
    OptionKind::Choice {
        default: None,
        items: ["Yes", "No"]
            .iter()
            .map(|v| hydra::common::ChoiceItem {
                value: (*v).to_string(),
                label: (*v).to_string(),
            })
            .collect(),
    }
}

#[tauri::command(async)]
/// Every record set attached to one element (§4.5.2.3).
///
/// Empty for an element that carries none, and for an engine this build
/// cannot open — never an error, because a panel asking about an element
/// with nothing attached wants to draw nothing rather than a failure.
///
/// `kind` says which family the id belongs to, where the caller knows.
/// Water distribution keeps nodes and links in separate namespaces, so
/// an id alone is half an address there.
pub fn get_element_records(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    element_id: String,
    kind: Option<String>,
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
            Ok(guard.wds_network().map_or_else(Vec::new, |net| {
                wds_records(net, kind.as_deref(), &element_id)
            }))
        }
        other => Err(super::projects::unknown_engine(other)),
    }
}

#[tauri::command(async)]
/// Replace one record set on one element.
///
/// The whole set every time: adding a record is writing it with a row
/// more, which keeps one validation pass and makes the inverse the set
/// that was there.
///
/// `kind` addresses the element together with its id, as it does for
/// [`get_element_records`]. A write to an ambiguous id without one is
/// refused rather than applied to whichever element the lookup reaches
/// first.
pub fn set_element_records(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    element_id: String,
    set: String,
    rows: Vec<Vec<serde_json::Value>>,
    kind: Option<String>,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => super::mutations::mutate_uds(&app, &state, &project_id, |network| {
            set_uds_records(network, &element_id, &set, &rows)
        }),
        "wds" => super::mutations::mutate_wds(&app, &state, &project_id, |network| {
            set_wds_records(network, kind.as_deref(), &element_id, &set, &rows)
        }),
        other => Err(super::projects::unknown_engine(other)),
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

/// Whether the id, addressed as `kind`, is a node this engine keeps
/// records for.
///
/// Every record set here belongs to a node, so a caller naming a link
/// kind is asking about something that carries none — and a caller
/// naming nothing gets the same answer as the attribute path gives: the
/// element, when the id names one thing, and neither when it names two.
fn carries_records(network: &hydra::Network, kind: Option<&str>, element_id: &str) -> bool {
    match kind {
        Some("junction" | "reservoir" | "tank") => true,
        // A named kind that is not a node — a pipe, a curve — carries no
        // records, whatever a node of the same id happens to hold.
        Some(_) => false,
        None => !network.links.iter().any(|l| l.base.id == element_id),
    }
}

/// The demand categories of a junction (§4.5.2.3).
///
/// The record set that made this section necessary. A junction may carry
/// several, each with its own base demand and pattern, and the attribute
/// schema can only publish their sum and the first one's pattern — so
/// before this, a junction with two categories read as one and could not
/// be edited at all.
/// `kind` says which family the id belongs to, for the reason the
/// attribute read takes one: this engine keeps nodes and links in
/// separate namespaces, and only its nodes carry records. Looking up
/// nodes alone meant a pipe `10` beside a junction `10` was served the
/// junction's demand categories — and this set is editable, so a
/// category added under the pipe landed on the junction.
pub(crate) fn wds_records(
    network: &hydra::Network,
    kind: Option<&str>,
    element_id: &str,
) -> Vec<RecordSetDto> {
    if !carries_records(network, kind, element_id) {
        return Vec::new();
    }
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
        // A junction may have as many categories as a modeller cares to
        // separate.
        capacity: None,
    }]
}

pub(crate) fn set_wds_records(
    network: &mut hydra::Network,
    kind: Option<&str>,
    element_id: &str,
    set: &str,
    rows: &[Vec<serde_json::Value>],
) -> Result<(), String> {
    if set != "demands" {
        return Err(format!("no record set '{set}'"));
    }
    // Before anything is parsed, because the answer decides which element
    // is being written to and getting that wrong is worse than refusing.
    if !carries_records(network, kind, element_id) {
        return Err(format!("'{element_id}' carries no demand categories"));
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

// ── A control measure's layers ───────────────────────────────────────────────
//
// Six layers, three to seven parameters each, and no two of them the
// same shape — which is why one table cannot hold them honestly and why
// this kind read as "defined by its layers" and could be neither opened
// nor created.
//
// Six *sets* can. §4.5.2.3 already lets an element carry several, each
// with its own columns, and a layer is a set holding at most one row:
// present is a row, absent is none. Adding the row is the layer
// appearing, and removing it is the layer going away — which is exactly
// what the file means by omitting the section.
//
// All six are offered whatever the unit type is. Hiding the ones a type
// does not use would make a layer that is absent indistinguishable from
// one that does not apply, and the engine refuses neither — a rain
// barrel with a soil layer is a model the reader may hold, and this is
// an editor rather than a second opinion about it.

/// One layer: its set key, what to call it, and its columns.
///
/// The columns are named once and read by both directions. A read and a
/// write that each listed them would be two lists to keep in step, and
/// the one that drifted would put a porosity into a wilting point.
struct Layer {
    key: &'static str,
    label: &'static str,
    /// `(key, label, quantity)` per column, in file order.
    columns: &'static [(&'static str, &'static str, Option<&'static str>)],
}

const LAYERS: [Layer; 6] = [
    Layer {
        key: "surface",
        label: "Surface layer",
        columns: &[
            ("thickness", "Berm height", Some("depth")),
            ("voidFrac", "Vegetation volume", None),
            ("roughness", "Roughness", None),
            ("slope", "Surface slope", None),
            ("sideSlope", "Side slope", None),
        ],
    },
    Layer {
        key: "soil",
        label: "Soil layer",
        columns: &[
            ("thickness", "Thickness", Some("depth")),
            ("porosity", "Porosity", None),
            ("fieldCapacity", "Field capacity", None),
            ("wiltingPoint", "Wilting point", None),
            ("kSat", "Conductivity", None),
            ("kSlope", "Conductivity slope", None),
            ("suction", "Suction head", Some("depth")),
        ],
    },
    Layer {
        key: "pavement",
        label: "Pavement layer",
        columns: &[
            ("thickness", "Thickness", Some("depth")),
            ("voidFrac", "Void fraction", None),
            ("impervFrac", "Impervious fraction", None),
            ("kSat", "Permeability", None),
            ("clogFactor", "Clogging factor", None),
            ("regenDays", "Regeneration interval", None),
            ("regenDegree", "Regeneration degree", None),
        ],
    },
    Layer {
        key: "storage",
        label: "Storage layer",
        columns: &[
            ("thickness", "Thickness", Some("depth")),
            ("voidFrac", "Void fraction", None),
            ("kSat", "Exfiltration rate", None),
            ("clogFactor", "Clogging factor", None),
            ("covered", "Covered", None),
        ],
    },
    Layer {
        key: "drain",
        label: "Underdrain",
        columns: &[
            ("coeff", "Coefficient", None),
            ("exponent", "Exponent", None),
            ("offset", "Offset", Some("depth")),
            ("delay", "Delay", None),
            ("hOpen", "Open head", Some("depth")),
            ("hClose", "Close head", Some("depth")),
            ("curve", "Control curve", None),
        ],
    },
    Layer {
        key: "drainMat",
        label: "Drainage mat",
        columns: &[
            ("thickness", "Thickness", Some("depth")),
            ("voidFrac", "Void fraction", None),
            ("roughness", "Roughness", None),
        ],
    },
];

/// The layer sets of a control measure, one per layer whatever it holds.
fn lid_records(net: &hydra::uds::model::Network, id: &str) -> Vec<RecordSetDto> {
    let Some(lid) = net
        .lid_controls
        .iter()
        .find(|c| c.id.eq_ignore_ascii_case(id))
    else {
        return Vec::new();
    };
    LAYERS
        .iter()
        .map(|layer| {
            let mut columns: Vec<RecordColumnDto> = layer
                .columns
                .iter()
                .map(|(key, label, quantity)| match *key {
                    // The one cell that is not a number: a rain barrel
                    // is covered or it is not.
                    "covered" => column(key, label, yes_no()),
                    // And the one that names another element, so the
                    // field offers the model's own curves (§4.5.1.1).
                    "curve" => RecordColumnDto {
                        references: vec!["curve".to_string()],
                        ..column(key, label, text())
                    },
                    _ => RecordColumnDto {
                        quantity: quantity.and_then(super::uds_results::quantity_descriptor),
                        ..column(key, label, number())
                    },
                })
                .collect();
            columns.shrink_to_fit();
            RecordSetDto {
                key: layer.key.to_string(),
                label: layer.label.to_string(),
                columns,
                rows: lid_layer_row(net, lid, layer.key).into_iter().collect(),
                editable: true,
                // A measure has one of each layer or none of it. The
                // row is the layer.
                capacity: Some(1),
            }
        })
        .collect()
}

/// The one row a layer has, or none when the control measure lacks it.
fn lid_layer_row(
    net: &hydra::uds::model::Network,
    lid: &hydra::uds::model::LidControl,
    key: &str,
) -> Option<Vec<serde_json::Value>> {
    use serde_json::json;
    let curve_id = |i: Option<usize>| {
        i.and_then(|i| net.curves.get(i))
            .map_or_else(String::new, |c| c.id.clone())
    };
    match key {
        "surface" => lid.surface.as_ref().map(|l| {
            vec![
                json!(l.thickness),
                json!(l.void_frac),
                json!(l.roughness),
                json!(l.slope),
                json!(l.side_slope),
            ]
        }),
        "soil" => lid.soil.as_ref().map(|l| {
            vec![
                json!(l.thickness),
                json!(l.porosity),
                json!(l.field_capacity),
                json!(l.wilting_point),
                json!(l.k_sat),
                json!(l.k_slope),
                json!(l.suction),
            ]
        }),
        "pavement" => lid.pavement.as_ref().map(|l| {
            vec![
                json!(l.thickness),
                json!(l.void_frac),
                json!(l.imperv_frac),
                json!(l.k_sat),
                json!(l.clog_factor),
                json!(l.regen_days),
                json!(l.regen_degree),
            ]
        }),
        "storage" => lid.storage.as_ref().map(|l| {
            vec![
                json!(l.thickness),
                json!(l.void_frac),
                json!(l.k_sat),
                json!(l.clog_factor),
                json!(if l.covered { "Yes" } else { "No" }),
            ]
        }),
        "drain" => lid.drain.as_ref().map(|l| {
            vec![
                json!(l.coeff),
                json!(l.exponent),
                json!(l.offset),
                json!(l.delay),
                json!(l.h_open),
                json!(l.h_close),
                json!(curve_id(l.curve)),
            ]
        }),
        "drainMat" => lid
            .drain_mat
            .as_ref()
            .map(|l| vec![json!(l.thickness), json!(l.void_frac), json!(l.roughness)]),
        _ => None,
    }
}

/// Replace one layer: a row makes it present, no rows takes it away.
fn set_lid_layer(
    net: &mut hydra::uds::model::Network,
    element_id: &str,
    key: &str,
    rows: &[Vec<serde_json::Value>],
) -> Result<(), String> {
    use hydra::uds::model as m;
    let layer = LAYERS
        .iter()
        .find(|l| l.key == key)
        .ok_or_else(|| format!("no record set '{key}'"))?;
    if rows.len() > 1 {
        return Err(format!(
            "a control measure has one {} or none",
            layer.label.to_lowercase()
        ));
    }
    // Resolved before the layer is touched, so a curve nobody has does
    // not leave the measure half-written.
    let curve = match rows.first().and_then(|r| cell_text(r, 6)) {
        Some(name) => Some(
            net.curves
                .iter()
                .position(|c| c.id.eq_ignore_ascii_case(&name))
                .ok_or_else(|| format!("'{name}' is not a curve in this model"))?,
        ),
        None => None,
    };
    let at = net
        .lid_controls
        .iter()
        .position(|c| c.id.eq_ignore_ascii_case(element_id))
        .ok_or_else(|| format!("no control measure '{element_id}'"))?;
    let row = rows.first();
    if let Some(r) = row {
        if r.len() != layer.columns.len() {
            return Err(wrong_width(r.len(), layer.columns.len()));
        }
    }
    let n = |at: usize, what: &str| -> Result<f64, String> {
        cell_number(row.expect("checked"), at, what)
    };
    let lid = &mut net.lid_controls[at];
    match key {
        "surface" => {
            lid.surface = match row {
                None => None,
                Some(_) => Some(m::LidSurface {
                    thickness: n(0, "a berm height")?,
                    void_frac: n(1, "a vegetation volume")?,
                    roughness: n(2, "a roughness")?,
                    slope: n(3, "a surface slope")?,
                    side_slope: n(4, "a side slope")?,
                }),
            }
        }
        "soil" => {
            lid.soil = match row {
                None => None,
                Some(_) => Some(m::LidSoil {
                    thickness: n(0, "a thickness")?,
                    porosity: n(1, "a porosity")?,
                    field_capacity: n(2, "a field capacity")?,
                    wilting_point: n(3, "a wilting point")?,
                    k_sat: n(4, "a conductivity")?,
                    k_slope: n(5, "a conductivity slope")?,
                    suction: n(6, "a suction head")?,
                }),
            }
        }
        "pavement" => {
            lid.pavement = match row {
                None => None,
                Some(_) => Some(m::LidPavement {
                    thickness: n(0, "a thickness")?,
                    void_frac: n(1, "a void fraction")?,
                    imperv_frac: n(2, "an impervious fraction")?,
                    k_sat: n(3, "a permeability")?,
                    clog_factor: n(4, "a clogging factor")?,
                    regen_days: n(5, "a regeneration interval")?,
                    regen_degree: n(6, "a regeneration degree")?,
                }),
            }
        }
        "storage" => {
            lid.storage = match row {
                None => None,
                Some(r) => Some(m::LidStorage {
                    thickness: n(0, "a thickness")?,
                    void_frac: n(1, "a void fraction")?,
                    k_sat: n(2, "an exfiltration rate")?,
                    clog_factor: n(3, "a clogging factor")?,
                    covered: cell_text(r, 4).is_some_and(|v| v.eq_ignore_ascii_case("yes")),
                }),
            }
        }
        "drain" => {
            lid.drain = match row {
                None => None,
                Some(_) => Some(m::LidDrain {
                    coeff: n(0, "a coefficient")?,
                    exponent: n(1, "an exponent")?,
                    offset: n(2, "an offset")?,
                    delay: n(3, "a delay")?,
                    h_open: n(4, "an open head")?,
                    h_close: n(5, "a close head")?,
                    curve,
                }),
            }
        }
        "drainMat" => {
            lid.drain_mat = match row {
                None => None,
                Some(_) => Some(m::LidDrainMat {
                    thickness: n(0, "a thickness")?,
                    void_frac: n(1, "a void fraction")?,
                    roughness: n(2, "a roughness")?,
                }),
            }
        }
        _ => return Err(format!("no record set '{key}'")),
    }
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

/// The three duration classes a month's response may be given for, in
/// the order the file writes them.
const UH_TERMS: [&str; 3] = ["Short", "Medium", "Long"];

/// The months, by the names a reader uses rather than by an index.
const UH_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The responses of a unit hydrograph group (§4.5.2.3).
///
/// Twelve months by three duration classes, any of the thirty-six
/// present or absent — which the catalog could only ever publish as how
/// many were defined. A row per response the file actually carries, so a
/// group given one January response shows one row rather than
/// thirty-six, most of them zeros that would read as real answers.
///
/// Read-only: a write means deciding what an absent month becomes, and
/// unlike a snow pack's three surfaces there are thirty-six slots and no
/// natural order for a reader to add them in. Served so a group can be
/// read at all, which is what §4.5.2.3 allows.
fn hydrograph_records(net: &hydra::uds::model::Network, id: &str) -> Option<RecordSetDto> {
    let group = net
        .unit_hydrographs
        .iter()
        .find(|g| g.id.eq_ignore_ascii_case(id))?;
    let mut columns = vec![
        column("month", "Month", text()),
        column("term", "Duration", text()),
    ];
    for (key, label, quantity) in [
        ("r", "Rainfall fraction", None),
        ("tPeak", "Time to peak", Some("time")),
        ("k", "Recession ratio", None),
        ("iaMax", "Initial abstraction", Some("depth")),
        ("iaInit", "Initial abstraction used", Some("depth")),
        ("iaRecovery", "Abstraction recovery", Some("depth")),
    ] {
        columns.push(RecordColumnDto {
            quantity: quantity.and_then(super::uds_results::quantity_descriptor),
            ..column(key, label, number())
        });
    }
    let mut rows = Vec::new();
    for (m, month) in UH_MONTHS.iter().enumerate() {
        for (t, term) in UH_TERMS.iter().enumerate() {
            let Some(r) = group.months[m][t].as_ref() else {
                continue;
            };
            rows.push(vec![
                serde_json::json!(month),
                serde_json::json!(term),
                serde_json::json!(r.r),
                serde_json::json!(r.t_peak),
                serde_json::json!(r.k),
                serde_json::json!(r.ia_max),
                serde_json::json!(r.ia_init),
                serde_json::json!(r.ia_recovery),
            ]);
        }
    }
    Some(RecordSetDto {
        key: "responses".to_string(),
        label: "Monthly responses".to_string(),
        columns,
        rows,
        editable: false,
        // Twelve months, three terms each.
        capacity: Some(36),
    })
}

/// The three surfaces a pack may carry, in the order the file writes
/// them, each with the accessor that reaches it.
///
/// One table rather than three matching lists: the read walks it, the
/// write walks it, and a surface added to one and not the other would be
/// a row that reads and cannot be written back.
type SnowSurfaceOf = fn(&hydra::uds::model::Snowpack) -> &Option<hydra::uds::model::SnowSurface>;
const SNOW_SURFACES: [(&str, SnowSurfaceOf); 3] = [
    ("Plowable", |p| &p.plowable),
    ("Impervious", |p| &p.impervious),
    ("Pervious", |p| &p.pervious),
];

/// The three surfaces of a snow pack (§4.5.2.3).
///
/// The parameter set that fits this shape best, and the reason it needed
/// one: a pack is three identical records — plowable, impervious,
/// pervious — each seven melt parameters, and any of the three may be
/// absent. The catalog could only ever publish how many were defined,
/// which is a count of something nobody could then read.
///
/// The surface a row is about is its first column, and which of the
/// three it names is what the row is keyed by — so the set is capped at
/// three and a second row naming one already there is refused.
fn snowpack_records(net: &hydra::uds::model::Network, id: &str) -> Option<RecordSetDto> {
    let pack = net
        .snowpacks
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(id))?;
    let mut columns = vec![column(
        "surface",
        "Surface",
        // Which of the three, chosen rather than typed: there are
        // exactly three and they are not interchangeable, so a free
        // field would let a pack have two pervious surfaces.
        OptionKind::Choice {
            default: None,
            items: SNOW_SURFACES
                .iter()
                .map(|(v, _)| hydra::common::ChoiceItem {
                    value: (*v).to_string(),
                    label: (*v).to_string(),
                })
                .collect(),
        },
    )];
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
        rows: SNOW_SURFACES
            .iter()
            .filter_map(|(name, which)| which(pack).as_ref().map(|s| row(name, s)))
            .collect(),
        editable: true,
        // Plowable, impervious, pervious — and no more, because the
        // three are what a pack is made of rather than a list it keeps.
        capacity: Some(SNOW_SURFACES.len()),
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
        let layers = lid_records(net, element_id);
        if !layers.is_empty() {
            return layers;
        }
        return snowpack_records(net, element_id)
            .or_else(|| hydrograph_records(net, element_id))
            .into_iter()
            .collect();
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
        // One per constituent, and a model may carry any number of
        // those.
        capacity: None,
    }]
}

pub(crate) fn set_uds_records(
    net: &mut hydra::uds::model::Network,
    element_id: &str,
    set: &str,
    rows: &[Vec<serde_json::Value>],
) -> Result<(), String> {
    if LAYERS.iter().any(|l| l.key == set) {
        return set_lid_layer(net, element_id, set, rows);
    }
    if set != "surfaces" {
        return Err(format!("'{set}' cannot be edited here yet"));
    }
    let pack = net
        .snowpacks
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(element_id))
        .ok_or_else(|| format!("no snow pack '{element_id}'"))?;

    // Parsed whole before anything is assigned, so a refusal on the
    // third row leaves the first two where they were.
    let mut parsed: Vec<(usize, hydra::uds::model::SnowSurface)> = Vec::new();
    for row in rows {
        if row.len() != 8 {
            return Err(wrong_width(row.len(), 8));
        }
        let name = row[0].as_str().unwrap_or("").trim();
        let at = SNOW_SURFACES
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("'{name}' is not one of this pack's surfaces"))?;
        if parsed.iter().any(|(other, _)| *other == at) {
            return Err(format!("a pack has one {name} surface, not two"));
        }
        // The plowable surface is always fully covered, so it carries no
        // depth at which cover becomes complete — a value there would
        // describe a surface that goes bare, which that one cannot.
        let full_cover_depth = match &row[7] {
            serde_json::Value::Null => None,
            v => {
                if at == 0 {
                    return Err("a plowable surface is always fully covered".into());
                }
                Some(
                    v.as_f64()
                        .filter(|d| d.is_finite())
                        .ok_or_else(|| "a depth at full cover has to be a number".to_string())?,
                )
            }
        };
        parsed.push((
            at,
            hydra::uds::model::SnowSurface {
                dh_min: cell_number(row, 1, "a minimum melt coefficient")?,
                dh_max: cell_number(row, 2, "a maximum melt coefficient")?,
                t_base: cell_number(row, 3, "a base temperature")?,
                fw_frac: cell_number(row, 4, "a free-water capacity")?,
                init_depth: cell_number(row, 5, "an initial depth")?,
                init_free_water: cell_number(row, 6, "an initial free water")?,
                full_cover_depth,
            },
        ));
    }

    // A surface the write did not mention is one the pack no longer has,
    // which is how a record is removed: the set is replaced whole.
    let pack = &mut net.snowpacks[pack];
    pack.plowable = None;
    pack.impervious = None;
    pack.pervious = None;
    for (at, surface) in parsed {
        match at {
            0 => pack.plowable = Some(surface),
            1 => pack.impervious = Some(surface),
            _ => pack.pervious = Some(surface),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    /// Whatever record set the read serves as editable, the write takes
    /// back — and whatever it serves read-only, the write refuses — for
    /// every element of every kind in the catalog.
    ///
    /// The same invariant the contents sweep holds, for §4.5.2.3: a set
    /// served editable with no write arm behind it is an add button that
    /// can only refuse, and a set served read-only that the write took
    /// would let an application reach a state the read said was closed.
    /// The fixture carries one element of every kind and the absence
    /// list is asserted empty, so no kind is skipped in silence.
    #[test]
    fn every_record_set_agrees_with_its_write_for_every_kind() {
        let (net, diags) =
            hydra::swmm::objects::parse_network(crate::commands::test_fixtures::UDS_FULL_INP);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");

        let mut editable = Vec::new();
        let mut read_only = Vec::new();
        let mut absent = Vec::new();
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let ids = crate::commands::uds_attrs::kind_elements(&net, kind.id).ids;
            let Some(id) = ids.first() else {
                absent.push(kind.id);
                continue;
            };
            for set in uds_records(&net, id) {
                let mut draft = net.clone();
                let write = set_uds_records(&mut draft, id, &set.key, &set.rows);
                let name = format!("{}.{}", kind.id, set.key);
                if set.editable {
                    write.unwrap_or_else(|e| panic!("{name} is served editable but: {e}"));
                    editable.push(name);
                } else {
                    assert!(
                        write.is_err(),
                        "{name} is served read-only, but the write took it"
                    );
                    read_only.push(name);
                }
            }
        }
        assert!(
            absent.is_empty(),
            "the fixture has no element of: {absent:?} — those kinds' record \
             sets are unverified"
        );
        // The sweep has to have seen the sets that exist, or it proves
        // nothing — a fixture change that dropped them would pass.
        for want in ["lidcontrol.surface", "snowpack.surfaces"] {
            assert!(
                editable.iter().any(|e| e == want),
                "the sweep never exercised editable {want}; saw {editable:?}"
            );
        }
        for want in ["hydrograph.responses", "junction.dryWeather"] {
            assert!(
                read_only.iter().any(|e| e == want),
                "the sweep never exercised read-only {want}; saw {read_only:?}"
            );
        }
    }

    /// A junction and a pipe that share an id, which EPANET allows.
    const COLLIDING_INP: &str = "\
[JUNCTIONS]
10  10  5
[RESERVOIRS]
R1  100
[PIPES]
10  R1  10  1000  12  100  0  Open
[COORDINATES]
10  1.0  2.0
R1  0.0  0.0
[OPTIONS]
Units  GPM
[TIMES]
Duration  0
[END]
";

    /// Records belong to an element, and an id names one only with its
    /// kind.
    ///
    /// Every set this engine has hangs off a node, and the lookup walked
    /// the nodes and stopped — so a pipe `10` beside a junction `10` was
    /// served the junction's demand categories. That set is editable, so
    /// a category added while looking at the pipe was written onto the
    /// junction and reported as saved.
    #[test]
    fn records_belong_to_the_kind_that_carries_them() {
        let mut net = hydra::io::parse(COLLIDING_INP.as_bytes()).expect("a legal model");

        assert_eq!(wds_records(&net, Some("junction"), "10").len(), 1);
        // A pipe carries none, whatever a junction of the same id holds.
        assert!(wds_records(&net, Some("pipe"), "10").is_empty());
        // Nothing said, and the id names two things: neither.
        assert!(wds_records(&net, None, "10").is_empty());
        // An id that names one thing still answers without a kind.
        assert!(
            wds_records(&net, None, "R1").is_empty(),
            "a reservoir has none"
        );

        let row = vec![vec![
            serde_json::json!(5.0),
            serde_json::json!(""),
            serde_json::json!("Residential"),
        ]];
        // The write refuses on the pipe rather than landing on the
        // junction, and refuses when nothing said which.
        assert!(set_wds_records(&mut net, Some("pipe"), "10", "demands", &row).is_err());
        assert!(set_wds_records(&mut net, None, "10", "demands", &row).is_err());
        let junction = net.nodes.iter().find(|n| n.base.id == "10").expect("10");
        let hydra::NodeKind::Junction(j) = &junction.kind else {
            panic!("10 is a junction");
        };
        assert_eq!(j.demands.len(), 1, "a refusal wrote nothing");

        set_wds_records(&mut net, Some("junction"), "10", "demands", &row).expect("the junction");
        let junction = net.nodes.iter().find(|n| n.base.id == "10").expect("10");
        let hydra::NodeKind::Junction(j) = &junction.kind else {
            panic!("10 is a junction");
        };
        assert_eq!(j.demands.len(), 1);
        assert_eq!(j.demands[0].name.as_deref(), Some("Residential"));
    }

    /// A model with one control measure carrying two of its six layers.
    fn lid_model() -> hydra::uds::model::Network {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\n\
                     [JUNCTIONS]\nJ1 10 3 0 0 0\n\
                     [CURVES]\nDC1 CONTROL 0 1\nDC1 1 2\n\
                     [LID_CONTROLS]\n\
                     GR1 BC\n\
                     GR1 SURFACE 150 0.0 0.1 1.0 5\n\
                     GR1 SOIL 600 0.5 0.2 0.1 10.0 30 3.5\n";
        hydra::swmm::objects::parse_network(model).0
    }

    /// Six sets, one per layer, whatever the measure holds.
    ///
    /// Not one table: no two layers have the same shape — a surface has
    /// five parameters and a soil seven, and they are different
    /// parameters — so a single table of the union would be mostly empty
    /// cells meaning different things per row. Six sets is what
    /// §4.5.2.3 already offers, and a layer is one that holds at most
    /// one row.
    #[test]
    fn a_control_measure_serves_a_set_for_every_layer() {
        let net = lid_model();
        let sets = uds_records(&net, "GR1");
        assert_eq!(
            sets.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            vec!["surface", "soil", "pavement", "storage", "drain", "drainMat"]
        );

        // Present is a row; absent is none. All six are offered whatever
        // the type is, because a layer that is absent and one that does
        // not apply are different things and hiding the second would
        // make them look the same.
        let rows = |key: &str| sets.iter().find(|s| s.key == key).expect(key).rows.len();
        assert_eq!(rows("surface"), 1);
        assert_eq!(rows("soil"), 1);
        assert_eq!(rows("pavement"), 0);
        assert_eq!(rows("drain"), 0);

        // Every one can be written, which is what makes an absent layer
        // addable: the headings and the add button are how the first row
        // is entered.
        assert!(sets.iter().all(|s| s.editable));
    }

    /// Adding a row is the layer appearing; removing it is the layer
    /// going away — which is what the file means by omitting a section.
    #[test]
    fn writing_a_layer_adds_it_and_emptying_it_takes_it_away() {
        let mut net = lid_model();
        assert!(net.lid_controls[0].drain.is_none());

        let row = vec![vec![
            serde_json::json!(0.5),
            serde_json::json!(0.5),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!("DC1"),
        ]];
        set_uds_records(&mut net, "GR1", "drain", &row).expect("the drain goes in");
        let drain = net.lid_controls[0].drain.as_ref().expect("a drain");
        assert!((drain.coeff - 0.5).abs() < 1e-9);
        // The curve column names another element, and the write resolves
        // it to the index the model keys by.
        assert_eq!(
            drain.curve.and_then(|i| net.curves.get(i)).map(|c| &c.id),
            Some(&"DC1".to_string())
        );

        set_uds_records(&mut net, "GR1", "drain", &[]).expect("and comes out again");
        assert!(net.lid_controls[0].drain.is_none());
    }

    /// A layer is one or none, and a curve nobody has is refused before
    /// anything is touched.
    #[test]
    fn a_layer_refuses_what_it_cannot_hold() {
        let mut net = lid_model();
        let one = vec![
            serde_json::json!(0.1),
            serde_json::json!(0.2),
            serde_json::json!(0.3),
        ];
        assert!(
            set_uds_records(&mut net, "GR1", "drainMat", &[one.clone(), one]).is_err(),
            "a measure has one drainage mat or none"
        );

        // The curve is resolved before the layer is assigned, so a name
        // the model does not have leaves the measure exactly as it was.
        let bad = vec![vec![
            serde_json::json!(0.5),
            serde_json::json!(0.5),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!("NOPE"),
        ]];
        assert!(set_uds_records(&mut net, "GR1", "drain", &bad).is_err());
        assert!(
            net.lid_controls[0].drain.is_none(),
            "a refusal wrote nothing"
        );

        // And the width has to match the columns it was served with.
        assert!(
            set_uds_records(&mut net, "GR1", "surface", &[vec![serde_json::json!(1.0)]]).is_err()
        );
    }

    fn pack_model() -> hydra::uds::model::Network {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\n\
                     [JUNCTIONS]\nJ1 10 3 0 0 0\n\
                     [SNOWPACKS]\n\
                     SP1 PLOWABLE 0.001 0.002 0.0 0.1 0.0 0.0 0.0\n\
                     SP1 IMPERVIOUS 0.001 0.002 0.0 0.1 0.0 0.0 0.5\n";
        hydra::swmm::objects::parse_network(model).0
    }

    fn wds_model() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("fixture")
    }

    fn demands(network: &hydra::Network, id: &str) -> RecordSetDto {
        wds_records(network, None, id)
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
            None,
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
        set_wds_records(&mut network, None, "J1", "demands", &[]).expect("empty");
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
            None,
            "J1",
            "demands",
            &[vec![serde_json::json!(1.0)]],
        )
        .expect_err("short row");
        assert!(err.contains("3 values"), "{err}");
        assert_eq!(demands(&network, "J1").rows, before);

        assert!(set_wds_records(&mut network, None, "J1", "nope", &[]).is_err());
        assert!(set_wds_records(&mut network, None, "NOPE", "demands", &[]).is_err());
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
        let (net, _diags) = hydra::swmm::objects::parse_network(model);

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

    /// The first sub-record write, and the shape §4.5.2.3 promised: a
    /// set is replaced whole, so adding a surface is writing a row more
    /// and removing one is writing a row fewer.
    #[test]
    fn a_snow_pack_takes_a_surface_added_and_loses_one_left_out() {
        let mut net = pack_model();
        let rows = uds_records(&net, "SP1")[0].rows.clone();
        assert_eq!(rows.len(), 2);

        // A third surface, added by writing the set with a row more.
        let mut with_pervious = rows.clone();
        with_pervious.push(vec![
            serde_json::json!("Pervious"),
            serde_json::json!(0.002),
            serde_json::json!(0.004),
            serde_json::json!(0.0),
            serde_json::json!(0.1),
            serde_json::json!(0.0),
            serde_json::json!(0.0),
            serde_json::json!(0.25),
        ]);
        set_uds_records(&mut net, "SP1", "surfaces", &with_pervious).expect("add");
        assert_eq!(uds_records(&net, "SP1")[0].rows.len(), 3);
        assert!(net.snowpacks[0].pervious.is_some());

        // And one left out is one the pack no longer has.
        set_uds_records(&mut net, "SP1", "surfaces", &with_pervious[..1]).expect("drop");
        assert!(net.snowpacks[0].impervious.is_none());
        assert!(net.snowpacks[0].plowable.is_some());
    }

    /// The rules only the whole set can be judged for, which is why the
    /// write takes all of it.
    #[test]
    fn a_pack_cannot_have_two_of_one_surface_or_a_bare_plowable_one() {
        let mut net = pack_model();
        let rows = uds_records(&net, "SP1")[0].rows.clone();
        let before = net.snowpacks[0].clone();

        let mut twice = rows.clone();
        twice.push(rows[0].clone());
        let err = set_uds_records(&mut net, "SP1", "surfaces", &twice).expect_err("two plowable");
        assert!(err.contains("not two"), "{err}");

        // The plowable surface is always fully covered, so a depth at
        // which cover becomes complete describes something it cannot do.
        let mut bare = rows.clone();
        bare[0][7] = serde_json::json!(0.3);
        let err = set_uds_records(&mut net, "SP1", "surfaces", &bare).expect_err("a bare plow");
        assert!(err.contains("fully covered"), "{err}");

        assert!(
            set_uds_records(&mut net, "SP1", "surfaces", &[vec![serde_json::json!("X")]]).is_err()
        );
        // Nothing moved through any of it: the set is parsed whole
        // before a single surface is assigned.
        assert_eq!(net.snowpacks[0], before);
    }

    /// The bound a set publishes is the one its write enforces
    /// (§4.5.2.3).
    ///
    /// Both halves matter and neither implies the other. A set that
    /// under-reports its capacity hides rows a modeller could have
    /// added; a set that over-reports it offers a row the write then
    /// refuses, which is the button-that-never-works this field was
    /// added to remove. The two are checked against each other rather
    /// than against a written-down number, so a layer added later is
    /// covered without anyone remembering to add it here.
    #[test]
    fn a_set_that_is_full_is_a_set_the_write_takes_no_more_of() {
        let cases: Vec<(hydra::uds::model::Network, &str)> =
            vec![(lid_model(), "GR1"), (pack_model(), "SP1")];
        let mut bounded = 0;
        for (net, id) in cases {
            for set in uds_records(&net, id) {
                let Some(capacity) = set.capacity else {
                    continue;
                };
                if !set.editable {
                    continue;
                }
                bounded += 1;
                // A row of whatever each column holds, repeated until the
                // set is one past its bound. What the cells say does not
                // matter: the count alone has to be refused, or the
                // capacity is a number the write does not believe.
                let filler: Vec<serde_json::Value> = set
                    .columns
                    .iter()
                    .map(|c| match c.kind {
                        OptionKind::Text { .. } | OptionKind::Choice { .. } => {
                            serde_json::json!("")
                        }
                        _ => serde_json::json!(0.0),
                    })
                    .collect();
                let too_many = vec![filler; capacity + 1];
                let mut probe = net.clone();
                assert!(
                    set_uds_records(&mut probe, id, &set.key, &too_many).is_err(),
                    "'{}' publishes room for {capacity} and took {}",
                    set.key,
                    capacity + 1
                );
                // And the model as it stands is within what it says.
                assert!(set.rows.len() <= capacity, "'{}' is over full", set.key);
            }
        }
        assert!(bounded >= 7, "the bounded sets went missing: {bounded}");
    }

    /// Thirty-six slots, any of them present or absent, which the
    /// catalog could only publish as how many were defined. A row per
    /// response the file carries — not thirty-six with most of them
    /// zeros, which would read as real answers.
    #[test]
    fn a_unit_hydrograph_reports_a_row_per_response_it_has() {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\n\
                     [JUNCTIONS]\nJ1 10 3 0 0 0\n\
                     [RAINGAGES]\nG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
                     [TIMESERIES]\nTS1 0:00 0.0\nTS1 1:00 0.0\n\
                     [HYDROGRAPHS]\n\
                     UH1  G1\n\
                     UH1  JUL  MEDIUM  0.05  4.0  2.0  0.1  0  0.5\n";
        let (net, diags) = hydra::swmm::objects::parse_network(model);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}"
        );

        let sets = uds_records(&net, "UH1");
        let set = sets.first().expect("a responses set");
        assert_eq!(set.key, "responses");
        assert_eq!(set.rows.len(), 1, "one response was defined");
        assert_eq!(set.rows[0][0].as_str(), Some("July"));
        assert_eq!(set.rows[0][1].as_str(), Some("Medium"));
        // Named rather than indexed: a reader should not have to know
        // that month 6 is July or that class 1 is the medium-term one.
        assert!(set.columns[0].label == "Month" && set.columns[1].label == "Duration");
        // Read-only, and the reason is in the module: thirty-six slots
        // with no natural order to add them in.
        assert!(!set.editable);
    }

    /// An element that carries no records of a kind reports none rather
    /// than an empty set with columns, so a panel draws nothing.
    #[test]
    fn an_element_with_nothing_attached_reports_nothing() {
        let network = wds_model();
        assert!(wds_records(&network, None, "R1").is_empty(), "a reservoir");
        assert!(wds_records(&network, None, "P1").is_empty(), "a pipe");
        assert!(wds_records(&network, None, "NOPE").is_empty());
    }
}
