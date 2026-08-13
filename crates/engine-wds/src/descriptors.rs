//! The engine's published catalogs under the hydra-common element,
//! quantity, and result-variable contracts (hydra-common spec §4–§6).
//!
//! These are presentation-facing projections of the data model (model spec
//! §2): they let an application enumerate, render, and inspect a wds model
//! and its results without wds knowledge. Ids here follow the block-id
//! stability rule — applications persist them in preferences and saved
//! views, so removing or repurposing one is a compatibility break.

use hydra_common::{
    AttributeDescriptor, CategoryItem, CategorySeverity, ElementClass, ElementKind, ElementRole,
    OptionKind, QuantityDescriptor, RampHint, VariableDescriptor,
};

// ── Element kinds (spec §4.2) ─────────────────────────────────────────────────

/// The engine's element kinds, in presentation order.
pub const ELEMENT_KINDS: &[ElementKind] = &[
    ElementKind {
        id: "junction",
        group: Some("Nodes"),
        label: "Junction",
        label_plural: "Junctions",
        class: ElementClass::Point,
        role: Some(ElementRole::Conveyance),
        badge: "J",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "reservoir",
        group: Some("Nodes"),
        label: "Reservoir",
        label_plural: "Reservoirs",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "R",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "tank",
        group: Some("Nodes"),
        label: "Tank",
        label_plural: "Tanks",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "TK",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "pipe",
        group: Some("Links"),
        label: "Pipe",
        label_plural: "Pipes",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Conveyance),
        badge: "P",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "pump",
        group: Some("Links"),
        label: "Pump",
        label_plural: "Pumps",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "PU",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "valve",
        group: Some("Links"),
        label: "Valve",
        label_plural: "Valves",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "V",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "pattern",
        group: Some("Patterns and curves"),
        label: "Pattern",
        label_plural: "Patterns",
        class: ElementClass::Collection,
        role: None,
        badge: "Pa",
        // Creatable since its multipliers became editable: a new one is
        // twenty-four hours of no variation, which is a complete answer
        // rather than a value standing in for one.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "curve",
        group: Some("Patterns and curves"),
        label: "Curve",
        label_plural: "Curves",
        class: ElementClass::Collection,
        role: None,
        badge: "Cv",
        // Creatable since its points became editable. A curve's purpose
        // here is inferred from what references it (model spec §2.3), so
        // a new one nothing points at is generic — and generic is the
        // one kind whose axes impose no unit on its numbers, which is
        // why nothing has to be guessed to make one.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "control",
        group: Some("Controls"),
        label: "Control",
        label_plural: "Controls",
        class: ElementClass::Collection,
        role: None,
        badge: "Ct",
        creatable: false,
        not_creatable_because: Some(
            "a control is a statement about the network, which has to be written out",
        ),
    },
    ElementKind {
        id: "rule",
        group: Some("Controls"),
        label: "Rule",
        label_plural: "Rules",
        class: ElementClass::Collection,
        role: None,
        badge: "Ru",
        creatable: false,
        not_creatable_because: Some(
            "a rule is a statement about the network, which has to be written out",
        ),
    },
];

// ── Quantities (spec §5) ──────────────────────────────────────────────────────

/// The engine's quantity catalog. Conversion factors and precision hints
/// match the display conventions the official applications ship with.
pub const QUANTITIES: &[QuantityDescriptor] = &[
    q("length", "m", "ft", 3.280_84, 1, 1),
    q("elevation", "m", "ft", 3.280_84, 2, 1),
    q("head", "m", "ft", 3.280_84, 2, 1),
    q("diameter", "mm", "in", 0.039_370_1, 0, 2),
    q("flow", "L/s", "gpm", 15.850_323, 2, 1),
    q("demand", "L/s", "gpm", 15.850_323, 2, 1),
    q("velocity", "m/s", "ft/s", 3.280_84, 2, 2),
    q("pressure", "m", "psi", 1.421_970_2, 1, 1),
    q("headloss", "m/km", "ft/kft", 1.0, 2, 1),
    // Identity in both display systems (analysis spec §5): the quality
    // criteria read as mg/L and hours everywhere.
    q("concentration", "mg/L", "mg/L", 1.0, 2, 2),
    q("age", "h", "h", 1.0, 1, 1),
    // ft³, not gallons: an INP expresses every volume in cubic feet on a
    // US model (the importer divides by 35.315 ft³/m³), so a reader in US
    // mode must see the number their own file carries. Gallons made a
    // 5000 ft³ tank read as 37 401 — right, and unrecognisable.
    q("volume", "m³", "ft³", 35.314_667, 1, 0),
    // Unitless but not dimensionless-anonymous: a pump efficiency and a
    // valve's loss ratio are read as percentages, and a reader who is not
    // told so cannot tell 0.85 from 85. Same in both systems.
    q("percent", "%", "%", 1.0, 1, 1),
];

