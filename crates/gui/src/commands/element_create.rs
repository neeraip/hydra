//! Adding an element, for whichever engine holds the model.
//!
//! The §4.5.3 shape: an identifier, a position if the kind's class is
//! spatial, the two ends if it is a polyline, and values for its editable
//! attributes. Everything else is the engine's default, because defaults
//! are the engine's judgement — a zero maximum depth that means "raise it
//! to the crown of the highest connecting conduit" is sensible in one
//! engine and meaningless in another.
//!
//! **The supplied values go in through the same write an edit does.** A
//! create that parsed and converted its own fields would be a second
//! implementation of every unit rule, and the unit rules are where the
//! mistakes live: a diameter is millimetres in one engine, a tank's
//! elevation is its bottom rather than what the model stores, an area is
//! hectares. Creating a default element and then editing it into shape
//! reuses all of that, and refuses in the same words.
//!
//! **It is atomic.** The engine mutation wrappers work in place, so a
//! create that failed on its third field would leave an element behind
//! that nobody asked for and no error mentioned. Everything happens on a
//! draft, which replaces the model only once all of it has succeeded.
//! That is the same guarantee removal makes (§4.5.4), for the same
//! reason: the third outcome — partly done, reported as done — is the
//! one worth making unrepresentable.

use std::collections::HashMap;

use hydra::common::ElementClass;
use serde::Deserialize;

use super::network_dto::NetworkState;
use super::projects::{app_data_dir, project_engine_key, validate_target_ids};

/// What an application supplies to create one element.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewElement {
    /// The engine's kind id, from its catalog (§4.2).
    pub kind: String,
    pub id: String,
    /// Where to put it, in the model's own coordinate system. Required
    /// for a `point` or a `region`, meaningless for anything else.
    pub position: Option<[f64; 2]>,
    /// The two ends, for a `polyline`.
    pub from_id: Option<String>,
    pub to_id: Option<String>,
    /// Values for the kind's editable attributes, by schema key. Any the
    /// caller omits keep the engine's default.
    #[serde(default)]
    pub fields: HashMap<String, serde_json::Value>,
}

/// The catalog entry for a kind, and what it says about creating one.
fn creatable_class(engine: &str, kind: &str) -> Result<ElementClass, String> {
    let catalog: &[hydra::common::ElementKind] = match engine {
        "wds" => hydra::descriptors::ELEMENT_KINDS,
        "uds" => hydra::uds::descriptors::ELEMENT_KINDS,
        other => return Err(format!("no editing surface for engine '{other}'")),
    };
    let descriptor = catalog
        .iter()
        .find(|k| k.id == kind)
        .ok_or_else(|| format!("unknown element kind '{kind}'"))?;
    if !descriptor.creatable {
        // The engine's own words, not ours: a kind that needs a rating
        // curve says so, and an application that invented its own
        // explanation would be guessing about a model it cannot read.
        return Err(descriptor.not_creatable_because.map_or_else(
            || format!("a {} cannot be added here", descriptor.label.to_lowercase()),
            str::to_string,
        ));
    }
    Ok(descriptor.class)
}

/// The position a spatial kind needs, or the two ends a polyline does.
enum Placement {
    At(f64, f64),
    Between(String, String),
}

fn placement(new: &NewElement, class: ElementClass) -> Result<Placement, String> {
    match class {
        ElementClass::Point | ElementClass::Region => new
            .position
            .map(|[x, y]| Placement::At(x, y))
            .ok_or_else(|| format!("a {} has to be put somewhere", new.kind)),
        ElementClass::Polyline => match (&new.from_id, &new.to_id) {
            (Some(from), Some(to)) => Ok(Placement::Between(from.clone(), to.clone())),
            _ => Err(format!("a {} needs two ends", new.kind)),
        },
        // A collection is not anywhere, and the kinds that are
        // collections are refused as uncreatable before this is reached.
        ElementClass::Collection => Err(format!("a {} is not placed on the map", new.kind)),
    }
}

