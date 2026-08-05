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
        label: "Junction",
        label_plural: "Junctions",
        class: ElementClass::Point,
        role: Some(ElementRole::Conveyance),
        badge: "J",
    },
    ElementKind {
        id: "reservoir",
        label: "Reservoir",
        label_plural: "Reservoirs",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "R",
    },
    ElementKind {
        id: "tank",
        label: "Tank",
        label_plural: "Tanks",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "TK",
    },
    ElementKind {
        id: "pipe",
        label: "Pipe",
        label_plural: "Pipes",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Conveyance),
        badge: "P",
    },
    ElementKind {
        id: "pump",
        label: "Pump",
        label_plural: "Pumps",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "PU",
    },
    ElementKind {
        id: "valve",
        label: "Valve",
        label_plural: "Valves",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "V",
    },
    ElementKind {
        id: "pattern",
        label: "Pattern",
        label_plural: "Patterns",
        class: ElementClass::Collection,
        role: None,
        badge: "Pa",
    },
    ElementKind {
        id: "curve",
        label: "Curve",
        label_plural: "Curves",
        class: ElementClass::Collection,
        role: None,
        badge: "Cv",
    },
    ElementKind {
        id: "control",
        label: "Control",
        label_plural: "Controls",
        class: ElementClass::Collection,
        role: None,
        badge: "Ct",
    },
    ElementKind {
        id: "rule",
        label: "Rule",
        label_plural: "Rules",
        class: ElementClass::Collection,
        role: None,
        badge: "Ru",
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
    match kind_id {
        "junction" => vec![
            attr("elevation", "Elevation", num(), Some("elevation")),
            attr("baseDemand", "Base demand", num(), Some("demand")),
            attr("demandPattern", "Demand pattern", text(), None),
        ],
        "reservoir" => vec![
            attr("head", "Head", num(), Some("head")),
            attr("headPattern", "Head pattern", text(), None),
        ],
        "tank" => vec![
            attr("elevation", "Elevation", num(), Some("elevation")),
            attr("initLevel", "Initial level", num(), Some("length")),
            attr("minLevel", "Minimum level", num(), Some("length")),
            attr("maxLevel", "Maximum level", num(), Some("length")),
            attr("diameter", "Diameter", num(), Some("length")),
            attr("minVolume", "Minimum volume", num(), Some("volume")),
            attr("volumeCurve", "Volume curve", text(), None),
            attr(
                "overflow",
                "Overflow",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "pipe" => vec![
            attr("length", "Length", num(), Some("length")),
            attr("diameter", "Diameter", num(), Some("diameter")),
            attr("roughness", "Roughness", num(), None),
            attr("minorLoss", "Minor loss", num(), None),
            attr(
                "checkValve",
                "Check valve",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "pump" => vec![
            attr("headCurve", "Head curve", text(), None),
            attr("power", "Rated power", num(), None),
            attr("speed", "Relative speed", num(), None),
            attr("speedPattern", "Speed pattern", text(), None),
        ],
        "valve" => vec![
            attr(
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
            attr("diameter", "Diameter", num(), Some("diameter")),
            // The setting's unit depends on the valve type (pressure for
            // PRV/PSV/PBV, flow for FCV, dimensionless otherwise), so no
            // single quantity is truthful here.
            attr("setting", "Setting", num(), None),
            attr("minorLoss", "Minor loss", num(), None),
        ],
        _ => Vec::new(),
    }
}

fn attr(key: &str, label: &str, kind: OptionKind, quantity: Option<&str>) -> AttributeDescriptor {
    AttributeDescriptor {
        key: key.to_string(),
        label: label.to_string(),
        kind,
        quantity: quantity.map(str::to_string),
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
                RampHint::Banded,
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
                RampHint::Banded,
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
