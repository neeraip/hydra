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

/// One Properties row: engine-authored label, plus either a numeric SI
/// value with its quantity descriptor or a display text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementAttributeDto {
    pub label: String,
    /// Numeric value in SI units; interpret via `quantity`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    /// Display text for non-numeric attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The §5 quantity descriptor for `number`, absent for unitless values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
}

/// A schema key's extracted value.
enum AttrValue {
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
    if let Some(p) = net.parcels.iter().find(|p| p.id == element_id) {
        return Some(("subcatchment", parcel_values(net, p)));
    }
    if let Some(v) = net.vertices.iter().find(|v| v.id == element_id) {
        return Some(vertex_values(net, v));
    }
    let l = net.links.iter().find(|l| l.id == element_id)?;
    Some(link_values(net, l))
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
            "orifice"
        }
        LinkKind::Weir {
            discharge_coeff, ..
        } => {
            m.insert("dischargeCoeff", Number(*discharge_coeff));
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
            m.insert("gated", yes_no(*flap_gate));
            "outlet"
        }
    };
    (kind, m)
}

/// Properties rows for one element: the §4 schema's rows, in schema order,
/// with values the model actually carries.
pub fn element_attributes(net: &Network, element_id: &str) -> Option<Vec<ElementAttributeDto>> {
    let (kind, mut values) = extract(net, element_id)?;
    let rows = hydra::uds::descriptors::attribute_schema(kind)
        .into_iter()
        .filter_map(|attr| {
            let value = values.remove(attr.key.as_str())?;
            let quantity = attr.quantity.as_deref().and_then(|key| {
                hydra::uds::descriptors::QUANTITIES
                    .iter()
                    .find(|q| q.key == key)
                    .copied()
            });
            Some(match value {
                AttrValue::Number(n) => ElementAttributeDto {
                    label: attr.label,
                    number: Some(n),
                    text: None,
                    quantity,
                },
                AttrValue::Text(t) => ElementAttributeDto {
                    label: attr.label,
                    number: None,
                    text: Some(t),
                    quantity: None,
                },
            })
        })
        .collect();
    Some(rows)
}

/// One column of a kind's property table: an engine-declared attribute
/// (§4.3) with every element's value for it, parallel to `ids`.
///
/// Columnar rather than row-major because a table is read by column — and
/// because one array per attribute stays compact where a per-row object
/// would repeat every key thousands of times.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindColumnDto {
    pub key: String,
    pub label: String,
    /// The §5 quantity for numeric values (SI), absent for text columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
    /// One entry per id: a number for numeric attributes, a string for
    /// textual ones, `null` where the element does not carry the
    /// attribute at all.
    pub values: Vec<serde_json::Value>,
}

/// Every element of one kind, with its §4.3 attribute columns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindElementsDto {
    /// Element ids, in model order — the row order every column follows.
    pub ids: Vec<String>,
    pub columns: Vec<KindColumnDto>,
}

/// The elements of one kind with their declared properties.
///
/// The per-element command answers "what is this thing?"; a table needs
/// "what are all of these?", and asking element by element would be one
/// IPC round trip per row. Empty for engines whose attributes reach the
/// frontend by another route — wds carries its own in the network
/// snapshot — and for a kind the model has none of.
/// One §4.3 attribute of an element kind, without any element's values.
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
}

#[tauri::command]
/// The declared attribute schema of one element kind (spec §4.3): every
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
            key: attr.key,
            label: attr.label,
            quantity: attr
                .quantity
                .as_deref()
                .and_then(super::uds_results::quantity_descriptor),
        })
        .collect()
}

#[tauri::command(async)]
pub fn get_kind_elements(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    kind: String,
) -> Result<KindElementsDto, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let empty = KindElementsDto {
        ids: Vec::new(),
        columns: Vec::new(),
    };
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Ok(empty);
    }
    let net = uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    Ok(kind_elements(&net, &kind))
}

/// Build one kind's table: ids in model order, and one column per §4.3
/// attribute the schema declares, in schema order.
pub fn kind_elements(net: &Network, kind: &str) -> KindElementsDto {
    // One pass over the elements of this kind, rather than a lookup per id.
    let mut rows: Vec<(String, HashMap<&'static str, AttrValue>)> = Vec::new();
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
                key: attr.key,
                label: attr.label,
                quantity,
                values,
            }
        })
        .collect();
    KindElementsDto { ids, columns }
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
            })
        })
        .collect())
}

/// Engine-described attribute rows for one element of the target's model.
/// `Ok(None)` for engines that serve their attributes elsewhere (wds) or
/// for an unknown element id.
#[tauri::command(async)]
pub fn get_element_details(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    element_id: String,
) -> Result<Option<Vec<ElementAttributeDto>>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Ok(None);
    }
    let network = uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    Ok(element_attributes(&network, &element_id))
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
        // Schema order: invert, maxDepth, initDepth.
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["Invert elevation", "Maximum depth", "Initial depth"],
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
            vec!["Invert elevation", "Maximum depth", "Initial depth"],
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
