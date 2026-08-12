//! Element attributes, for whichever engine holds the model.
//!
//! The neutral half of the element taxonomy contract (hydra-common §4.4,
//! §4.5.1): the row shape an application reads, the schema-driven builder
//! that produces it, and the command that dispatches to an engine. What
//! each engine's values *are* stays with that engine, in `uds_attrs` and
//! `wds_attrs`.
//!
//! This module exists because the two engines had arrived at the same
//! feature by different roads. Drainage served attributes through the
//! contract from the day its viewer shipped. Water distribution served
//! typed columns in a binary snapshot, labelled in the frontend and named
//! by whatever the editor happened to call them — so the two engines'
//! elements could not be shown by one surface, and the difference between
//! two file formats reached the screen as a difference between two
//! editors.

use std::collections::HashMap;

use hydra::common::QuantityDescriptor;
use serde::Serialize;

use super::network_dto::NetworkState;
use super::projects::{app_data_dir, project_engine_key, validate_target_ids};
use super::uds_attrs::AttrValue;

/// One property row of the engine-generic element inspector: an
/// engine-authored label with either a numeric value or display text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementAttributeDto {
    /// The engine's schema key — what a write is addressed by, so a
    /// caller that read a row can write it back.
    pub key: String,
    /// Whether this row can be written (§4.5.1). The engine's own
    /// answer, carried on the attribute descriptor, so a row that offers
    /// an input and a key the write accepts cannot disagree.
    pub editable: bool,
    pub label: String,
    /// Numeric value in the unit this attribute's quantity declares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    /// Display text for non-numeric attributes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The §5 quantity descriptor for `number`, absent for unitless values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
}

/// Build an element's rows from its kind's schema and its own values.
///
/// Schema order, not value order: the schema is what an application drew
/// its rows from before any element was fetched, and a value arriving in
/// a different order would reshuffle the panel under the reader.
///
/// A key the element has no value for produces **no row**. That is the
/// §4.5.1 distinction between an attribute being writable and a given
/// element having something to write — an element that carries no value
/// has nothing to show and nothing to change, and a blank row offering
/// an input would invite creating a value the model never held.
pub(crate) fn rows_from_schema(
    schema: Vec<hydra::common::AttributeDescriptor>,
    mut values: HashMap<&'static str, AttrValue>,
    quantity: impl Fn(&str) -> Option<QuantityDescriptor>,
) -> Vec<ElementAttributeDto> {
    schema
        .into_iter()
        .filter_map(|attr| {
            let value = values.remove(attr.key.as_str())?;
            Some(match value {
                AttrValue::Number(n) => ElementAttributeDto {
                    editable: attr.editable,
                    key: attr.key,
                    label: attr.label,
                    number: Some(n),
                    text: None,
                    quantity: attr.quantity.as_deref().and_then(&quantity),
                },
                AttrValue::Text(t) => ElementAttributeDto {
                    editable: attr.editable,
                    key: attr.key,
                    label: attr.label,
                    number: None,
                    text: Some(t),
                    // A quantity describes a number. A row that is text
                    // carries none even where the schema names one.
                    quantity: None,
                },
            })
        })
        .collect()
}

#[tauri::command(async)]
/// The §4.4 property rows for one element, from whichever engine holds
/// the model.
///
/// `None` for a project whose engine this build cannot open, and for an
/// id no element answers to — a caller cannot tell which, deliberately,
/// because neither is a case it can act on differently.
pub fn get_element_details(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    element_id: String,
) -> Result<Option<Vec<ElementAttributeDto>>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let network = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            Ok(super::uds_attrs::element_attributes(&network, &element_id))
        }
        "wds" => {
            let guard = state.0.lock();
            let Some(network) = guard.wds_network() else {
                return Ok(None);
            };
            Ok(super::wds_attrs::element_attributes(network, &element_id))
        }
        _ => Ok(None),
    }
}
