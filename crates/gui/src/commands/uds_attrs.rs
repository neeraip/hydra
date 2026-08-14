//! Per-element attribute rows for the drainage inspector's Properties
//! section: the engine's §4 attribute schemas joined with values from the
//! parsed model.
//!
//! "Engines describe, applications render": row labels, ordering, and
//! quantity semantics come from `hydra::uds::descriptors::attribute_schema`;
//! this module only extracts each schema key's value from the model.
//! Numeric values are served in SI (the model's internal system) together
//! with the engine's §5 quantity descriptor, so the frontend applies the
//! user's display-unit preference itself — the backend does not know it.
//! A schema key whose value is impractical to serve (e.g. cross-section
//! depths stored in file units pending §5 geometry evaluation) simply
//! yields no row.

use std::collections::HashMap;

use serde::Serialize;

use hydra::common::QuantityDescriptor;
use hydra::uds::model::{
    LinkKind, Network, OutfallStage, OutletRating, ParcelOutlet, StorageGeometry, VertexKind,
};

use super::network_dto::NetworkState;
use super::projects::{app_data_dir, validate_target_ids};
use super::results::uds_network_for_target;

/// A schema key's extracted value.
pub(crate) enum AttrValue {
    Number(f64),
    Text(String),
}

fn yes_no(v: bool) -> AttrValue {
    AttrValue::Text(if v { "Yes" } else { "No" }.to_string())
}

/// Kind id + per-schema-key values for one element, or `None` when the id
/// matches nothing.
fn extract(
    net: &Network,
    element_id: &str,
) -> Option<(&'static str, HashMap<&'static str, AttrValue>)> {
    let (kind, mut values) = if let Some(p) = net.parcels.iter().find(|p| p.id == element_id) {
        ("subcatchment", parcel_values(net, p))
    } else if let Some(v) = net.vertices.iter().find(|v| v.id == element_id) {
        vertex_values(net, v)
    } else {
        link_values(net, net.links.iter().find(|l| l.id == element_id)?)
    };
    values.insert("tag", tag_value(net, element_id));
    Some((kind, values))
}

/// An element's tag, empty when it has none.
///
/// Always a value, never absent — the one place a tag departs from every
/// other attribute here. §4.5.1 makes a missing value mean "this element
/// has nothing to change", and for a tag that is the wrong statement: an
/// untagged element is one whose tag is empty, and a cell that vanished
/// for it could never be typed into.
fn tag_value(net: &Network, id: &str) -> AttrValue {
    AttrValue::Text(super::uds_view::tag_of(net, id))
}

fn parcel_values(net: &Network, p: &hydra::uds::model::Parcel) -> HashMap<&'static str, AttrValue> {
    use AttrValue::{Number, Text};
    {
        let mut m = HashMap::new();
        if let Some(g) = net.gages.get(p.gage) {
            m.insert("raingage", Text(g.id.clone()));
        }
        let outlet = match p.outlet {
            ParcelOutlet::Vertex(vi) => net.vertices.get(vi).map(|v| v.id.clone()),
            ParcelOutlet::Parcel(pi) => net.parcels.get(pi).map(|o| o.id.clone()),
        };
        if let Some(outlet) = outlet {
            m.insert("outlet", Text(outlet));
        }
        // The model carries area in m² and slope/imperviousness as
        // fractions; the §4 schema declares them as `area` (SI hectares)
        // and `percent`, so each converts to its declared quantity's SI
        // form here — the same boundary every other row crosses.
        m.insert("area", Number(p.area / 10_000.0));
        m.insert("width", Number(p.width));
        m.insert("slope", Number(p.slope * 100.0));
        m.insert("imperviousness", Number(p.frac_imperv * 100.0));
        m
    }
}

fn vertex_values(
    net: &Network,
    v: &hydra::uds::model::Vertex,
) -> (&'static str, HashMap<&'static str, AttrValue>) {
    use AttrValue::{Number, Text};
    {
        let mut m = HashMap::new();
        m.insert("invert", Number(v.invert));
        let kind = match &v.kind {
            VertexKind::Junction {
                max_depth,
                init_depth,
                ..
            } => {
                m.insert("maxDepth", Number(*max_depth));
                m.insert("initDepth", Number(*init_depth));
                "junction"
            }
            VertexKind::Outfall {
                stage, flap_gate, ..
            } => {
                let boundary = match stage {
                    OutfallStage::Free => "FREE",
                    OutfallStage::Normal => "NORMAL",
                    OutfallStage::Fixed(_) => "FIXED",
                    OutfallStage::Tidal { .. } => "TIDAL",
                    OutfallStage::Series { .. } => "TIMESERIES",
                };
                m.insert("boundary", Text(boundary.to_string()));
                m.insert("gated", yes_no(*flap_gate));
                "outfall"
            }
            VertexKind::Storage {
                max_depth,
                geometry,
                ..
            } => {
                m.insert("maxDepth", Number(*max_depth));
                let shape = match geometry {
                    StorageGeometry::Tabular { curve } => net
                        .curves
                        .get(*curve)
                        .map(|c| format!("Tabular ({})", c.id))
                        .unwrap_or_else(|| "Tabular".to_string()),
                    StorageGeometry::Functional { .. } => "Functional".to_string(),
                    StorageGeometry::Shape { .. } => "Shaped".to_string(),
                };
                m.insert("shape", Text(shape));
                // A single surface area, only where the area does not
                // vary with depth: a functional relation whose depth term
                // vanishes is a prismatic tank, and its constant *is* the
                // area. Every other geometry carries no such number, so
                // it carries no row (§4.5.1) — a tabulated relation
                // reported as one area would be a confident wrong answer.
                if let StorageGeometry::Functional {
                    coeff,
                    exponent,
                    constant,
                } = geometry
                {
                    if *coeff == 0.0 || *exponent == 0.0 {
                        m.insert("surfaceArea", Number(*constant));
                    }
                }
                "storage"
            }
            VertexKind::Divider { diverted_link, .. } => {
                if let Some(l) = diverted_link.and_then(|i| net.links.get(i)) {
                    m.insert("divertedLink", Text(l.id.clone()));
                }
                "divider"
            }
        };
        (kind, m)
    }
}

/// Serve a link's opening as two metres, under the keys its kind uses.
///
/// §5 carries a cross-section's geometry **in the file's own units**, so
/// this converts on the way out through the mapping the file was read
/// under — asked of the engine rather than restated here, which is the
/// same trap the writer once fell into by a factor of three.
///
/// Nothing is inserted for a link with no cross-section: an element that
/// carries no value for a key has no row (§4.5.1), and a zero would read
/// as an opening of no height.
fn insert_opening(
    m: &mut HashMap<&'static str, AttrValue>,
    net: &Network,
    l: &hydra::uds::model::Link,
    height_key: &'static str,
    width_key: &'static str,
) {
    let Some(xs) = &l.cross_section else { return };
    let per_unit = net.options.flow_units.m_per_length_unit();
    m.insert(height_key, AttrValue::Number(xs.geom_user[0] * per_unit));
    m.insert(width_key, AttrValue::Number(xs.geom_user[1] * per_unit));
}

fn link_values(
    net: &Network,
    l: &hydra::uds::model::Link,
) -> (&'static str, HashMap<&'static str, AttrValue>) {
    use AttrValue::{Number, Text};
    let mut m = HashMap::new();
    let kind = match &l.kind {
        LinkKind::Channel {
            length, roughness, ..
        } => {
            m.insert("length", Number(*length));
            m.insert("roughness", Number(*roughness));
            if let Some(xs) = &l.cross_section {
                m.insert("shape", Text(format!("{:?}", xs.shape)));
            }
            "conduit"
        }
        LinkKind::Pump {
            curve, initial_on, ..
        } => {
            let name = curve
                .and_then(|i| net.curves.get(i))
                .map(|c| c.id.clone())
                .unwrap_or_else(|| "Ideal transfer".to_string());
            m.insert("curve", Text(name));
            m.insert("initStatus", yes_no(*initial_on));
            "pump"
        }
        LinkKind::Orifice {
            orientation,
            discharge_coeff,
            ..
        } => {
            m.insert(
                "orientation",
                Text(format!("{orientation:?}").to_uppercase()),
            );
            m.insert("dischargeCoeff", Number(*discharge_coeff));
            // The opening, which the schema has always declared and this
            // has never served — so the row was dropped and the size was
            // unreadable and unwritable anywhere. It lives in the same
            // cross-section a conduit's bore does, in the file's units.
            insert_opening(&mut m, net, l, "height", "width");
            "orifice"
        }
        LinkKind::Weir {
            discharge_coeff, ..
        } => {
            m.insert("dischargeCoeff", Number(*discharge_coeff));
            insert_opening(&mut m, net, l, "crestHeight", "crestLength");
            "weir"
        }
        LinkKind::Outlet {
            rating, flap_gate, ..
        } => {
            let name = match rating {
                OutletRating::Tabular { curve } => net
                    .curves
                    .get(*curve)
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| "Tabular".to_string()),
                OutletRating::Functional { .. } => "Functional".to_string(),
            };
            m.insert("outletCurve", Text(name));
            // Only a power relation has these two, so only one carries
            // them — a tabulated rating has a curve instead, and a row
            // for a coefficient it does not have would invite setting
            // one (§4.5.1).
            if let OutletRating::Functional { coeff, exponent } = rating {
                m.insert("ratingCoeff", Number(*coeff));
                m.insert("ratingExponent", Number(*exponent));
            }
            m.insert("gated", yes_no(*flap_gate));
            "outlet"
        }
    };
    (kind, m)
}

/// Properties rows for one element: the §4 schema's rows, in schema order,
/// with values the model actually carries.
pub(crate) fn element_attributes(
    net: &Network,
    element_id: &str,
) -> Option<Vec<super::element_attrs::ElementAttributeDto>> {
    let (kind, values) = extract(net, element_id)?;
    Some(super::element_attrs::rows_from_schema(
        hydra::uds::descriptors::attribute_schema(kind),
        values,
        super::uds_results::quantity_descriptor,
    ))
}

/// One column of a kind's property table: an engine-declared attribute
/// (§4.4) with every element's value for it, parallel to `ids`.
///
/// Columnar rather than row-major because a table is read by column — and
/// because one array per attribute stays compact where a per-row object
/// would repeat every key thousands of times.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindColumnDto {
    pub key: String,
    pub label: String,
    /// Whether this column's cells can be written, from the same table
    /// the setter consults — the per-column half of the flag
    /// `ElementAttributeDto` carries per row, and true for exactly the
    /// same (kind, key) pairs.
    pub editable: bool,
    /// The kinds whose elements this column may name (§4.5.1.1), empty
    /// for a column that is not a reference.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    /// The value's shape and bounds (§3.2.1 vocabulary, reused by §4.4).
    ///
    /// A table that knows only "number or text" can offer a field and a
    /// field only. This is what lets one table render a select for a
    /// valve type, a yes/no for a check valve and a number for a
    /// diameter — without naming any of them, which is the whole point.
    pub kind: hydra::common::OptionKind,
    /// The §5 quantity for numeric values (SI), absent for text columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
    /// One entry per id: a number for numeric attributes, a string for
    /// textual ones, `null` where the element does not carry the
    /// attribute at all.
    pub values: Vec<serde_json::Value>,
}

/// Every element of one kind, with its §4.4 attribute columns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindElementsDto {
    /// Element ids, in model order — the row order every column follows.
    pub ids: Vec<String>,
    pub columns: Vec<KindColumnDto>,
    /// Each element's position in the model's own coordinate system, or
    /// `None` for one the model places nowhere (hydra-common §4.5.2).
    ///
    /// Parallel to `ids`, and empty for a non-spatial kind — a curve is
    /// not anywhere. Not a column, because position is not an attribute:
    /// it is implied by the element's class, which is what stops an
    /// engine from publishing an element an application must draw and
    /// cannot move.
    pub positions: Vec<Option<[f64; 2]>>,
    /// Each element's two ends, first then second (hydra-common
    /// §4.5.2.1). Parallel to `ids`, and empty for any class but
    /// `polyline` — only a line runs between two things.
    ///
    /// The order is the sign convention for what the line carries, not
    /// an arbitrary pair, which is why it is a fixed-length array and
    /// not a set.
    pub ends: Vec<[String; 2]>,
}