/// Shorthand constructor keeping the table above readable.
const fn q(
    key: &'static str,
    si: &'static str,
    us: &'static str,
    scale: f64,
    si_dec: u8,
    us_dec: u8,
) -> QuantityDescriptor {
    QuantityDescriptor {
        key,
        si_label: si,
        us_label: us,
        si_to_us_scale: scale,
        si_to_us_offset: 0.0,
        si_decimals: si_dec,
        us_decimals: us_dec,
    }
}

// ── Attribute schemas (spec §4.3) ─────────────────────────────────────────────

/// The display attributes of one element kind, or empty for an unknown id —
/// advisory, like a block-options description.
pub fn attribute_schema(kind_id: &str) -> Vec<AttributeDescriptor> {
    let mut schema = own_attributes(kind_id);
    // Last, and after every one of the model's own values: a tag is a
    // note the modeller keeps beside the element, not something the
    // solver reads. Offered for every kind the tag section can name,
    // which is every kind that is an element rather than a container.
    if taggable(kind_id) {
        schema.push(rw("tag", "Tag", text(), None));
    }
    schema
}

/// Whether `[TAGS]` can name this kind — the spatial classes, which for
/// this engine is its nodes and its links. A curve or a pattern is a
/// container the section has no grammar for.
fn taggable(kind_id: &str) -> bool {
    ELEMENT_KINDS.iter().any(|k| {
        k.id == kind_id
            && matches!(
                k.class,
                ElementClass::Point | ElementClass::Polyline | ElementClass::Region
            )
    })
}