#[tauri::command(async)]
/// Add one element to the loaded model (§4.5.3).
pub fn create_element(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    element: NewElement,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    let engine = project_engine_key(&app_data, &project_id);
    let class = creatable_class(&engine, &element.kind)?;
    let where_ = placement(&element, class)?;
    let id = super::mutations::validate_element_id(&element.id)?;

    match engine.as_str() {
        "uds" => super::mutations::mutate_uds(&app, &state, |network| {
            let mut draft = network.clone();
            match &where_ {
                Placement::At(x, y) => super::uds_create::create_uds_vertex(
                    &mut draft,
                    &element.kind,
                    &id,
                    *x,
                    *y,
                    // The invert is an ordinary editable attribute and
                    // arrives with the rest; zero is the engine's own
                    // default for a vertex nobody has surveyed.
                    0.0,
                )?,
                Placement::Between(from, to) => super::uds_create::create_uds_link(
                    &mut draft,
                    &element.kind,
                    &id,
                    from,
                    to,
                    length_of(&element)?,
                    diameter_of(&element)?,
                )?,
            }
            apply_fields(&element, |key, value| {
                let number = value
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("'{key}' takes a number"))?;
                super::uds_attrs::set_attribute(&mut draft, &id, key, number)
            })?;
            *network = draft;
            Ok(())
        }),
        "wds" => super::mutations::mutate_wds(&app, &state, |network| {
            let mut draft = network.clone();
            match &where_ {
                Placement::At(x, y) => super::mutations::create_node_in_network(
                    &mut draft,
                    &element.kind,
                    &id,
                    *x,
                    *y,
                    None,
                    None,
                    None,
                    None,
                )?,
                Placement::Between(from, to) => super::mutations::create_link_in_network(
                    &mut draft,
                    &element.kind,
                    &id,
                    from,
                    to,
                )?,
            }
            apply_fields(&element, |key, value| {
                super::wds_attrs::set_attribute(&mut draft, &id, key, value)
            })?;
            *network = draft;
            Ok(())
        }),
        other => Err(format!("no editing surface for engine '{other}'")),
    }
}