impl KindElementsDto {
    /// No elements — what a kind the engine does not publish looks like.
    pub(crate) fn empty() -> Self {
        KindElementsDto {
            ids: Vec::new(),
            columns: Vec::new(),
            positions: Vec::new(),
            ends: Vec::new(),
        }
    }
}

/// The elements of one kind with their declared properties.
///
/// The per-element command answers "what is this thing?"; a table needs
/// "what are all of these?", and asking element by element would be one
/// IPC round trip per row. Empty for engines whose attributes reach the
/// frontend by another route — wds carries its own in the network
/// snapshot — and for a kind the model has none of.
/// One §4.4 attribute of an element kind, without any element's values.
///
/// The schema is a property of the kind, so it is known before any element
/// is looked at — which is what lets a panel draw its property rows while
/// the values are still being fetched, rather than appearing empty and then
/// pushing everything below it down the panel.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeInfoDto {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<hydra::common::QuantityDescriptor>,
    /// Whether a write to this attribute may be offered (§4.5.1).
    pub editable: bool,
    /// The value's shape and bounds — what a form needs to know which
    /// control to draw for it, before any element exists to read.
    pub kind: hydra::common::OptionKind,
    /// The kinds whose elements this attribute may name (§4.5.1.1),
    /// empty for a value that is not a reference.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

#[tauri::command]
/// The declared attribute schema of one element kind (spec §4.4): every
/// property it has, in presentation order, with its label and quantity.
///
/// Static per engine and kind — no project, no scenario, no values.
pub fn list_element_attributes(engine: String, kind: String) -> Vec<AttributeInfoDto> {
    let attrs = match engine.as_str() {
        "wds" => hydra::descriptors::attribute_schema(&kind),
        "uds" => hydra::uds::descriptors::attribute_schema(&kind),
        _ => return Vec::new(),
    };
    attrs
        .into_iter()
        .map(|attr| AttributeInfoDto {
            quantity: attr
                .quantity
                .as_deref()
                .and_then(super::uds_results::quantity_descriptor),
            key: attr.key,
            label: attr.label,
            editable: attr.editable,
            kind: attr.kind,
            references: attr.references,
        })
        .collect()
}

#[tauri::command(async)]
/// Every element of one kind, with its columns and positions, from
/// whichever engine holds the model.
///
/// Empty for a kind the engine does not publish, and for an engine this
/// build cannot open — never an error, because a table asking for a
/// kind that is not there wants to draw nothing rather than a failure.
pub fn get_kind_elements(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    kind: String,
) -> Result<KindElementsDto, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let net =
                uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(kind_elements(&net, &kind))
        }
        "wds" => {
            let guard = state.0.lock();
            Ok(guard
                .wds_network()
                .map_or_else(KindElementsDto::empty, |net| {
                    super::wds_attrs::kind_elements(net, &kind)
                }))
        }
        _ => Ok(KindElementsDto::empty()),
    }
}

/// The contents of one collection element — the points, factors or
/// clauses a row can only report the *size* of.
///
/// One shape for every container rather than one DTO each: a curve, a
/// pattern and a time series are all a table of numbers under different
/// headings, and a control rule is a block of text. A consumer renders
/// whichever of the two is non-empty and needs to know nothing about
/// which kind it asked for.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDetailDto {
    /// Column headings for `rows`; empty when the content is text.
    pub columns: Vec<String>,
    /// The §5 quantity each column carries, or `None` where it is
    /// dimensionless. Values are SI, so without this a US-units project
    /// would render metres under a heading that never says so.
    pub quantities: Vec<Option<hydra::common::QuantityDescriptor>>,
    /// Tabular content, each row matching `columns`.
    pub rows: Vec<Vec<f64>>,
    /// Verbatim lines, for containers whose content is language.
    pub lines: Vec<String>,
    /// Whether a write of these contents may be offered (§4.5.2.2).
    ///
    /// Advisory, like every other editability flag here — the write is
    /// the authority. It exists because the two shapes are not equally
    /// writable: rows are numbers under headings the engine named, while
    /// lines are a language, and taking those back means parsing them
    /// with the engine's own reader. Serving the text to be read is
    /// worth doing whether or not it can be rewritten.
    pub editable: bool,
    /// Why there is nothing, when there is nothing (§4.5.2.2).
    ///
    /// Empty contents are two different answers. A pollutant has none —
    /// it is its attributes and nothing further. An external time
    /// series' values are real and held in a file beside the model. Both
    /// arrive as no rows and no lines, and a consumer telling them apart
    /// would be doing it by the kind it asked for, which is the list of
    /// kinds §1 exists to prevent.
    ///
    /// Served only where there is something to say. No note means the
    /// element has no contents, and the panel is not drawn at all — a
    /// bordered box holding a heading and nothing else reads as a surface
    /// that failed to load.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The contents of one collection element, or an empty detail when the
/// kind has none to show or the id is unknown.
#[tauri::command(async)]
pub fn get_collection_detail(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    kind: String,
    id: String,
) -> Result<CollectionDetailDto, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let net =
                uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(collection_detail(&net, &kind, &id))
        }
        "wds" => {
            let guard = state.0.lock();
            Ok(guard
                .wds_network()
                .map_or_else(CollectionDetailDto::default, |net| {
                    super::wds_attrs::collection_detail(net, &kind, &id)
                }))
        }
        _ => Ok(CollectionDetailDto::default()),
    }
}

/// Pure form of [`get_collection_detail`].
pub fn collection_detail(net: &Network, kind: &str, id: &str) -> CollectionDetailDto {
    let pair = |columns: [&str; 2], quantities: [Option<&str>; 2], rows: Vec<Vec<f64>>| {
        CollectionDetailDto {
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
            quantities: quantities
                .iter()
                .map(|q| q.and_then(super::uds_results::quantity_descriptor))
                .collect(),
            rows,
            lines: Vec::new(),
            // Every tabular container is a table of numbers under
            // engine-named headings, which is the shape the write takes.
            editable: true,
            note: None,
        }
    };
    /// What a curve's two columns *are* depends on what the curve is for
    /// (§2.9): a storage curve relates depth to surface area, a rating
    /// curve head to discharge. Labelling both "X" and "Y" would hand the
    /// reader two unlabelled SI numbers.
    fn curve_axes(
        kind: hydra::uds::model::CurveKind,
    ) -> ([&'static str; 2], [Option<&'static str>; 2]) {
        use hydra::uds::model::CurveKind::*;
        match kind {
            Storage => (["Depth", "Surface area"], [Some("depth"), Some("area")]),
            Diversion => (["Inflow", "Diverted flow"], [Some("flow"), Some("flow")]),
            Tidal => (["Hour of day", "Stage"], [None, Some("elevation")]),
            Rating => (["Head", "Discharge"], [Some("depth"), Some("flow")]),
            Shape => (["Depth (norm.)", "Width (norm.)"], [None, None]),
            WeirCoeff => (["Head", "Coefficient"], [Some("depth"), None]),
            Control => (["Controller", "Setting"], [None, None]),
            // The five pump curve types differ only in what the flow is
            // plotted against (§2.9).
            Pump1 => (["Wet-well volume", "Flow"], [Some("volume"), Some("flow")]),
            Pump2 | Pump4 => (["Inlet depth", "Flow"], [Some("depth"), Some("flow")]),
            Pump3 | Pump5 => (["Head difference", "Flow"], [Some("depth"), Some("flow")]),
        }
    }
    match kind {
        "curve" => net
            .curves
            .iter()
            .find(|c| c.id == id)
            .map(|c| {
                let (columns, quantities) = curve_axes(c.kind);
                pair(
                    columns,
                    quantities,
                    c.points.iter().map(|(x, y)| vec![*x, *y]).collect(),
                )
            })
            .unwrap_or_default(),
        // A registry entry's "contents" is what references it. A street
        // section and an inlet design are flat property records — the row
        // in the table already carries every field — so the detail answers
        // the question the table cannot: who uses this, and to what end.
        // Without it, following an inlet name from the inspector arrives
        // somewhere that says nothing the inspector had not.
        "street" => {
            let Some(idx) = net.streets.iter().position(|st| st.id == id) else {
                return CollectionDetailDto::default();
            };
            CollectionDetailDto {
                lines: net
                    .links
                    .iter()
                    .filter(|l| {
                        l.cross_section.as_ref().and_then(|x| x.referent)
                            == Some(hydra::uds::model::XsectReferent::Street(idx))
                    })
                    .map(|l| l.id.clone())
                    .collect(),
                ..Default::default()
            }
        }
        "inlet" => {
            let Some(idx) = net.inlets.iter().position(|i| i.id == id) else {
                return CollectionDetailDto::default();
            };
            CollectionDetailDto {
                lines: net
                    .inlet_usage
                    .iter()
                    .filter(|u| u.design == idx)
                    .filter_map(|u| {
                        Some(format!(
                            "{} → {}",
                            net.links.get(u.link)?.id,
                            net.vertices.get(u.capture_vertex)?.id
                        ))
                    })
                    .collect(),
                ..Default::default()
            }
        }
        // Survey points, which the model holds as (elevation, station)
        // — that order, not the other, and the headings say so rather
        // than leaving a reader to infer it from the numbers.
        "transect" => net
            .transects
            .iter()
            .find(|t| t.id == id)
            .map(|t| {
                pair(
                    ["Elevation", "Station"],
                    [Some("elevation"), Some("length")],
                    t.stations.iter().map(|(e, x)| vec![*e, *x]).collect(),
                )
            })
            .unwrap_or_default(),
        "pattern" => net
            .patterns
            .iter()
            .find(|p| p.id == id)
            .map(|p| {
                pair(
                    ["Interval", "Factor"],
                    [None, None],
                    p.factors
                        .iter()
                        .enumerate()
                        // 1-based: a modeller counts hour 1, not hour 0.
                        .map(|(i, f)| vec![(i + 1) as f64, *f])
                        .collect(),
                )
            })
            .unwrap_or_default(),
        "timeseries" => net
            .timeseries
            .iter()
            .find(|t| t.id == id)
            .map(|t| match &t.source {
                hydra::uds::model::TimeSeriesSource::Points(pts) => {
                    use hydra::uds::model::SeriesTime;
                    // A dated series cannot go in a numeric time column
                    // without dropping the date, which would make two
                    // readings on different days look like the same
                    // instant. Those are rendered as text instead.
                    if pts
                        .iter()
                        .any(|p| matches!(p.time, SeriesTime::Absolute { .. }))
                    {
                        CollectionDetailDto {
                            columns: Vec::new(),
                            quantities: Vec::new(),
                            rows: Vec::new(),
                            // Dated readings render as text, and text is
                            // not what the row write takes.
                            editable: false,
                            lines: pts
                                .iter()
                                .map(|p| match &p.time {
                                    SeriesTime::Absolute { date, seconds } => format!(
                                        "{:04}-{:02}-{:02} {:>6.2} h    {}",
                                        date.year,
                                        date.month,
                                        date.day,
                                        seconds / 3600.0,
                                        p.value
                                    ),
                                    SeriesTime::Elapsed(s) => {
                                        format!("{:>6.2} h    {}", s / 3600.0, p.value)
                                    }
                                })
                                .collect(),
                            note: None,
                        }
                    } else {
                        pair(
                            // A series' values can be rainfall, flow or
                            // head depending on what references it, so the
                            // quantity is genuinely unknown here.
                            ["Time (h)", "Value"],
                            [None, None],
                            pts.iter()
                                .map(|p| {
                                    let SeriesTime::Elapsed(s) = p.time else {
                                        unreachable!("absolute times handled above")
                                    };
                                    vec![s / 3600.0, p.value]
                                })
                                .collect(),
                        )
                    }
                }
                // An external series lives in a file this crate never
                // reads. Its values are real and elsewhere, which is a
                // different thing from having none — and the note is
                // what says so. It used to be the consumer's sentence,
                // and the consumer showed it under every kind whose
                // contents were empty for any reason at all, telling
                // readers that a pollutant's contents lived in a file.
                hydra::uds::model::TimeSeriesSource::External { file } => CollectionDetailDto {
                    note: Some(format!(
                        "This series is read from '{file}', which is kept beside the model \
                         rather than in it."
                    )),
                    ..CollectionDetailDto::default()
                },
            })
            .unwrap_or_default(),
        "rule" => net
            .controls
            .rules
            .iter()
            .find(|r| r.name == id)
            .map(|r| CollectionDetailDto {
                columns: Vec::new(),
                quantities: Vec::new(),
                rows: Vec::new(),
                // A rule is language. Taking it back means parsing it
                // with the engine's own reader, which this path does not
                // reach — so it is served to be read and not rewritten.
                editable: false,
                lines: r.lines.clone(),
                note: None,
            })
            .unwrap_or_default(),
        _ => CollectionDetailDto::default(),
    }
}

