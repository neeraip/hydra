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

/// Where a new element goes: a position, two ends, or nowhere.
enum Placement {
    At(f64, f64),
    Between(String, String),
    /// A collection is not anywhere. It is named and it has contents,
    /// and both of those are edited afterwards (§4.5.2.2).
    Nowhere,
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
        ElementClass::Collection => Ok(Placement::Nowhere),
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
                Placement::At(x, y) if class == ElementClass::Region => {
                    // Its gage and its outlet go in up front: the model
                    // holds both as indices and there is no value meaning
                    // "not yet chosen", so a parcel exists pointing at
                    // something or it does not exist at all.
                    super::uds_create::create_uds_parcel(
                        &mut draft,
                        &id,
                        *x,
                        *y,
                        &required_text(&element, "raingage")?,
                        &required_text(&element, "outlet")?,
                    )?;
                }
                Placement::At(x, y) if element.kind == "storage" => {
                    // A prismatic tank: the one storage shape a pair of
                    // numbers can describe, and both are the caller's
                    // because neither a depth nor an area has a
                    // conventional value.
                    super::uds_create::create_uds_storage(
                        &mut draft,
                        &id,
                        *x,
                        *y,
                        0.0,
                        required(&element, "maxDepth")?,
                        required(&element, "surfaceArea")?,
                    )?;
                }
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
                Placement::Nowhere => {
                    super::uds_create::create_uds_container(&mut draft, &element.kind, &id)?;
                }
                Placement::Between(from, to) if element.kind != "conduit" => {
                    // An opening rather than a channel. Its two
                    // dimensions come from the caller, because neither an
                    // orifice's height nor a weir's crest has the
                    // conventional value a bore does — while the
                    // discharge coefficients, which do, are the engine's.
                    let (height, width) = opening_keys(&element.kind);
                    super::uds_create::create_uds_opening(
                        &mut draft,
                        &element.kind,
                        &id,
                        from,
                        to,
                        required(&element, height)?,
                        required(&element, width)?,
                    )?;
                }
                Placement::Between(from, to) => super::uds_create::create_uds_link(
                    &mut draft,
                    &element.kind,
                    &id,
                    from,
                    to,
                    length_of(&element)?,
                    // Optional: a cross-section is more than a bore, and
                    // the engine's default is the honest answer until
                    // the rest of one can be edited too.
                    optional(&element, "diameter"),
                )?,
            }
            let consumed: &[&str] = match &where_ {
                Placement::At(_, _) if class == ElementClass::Region => &["raingage", "outlet"],
                Placement::At(_, _) if element.kind == "storage" => &["maxDepth", "surfaceArea"],
                Placement::At(_, _) | Placement::Nowhere => &[],
                Placement::Between(_, _) if element.kind == "orifice" => &["height", "width"],
                Placement::Between(_, _) if element.kind == "weir" => {
                    &["crestHeight", "crestLength"]
                }
                Placement::Between(_, _) => &["length", "diameter"],
            };
            apply_fields(&element, consumed, |key, value| {
                super::uds_attrs::set_attribute(&mut draft, &id, key, value)
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
                Placement::Nowhere => {
                    super::mutations::create_container_in_network(&mut draft, &element.kind, &id)?;
                }
            }
            apply_fields(&element, &[], |key, value| {
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
///
/// `consumed` names the fields the engine's constructor already took, so
/// they are not written a second time. Some of them are not attributes
/// at all: a drainage conduit's diameter reaches its cross-section and
/// appears in no schema, so writing it back was refused — and refused
/// the whole create with it, which is why the drainage Add dialog could
/// not make a conduit.
fn apply_fields(
    element: &NewElement,
    consumed: &[&str],
    mut write: impl FnMut(&str, &serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let mut keys: Vec<&String> = element
        .fields
        .keys()
        .filter(|k| !consumed.contains(&k.as_str()))
        .collect();
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

/// A supplied value the engine may default, or `None`.
fn optional(element: &NewElement, key: &str) -> Option<f64> {
    element
        .fields
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
}

/// The two keys a kind calls its opening's dimensions by. They differ
/// because the things differ: an orifice has a height and a width, a
/// weir a crest height and a crest length.
fn opening_keys(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "weir" => ("crestHeight", "crestLength"),
        _ => ("height", "width"),
    }
}

/// A supplied name the constructor needs, which no default can stand in
/// for — a reference to an element that has to already exist.
fn required_text(element: &NewElement, key: &str) -> Result<String, String> {
    element
        .fields
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("a {} needs its {key}", element.kind))
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
        let err = creatable_class("uds", "outlet").expect_err("should refuse");
        assert!(err.contains("defensible"), "unhelpful: {err}");
        // The two engines answer differently for the same kind, and the
        // difference is the data model's rather than the editor's: a
        // water-distribution curve's purpose is inferred from what
        // references it, so a new one needs nothing said; a drainage
        // curve declares a role that decides what units its columns are
        // read in, and there is no defensible one.
        let err = creatable_class("uds", "curve").expect_err("should refuse");
        assert!(err.contains("units"), "unhelpful: {err}");
        assert!(creatable_class("wds", "curve").is_ok());
        assert!(creatable_class("uds", "junction").is_ok());
    }

    /// A container is created by naming it and nothing else — no
    /// position, no ends. Its contents are the point of it, and those
    /// are edited afterwards (§4.5.2.2), which is what makes creating
    /// one possible at all: before the contents could be edited, a new
    /// curve was a thing you could make and never finish.
    #[test]
    fn a_container_is_created_by_name_alone_and_starts_complete() {
        let mut network =
            hydra::io::parse(crate::commands::test_fixtures::TEST_INP.as_bytes()).expect("fixture");
        assert!(matches!(
            placement(&new("curve", "C9"), ElementClass::Collection),
            Ok(Placement::Nowhere)
        ));

        super::super::mutations::create_container_in_network(&mut network, "curve", "C9")
            .expect("curve");
        let curve = network.curves.iter().find(|c| c.id == "C9").expect("C9");
        // Generic, not pump-head: nothing references it, so nothing has
        // said what its numbers are, and generic is the one kind that
        // imposes no unit on them.
        assert_eq!(curve.kind, hydra::CurveKind::Generic);
        // Two points, ascending — the shape the contents write demands,
        // so the thing that was just made can be edited without first
        // being repaired.
        assert_eq!(curve.points.len(), 2);
        assert!(curve.points[1].x > curve.points[0].x);

        super::super::mutations::create_container_in_network(&mut network, "pattern", "PX")
            .expect("pattern");
        let pattern = network.patterns.iter().find(|p| p.id == "PX").expect("PX");
        assert_eq!(pattern.factors, vec![1.0; 24]);

        // And a second one of the same name is refused rather than
        // shadowing the first.
        assert!(
            super::super::mutations::create_container_in_network(&mut network, "curve", "C9")
                .is_err()
        );
    }

    /// Every kind the catalog says can be created has a constructor
    /// behind it. The two are set in different files, and a flag flipped
    /// without its arm gives a button that refuses with "no constructor
    /// for" — which reads as a bug rather than as a limit.
    #[test]
    fn every_creatable_kind_can_actually_be_created() {
        // Not an end-to-end create: this asserts the pair exists, and
        // each constructor's own test asserts what it builds.
        for (engine, catalog) in [
            ("wds", hydra::descriptors::ELEMENT_KINDS),
            ("uds", hydra::uds::descriptors::ELEMENT_KINDS),
        ] {
            for kind in catalog.iter().filter(|k| k.creatable) {
                let class = creatable_class(engine, kind.id)
                    .unwrap_or_else(|e| panic!("{engine}.{} is creatable but: {e}", kind.id));
                assert_eq!(class, kind.class);
                // And a placement is expressible for it, which is what
                // the create asks for before it builds anything.
                let mut element = new(kind.id, "X");
                element.position = Some([0.0, 0.0]);
                element.from_id = Some("A".into());
                element.to_id = Some("B".into());
                placement(&element, class)
                    .unwrap_or_else(|e| panic!("{engine}.{} cannot be placed: {e}", kind.id));
            }
        }
    }

    /// A refusal says what a new one would *lack*, never where to go
    /// instead.
    ///
    /// Five of them named the curve, pattern and controls editors. Those
    /// screens were deleted when their work moved onto the editing
    /// contract, and the sentences kept pointing at them — the same
    /// staleness as any other copy naming a screen, except this copy is
    /// engine data and outlives whatever application read it.
    #[test]
    fn no_refusal_sends_the_reader_to_a_screen() {
        let catalogs: [&[hydra::common::ElementKind]; 2] = [
            hydra::descriptors::ELEMENT_KINDS,
            hydra::uds::descriptors::ELEMENT_KINDS,
        ];
        for kinds in catalogs {
            for kind in kinds {
                let Some(why) = kind.not_creatable_because else {
                    continue;
                };
                assert!(
                    !why.contains("editor"),
                    "{}'s refusal names a screen: {why:?}",
                    kind.id
                );
            }
        }
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
        apply_fields(&element, &[], |key, value| {
            super::super::uds_attrs::set_attribute(&mut net, "J2", key, value)
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
        let err = apply_fields(&element, &[], |key, value| {
            super::super::uds_attrs::set_attribute(&mut net, "J2", key, value)
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
        apply_fields(&element, &[], |key, _| {
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
        // A diameter, unlike a length, is not asked of the caller: a
        // cross-section is more than a bore, so the engine's own default
        // stands until the whole of one can be edited. Absent and zero
        // both mean "nothing was said" here, and the engine decides.
        assert_eq!(optional(&element, "diameter"), None);
        element
            .fields
            .insert("diameter".into(), serde_json::json!(0.0));
        assert_eq!(optional(&element, "diameter"), None, "zero is not a size");
        element
            .fields
            .insert("diameter".into(), serde_json::json!(0.45));
        assert_eq!(optional(&element, "diameter"), Some(0.45));
    }

    /// The whole create, as the dialog sends it — which is the only way
    /// this defect shows. `length` and `diameter` reach the engine's
    /// constructor because a conduit cannot be built without them, and
    /// they were then applied a second time as attribute writes. A
    /// conduit publishes no `diameter` attribute, so the second pass
    /// refused it and took the create down with it: the drainage Add
    /// dialog could not make a conduit at all.
    #[test]
    fn a_conduit_is_created_from_the_fields_the_dialog_sends() {
        let mut net = uds_model();
        let mut element = new("conduit", "C2");
        element.from_id = Some("J1".into());
        element.to_id = Some("O1".into());
        element
            .fields
            .insert("length".into(), serde_json::json!(55.0));
        element
            .fields
            .insert("diameter".into(), serde_json::json!(0.45));

        let where_ = placement(&element, ElementClass::Polyline).expect("ends");
        let Placement::Between(from, to) = &where_ else {
            panic!("a polyline is placed between its ends");
        };
        super::super::uds_create::create_uds_link(
            &mut net,
            &element.kind,
            &element.id,
            from,
            to,
            length_of(&element).expect("a length"),
            optional(&element, "diameter"),
        )
        .expect("create");
        apply_fields(&element, &["length", "diameter"], |key, value| {
            super::super::uds_attrs::set_attribute(&mut net, &element.id, key, value)
        })
        .expect("the fields the constructor already consumed must not be rewritten");

        let link = net.links.iter().find(|l| l.id == "C2").expect("C2");
        let hydra::uds::model::LinkKind::Channel { length, .. } = link.kind else {
            panic!("not a channel");
        };
        assert!((length - 55.0).abs() < 1e-9);
    }

    /// The other engine's whole create, for the kind whose fields are
    /// most entangled: a tank's published elevation is its bottom while
    /// the model stores bottom plus minimum level, and both its
    /// elevation and its minimum level move that stored value. The
    /// fields arrive in sorted order, so elevation is written before
    /// minLevel — and the result has to be the tank the dialog
    /// described either way.
    #[test]
    fn a_tank_is_created_from_the_fields_the_dialog_sends() {
        let mut net =
            hydra::io::parse(crate::commands::test_fixtures::TEST_INP.as_bytes()).expect("parse");
        let mut element = new("tank", "T2");
        element.position = Some([5.0, 6.0]);
        for (key, value) in [
            ("elevation", 40.0),
            ("minLevel", 2.0),
            ("maxLevel", 12.0),
            ("initLevel", 4.0),
            ("diameter", 20_000.0),
        ] {
            element.fields.insert(key.into(), serde_json::json!(value));
        }

        super::super::mutations::create_node_in_network(
            &mut net, "tank", "T2", 5.0, 6.0, None, None, None, None,
        )
        .expect("create");
        apply_fields(&element, &[], |key, value| {
            super::super::wds_attrs::set_attribute(&mut net, "T2", key, value)
        })
        .expect("fields");

        let rows = super::super::wds_attrs::element_attributes(&net, "T2").expect("rows");
        let read = |key: &str| {
            rows.iter()
                .find(|r| r.key == key)
                .and_then(|r| r.number)
                .unwrap_or_else(|| panic!("no {key}"))
        };
        assert!((read("elevation") - 40.0).abs() < 1e-9, "bottom moved");
        assert!((read("minLevel") - 2.0).abs() < 1e-9);
        assert!((read("maxLevel") - 12.0).abs() < 1e-9);
        assert!((read("diameter") - 20_000.0).abs() < 1e-6);
        // And the stored value is the bottom plus the minimum, which is
        // the invariant the two setters have to preserve between them.
        let stored = net
            .nodes
            .iter()
            .find(|n| n.base.id == "T2")
            .map(|n| n.base.elevation)
            .expect("T2");
        assert!((stored - 42.0).abs() < 1e-9, "stored elevation is {stored}");
    }
}
