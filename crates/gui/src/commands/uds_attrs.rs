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
    LinkKind, Network, OutfallStage, OutletRating, StorageGeometry, VertexKind,
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
    use AttrValue::{Number, Text};

    if let Some(v) = net.vertices.iter().find(|v| v.id == element_id) {
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
        return Some((kind, m));
    }

    let l = net.links.iter().find(|l| l.id == element_id)?;
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
    Some((kind, m))
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
}