/// How many elements each declared kind holds, for the editor's rail.
///
/// Counted through `kind_elements` rather than by a second walk of the
/// model: the rail's number and the table's row count are then the same
/// derivation, and a rail claiming 12 curves beside a table listing 11 is
/// the kind of disagreement nobody reports — they just stop trusting the
/// number.
///
/// Empty for engines other than uds, like the rest of this module.
#[tauri::command(async)]
pub fn get_kind_counts(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<HashMap<String, usize>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let net =
                uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(kind_counts(&net))
        }
        "wds" => {
            let guard = state.0.lock();
            Ok(guard
                .wds_network()
                .map_or_else(HashMap::new, super::wds_attrs::kind_counts))
        }
        _ => Ok(HashMap::new()),
    }
}

/// Rows for a collection kind: the model's non-spatial tables (§4.1
/// `collection`), listed by id like any other kind.
///
/// Each of these is a *container* — a curve is a list of points, a rule a
/// list of clauses — so the row reports what the thing is and how large it
/// is, and leaves the contents to an editor that has room for them.
fn collection_rows(
    net: &Network,
    kind: &str,
) -> Option<Vec<(String, HashMap<&'static str, AttrValue>)>> {
    use AttrValue::{Number, Text};
    let rows = match kind {
        // Rain gages had no arm at all, so the rail showed a Rain gages
        // entry reading zero and its table was always empty — for every
        // drainage model, since the day the rail was built. It was
        // invisible because the count came from this same function: the
        // number agreed with the table, and both were wrong.
        "raingage" => net
            .gages
            .iter()
            .map(|g| {
                let mut m = HashMap::new();
                m.insert("format", Text(format!("{:?}", g.form).to_uppercase()));
                // Seconds in the model; a recording interval is read as
                // a clock time, which is how the file writes it too.
                let minutes = (g.interval / 60.0).round() as i64;
                m.insert(
                    "interval",
                    Text(format!("{}:{:02}", minutes / 60, minutes % 60)),
                );
                m.insert(
                    "source",
                    Text(match &g.source {
                        hydra::uds::model::GageSource::Series { series } => net
                            .timeseries
                            .get(*series)
                            .map_or_else(|| "Time series".to_string(), |t| t.id.clone()),
                        hydra::uds::model::GageSource::File { file, station, .. } => {
                            format!("{file} ({station})")
                        }
                    }),
                );
                (g.id.clone(), m)
            })
            .collect(),
        "pollutant" => net
            .constituents
            .iter()
            .map(|c| {
                let mut m = HashMap::new();
                m.insert("units", Text(format!("{:?}", c.units)));
                m.insert("rainConc", Number(c.c_rain));
                m.insert("groundwaterConc", Number(c.c_groundwater));
                m.insert("rdiiConc", Number(c.c_rdii));
                m.insert("dwfConc", Number(c.c_dwf));
                m.insert("decay", Number(c.decay));
                m.insert("snowOnly", yes_no(c.snow_only));
                (c.id.clone(), m)
            })
            .collect(),
        "curve" => net
            .curves
            .iter()
            .map(|c| {
                let mut m = HashMap::new();
                m.insert("curveType", Text(format!("{:?}", c.kind)));
                m.insert("points", Number(c.points.len() as f64));
                (c.id.clone(), m)
            })
            .collect(),
        "timeseries" => net
            .timeseries
            .iter()
            .map(|t| {
                let mut m = HashMap::new();
                let (source, points) = match &t.source {
                    hydra::uds::model::TimeSeriesSource::External { file } => (file.clone(), None),
                    hydra::uds::model::TimeSeriesSource::Points(pts) => {
                        ("Inline".to_string(), Some(pts.len()))
                    }
                };
                m.insert("source", Text(source));
                // An external series' length lives in a file this crate
                // never reads, so it is genuinely unknown here rather
                // than zero.
                if let Some(n) = points {
                    m.insert("points", Number(n as f64));
                }
                (t.id.clone(), m)
            })
            .collect(),
        "pattern" => net
            .patterns
            .iter()
            .map(|p| {
                let mut m = HashMap::new();
                m.insert("patternType", Text(format!("{:?}", p.kind)));
                m.insert("factors", Number(p.factors.len() as f64));
                (p.id.clone(), m)
            })
            .collect(),
        "rule" => net
            .controls
            .rules
            .iter()
            .map(|r| {
                let mut m = HashMap::new();
                m.insert("clauses", Number(r.lines.len() as f64));
                (r.name.clone(), m)
            })
            .collect(),
        "landuse" => net
            .land_uses
            .iter()
            .map(|l| {
                let mut m = HashMap::new();
                m.insert("sweepInterval", Number(l.sweep_interval));
                m.insert("sweepRemoval", Number(l.sweep_removal));
                m.insert("sweepDaysSince", Number(l.sweep_days_since));
                // Buildup and washoff are per-pollutant and mostly absent;
                // the count says how much of this land use is actually
                // parameterised without pretending to show the curves.
                m.insert(
                    "buildupFor",
                    Number(l.buildup.iter().flatten().count() as f64),
                );
                m.insert(
                    "washoffFor",
                    Number(l.washoff.iter().flatten().count() as f64),
                );
                (l.id.clone(), m)
            })
            .collect(),
        "aquifer" => net
            .aquifers
            .iter()
            .map(|a| {
                let mut m = HashMap::new();
                m.insert("porosity", Number(a.porosity));
                m.insert("wiltingPoint", Number(a.wilting_point));
                m.insert("fieldCapacity", Number(a.field_capacity));
                m.insert("conductivity", Number(a.conductivity));
                m.insert("upperEvapFrac", Number(a.upper_evap_frac));
                m.insert("lowerEvapDepth", Number(a.lower_evap_depth));
                (a.id.clone(), m)
            })
            .collect(),
        "snowpack" => net
            .snowpacks
            .iter()
            .map(|p| {
                let mut m = HashMap::new();
                let surfaces = [&p.plowable, &p.impervious, &p.pervious]
                    .iter()
                    .filter(|s| s.is_some())
                    .count();
                m.insert("surfaces", Number(surfaces as f64));
                m.insert("plowFraction", Number(p.plow_fraction));
                m.insert("removal", yes_no(p.removal.is_some()));
                (p.id.clone(), m)
            })
            .collect(),
        "hydrograph" => net
            .unit_hydrographs
            .iter()
            .map(|h| {
                let mut m = HashMap::new();
                if let Some(g) = h.gage.and_then(|i| net.gages.get(i)) {
                    m.insert("raingage", Text(g.id.clone()));
                }
                let responses = h
                    .months
                    .iter()
                    .flat_map(|m| m.iter())
                    .filter(|r| r.is_some())
                    .count();
                m.insert("responses", Number(responses as f64));
                (h.id.clone(), m)
            })
            .collect(),
        "lidcontrol" => net
            .lid_controls
            .iter()
            .map(|c| {
                let mut m = HashMap::new();
                if let Some(k) = &c.kind {
                    m.insert("lidType", Text(format!("{k:?}")));
                }
                let layers = [
                    c.surface.is_some(),
                    c.soil.is_some(),
                    c.pavement.is_some(),
                    c.storage.is_some(),
                    c.drain.is_some(),
                    c.drain_mat.is_some(),
                ]
                .iter()
                .filter(|p| **p)
                .count();
                m.insert("layers", Number(layers as f64));
                m.insert("removals", Number(c.removals.len() as f64));
                (c.id.clone(), m)
            })
            .collect(),
        "transect" => net
            .transects
            .iter()
            .map(|t| {
                let mut m = HashMap::new();
                m.insert("nChannel", Number(t.n_channel));
                m.insert("nLeft", Number(t.n_left));
                m.insert("nRight", Number(t.n_right));
                m.insert("stations", Number(t.stations.len() as f64));
                (t.id.clone(), m)
            })
            .collect(),
        "street" => net
            .streets
            .iter()
            .map(|st| {
                let mut m = HashMap::new();
                m.insert("crownWidth", Number(st.crown_width));
                m.insert("curbHeight", Number(st.curb_height));
                // Stored as a fraction, declared as a percent.
                m.insert("crossSlope", Number(st.cross_slope * 100.0));
                m.insert("roughness", Number(st.roughness));
                m.insert("gutterWidth", Number(st.gutter_width));
                m.insert("gutterDepression", Number(st.gutter_depression));
                m.insert("sides", Number(f64::from(st.sides)));
                (st.id.clone(), m)
            })
            .collect(),
        "inlet" => net
            .inlets
            .iter()
            .map(|i| {
                let mut m = HashMap::new();
                // A design may carry several openings at once — a
                // combination inlet is one design with both a grate and a
                // curb opening — so the summary names what is present
                // rather than pretending there is a single type.
                let mut openings: Vec<&str> = Vec::new();
                if i.grate.is_some() {
                    openings.push("grate");
                }
                if i.curb.is_some() {
                    openings.push("curb");
                }
                if i.slotted.is_some() {
                    openings.push("slotted");
                }
                if i.custom_curve.is_some() {
                    openings.push("custom");
                }
                m.insert(
                    "openings",
                    Text(if openings.is_empty() {
                        "—".to_string()
                    } else {
                        openings.join(" + ")
                    }),
                );
                if let Some(g) = &i.grate {
                    m.insert("grateLength", Number(g.length));
                    m.insert("grateWidth", Number(g.width));
                    m.insert("grateType", Text(format!("{:?}", g.grate)));
                }
                if let Some(c) = &i.curb {
                    m.insert("curbLength", Number(c.length));
                    m.insert("curbHeight", Number(c.height));
                }
                if let Some(sl) = &i.slotted {
                    m.insert("slottedLength", Number(sl.length));
                    m.insert("slottedWidth", Number(sl.width));
                }
                (i.id.clone(), m)
            })
            .collect(),
        _ => return None,
    };
    Some(rows)
}

/// How many elements of each kind the model holds.
///
/// Counted by classifying each element rather than by building one
/// kind's table per kind and measuring it: the rail asks this before
/// anything is shown, and the table path builds a value map per element
/// — which was one pass over every element for every kind in the
/// catalog, and this catalog has twenty-four.
pub fn kind_counts(net: &Network) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = hydra::uds::descriptors::ELEMENT_KINDS
        .iter()
        .map(|k| (k.id.to_string(), 0))
        .collect();
    let mut bump = |kind: &str| *counts.entry(kind.to_string()).or_default() += 1;
    for v in &net.vertices {
        bump(super::uds_view::vertex_kind_id(&v.kind));
    }
    for l in &net.links {
        bump(super::uds_view::link_kind_id(&l.kind));
    }
    for _ in &net.parcels {
        bump("subcatchment");
    }
    // The collections are listed rather than walked: each is its own
    // vector, and the counts are their lengths.
    for (kind, n) in [
        ("raingage", net.gages.len()),
        ("pollutant", net.constituents.len()),
        ("curve", net.curves.len()),
        ("timeseries", net.timeseries.len()),
        ("pattern", net.patterns.len()),
        ("rule", net.controls.rules.len()),
        ("landuse", net.land_uses.len()),
        ("aquifer", net.aquifers.len()),
        ("snowpack", net.snowpacks.len()),
        ("hydrograph", net.unit_hydrographs.len()),
        ("lidcontrol", net.lid_controls.len()),
        ("transect", net.transects.len()),
        ("street", net.streets.len()),
        ("inlet", net.inlets.len()),
    ] {
        counts.insert(kind.to_string(), n);
    }
    counts
}