/// Apply the supplied fields in a stable order.
///
/// Sorted by key, so a create that refuses one of them refuses the same
/// one every time — a map's iteration order is not a promise, and an
/// error that moved between runs would be read as flakiness.
fn apply_fields(
    element: &NewElement,
    mut write: impl FnMut(&str, &serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut keys: Vec<&String> = element.fields.keys().collect();
    keys.sort();
    for key in keys {
        write(key, &element.fields[key])?;
    }
    Ok(())
}

/// A drainage conduit's length, which its create takes up front rather
/// than as an attribute — it has no defensible default, unlike an
/// invert.
fn length_of(element: &NewElement) -> Result<f64, String> {
    required(element, "length")
}

/// A drainage conduit's diameter, which reaches the cross-section rather
/// than any attribute the schema publishes.
fn diameter_of(element: &NewElement) -> Result<f64, String> {
    required(element, "diameter")
}

fn required(element: &NewElement, key: &str) -> Result<f64, String> {
    element
        .fields
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| format!("a {} needs a positive {key}", element.kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uds_model() -> hydra::uds::model::Network {
        let (net, _) = hydra::uds::io::objects::parse_network(
            "[OPTIONS]\nFLOW_UNITS CFS\n\
             [JUNCTIONS]\nJ1 100 4\n\
             [OUTFALLS]\nO1 90 FREE NO\n\
             [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
             [COORDINATES]\nJ1 0 0\nO1 100 0\n",
        );
        net
    }

    fn new(kind: &str, id: &str) -> NewElement {
        NewElement {
            kind: kind.to_string(),
            id: id.to_string(),
            position: None,
            from_id: None,
            to_id: None,
            fields: HashMap::new(),
        }
    }

    /// The catalog decides, and its refusal is the engine's own sentence
    /// rather than one written here (§4.5.3).
    #[test]
    fn a_kind_that_cannot_be_created_refuses_in_the_engines_words() {
        let err = creatable_class("uds", "storage").expect_err("should refuse");
        assert!(err.contains("stage-area"), "unhelpful: {err}");
        let err = creatable_class("wds", "curve").expect_err("should refuse");
        assert!(err.contains("curve editor"), "unhelpful: {err}");
        assert!(creatable_class("uds", "junction").is_ok());
    }

    /// The class says what a new one needs: somewhere to be, or two ends.
    /// Asking for the wrong one is refused before anything is built.
    #[test]
    fn a_kind_is_placed_the_way_its_class_says() {
        let mut point = new("junction", "J2");
        assert!(placement(&point, ElementClass::Point).is_err());
        point.position = Some([1.0, 2.0]);
        assert!(matches!(
            placement(&point, ElementClass::Point),
            Ok(Placement::At(_, _))
        ));

        let mut line = new("conduit", "C2");
        line.position = Some([1.0, 2.0]);
        // A position is not two ends: a polyline placed by a coordinate
        // is still missing what it actually needs.
        assert!(placement(&line, ElementClass::Polyline).is_err());
        line.from_id = Some("J1".into());
        line.to_id = Some("O1".into());
        assert!(matches!(
            placement(&line, ElementClass::Polyline),
            Ok(Placement::Between(_, _))
        ));
    }

    /// The supplied values go in through the ordinary write, so a create
    /// converts exactly as an edit does and cannot drift from it.
    #[test]
    fn supplied_fields_are_applied_through_the_write() {
        let mut net = uds_model();
        super::super::uds_create::create_uds_vertex(&mut net, "junction", "J2", 5.0, 6.0, 0.0)
            .expect("create");
        let mut element = new("junction", "J2");
        element
            .fields
            .insert("invert".into(), serde_json::json!(87.5));
        apply_fields(&element, |key, value| {
            let n = value.as_f64().expect("a number");
            super::super::uds_attrs::set_attribute(&mut net, "J2", key, n)
        })
        .expect("fields");

        let rows = super::super::uds_attrs::element_attributes(&net, "J2").expect("rows");
        let invert = rows
            .iter()
            .find(|r| r.key == "invert")
            .and_then(|r| r.number)
            .expect("an invert");
        assert!((invert - 87.5).abs() < 1e-9, "invert came out {invert}");
    }

    /// A field the engine refuses stops the whole create, and the caller
    /// is told which one.
    #[test]
    fn a_refused_field_stops_the_create() {
        let mut net = uds_model();
        super::super::uds_create::create_uds_vertex(&mut net, "junction", "J2", 5.0, 6.0, 0.0)
            .expect("create");
        let mut element = new("junction", "J2");
        element
            .fields
            .insert("invert".into(), serde_json::json!(87.5));
        element
            .fields
            .insert("shape".into(), serde_json::json!(1.0));
        let err = apply_fields(&element, |key, value| {
            let n = value.as_f64().expect("a number");
            super::super::uds_attrs::set_attribute(&mut net, "J2", key, n)
        })
        .expect_err("a junction has no shape");
        assert!(err.contains("shape"), "unhelpful: {err}");
    }

    /// Sorted, so a create with two bad fields names the same one every
    /// run — a map's iteration order is not a promise, and an error that
    /// moved between runs would read as flakiness.
    #[test]
    fn fields_are_applied_in_a_stable_order() {
        let mut element = new("junction", "J2");
        for key in ["zeta", "alpha", "middle"] {
            element.fields.insert(key.into(), serde_json::json!(1.0));
        }
        let mut seen = Vec::new();
        apply_fields(&element, |key, _| {
            seen.push(key.to_string());
            Ok(())
        })
        .expect("all applied");
        assert_eq!(seen, vec!["alpha", "middle", "zeta"]);
    }

    #[test]
    fn a_conduit_without_a_size_is_refused_before_anything_is_built() {
        let mut element = new("conduit", "C2");
        element.from_id = Some("J1".into());
        element.to_id = Some("O1".into());
        assert!(length_of(&element).is_err());
        element
            .fields
            .insert("length".into(), serde_json::json!(50.0));
        assert!(diameter_of(&element).is_err());
        element
            .fields
            .insert("diameter".into(), serde_json::json!(0.0));
        assert!(diameter_of(&element).is_err(), "zero is not a diameter");
    }
}