/// The attributes a kind has of its own, before the ones every element
/// carries.
fn own_attributes(kind_id: &str) -> Vec<AttributeDescriptor> {
    match kind_id {
        "junction" => vec![
            rw("elevation", "Elevation", num(), Some("elevation")),
            rw("baseDemand", "Base demand", num(), Some("demand")),
            rw("demandPattern", "Demand pattern", text(), None),
        ],
        "reservoir" => vec![
            rw("head", "Head", num(), Some("head")),
            rw("headPattern", "Head pattern", text(), None),
        ],
        "tank" => vec![
            rw("elevation", "Elevation", num(), Some("elevation")),
            rw("initLevel", "Initial level", num(), Some("length")),
            rw("minLevel", "Minimum level", num(), Some("length")),
            rw("maxLevel", "Maximum level", num(), Some("length")),
            rw("diameter", "Diameter", num(), Some("length")),
            rw("minVolume", "Minimum volume", num(), Some("volume")),
            rw("volumeCurve", "Volume curve", text(), None),
            rw(
                "overflow",
                "Overflow",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "pipe" => vec![
            rw("length", "Length", num(), Some("length")),
            rw("diameter", "Diameter", num(), Some("diameter")),
            rw("roughness", "Roughness", num(), None),
            rw("minorLoss", "Minor loss", num(), None),
            rw(
                "checkValve",
                "Check valve",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "pump" => vec![
            rw("headCurve", "Head curve", text(), None),
            rw("power", "Rated power", num(), None),
            rw("speed", "Relative speed", num(), None),
            rw("speedPattern", "Speed pattern", text(), None),
        ],
        "valve" => vec![
            rw(
                "valveType",
                "Type",
                OptionKind::Choice {
                    default: None,
                    items: ["PRV", "PSV", "PBV", "FCV", "TCV", "GPV", "PCV"]
                        .iter()
                        .map(|v| hydra_common::ChoiceItem {
                            value: (*v).to_string(),
                            label: (*v).to_string(),
                        })
                        .collect(),
                },
                None,
            ),
            rw("diameter", "Diameter", num(), Some("diameter")),
            // The setting's unit depends on the valve type (pressure for
            // PRV/PSV/PBV, flow for FCV, dimensionless otherwise), so no
            // single quantity is truthful here.
            rw("setting", "Setting", num(), None),
            rw("minorLoss", "Minor loss", num(), None),
        ],
        _ => Vec::new(),
    }
}

/// A read-only attribute: shown, never offered for editing.
///
/// Unused today — every attribute this engine publishes is a stored
/// model value, so every one is [`rw`]. It exists because the
/// distinction is the schema's to draw and a derived attribute would
/// need it, not because nothing is read-only by accident.
#[allow(dead_code)]
fn attr(key: &str, label: &str, kind: OptionKind, quantity: Option<&str>) -> AttributeDescriptor {
    descriptor(key, label, kind, quantity, false)
}

/// A writable attribute (spec §4.5.1).
fn rw(key: &str, label: &str, kind: OptionKind, quantity: Option<&str>) -> AttributeDescriptor {
    descriptor(key, label, kind, quantity, true)
}

fn descriptor(
    key: &str,
    label: &str,
    kind: OptionKind,
    quantity: Option<&str>,
    editable: bool,
) -> AttributeDescriptor {
    AttributeDescriptor {
        key: key.to_string(),
        label: label.to_string(),
        kind,
        quantity: quantity.map(str::to_string),
        editable,
        references: referenced_kinds(key),
    }
}

/// The kind an attribute names, for the attributes that name one
/// (spec §4.5.1.1).
///
/// Keyed by attribute key rather than declared at each call site
/// because the answer is a property of the key: wherever a schema
/// publishes it, it means the same thing. A key absent from here is not
/// a reference, which is the common case.
fn referenced_kinds(key: &str) -> Vec<String> {
    let one = |kind: &str| vec![kind.to_string()];
    match key {
        "demandPattern" | "headPattern" | "speedPattern" => one("pattern"),
        "volumeCurve" | "headCurve" => one("curve"),
        _ => Vec::new(),
    }
}

fn num() -> OptionKind {
    OptionKind::Number {
        default: None,
        min: None,
        max: None,
    }
}

fn text() -> OptionKind {
    OptionKind::Text { default: None }
}

// ── Result variables (spec §6) ────────────────────────────────────────────────

/// Result variables for an element class, in presentation order. Classes
/// the engine produces no results for yield an empty list.
pub fn result_variables(class: ElementClass) -> Vec<VariableDescriptor> {
    match class {
        ElementClass::Point => vec![
            var(
                "pressure",
                "Pressure",
                "p",
                Some("pressure"),
                RampHint::Banded {
                    criterion: "pressure",
                },
            ),
            var("head", "Head", "H", Some("head"), RampHint::Sequential),
            var(
                "demand",
                "Demand",
                "q",
                Some("demand"),
                RampHint::Sequential,
            ),
            var("quality", "Quality", "C", None, RampHint::Sequential),
        ],
        ElementClass::Polyline => vec![
            var("flow", "Flow", "Q", Some("flow"), RampHint::Diverging),
            var(
                "velocity",
                "Velocity",
                "v",
                Some("velocity"),
                RampHint::Banded {
                    criterion: "velocity",
                },
            ),
            var(
                "headloss",
                "Unit headloss",
                "hf",
                Some("headloss"),
                RampHint::Sequential,
            ),
            var("quality", "Quality", "C", None, RampHint::Sequential),
            VariableDescriptor {
                id: "status",
                label: "Status",
                symbol: Some("St"),
                quantity: None,
                // The codes the binary results format stores (model spec
                // §4.4.4): EPANET's status enumeration.
                // Every code the results writer can emit (`status_to_f32`),
                // not merely the common ones: a consumer that colours by
                // this catalog renders an undeclared code as "no value",
                // so a pump shut down on excess head would vanish from the
                // one view that would have explained it.
                //
                // Severity is a hydraulic judgement, not a display choice:
                // a link carrying no flow in a pressurised network is an
                // abnormal condition, and a valve or pump that cannot meet
                // what was asked of it is worth noticing, whoever is
                // looking.
                ramp: RampHint::Categorical {
                    items: vec![
                        cat(0, "Excess head", CategorySeverity::Alarm),
                        cat(1, "Temporarily closed", CategorySeverity::Alarm),
                        cat(2, "Closed", CategorySeverity::Alarm),
                        cat(3, "Open", CategorySeverity::Nominal),
                        cat(4, "Active", CategorySeverity::Caution),
                        cat(6, "Setpoint not met", CategorySeverity::Caution),
                        cat(7, "Excess pressure", CategorySeverity::Caution),
                    ],
                },
            },
        ],
        ElementClass::Region | ElementClass::Collection => Vec::new(),
    }
}

fn var(
    id: &'static str,
    label: &'static str,
    symbol: &'static str,
    quantity: Option<&'static str>,
    ramp: RampHint,
) -> VariableDescriptor {
    VariableDescriptor {
        id,
        label,
        symbol: Some(symbol),
        quantity,
        ramp,
    }
}

fn cat(value: i64, label: &str, severity: CategorySeverity) -> CategoryItem {
    CategoryItem {
        value,
        label: label.to_string(),
        severity: Some(severity),
    }
}

#[cfg(test)]
mod tests {
    /// A group is a name on kinds already adjacent in the catalog, not
    /// an instruction to gather kinds that are not (§4.2.1) — so an
    /// application draws a heading wherever the group changes and never
    /// reorders. A group appearing twice would draw two headings with
    /// the same words.
    /// A heading that repeats its only entry says nothing twice. "Rain
    /// gages" over Rain gages was a group of one named after its member,
    /// which reads as the rail stuttering rather than as a heading.
    ///
    /// Near-repeats are fine and deliberately not caught: "Catchments"
    /// over Subcatchments still says where the catchment side begins.
    #[test]
    fn no_heading_repeats_the_entry_beneath_it() {
        for kind in ELEMENT_KINDS {
            let Some(group) = kind.group else { continue };
            let alone = ELEMENT_KINDS
                .iter()
                .filter(|k| k.group == Some(group))
                .count()
                == 1;
            assert!(
                !(alone && group.eq_ignore_ascii_case(kind.label_plural)),
                "'{group}' is a heading over nothing but itself"
            );
        }
    }

    #[test]
    fn each_group_is_one_run_of_the_catalog() {
        let mut runs: Vec<&str> = Vec::new();
        for kind in ELEMENT_KINDS {
            let Some(group) = kind.group else { continue };
            if runs.last() != Some(&group) {
                assert!(
                    !runs.contains(&group),
                    "'{group}' is split across the catalog"
                );
                runs.push(group);
            }
        }
        assert!(runs.len() > 1, "no grouping to check");
    }

    /// No group spans the rule an application draws between what is on
    /// the map and what is not. A heading with a line through it is two
    /// half-headings, and a reader cannot tell which they are under.
    #[test]
    fn no_group_straddles_the_spatial_divide() {
        for kind in ELEMENT_KINDS {
            let Some(group) = kind.group else { continue };
            let spatial = kind.class != ElementClass::Collection;
            for other in ELEMENT_KINDS.iter().filter(|k| k.group == Some(group)) {
                assert_eq!(
                    other.class != ElementClass::Collection,
                    spatial,
                    "'{group}' holds both {} and {}",
                    kind.id,
                    other.id
                );
            }
        }
    }

    /// A reference names a kind, and a kind id that no catalog entry
    /// answers to is a completion list that can never be built (spec
    /// §4.5.1.1). Cheap to assert, and the only thing that catches a
    /// key renamed on one side of the pair.
    #[test]
    fn every_declared_reference_names_a_kind_that_exists() {
        let mut seen = 0;
        for kind in ELEMENT_KINDS {
            for a in attribute_schema(kind.id) {
                for target in &a.references {
                    assert!(
                        ELEMENT_KINDS.iter().any(|k| k.id == *target),
                        "{}.{} references '{target}', which is not a kind",
                        kind.id,
                        a.key
                    );
                    seen += 1;
                }
            }
        }
        assert!(seen > 0, "no attribute declares a reference");
    }

    /// The editing contract's one hard rule about the catalog
    /// (hydra-common §4.5.3): a kind that cannot be created has to say
    /// what a new one would need. A refusal with nothing behind it is a
    /// dead end, and the application shows this text rather than
    /// inventing its own.
    #[test]
    fn every_uncreatable_kind_says_what_is_missing() {
        for kind in ELEMENT_KINDS {
            if kind.creatable {
                assert!(
                    kind.not_creatable_because.is_none(),
                    "{} is creatable and still explains why it is not",
                    kind.id
                );
            } else {
                let why = kind
                    .not_creatable_because
                    .unwrap_or_else(|| panic!("{} refuses creation without a reason", kind.id));
                assert!(
                    why.len() > 20 && !why.ends_with('.'),
                    "{}: a reason is a clause the caller builds a sentence from, got {why:?}",
                    kind.id
                );
            }
        }
    }

    /// Everything this engine publishes is a stored model value, so
    /// everything it publishes is editable.
    ///
    /// Asserted rather than assumed, because the reverse — an attribute
    /// added for display only — is the case that needs the flag, and it
    /// would arrive silently. This engine's editable set includes
    /// references and choices, not only numbers: a demand pattern and a
    /// valve type are both edited in its tables today, which is why the
    /// contract does not restrict editability to numbers even though the
    /// drainage engine's write happens to.
    #[test]
    fn every_published_attribute_is_a_value_the_user_may_set() {
        let mut seen = 0;
        for kind in ELEMENT_KINDS {
            for a in attribute_schema(kind.id) {
                assert!(
                    a.editable,
                    "{}.{} is published but not editable — if that is deliberate, \
                     say why here",
                    kind.id, a.key
                );
                seen += 1;
            }
        }
        assert!(seen > 20, "only {seen} attributes were checked");
    }

    /// Role is the engine's judgement, not a lookup — see the drainage
    /// engine's counterpart. Pinned so an unsimulated network cannot change
    /// how it reads without someone choosing to change it.
    #[test]
    fn every_kind_declares_the_role_it_plays() {
        use ElementRole::*;
        let role = |id: &str| ELEMENT_KINDS.iter().find(|k| k.id == id).unwrap().role;

        // A reservoir fixes head, a tank holds volume: both are where the
        // model meets what it does not simulate.
        assert_eq!(role("reservoir"), Some(Boundary));
        assert_eq!(role("tank"), Some(Boundary));

        assert_eq!(role("pump"), Some(Control));
        assert_eq!(role("valve"), Some(Control));

        assert_eq!(role("junction"), Some(Conveyance));
        assert_eq!(role("pipe"), Some(Conveyance));

        for k in ELEMENT_KINDS
            .iter()
            .filter(|k| k.class == ElementClass::Collection)
        {
            assert_eq!(k.role, None, "{}", k.id);
        }
    }

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn kind_ids_are_unique_and_badged() {
        let mut seen = HashSet::new();
        for k in ELEMENT_KINDS {
            assert!(seen.insert(k.id), "duplicate kind id {}", k.id);
            assert!(!k.badge.is_empty() && k.badge.len() <= 2);
        }
    }

    /// The GUI keeps a hand-mirror of this table (`units.ts`), because its
    /// conversion helpers take a closed `Quantity` union rather than a
    /// descriptor. The two drifted once — `volume` moved to cubic feet here
    /// while the GUI kept converting to gallons — so the claim is pinned on
    /// both sides. The frontend half is "the engine quantity catalog agrees
    /// with this module on every shared quantity", in `units.test.ts`.
    #[test]
    fn the_gui_unit_table_mirrors_this_catalog() {
        let expected: &[(&str, &str, &str, f64)] = &[
            ("length", "m", "ft", 3.280_84),
            ("elevation", "m", "ft", 3.280_84),
            ("head", "m", "ft", 3.280_84),
            ("diameter", "mm", "in", 0.039_370_1),
            ("flow", "L/s", "gpm", 15.850_323),
            ("demand", "L/s", "gpm", 15.850_323),
            ("velocity", "m/s", "ft/s", 3.280_84),
            ("pressure", "m", "psi", 1.421_970_2),
            ("headloss", "m/km", "ft/kft", 1.0),
            ("volume", "m³", "ft³", 35.314_667),
            ("concentration", "mg/L", "mg/L", 1.0),
            ("age", "h", "h", 1.0),
        ];
        for (key, si, us, scale) in expected {
            let q = QUANTITIES
                .iter()
                .find(|q| q.key == *key)
                .unwrap_or_else(|| panic!("{key} left the catalog; update units.ts too"));
            assert_eq!(q.si_label, *si, "{key} SI label");
            assert_eq!(q.us_label, *us, "{key} US label");
            assert!(
                (q.si_to_us_scale - scale).abs() < 1e-6,
                "{key} scale: catalog {} vs GUI {scale}",
                q.si_to_us_scale
            );
        }
    }

    #[test]
    fn every_attribute_quantity_is_in_the_catalog() {
        let keys: HashSet<&str> = QUANTITIES.iter().map(|q| q.key).collect();
        for kind in ELEMENT_KINDS {
            for a in attribute_schema(kind.id) {
                if let Some(q) = &a.quantity {
                    assert!(
                        keys.contains(q.as_str()),
                        "{}.{} names unknown quantity {q}",
                        kind.id,
                        a.key
                    );
                }
            }
        }
    }

    #[test]
    fn every_variable_quantity_is_in_the_catalog() {
        let keys: HashSet<&str> = QUANTITIES.iter().map(|q| q.key).collect();
        for class in [
            ElementClass::Point,
            ElementClass::Polyline,
            ElementClass::Region,
            ElementClass::Collection,
        ] {
            for v in result_variables(class) {
                if let Some(q) = v.quantity {
                    assert!(
                        keys.contains(q),
                        "variable {} names unknown quantity {q}",
                        v.id
                    );
                }
            }
        }
    }

    /// Whether a link state is abnormal is a hydraulic judgement only this
    /// engine can make. An application that is given the states without it
    /// can order them but not rank them, so it must paint a closed link and
    /// an open one as merely different — losing a distinction that matters
    /// at a glance. Every state must therefore carry one.
    #[test]
    fn every_link_state_declares_how_remarkable_it_is() {
        let status = result_variables(ElementClass::Polyline)
            .into_iter()
            .find(|v| v.id == "status")
            .expect("link status is published");
        let RampHint::Categorical { items } = status.ramp else {
            panic!("link status must be categorical, got {:?}", status.ramp);
        };
        for item in &items {
            assert!(
                item.severity.is_some(),
                "state {:?} declares no severity",
                item.label
            );
        }
        // The judgement itself, not merely its presence: a network is read
        // for what has stopped conducting.
        let by_label = |l: &str| {
            items
                .iter()
                .find(|i| i.label == l)
                .unwrap_or_else(|| panic!("{l} state is published"))
                .severity
        };
        assert_eq!(by_label("Open"), Some(CategorySeverity::Nominal));
        assert_eq!(by_label("Closed"), Some(CategorySeverity::Alarm));
        assert_eq!(by_label("Active"), Some(CategorySeverity::Caution));
    }

    /// The catalog must name every status code the results writer can
    /// actually emit. An undeclared code has no label and no colour, so a
    /// link in that state reads as having no result at all — the failure
    /// states, which are exactly the ones worth seeing, would be the ones
    /// that disappeared.
    #[test]
    fn every_status_the_writer_emits_is_declared() {
        use crate::LinkStatus::*;
        let status = result_variables(ElementClass::Polyline)
            .into_iter()
            .find(|v| v.id == "status")
            .expect("link status is published");
        let RampHint::Categorical { items } = status.ramp else {
            panic!("link status must be categorical");
        };
        for st in [XHead, TempClosed, Closed, Open, Active, XFcv, XPressure] {
            let code = crate::io::out_writer::status_out_code(st) as i64;
            assert!(
                items.iter().any(|i| i.value == code),
                "status {st:?} is written as {code} but no catalog item declares it"
            );
        }
    }

    #[test]
    fn spatial_kinds_have_schemas_and_collections_do_not() {
        for kind in ELEMENT_KINDS {
            let schema = attribute_schema(kind.id);
            match kind.class {
                ElementClass::Collection => assert!(schema.is_empty(), "{}", kind.id),
                _ => assert!(!schema.is_empty(), "{} has no schema", kind.id),
            }
        }
    }
}