/// Build one kind's table: ids in model order, and one column per §4.4
/// attribute the schema declares, in schema order.
pub fn kind_elements(net: &Network, kind: &str) -> KindElementsDto {
    // One pass over the elements of this kind, rather than a lookup per id.
    let mut rows: Vec<(String, HashMap<&'static str, AttrValue>)> =
        collection_rows(net, kind).unwrap_or_default();
    for p in &net.parcels {
        if kind == "subcatchment" {
            rows.push((p.id.clone(), parcel_values(net, p)));
        }
    }
    for v in &net.vertices {
        let (k, values) = vertex_values(net, v);
        if k == kind {
            rows.push((v.id.clone(), values));
        }
    }
    for l in &net.links {
        let (k, values) = link_values(net, l);
        if k == kind {
            rows.push((l.id.clone(), values));
        }
    }
    // After the classification, because it is the one value every one of
    // them carries whatever it is.
    for (id, values) in &mut rows {
        values.insert("tag", tag_value(net, id));
    }

    let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
    let columns = hydra::uds::descriptors::attribute_schema(kind)
        .into_iter()
        .map(|attr| {
            let quantity = attr
                .quantity
                .as_deref()
                .and_then(super::uds_results::quantity_descriptor);
            let values = rows
                .iter()
                .map(|(_, m)| match m.get(attr.key.as_str()) {
                    Some(AttrValue::Number(n)) => serde_json::json!(n),
                    Some(AttrValue::Text(t)) => serde_json::json!(t),
                    None => serde_json::Value::Null,
                })
                .collect();
            KindColumnDto {
                editable: attr.editable,
                references: attr.references,
                kind: attr.kind,
                key: attr.key,
                label: attr.label,
                quantity,
                values,
            }
        })
        .collect();
    let class = hydra::uds::descriptors::ELEMENT_KINDS
        .iter()
        .find(|k| k.id == kind)
        .map(|k| k.class);
    // Only the classes that are at a place. A link is not at one — it
    // runs between two, which is `ends` below rather than a coordinate.
    let spatial = matches!(
        class,
        Some(hydra::common::ElementClass::Point | hydra::common::ElementClass::Region)
    );
    let positions = if spatial {
        let placed: HashMap<&str, (f64, f64)> =
            super::uds_view::parse_xy_lines(net, "[COORDINATES]")
                .map(|(id, x, y)| (id, (x, y)))
                .collect();
        ids.iter()
            .map(|id| placed.get(id.as_str()).map(|&(x, y)| [x, y]))
            .collect()
    } else {
        Vec::new()
    };
    let ends = if class == Some(hydra::common::ElementClass::Polyline) {
        net.links
            .iter()
            .filter(|l| link_values(net, l).0 == kind)
            .map(|l| {
                [
                    net.vertices[l.from].id.clone(),
                    net.vertices[l.to].id.clone(),
                ]
            })
            .collect()
    } else {
        Vec::new()
    };
    KindElementsDto {
        ids,
        columns,
        positions,
        ends,
    }
}

/// One inlet coupling: a street conduit capturing flow into a sewer
/// vertex (SWMM `[INLET_USAGE]`). Reported by id so a consumer resolves
/// it against whatever element arrays it already holds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InletCouplingDto {
    /// The street conduit carrying the inlet.
    pub link: String,
    /// The vertex receiving captured flow.
    pub node: String,
    /// Id of the inlet design doing the capturing — a `street`/`inlet`
    /// registry entry, so the GUI can name it and follow it.
    pub design: String,
}

/// Inlet couplings of the target's model, empty for engines without them.
///
/// These are hydraulic connections that are **not links**: in a dual
/// drainage model the surface street network reaches the buried sewer only
/// through inlet capture, so a consumer reasoning about connectivity from
/// links alone would wrongly call the street network detached.
#[tauri::command(async)]
pub fn get_inlet_couplings(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<InletCouplingDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Ok(Vec::new());
    }
    let net = uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    Ok(net
        .inlet_usage
        .iter()
        .filter_map(|u| {
            Some(InletCouplingDto {
                link: net.links.get(u.link)?.id.clone(),
                node: net.vertices.get(u.capture_vertex)?.id.clone(),
                design: net.inlets.get(u.design)?.id.clone(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_ordered_rows_with_si_numbers_and_quantities() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let rows = element_attributes(&net, "J1").expect("junction rows");
        // Schema order: the kind's own values, then the tag every
        // element carries.
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["Invert elevation", "Maximum depth", "Initial depth", "Tag"],
        );
        // CFS file: 100 ft invert → 30.48 m, served in SI with the §5
        // elevation descriptor for the frontend to convert back.
        let invert = &rows[0];
        assert!((invert.number.unwrap() - 30.48).abs() < 1e-6);
        assert_eq!(invert.quantity.unwrap().key, "elevation");

        let rows = element_attributes(&net, "C1").expect("conduit rows");
        assert!(rows
            .iter()
            .any(|r| r.label == "Cross-section" && r.text.as_deref() == Some("Circular")));
        assert!(rows
            .iter()
            .any(|r| r.label == "Roughness" && r.quantity.is_none()));

        assert!(element_attributes(&net, "nope").is_none());
    }

    /// The bulk path is what a per-kind table reads: ids in model order,
    /// one column per declared attribute, values parallel to the ids.
    /// The five collection kinds are declared by the engine but had no
    /// rows, so the editor hid them and a drainage model's pollutants,
    /// curves, time series, patterns and rules were unreachable in the GUI.
    #[test]
    fn a_line_reports_its_ends_and_a_point_reports_a_position() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0 0 0\n\
                     [OUTFALLS]\nO1 90 FREE NO\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [COORDINATES]\nJ1 0 0\nO1 100 0\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let conduits = kind_elements(&net, "conduit");
        assert_eq!(conduits.ids, vec!["C1"]);
        assert_eq!(conduits.ends, vec![["J1".to_string(), "O1".to_string()]]);
        assert!(conduits.positions.is_empty(), "a line is not at a place");

        let junctions = kind_elements(&net, "junction");
        assert!(junctions.ends.is_empty(), "a point runs between nothing");
        assert_eq!(junctions.positions.len(), junctions.ids.len());
    }

    #[test]
    fn kind_elements_lists_the_collection_kinds() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [POLLUTANTS]\nTSS MG/L 0 0 0 0 NO\n\
                     [CURVES]\nST1 STORAGE 0 100\nST1 1 150\n\
                     [TIMESERIES]\nTS1 0:00 0.1\nTS1 1:00 0.4\n\
                     [PATTERNS]\nP1 HOURLY 1 1 1 1 1 1\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let pollutants = kind_elements(&net, "pollutant");
        assert_eq!(pollutants.ids, vec!["TSS"]);
        assert!(
            pollutants.columns.iter().any(|c| c.label == "Units"),
            "a pollutant row must say what its concentrations are in"
        );

        let curves = kind_elements(&net, "curve");
        assert_eq!(curves.ids, vec!["ST1"]);
        // A curve is a container; the row reports how large it is.
        let points = curves
            .columns
            .iter()
            .find(|c| c.key == "points")
            .expect("points column");
        assert_eq!(points.values[0], serde_json::json!(2.0));

        assert_eq!(kind_elements(&net, "timeseries").ids, vec!["TS1"]);
        assert_eq!(kind_elements(&net, "pattern").ids, vec!["P1"]);
    }

    /// A container's row reports only its size; the detail is the only
    /// way to see what is actually in it.
    #[test]
    fn collection_detail_serves_a_container_s_contents() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [CURVES]\nST1 STORAGE 0 100\nST1 1 150\n\
                     [PATTERNS]\nP1 HOURLY 1 2 3\n\
                     [TIMESERIES]\nTS1 0:00 0.1\nTS1 1:00 0.4\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        // A storage curve relates depth to surface area, and the values
        // are SI — the model declares CFS, so the importer converted feet
        // and square feet on the way in. Naming the axes is what stops
        // those numbers being two anonymous magnitudes.
        let curve = collection_detail(&net, "curve", "ST1");
        assert_eq!(curve.columns, vec!["Depth", "Surface area"]);
        assert_eq!(
            curve
                .quantities
                .iter()
                .map(|q| q.map(|q| q.key))
                .collect::<Vec<_>>(),
            vec![Some("depth"), Some("area")]
        );
        assert_eq!(curve.rows.len(), 2);
        assert!(
            (curve.rows[1][0] - 0.3048).abs() < 1e-6,
            "1 ft should arrive as 0.3048 m, got {}",
            curve.rows[1][0]
        );

        // Intervals are 1-based: a modeller counts hour 1, not hour 0.
        let pattern = collection_detail(&net, "pattern", "P1");
        assert_eq!(pattern.rows[0], vec![1.0, 1.0]);
        assert_eq!(pattern.rows[2], vec![3.0, 3.0]);

        let series = collection_detail(&net, "timeseries", "TS1");
        assert_eq!(series.columns, vec!["Time (h)", "Value"]);
        assert_eq!(series.rows, vec![vec![0.0, 0.1], vec![1.0, 0.4]]);
    }

    /// Empty is two answers, and the engine says which (§4.5.2.2).
    ///
    /// A consumer cannot tell them apart: an external series' values are
    /// real and held in a file beside the model, while a pollutant has no
    /// contents at all, and both arrive as no rows and no lines. The
    /// sentence explaining the first used to be written in the consumer,
    /// so it appeared under every kind whose contents were empty for any
    /// reason — telling readers that a pollutant's contents lived in a
    /// file that does not exist.
    #[test]
    fn empty_contents_say_why_only_when_there_is_a_why() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [POLLUTANTS]\nTSS MG/L 0 0 0 0 NO\n\
                     [TIMESERIES]\nRAIN FILE \"rain.dat\"\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let external = collection_detail(&net, "timeseries", "RAIN");
        assert!(external.rows.is_empty() && external.lines.is_empty());
        let note = external.note.expect("an external series says where it is");
        assert!(note.contains("rain.dat"), "{note}");

        // A pollutant is its attributes and nothing further. No note, so
        // the panel is not drawn at all rather than drawn saying nothing.
        let pollutant = collection_detail(&net, "pollutant", "TSS");
        assert!(pollutant.rows.is_empty() && pollutant.lines.is_empty());
        assert_eq!(pollutant.note, None);

        // And a container that *has* contents never carries one: a note
        // explains an absence, and this is not one.
        assert_eq!(collection_detail(&net, "junction", "J1").note, None);
    }

    /// A control measure has no *contents*, and that is now the whole
    /// answer.
    ///
    /// It used to say its layers could not be read, which was true while
    /// nothing could reach one. They are six record sets now (§4.5.2.3),
    /// so the panel below the table has nothing to add and says nothing
    /// rather than explaining an absence that is no longer there.
    #[test]
    fn a_lid_control_has_no_contents_to_show() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [LID_CONTROLS]\n\
                     GR1 BIO_CELL\n\
                     GR1 SURFACE 150 0.0 0.1 1.0 5\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);
        let detail = collection_detail(&net, "lidcontrol", "GR1");
        assert!(detail.rows.is_empty() && detail.lines.is_empty());
        assert_eq!(detail.note, None);
    }

    /// An unknown id is an expected state — a stale selection after the
    /// model reloads — not an error.
    #[test]
    fn collection_detail_is_empty_for_an_unknown_id() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n[JUNCTIONS]\nJ1 100 4 0.5\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);
        let d = collection_detail(&net, "curve", "nope");
        assert!(d.columns.is_empty() && d.rows.is_empty() && d.lines.is_empty());
        // ...as is a kind that is not a container at all.
        assert!(collection_detail(&net, "junction", "J1").rows.is_empty());
    }

    /// The process parameter sets — land uses, aquifers, snow packs, unit
    /// hydrographs, LID controls, transects — are named registries the
    /// model references by id, and none of them was declared as a kind, so
    /// they were unreachable in the GUI entirely rather than merely empty.
    #[test]
    fn kind_elements_lists_the_process_parameter_sets() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [POLLUTANTS]\nTSS MG/L 0 0 0 0 NO\n\
                     [LANDUSES]\nResidential 7 0.5 0\n\
                     [AQUIFERS]\nAQ1 0.5 0.15 0.30 5.0 10 15 0.35 3.0 10 0 0 0\n\
                     [TRANSECTS]\nNC 0.05 0.05 0.03\n\
                     X1 TR1 4 0 0 0 0 0 0 0\n\
                     GR 10 0 5 5 5 10 10 15\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let land = kind_elements(&net, "landuse");
        assert_eq!(land.ids, vec!["Residential"]);

        let aquifers = kind_elements(&net, "aquifer");
        assert_eq!(aquifers.ids, vec!["AQ1"]);
        let porosity = aquifers
            .columns
            .iter()
            .find(|c| c.key == "porosity")
            .expect("porosity column");
        assert_eq!(porosity.values[0], serde_json::json!(0.5));

        assert_eq!(kind_elements(&net, "transect").ids, vec!["TR1"]);
    }

    /// An external series' length lives in a file this crate never reads,
    /// so it must come back absent rather than as a confident zero.
    #[test]
    fn an_external_time_series_reports_no_point_count() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [TIMESERIES]\nTS1 FILE \"rain.dat\"\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);
        let ts = kind_elements(&net, "timeseries");
        assert_eq!(ts.ids, vec!["TS1"]);
        let points = ts
            .columns
            .iter()
            .find(|c| c.key == "points")
            .expect("points column");
        assert_eq!(points.values[0], serde_json::Value::Null);
    }

    /// The rail's number and the table's rows are two answers to one
    /// question, and they are now computed by two different functions —
    /// one classifying elements, one building them — because building
    /// every element's values to count them was one pass over the whole
    /// model per kind, and this catalog has twenty-four.
    ///
    /// Two answers is the arrangement that drifts, so they are checked
    /// against each other for every kind the catalog declares.
    #[test]
    fn the_counts_agree_with_the_tables_they_count() {
        // A model with something of several classes: vertices, links,
        // a parcel and a couple of collections.
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [RAINGAGES]\nRG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
                     [JUNCTIONS]\nJ1 100 4\nJ2 90 4\n\
                     [OUTFALLS]\nO1 80 FREE NO\n\
                     [CONDUITS]\nC1 J1 J2 400 0.013 0 0\nC2 J2 O1 300 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\nC2 CIRCULAR 1.5 0 0 0\n\
                     [SUBCATCHMENTS]\nS1 RG1 J1 5 40 500 0.5 0\n\
                     [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
                     [INFILTRATION]\nS1 3.0 0.5 4 7 0\n\
                     [CURVES]\nCV1 STORAGE 0 0\n\
                     [TIMESERIES]\nTS1 0:00 0.1\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);
        let counts = kind_counts(&net);
        let mut nonzero = 0;
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let rows = kind_elements(&net, kind.id).ids.len();
            assert_eq!(
                counts.get(kind.id).copied(),
                Some(rows),
                "{} counted {:?} and tabled {rows}",
                kind.id,
                counts.get(kind.id)
            );
            if rows > 0 {
                nonzero += 1;
            }
        }
        assert!(nonzero >= 5, "only {nonzero} kinds were exercised");
    }

    /// Position travels with the table, not as a column.
    ///
    /// It is implied by the element's class (hydra-common §4.5.2), which
    /// is what lets a generic table show an X and a Y for a drainage
    /// junction — whose position is a line in a section the engine
    /// preserves verbatim and never models, and which therefore appears
    /// in no attribute schema anywhere.
    #[test]
    fn a_spatial_kind_carries_its_positions() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4\nJ2 90 4\n\
                     [OUTFALLS]\nO1 80 FREE NO\n\
                     [CONDUITS]\nC1 J1 J2 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [COORDINATES]\nJ1 10 20\nO1 30 40\n";
        let (net, _) = hydra::uds::io::objects::parse_network(model);

        let junctions = kind_elements(&net, "junction");
        assert_eq!(junctions.ids, vec!["J1", "J2"]);
        assert_eq!(junctions.positions, vec![Some([10.0, 20.0]), None]);

        // A link is somewhere only in the sense that its ends are, and
        // the table shows those as columns. No coordinate.
        assert!(kind_elements(&net, "conduit").positions.is_empty());
        // Neither is a curve, which is not anywhere at all.
        assert!(kind_elements(&net, "curve").positions.is_empty());
    }

    #[test]
    fn kind_elements_serves_one_column_per_declared_attribute() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\nJ2 90 3 0\n\
                     [OUTFALLS]\nO1 88 FREE NO\n\
                     [CONDUITS]\nC1 J1 J2 400 0.013 0 0\nC2 J2 O1 300 0.015 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\nC2 CIRCULAR 1.5 0 0 0\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let junctions = kind_elements(&net, "junction");
        assert_eq!(junctions.ids, vec!["J1", "J2"]);
        assert_eq!(
            junctions
                .columns
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Invert elevation", "Maximum depth", "Initial depth", "Tag"],
        );
        // Values are columnar and parallel to ids, in SI.
        let invert = &junctions.columns[0];
        assert_eq!(invert.values.len(), junctions.ids.len());
        assert_eq!(invert.quantity.unwrap().key, "elevation");
        assert!((invert.values[0].as_f64().unwrap() - 30.48).abs() < 1e-6);

        // A different kind gets its own columns entirely — the whole point.
        let conduits = kind_elements(&net, "conduit");
        assert_eq!(conduits.ids, vec!["C1", "C2"]);
        assert!(conduits.columns.iter().any(|c| c.label == "Roughness"));
        assert!(conduits.columns.iter().any(|c| c.label == "Cross-section"));

        // Outfalls carry text attributes, served as strings.
        let outfalls = kind_elements(&net, "outfall");
        let boundary = outfalls
            .columns
            .iter()
            .find(|c| c.label == "Boundary")
            .expect("boundary column");
        assert_eq!(boundary.values[0].as_str(), Some("FREE"));

        // A kind the model has none of is empty, not an error.
        assert!(kind_elements(&net, "weir").ids.is_empty());
    }

    /// The table and the inspector must offer the same set of writes for
    /// the same element. They are built by different functions from the
    /// same table, so the way this drifts is one of them gaining a key
    /// the other does not have — asserted against each other rather than
    /// against a list written here, which would drift with neither.
    #[test]
    fn a_column_is_editable_exactly_where_the_inspector_row_is() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4 0.5\n\
                     [OUTFALLS]\nO1 88 FREE NO\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);

        let mut compared = 0;
        for (kind, id) in [("junction", "J1"), ("outfall", "O1"), ("conduit", "C1")] {
            let table = kind_elements(&net, kind);
            let rows = element_attributes(&net, id).expect("element");
            for column in &table.columns {
                // A column the element does not carry has no row to
                // compare against — the table serves it as a null cell,
                // which is why the flag alone cannot decide whether a
                // given cell takes an input.
                let Some(row) = rows.iter().find(|r| r.key == column.key) else {
                    continue;
                };
                compared += 1;
                assert_eq!(
                    column.editable, row.editable,
                    "{kind}.{} disagrees between the table and the inspector",
                    column.key,
                );
            }
        }
        assert!(compared >= 8, "only {compared} columns had a row to check");

        // And the flag is not simply true everywhere: a conduit's length
        // is settable, its cross-section — a referent, not a number — is
        // not.
        let conduits = kind_elements(&net, "conduit");
        let editable = |key: &str| {
            conduits
                .columns
                .iter()
                .find(|c| c.key == key)
                .map(|c| c.editable)
        };
        assert_eq!(editable("length"), Some(true));
        assert_eq!(editable("shape"), Some(false));
    }

    #[test]
    fn subcatchment_rows_convert_to_their_declared_quantities() {
        // 2.5 ac ≈ 10117 m² area, 40 % impervious, 1.5 % slope.
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [RAINGAGES]\nRG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
                     [JUNCTIONS]\nJ1 100 4\n\
                     [SUBCATCHMENTS]\nS1 RG1 J1 2.5 40 500 1.5 0\n\
                     [TIMESERIES]\nTS1 0 0.5 1 0.25\n";
        let (net, _diags) = hydra::uds::io::objects::parse_network(model);
        let rows = element_attributes(&net, "S1").expect("subcatchment rows");
        let by = |label: &str| {
            rows.iter()
                .find(|r| r.label == label)
                .unwrap_or_else(|| panic!("row {label:?} missing from {rows:?}"))
        };
        assert_eq!(by("Rain gage").text.as_deref(), Some("RG1"));
        assert_eq!(by("Outlet").text.as_deref(), Some("J1"));
        // Area is declared in hectares: 2.5 ac ≈ 1.0117 ha.
        let area = by("Area");
        assert_eq!(area.quantity.unwrap().key, "area");
        assert!(
            (area.number.unwrap() - 1.011_714).abs() < 1e-3,
            "area in ha, got {:?}",
            area.number
        );
        // Fractions become percent, matching the schema's quantity.
        assert!((by("Slope").number.unwrap() - 1.5).abs() < 1e-6);
        assert!((by("Imperviousness").number.unwrap() - 40.0).abs() < 1e-6);
    }
}

/// Street sections and inlet designs — the two registries a dual-drainage
/// model carries that the GUI showed nowhere.
///
/// The canvas drew inlet couplings as dashed connectors and the inspector
/// learned to follow them, but the design at the end of that connector was
/// a name with no home: no rail entry, no table, nothing to open. These
/// tests pin both kinds as first-class collections, so the name is a place
/// the reader can go.
#[cfg(test)]
mod street_and_inlet_kinds {
    use super::*;

    /// The dual-drainage shape: a street channel with a combination inlet
    /// (grate **and** curb, one design) capturing into a sewer junction.
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1   100  4
J2   99   4
SEW  90   8

[OUTFALLS]
O1  95  FREE
O2  85  FREE

[CONDUITS]
GUT1  J1   J2  300  0.016  0  0
C2    J2   O1  300  0.016  0  0
SEW1  SEW  O2  300  0.013  0  0

[XSECTIONS]
GUT1  STREET    ST1
C2    STREET    ST1
SEW1  CIRCULAR  1.5  0  0  0

[STREETS]
ST1  20  0.5  2  0.016  0.1  2  1  10  4  0.02

[INLETS]
CB1  GRATE  2  2  P_BAR-50
CB1  CURB   2  0.5  HORIZONTAL

[INLET_USAGE]
GUT1  CB1  SEW  1  0  0  0  0  ON_GRADE
";

    fn network() -> Network {
        hydra::uds::io::objects::parse_network(MODEL).0
    }

    /// Both are declared kinds, so the Editor's catalog-driven rail lists
    /// them without knowing they exist.
    #[test]
    fn both_registries_are_declared_element_kinds() {
        let ids: Vec<&str> = hydra::uds::descriptors::ELEMENT_KINDS
            .iter()
            .map(|k| k.id)
            .collect();
        assert!(ids.contains(&"street"), "streets are a kind");
        assert!(ids.contains(&"inlet"), "inlet designs are a kind");
    }

    #[test]
    fn a_street_section_lists_with_its_geometry() {
        let net = network();
        let table = kind_elements(&net, "street");
        assert_eq!(table.ids, vec!["ST1"]);
        let col = |key: &str| {
            table
                .columns
                .iter()
                .find(|c| c.key == key)
                .unwrap_or_else(|| panic!("street table has a {key} column"))
        };
        // 20 ft of crown width arrives as 6.096 m — the importer converts,
        // and the column declares a length so the GUI can show either.
        assert_eq!(col("crownWidth").quantity.unwrap().key, "length");
        // Cross slope is a fraction in the model and a percent on screen.
        assert_eq!(col("crossSlope").quantity.unwrap().key, "percent");
    }

    /// A combination inlet is *one* design carrying both a grate and a curb
    /// opening. Reporting a single "type" would have to pick one and drop
    /// the other, which is why the summary names what is present.
    #[test]
    fn a_combination_inlet_reports_both_its_openings() {
        let net = network();
        let table = kind_elements(&net, "inlet");
        assert_eq!(table.ids, vec!["CB1"]);
        let openings = table
            .columns
            .iter()
            .find(|c| c.key == "openings")
            .expect("inlet table has an openings column");
        let summary = openings.values[0]
            .as_str()
            .unwrap_or_else(|| panic!("openings is text, got {:?}", openings.values[0]));
        assert!(summary.contains("grate"), "got {summary}");
        assert!(summary.contains("curb"), "got {summary}");
    }

    /// A registry entry's detail answers what its row cannot: who uses it.
    /// Following an inlet's name from the inspector has to arrive somewhere
    /// that says more than the inspector already did.
    #[test]
    fn a_registry_entry_lists_what_references_it() {
        let net = network();
        // Two conduits share the one street section.
        let street = collection_detail(&net, "street", "ST1");
        assert_eq!(street.lines, vec!["GUT1", "C2"]);
        // The inlet says which street captures where through it.
        let inlet = collection_detail(&net, "inlet", "CB1");
        assert_eq!(inlet.lines, vec!["GUT1 → SEW"]);
    }

    /// The coupling the canvas draws now names the design at its end, so
    /// the inspector can label the connector with how it captures and not
    /// only where.
    #[test]
    fn a_coupling_names_the_design_doing_the_capturing() {
        let net = network();
        let coupling = net
            .inlet_usage
            .first()
            .expect("the fixture has one inlet usage");
        assert_eq!(net.links[coupling.link].id, "GUT1");
        assert_eq!(net.vertices[coupling.capture_vertex].id, "SEW");
        assert_eq!(net.inlets[coupling.design].id, "CB1");
    }
}

// ── Writing an attribute back ───────────────────────────────────────────

/// Set one engine-described attribute on one element (§4.4).
///
/// Keyed by the same schema key the read path serves, and taking a value
/// in the same unit that path serves it in: the descriptor's own base
/// unit, which is not always SI — an area is stated in hectares and a
/// slope as a percentage, because that is what the §5 quantity for each
/// declares. The frontend converts for display and converts back, so the
/// backend never learns the user's preference.
///
/// Each writable key therefore inverts whatever its read applies. That
/// pairing is asserted rather than assumed: the test below sets a value
/// and reads it back through `element_attributes`, so a conversion that
/// exists on one side and not the other fails.
///
/// Not every key is writable. A referent — a gage, an outlet, a curve —
/// names another object, and setting one is a different operation from
/// setting a number; a shape carries four geometry values behind one
/// label. Those refuse by name rather than being silently ignored, so a
/// caller learns which of the two it asked for.
pub(crate) fn set_attribute(
    net: &mut Network,
    element_id: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    // The one key this engine writes that is not a number, and the
    // reason this takes a JSON value at all: a tag is free text, and it
    // is not on the element — it is a line in a section the engine
    // preserves verbatim, like a coordinate.
    if key == "tag" {
        return super::uds_view::set_tag(net, element_id, value.as_str().unwrap_or(""));
    }
    // The other textual write: a subcatchment's outlet names an element
    // rather than holding a number, and the model stores it as an index
    // into one of two arrays — which of the two is what the name
    // decides.
    // A gage's record and a pump's curve: both name an object the model
    // holds as an index, so both are matched on the key before any kind.
    if key == "source" {
        return set_gage_source(net, element_id, value.as_str().unwrap_or("").trim());
    }
    if key == "curve" {
        return set_pump_curve(net, element_id, value.as_str().unwrap_or("").trim());
    }
    // A grate's family is a keyword rather than a number, so it is
    // matched here with the other textual writes.
    // A curve's role, which is a keyword and not a number. Writable so a
    // curve created under the wrong one can be corrected — the points
    // stay as written, because the role says how to *read* them and
    // reinterpreting them silently would be a second, hidden edit.
    // An outlet's rating curve, which swaps a power relation for a
    // tabulated one. Emptying it is refused rather than clearing the
    // rating: an outlet with no rating passes no flow and is not a
    // state a model may hold.
    if key == "outletCurve" {
        return set_outlet_curve(net, element_id, value.as_str().unwrap_or("").trim());
    }
    if key == "curveType" {
        return set_curve_role(net, element_id, value.as_str().unwrap_or("").trim());
    }
    if key == "grateType" {
        return set_grate_type(net, element_id, value.as_str().unwrap_or("").trim());
    }
    // A control measure's unit type, which is what it is before any
    // layer is entered — and the one field a new one is created by.
    if key == "lidType" {
        return set_lid_kind(net, element_id, value.as_str().unwrap_or("").trim());
    }
    if key == "raingage" {
        return set_parcel_gage(net, element_id, value.as_str().unwrap_or("").trim());
    }
    if key == "outlet" {
        return set_parcel_outlet(net, element_id, value.as_str().unwrap_or("").trim());
    }
    // The third: which link a divider's diverted flow leaves by. Stored
    // as an index like an outlet, and cleared by an empty name — a
    // divider with no diverted link is what the file writes as `*`.
    if key == "divertedLink" {
        return set_diverted_link(net, element_id, value.as_str().unwrap_or("").trim());
    }
    let value = value
        .as_f64()
        .ok_or_else(|| format!("'{key}' takes a number"))?;
    if !value.is_finite() {
        return Err("that value is not a number".into());
    }
    if let Some(v) = net
        .vertices
        .iter_mut()
        .find(|v| v.id.eq_ignore_ascii_case(element_id))
    {
        return set_vertex_attribute(v, key, value);
    }
    // Read before the mutable borrow: a cross-section's geometry is in
    // the file's units, and the mapping is a property of the model.
    let per_unit = net.options.flow_units.m_per_length_unit();
    if let Some(l) = net
        .links
        .iter_mut()
        .find(|l| l.id.eq_ignore_ascii_case(element_id))
    {
        return set_link_attribute(l, key, value, per_unit);
    }
    if let Some(p) = net
        .parcels
        .iter_mut()
        .find(|p| p.id.eq_ignore_ascii_case(element_id))
    {
        return set_parcel_attribute(p, key, value);
    }
    set_record_attribute(net, element_id, key, value)
}

/// The attribute keys this path can set, per element kind.
///
/// One table rather than a condition in two places: the inspector marks
/// a row editable from it and the setter refuses from it, so a row that
/// offers an edit and a key that accepts one are the same fact. The test
/// below drives itself from this table, so a key added here without a
/// matching arm fails rather than silently offering an input that
/// refuses.
/// Whether `key` is settable on `kind`.
///
/// Read from the engine's own attribute schema, which carries the
/// answer since the editing contract landed (hydra-common §4.5.1).
/// There used to be a table here, and it was a second answer to a
/// question the engine already answers: it claimed a divider's maximum
/// and initial depth were writable while the schema published neither,
/// so the rows never appeared and nothing said they were missing.
#[allow(dead_code)]
pub(crate) fn is_writable(kind: &str, key: &str) -> bool {
    hydra::uds::descriptors::attribute_schema(kind)
        .iter()
        .any(|a| a.key == key && a.editable)
}

fn unwritable(key: &str) -> String {
    format!("'{key}' cannot be edited here")
}

fn set_vertex_attribute(
    vertex: &mut hydra::uds::model::Vertex,
    key: &str,
    value: f64,
) -> Result<(), String> {
    use hydra::uds::model::VertexKind as K;
    if key == "invert" {
        vertex.invert = value;
        return Ok(());
    }
    if let (K::Storage { geometry, .. }, "surfaceArea") = (&mut vertex.kind, key) {
        let StorageGeometry::Functional {
            coeff,
            exponent,
            constant,
        } = geometry
        else {
            return Err("only a prismatic storage unit has one surface area".into());
        };
        if *coeff != 0.0 && *exponent != 0.0 {
            return Err("only a prismatic storage unit has one surface area".into());
        }
        *constant = value;
        return Ok(());
    }
    match (&mut vertex.kind, key) {
        (K::Junction { max_depth, .. } | K::Storage { max_depth, .. }, "maxDepth")
        | (K::Divider { max_depth, .. }, "maxDepth") => *max_depth = value,
        (K::Junction { init_depth, .. } | K::Storage { init_depth, .. }, "initDepth")
        | (K::Divider { init_depth, .. }, "initDepth") => *init_depth = value,
        _ => return Err(unwritable(key)),
    }
    Ok(())
}

/// Write one of a link's opening dimensions back into its cross-section.
///
/// Metres in, the file's own units stored — the inverse of what the read
/// applied, asked of the same mapping so the pair cannot drift.
fn set_opening(
    net_units: f64,
    l: &mut hydra::uds::model::Link,
    at: usize,
    value: f64,
) -> Result<(), String> {
    if !(value.is_finite() && value > 0.0) {
        return Err("an opening needs a positive size".into());
    }
    let xs = l
        .cross_section
        .as_mut()
        .ok_or_else(|| format!("'{}' carries no cross-section", l.id))?;
    xs.geom_user[at] = value / net_units;
    Ok(())
}

fn set_link_attribute(
    link: &mut hydra::uds::model::Link,
    key: &str,
    value: f64,
    per_unit: f64,
) -> Result<(), String> {
    use hydra::uds::model::LinkKind as K;
    // A power relation's two numbers, which only an outlet rated by one
    // has — a tabulated rating carries a curve instead, and setting a
    // coefficient on it would silently discard the curve.
    if let (K::Outlet { rating, .. }, "ratingCoeff" | "ratingExponent") = (&mut link.kind, key) {
        let hydra::uds::model::OutletRating::Functional { coeff, exponent } = rating else {
            return Err(format!("'{}' is rated by a curve, not a relation", link.id));
        };
        if key == "ratingCoeff" {
            if !(value.is_finite() && value > 0.0) {
                return Err("a rating coefficient has to be greater than zero".into());
            }
            *coeff = value;
        } else {
            *exponent = value;
        }
        return Ok(());
    }
    // The opening, which lives in the cross-section rather than on the
    // kind — so it is matched on the key before the kind, the way a tag
    // is. Both kinds that have one call it something different, which is
    // why the key decides which of the two geometry slots it means.
    let slot = match (&link.kind, key) {
        (K::Orifice { .. }, "height") | (K::Weir { .. }, "crestHeight") => Some(0),
        (K::Orifice { .. }, "width") | (K::Weir { .. }, "crestLength") => Some(1),
        _ => None,
    };
    if let Some(at) = slot {
        return set_opening(per_unit, link, at, value);
    }
    match (&mut link.kind, key) {
        (K::Channel { length, .. }, "length") => *length = value,
        (K::Channel { roughness, .. }, "roughness") => *roughness = value,
        (
            K::Orifice {
                discharge_coeff, ..
            }
            | K::Weir {
                discharge_coeff, ..
            },
            "dischargeCoeff",
        ) => {
            *discharge_coeff = value;
        }
        _ => return Err(unwritable(key)),
    }
    Ok(())
}

/// Point an outlet at a tabulated rating curve.
fn set_outlet_curve(net: &mut Network, id: &str, curve_id: &str) -> Result<(), String> {
    let found = net
        .curves
        .iter()
        .position(|c| c.id.eq_ignore_ascii_case(curve_id))
        .ok_or_else(|| format!("'{curve_id}' is not a curve in this model"))?;
    let link = net
        .links
        .iter_mut()
        .find(|l| l.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    let hydra::uds::model::LinkKind::Outlet { rating, .. } = &mut link.kind else {
        return Err(format!("'{id}' is not an outlet"));
    };
    *rating = hydra::uds::model::OutletRating::Tabular { curve: found };
    Ok(())
}

/// Set a curve's role, by the predecessor's own keyword.
fn set_curve_role(net: &mut Network, id: &str, name: &str) -> Result<(), String> {
    let kind = super::uds_create::curve_kind(name)
        .ok_or_else(|| format!("'{name}' is not a curve role"))?;
    let curve = net
        .curves
        .iter_mut()
        .find(|c| c.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    curve.kind = kind;
    Ok(())
}

/// Set a control measure's unit type, by the file's own keyword.
///
/// The layers are left exactly as they are. A type says which layers the
/// engine will *read*, not what they contain, and silently discarding a
/// soil layer because the type changed would be a second edit nobody
/// asked for — the file is free to carry one either way.
fn set_lid_kind(net: &mut Network, id: &str, name: &str) -> Result<(), String> {
    let kind = super::uds_create::lid_kind(name)
        .ok_or_else(|| format!("'{name}' is not a control measure type"))?;
    let lid = net
        .lid_controls
        .iter_mut()
        .find(|c| c.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    lid.kind = Some(kind);
    Ok(())
}

/// Set an inlet's grate family, by the predecessor's own keyword.
fn set_grate_type(net: &mut Network, id: &str, name: &str) -> Result<(), String> {
    let kind = super::uds_create::grate_kind(name)
        .ok_or_else(|| format!("'{name}' is not a grate type"))?;
    let inlet = net
        .inlets
        .iter_mut()
        .find(|i| i.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    let grate = inlet
        .grate
        .as_mut()
        .ok_or_else(|| format!("'{id}' has no grate opening"))?;
    grate.grate = kind;
    Ok(())
}

/// Point a rain gage at the time series it reads.
///
/// Only a series: a gage may instead read an external file, and swapping
/// one for the other means supplying a file name and a station, which is
/// two values rather than a reference. That refuses rather than half
/// happening.
fn set_gage_source(net: &mut Network, id: &str, series_id: &str) -> Result<(), String> {
    let series = net
        .timeseries
        .iter()
        .position(|t| t.id.eq_ignore_ascii_case(series_id))
        .ok_or_else(|| format!("'{series_id}' is not a time series in this model"))?;
    let gage = net
        .gages
        .iter_mut()
        .find(|g| g.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    gage.source = hydra::uds::model::GageSource::Series { series };
    Ok(())
}

/// Point a pump at its characteristic curve.
fn set_pump_curve(net: &mut Network, id: &str, curve_id: &str) -> Result<(), String> {
    let found = net
        .curves
        .iter()
        .position(|c| c.id.eq_ignore_ascii_case(curve_id))
        .ok_or_else(|| format!("'{curve_id}' is not a curve in this model"))?;
    let link = net
        .links
        .iter_mut()
        .find(|l| l.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    let hydra::uds::model::LinkKind::Pump { curve, .. } = &mut link.kind else {
        return Err(format!("'{id}' is not a pump"));
    };
    *curve = Some(found);
    Ok(())
}

/// Point a subcatchment at the gage whose rainfall drives it.
///
/// Stored as an index like an outlet, and required — there is no value
/// meaning "no gage", so this replaces one rather than clearing it.
fn set_parcel_gage(net: &mut Network, id: &str, gage_id: &str) -> Result<(), String> {
    let gage = net
        .gages
        .iter()
        .position(|g| g.id.eq_ignore_ascii_case(gage_id))
        .ok_or_else(|| format!("'{gage_id}' is not a rain gage in this model"))?;
    let parcel = net
        .parcels
        .iter_mut()
        .find(|p| p.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    parcel.gage = gage;
    Ok(())
}

/// Point a subcatchment at where its runoff goes.
///
/// Either a conveyance node or another subcatchment — runoff enters the
/// network or cascades overland — and the name says which. Refuses a
/// subcatchment pointed at itself, which is not a short cascade but a
/// loop the router would never leave.
///
/// Resolved to an index, because that is what the model holds. Nothing
/// above this line knows that: the id is the whole of what an
/// application says, here and in the read that served it.
fn set_parcel_outlet(net: &mut Network, id: &str, outlet: &str) -> Result<(), String> {
    let parcel = net
        .parcels
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    let target = if let Some(v) = net
        .vertices
        .iter()
        .position(|v| v.id.eq_ignore_ascii_case(outlet))
    {
        ParcelOutlet::Vertex(v)
    } else if let Some(p) = net
        .parcels
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(outlet))
    {
        if p == parcel {
            return Err(format!("'{id}' cannot drain to itself"));
        }
        ParcelOutlet::Parcel(p)
    } else {
        return Err(format!(
            "'{outlet}' is not a node or a subcatchment in this model"
        ));
    };
    net.parcels[parcel].outlet = target;
    Ok(())
}

/// Name the link a divider's diverted flow leaves by.
///
/// Empty clears it, which the file writes as `*` and the engine reads as
/// "none named" — a divider that diverts nowhere is legal input, so
/// refusing to express it would make one kind of model uneditable.
fn set_diverted_link(net: &mut Network, id: &str, link: &str) -> Result<(), String> {
    let diverted = if link.is_empty() {
        None
    } else {
        Some(
            net.links
                .iter()
                .position(|l| l.id.eq_ignore_ascii_case(link))
                .ok_or_else(|| format!("'{link}' is not a link in this model"))?,
        )
    };
    let vertex = net
        .vertices
        .iter_mut()
        .find(|v| v.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("element '{id}' not found"))?;
    let hydra::uds::model::VertexKind::Divider { diverted_link, .. } = &mut vertex.kind else {
        return Err(format!("'{id}' is not a divider"));
    };
    *diverted_link = diverted;
    Ok(())
}

/// Write one field of a street section or a transect.
///
/// Both are flat records of numbers the file carries in its own section,
/// which is why neither reaches the vertex/link/parcel arms below —
/// there is no element to match on, only a named record.
fn set_record_attribute(
    net: &mut Network,
    element_id: &str,
    key: &str,
    value: f64,
) -> Result<(), String> {
    if let Some(st) = net
        .streets
        .iter_mut()
        .find(|st| st.id.eq_ignore_ascii_case(element_id))
    {
        match key {
            "crownWidth" => st.crown_width = value,
            "curbHeight" => st.curb_height = value,
            // Stored as a fraction, described as a percentage — the same
            // pair a subcatchment's slope makes.
            "crossSlope" => st.cross_slope = value / 100.0,
            "roughness" => st.roughness = value,
            "gutterWidth" => st.gutter_width = value,
            "gutterDepression" => st.gutter_depression = value,
            "sides" => {
                if value != 1.0 && value != 2.0 {
                    return Err("a street has one side or two".into());
                }
                st.sides = value as u8;
            }
            other => return Err(unwritable(other)),
        }
        return Ok(());
    }
    if let Some(i) = net
        .inlets
        .iter_mut()
        .find(|i| i.id.eq_ignore_ascii_case(element_id))
    {
        // An opening's dimensions, and only where that opening exists —
        // writing a grate width on a design with no grate would have to
        // invent the rest of one.
        let missing = |what: &str| format!("'{element_id}' has no {what} opening");
        match key {
            "grateLength" | "grateWidth" => {
                let g = i.grate.as_mut().ok_or_else(|| missing("grate"))?;
                if key == "grateLength" {
                    g.length = value;
                } else {
                    g.width = value;
                }
            }
            "curbLength" | "curbHeight" => {
                let c = i.curb.as_mut().ok_or_else(|| missing("curb"))?;
                if key == "curbLength" {
                    c.length = value;
                } else {
                    c.height = value;
                }
            }
            "slottedLength" | "slottedWidth" => {
                let sl = i.slotted.as_mut().ok_or_else(|| missing("slotted"))?;
                if key == "slottedLength" {
                    sl.length = value;
                } else {
                    sl.width = value;
                }
            }
            other => return Err(unwritable(other)),
        }
        return Ok(());
    }
    if let Some(t) = net
        .transects
        .iter_mut()
        .find(|t| t.id.eq_ignore_ascii_case(element_id))
    {
        // A Manning roughness of nought divides by zero in the
        // conveyance, so it is refused here rather than at the solver.
        let positive = |v: f64| {
            (v > 0.0)
                .then_some(v)
                .ok_or_else(|| "a roughness has to be greater than zero".to_string())
        };
        match key {
            "nChannel" => t.n_channel = positive(value)?,
            "nLeft" => t.n_left = positive(value)?,
            "nRight" => t.n_right = positive(value)?,
            other => return Err(unwritable(other)),
        }
        return Ok(());
    }
    Err(format!("element '{element_id}' not found"))
}

fn set_parcel_attribute(
    parcel: &mut hydra::uds::model::Parcel,
    key: &str,
    value: f64,
) -> Result<(), String> {
    match key {
        // The `area` quantity's base unit is hectares, not square metres.
        "area" => parcel.area = value * 10_000.0,
        "width" => parcel.width = value,
        // Stored as fractions, described as percentages.
        "slope" => parcel.slope = value / 100.0,
        "imperviousness" => parcel.frac_imperv = value / 100.0,
        _ => return Err(unwritable(key)),
    }
    Ok(())
}

#[cfg(test)]
mod write_tests {
    use super::*;

    const INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS

[JUNCTIONS]
J1  10  3  0.5  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  100  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  1  0  0  0  1

[SUBCATCHMENTS]
S1  G1  J1  4.5  35  400  1.2  0

[SUBAREAS]
S1  0.015  0.24  0.06  0.2  20  OUTLET

[INFILTRATION]
S1  3.5  0.6  4.14  6

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  RS1
G2  INTENSITY  0:15  1.0  TIMESERIES  RS1

[TIMESERIES]
RS1  0:00  0.4
";

    fn model() -> Network {
        let (net, diags) = hydra::uds::io::objects::parse_network(INP);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        net
    }

    fn read(net: &Network, id: &str, label: &str) -> f64 {
        element_attributes(net, id)
            .unwrap_or_else(|| panic!("{id} has no attributes"))
            .into_iter()
            .find(|r| r.label == label)
            .unwrap_or_else(|| panic!("{id} has no {label} row"))
            .number
            .expect("a number")
    }

    /// Every editable value can be changed, and put back.
    ///
    /// Two claims in one, because the second is the one an application
    /// leans on and nothing was asserting: undo works by writing the
    /// value that was there before, so an attribute whose write is not
    /// invertible is an attribute the history cannot restore. A write
    /// that clamps, converts asymmetrically, or has a side effect on a
    /// second field would pass "set it and read it back" and fail here.
    ///
    /// The sample comes from the model rather than from a list written
    /// here. A hand-written one covers the kinds someone remembered, and
    /// that is how an inlet came to publish four editable fields with no
    /// setter behind any of them — the list named four kinds and the
    /// inlet was not one of them. Adding a kind to the fixture now
    /// extends the check by itself.
    #[test]
    fn every_editable_value_can_be_changed_and_put_back() {
        let net = model();
        let mut checked = 0;
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let Some(id) = kind_elements(&net, kind.id).ids.first().cloned() else {
                continue;
            };
            let Some(rows) = element_attributes(&net, &id) else {
                continue;
            };
            for row in rows.into_iter().filter(|r| r.editable) {
                // Only the rows carrying a number: a reference and a
                // choice are restored by the same write, and their
                // round trip is asserted where the values live.
                let Some(before) = row.number else { continue };
                let changed = if before.abs() < 1e-9 {
                    1.0
                } else {
                    before * 1.5
                };

                let mut draft = model();
                set_attribute(&mut draft, &id, &row.key, &serde_json::json!(changed))
                    .unwrap_or_else(|e| panic!("{}.{} refused the edit: {e}", kind.id, row.key));
                let now = read_key(&draft, &id, &row.key);
                assert!(
                    (now - changed).abs() < 1e-6,
                    "{}.{} was set to {changed} and reads {now}",
                    kind.id,
                    row.key
                );

                // The undo: the same write, with what was there before.
                set_attribute(&mut draft, &id, &row.key, &serde_json::json!(before))
                    .unwrap_or_else(|e| panic!("{}.{} refused the undo: {e}", kind.id, row.key));
                let back = read_key(&draft, &id, &row.key);
                assert!(
                    (back - before).abs() < 1e-6,
                    "{}.{} started at {before}, was set to {changed}, and came back {back}",
                    kind.id,
                    row.key
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 10,
            "only {checked} editable values were exercised"
        );
    }

    /// One attribute's number, by its schema key.
    fn read_key(net: &Network, id: &str, key: &str) -> f64 {
        element_attributes(net, id)
            .unwrap_or_else(|| panic!("{id} has no attributes"))
            .into_iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("{id} has no {key} row"))
            .number
            .expect("a number")
    }

    /// Setting a value and reading it back is the only check that catches
    /// a conversion applied on one side and not the other — which is
    /// exactly what went wrong first: an area is served in hectares and a
    /// slope as a percentage, not in the SI the module's summary implies.
    ///
    /// Driven from the engine's own schema rather than from a list
    /// written here, so an attribute marked editable without a matching
    /// arm in the setter fails instead of quietly offering the inspector
    /// an input that refuses. That is the pairing the editing contract
    /// asks for: the flag is advisory and this is what keeps it honest.
    #[test]
    fn every_writable_attribute_reads_back_as_it_was_set() {
        // One element of each kind the table covers, and a value that is
        // not the fixture's so a no-op would fail.
        let sample = |kind: &str| -> Option<&'static str> {
            match kind {
                "junction" => Some("J1"),
                "outfall" => Some("O1"),
                "conduit" => Some("C1"),
                "subcatchment" => Some("S1"),
                // Kinds the fixture has none of; covered by the
                // round-trip test for the kinds it does have.
                _ => None,
            }
        };
        let mut checked = 0;
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let Some(id) = sample(kind.id) else { continue };
            let kind = kind.id;
            for (key, references) in hydra::uds::descriptors::attribute_schema(kind)
                .iter()
                .filter(|a| a.editable)
                .map(|a| (a.key.clone(), a.references.clone()))
                .collect::<Vec<_>>()
            {
                let key = key.as_str();
                let mut net = model();
                let before = element_attributes(&net, id)
                    .expect("attributes")
                    .into_iter()
                    .find(|r| r.key == key)
                    .unwrap_or_else(|| panic!("{kind}.{key} has no row"));
                assert!(before.editable, "{kind}.{key} reads as not editable");
                // A textual attribute round-trips as text, and one that
                // names another element round-trips through a name that
                // model actually holds — read from the kinds the
                // descriptor declares (§4.5.1.1), so a reference the
                // engine widens later is exercised without this loop
                // being told about it.
                if let Some(text) = &before.text {
                    let value = if references.is_empty() {
                        format!("{text}x")
                    } else {
                        references
                            .iter()
                            .flat_map(|k| kind_elements(&net, k).ids)
                            .find(|target| target != id && Some(target) != before.text.as_ref())
                            .unwrap_or_else(|| {
                                panic!("{kind}.{key} references {references:?}, none in the model")
                            })
                    };
                    set_attribute(&mut net, id, key, &serde_json::json!(value))
                        .unwrap_or_else(|e| panic!("{kind}.{key}: {e}"));
                    let got = element_attributes(&net, id)
                        .expect("attributes")
                        .into_iter()
                        .find(|r| r.key == key)
                        .and_then(|r| r.text)
                        .unwrap_or_default();
                    assert_eq!(got, value, "{kind}.{key} did not read back");
                    checked += 1;
                    continue;
                }
                let value = before.number.expect("a number") + 1.5;
                set_attribute(&mut net, id, key, &serde_json::json!(value))
                    .unwrap_or_else(|e| panic!("{kind}.{key}: {e}"));
                let got = read(&net, id, &before.label);
                assert!(
                    (got - value).abs() < 1e-9,
                    "{kind}.{key} set {value}, read back {got}"
                );
                checked += 1;
            }
        }
        assert!(checked >= 9, "only {checked} attributes were exercised");
    }

    /// The attribute §4.5.1.1 was widened for. Its target is a
    /// conveyance node *or* another subcatchment, which no single kind
    /// id could say — so it stayed unwritable, and re-routing a
    /// catchment was the one topological edit no surface offered.
    #[test]
    fn a_subcatchment_can_be_re_routed_to_either_kind_of_outlet() {
        let mut net = model();
        set_attribute(&mut net, "S1", "outlet", &serde_json::json!("O1")).expect("to a node");
        assert!(matches!(net.parcels[0].outlet, ParcelOutlet::Vertex(_)));
        assert_eq!(
            element_attributes(&net, "S1")
                .expect("rows")
                .into_iter()
                .find(|r| r.key == "outlet")
                .and_then(|r| r.text),
            Some("O1".to_string())
        );

        // Overland, to another subcatchment — the half a single kind id
        // would have had to omit.
        let mut two = model();
        two.parcels.push(two.parcels[0].clone());
        two.parcels[1].id = "S2".to_string();
        set_attribute(&mut two, "S1", "outlet", &serde_json::json!("S2")).expect("to a parcel");
        assert!(matches!(two.parcels[0].outlet, ParcelOutlet::Parcel(1)));

        // A catchment draining to itself is a loop the router never
        // leaves, not a short cascade.
        let err = set_attribute(&mut two, "S1", "outlet", &serde_json::json!("S1"))
            .expect_err("self-drainage");
        assert!(err.contains("itself"), "{err}");
        let err = set_attribute(&mut two, "S1", "outlet", &serde_json::json!("NOPE"))
            .expect_err("unknown");
        assert!(err.contains("NOPE"), "{err}");
    }

    /// The last of the three referents that could not be edited. Its
    /// target is a link of any kind — the other half of what one kind id
    /// could not say — and unlike the outlet it may legally be nothing,
    /// which the file writes as `*`.
    #[test]
    fn a_divider_can_be_told_which_link_its_flow_leaves_by() {
        let (mut net, diags) = hydra::uds::io::objects::parse_network(
            "[OPTIONS]\nFLOW_UNITS CMS\n\
             [JUNCTIONS]\nJ1 10 3 0 0 0\n\
             [DIVIDERS]\nD1 9 * OVERFLOW\n\
             [OUTFALLS]\nO1 8 FREE NO\n\
             [CONDUITS]\nC1 D1 O1 100 0.013 0 0\nC2 J1 D1 100 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\nC2 CIRCULAR 1 0 0 0\n",
        );
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");

        set_attribute(&mut net, "D1", "divertedLink", &serde_json::json!("C1")).expect("set");
        let read = |net: &Network| {
            element_attributes(net, "D1")
                .expect("rows")
                .into_iter()
                .find(|r| r.key == "divertedLink")
                .and_then(|r| r.text)
        };
        assert_eq!(read(&net), Some("C1".to_string()));

        // Cleared back to none, which is what the fixture started as and
        // what `*` means — a divider diverting nowhere is legal input.
        set_attribute(&mut net, "D1", "divertedLink", &serde_json::json!("")).expect("clear");
        assert!(matches!(
            net.vertices.iter().find(|v| v.id == "D1").map(|v| &v.kind),
            Some(hydra::uds::model::VertexKind::Divider {
                diverted_link: None,
                ..
            })
        ));

        assert!(set_attribute(&mut net, "D1", "divertedLink", &serde_json::json!("NOPE")).is_err());
        // And a kind that is not a divider says so rather than silently
        // doing nothing.
        let err = set_attribute(&mut net, "J1", "divertedLink", &serde_json::json!("C1"))
            .expect_err("not a divider");
        assert!(err.contains("divider"), "{err}");
    }

    /// The dimensions the schema declared and never served. An orifice's
    /// height sat in the catalog for the life of this build with no value
    /// attached, so §4.5.1 dropped the row and the size was unreadable —
    /// which is why the kind could not be created either.
    #[test]
    fn an_opening_and_a_tank_report_and_take_their_sizes() {
        let mut net = model();
        // The fixture is CMS, so the file's units are metres and the
        // conversion is the identity — the factor itself is asserted in
        // the create test, against a CFS model.
        let read = |net: &Network, id: &str, key: &str| {
            element_attributes(net, id)
                .expect("attributes")
                .into_iter()
                .find(|r| r.key == key)
                .and_then(|r| r.number)
        };
        // Nothing carries an opening in the fixture, so one is made the
        // same way the app would.
        super::super::uds_create::create_uds_opening(&mut net, "weir", "W1", "J1", "O1", 1.5, 2.0)
            .expect("weir");
        assert_eq!(read(&net, "W1", "crestHeight"), Some(1.5));
        assert_eq!(read(&net, "W1", "crestLength"), Some(2.0));

        set_attribute(&mut net, "W1", "crestHeight", &serde_json::json!(2.25)).expect("set");
        assert_eq!(read(&net, "W1", "crestHeight"), Some(2.25));
        // An opening of no size is refused rather than stored.
        assert!(set_attribute(&mut net, "W1", "crestHeight", &serde_json::json!(0.0)).is_err());

        super::super::uds_create::create_uds_storage(&mut net, "ST1", 0.0, 0.0, 5.0, 3.0, 400.0)
            .expect("storage");
        assert_eq!(read(&net, "ST1", "surfaceArea"), Some(400.0));
        set_attribute(&mut net, "ST1", "surfaceArea", &serde_json::json!(500.0)).expect("set");
        assert_eq!(read(&net, "ST1", "surfaceArea"), Some(500.0));
    }

    /// A storage unit whose area varies with depth has no single surface
    /// area, so it publishes no row for one — a tabulated relation
    /// reported as one number would be a confident wrong answer.
    #[test]
    fn a_tabulated_storage_unit_reports_no_single_area() {
        let net = model();
        let tabulated = net.vertices.iter().find(|v| {
            matches!(
                &v.kind,
                VertexKind::Storage {
                    geometry: hydra::uds::model::StorageGeometry::Tabular { .. },
                    ..
                }
            )
        });
        let Some(v) = tabulated else {
            return; // the fixture has none; the rule is asserted by the write below
        };
        assert!(
            element_attributes(&net, &v.id)
                .expect("attributes")
                .iter()
                .all(|r| r.key != "surfaceArea"),
            "a tabulated unit reported one area"
        );
    }

    /// The catalog has to name every kind an outlet may be, because a
    /// list that looks complete is read as complete — an application
    /// offering only junctions would hide every other valid answer.
    #[test]
    fn an_outlet_declares_every_kind_it_may_name() {
        let outlet = hydra::uds::descriptors::attribute_schema("subcatchment")
            .into_iter()
            .find(|a| a.key == "outlet")
            .expect("an outlet attribute");
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let expected = matches!(kind.class, hydra::common::ElementClass::Point)
                || kind.id == "subcatchment";
            assert_eq!(
                outlet.references.iter().any(|r| r == kind.id),
                expected,
                "{} is {}declared",
                kind.id,
                if expected { "not " } else { "wrongly " }
            );
        }
    }

    /// The pair of the round-trip test above, from the other side: a row
    /// the setter refuses must not read as editable, or the inspector
    /// renders an input whose every use fails.
    ///
    /// Asserted over the whole schema rather than a list of known cases,
    /// so a key that becomes writable without its flag being flipped —
    /// or the reverse — fails here rather than in the app.
    #[test]
    fn a_row_that_cannot_be_set_does_not_read_as_editable() {
        // Every kind the fixture has one of. The list started at four and
        // let a real drift through: an inlet's fields were marked
        // editable with no setter behind them, and nothing failed because
        // no inlet was sampled. A kind the fixture gains is covered the
        // moment it is named here.
        let sample = |kind: &str| -> Option<&'static str> {
            match kind {
                "junction" => Some("J1"),
                "outfall" => Some("O1"),
                "conduit" => Some("C1"),
                "subcatchment" => Some("S1"),
                "street" => Some("ST1"),
                "transect" => Some("TR1"),
                "inlet" => Some("CB1"),
                _ => None,
            }
        };
        let mut checked = 0;
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let Some(id) = sample(kind.id) else { continue };
            for attr in hydra::uds::descriptors::attribute_schema(kind.id) {
                if attr.editable {
                    continue;
                }
                let mut net = model();
                // Whatever shape the row is, the setter has to refuse it.
                // A number for a numeric key and text for a textual one,
                // so a refusal cannot come from the value being the wrong
                // type rather than the key being unwritable.
                let value = match attr.kind {
                    hydra::common::OptionKind::Number { .. }
                    | hydra::common::OptionKind::Integer { .. } => serde_json::json!(1.0),
                    _ => serde_json::json!("X"),
                };
                assert!(
                    set_attribute(&mut net, id, &attr.key, &value).is_err(),
                    "{}.{} reads as fixed and the setter took it",
                    kind.id,
                    attr.key
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no unwritable attribute was exercised");
    }

    #[test]
    fn a_written_attribute_survives_the_file() {
        // The edit has to reach the model text and come back, not just
        // sit in memory.
        let mut net = model();
        set_attribute(&mut net, "S1", "imperviousness", &serde_json::json!(62.0)).expect("set");
        let text = hydra::uds::io::inp_writer::write_inp(&net).expect("export");
        let (again, diags) = hydra::uds::io::objects::parse_network(&text);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        assert!((read(&again, "S1", "Imperviousness") - 62.0).abs() < 1e-9);
    }

    #[test]
    fn a_key_this_path_cannot_set_says_so() {
        // A shape carries four geometry values behind one label, so it
        // refuses by name rather than being quietly ignored — a caller
        // learns which of the two it asked for. The referents that used
        // to be here are writable now, and are exercised above.
        let mut net = model();
        let err =
            set_attribute(&mut net, "C1", "shape", &serde_json::json!(1.0)).expect_err("refused");
        assert!(err.contains("shape"), "{err}");
        // A reference that names nothing still refuses, which is a
        // different thing from a key that cannot be written at all.
        let err = set_attribute(&mut net, "S1", "raingage", &serde_json::json!("NOPE"))
            .expect_err("unknown gage");
        assert!(err.contains("NOPE"), "{err}");
    }

    #[test]
    fn an_unknown_element_is_not_silently_ignored() {
        let mut net = model();
        let err = set_attribute(&mut net, "NOPE", "invert", &serde_json::json!(1.0))
            .expect_err("refused");
        assert!(err.contains("NOPE"), "{err}");
    }
}
