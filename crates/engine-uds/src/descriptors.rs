//! The engine's published catalogs under the hydra-common element,
//! quantity, and result-variable contracts (hydra-common spec §4–§6).
//!
//! Presentation-facing projections of the data model (model spec §2): they
//! let an application enumerate, render, and inspect a uds model and its
//! results without SWMM knowledge. The subcatchment is the contract's
//! region proof case — an areal element discharging to a point outlet.
//!
//! Pollutant concentration series are deliberately absent from the static
//! variable catalog: their identities are properties of the model (one
//! series per declared pollutant), so they arrive with the per-run
//! presence resolution (spec §6.2) when result reading lands.

use hydra_common::{
    AttributeDescriptor, ElementClass, ElementKind, OptionKind, QuantityDescriptor, RampHint,
    VariableDescriptor,
};

// ── Element kinds (spec §4.2) ─────────────────────────────────────────────────

/// The engine's element kinds, in presentation order.
pub const ELEMENT_KINDS: &[ElementKind] = &[
    ElementKind {
        id: "subcatchment",
        label: "Subcatchment",
        label_plural: "Subcatchments",
        class: ElementClass::Region,
        badge: "SC",
    },
    ElementKind {
        id: "junction",
        label: "Junction",
        label_plural: "Junctions",
        class: ElementClass::Point,
        badge: "J",
    },
    ElementKind {
        id: "outfall",
        label: "Outfall",
        label_plural: "Outfalls",
        class: ElementClass::Point,
        badge: "OF",
    },
    ElementKind {
        id: "divider",
        label: "Divider",
        label_plural: "Dividers",
        class: ElementClass::Point,
        badge: "FD",
    },
    ElementKind {
        id: "storage",
        label: "Storage unit",
        label_plural: "Storage units",
        class: ElementClass::Point,
        badge: "SU",
    },
    ElementKind {
        id: "raingage",
        label: "Rain gage",
        label_plural: "Rain gages",
        class: ElementClass::Point,
        badge: "RG",
    },
    ElementKind {
        id: "conduit",
        label: "Conduit",
        label_plural: "Conduits",
        class: ElementClass::Polyline,
        badge: "C",
    },
    ElementKind {
        id: "pump",
        label: "Pump",
        label_plural: "Pumps",
        class: ElementClass::Polyline,
        badge: "PU",
    },
    ElementKind {
        id: "orifice",
        label: "Orifice",
        label_plural: "Orifices",
        class: ElementClass::Polyline,
        badge: "OR",
    },
    ElementKind {
        id: "weir",
        label: "Weir",
        label_plural: "Weirs",
        class: ElementClass::Polyline,
        badge: "W",
    },
    ElementKind {
        id: "outlet",
        label: "Outlet",
        label_plural: "Outlets",
        class: ElementClass::Polyline,
        badge: "OL",
    },
    ElementKind {
        id: "pollutant",
        label: "Pollutant",
        label_plural: "Pollutants",
        class: ElementClass::Collection,
        badge: "Po",
    },
    ElementKind {
        id: "curve",
        label: "Curve",
        label_plural: "Curves",
        class: ElementClass::Collection,
        badge: "Cv",
    },
    ElementKind {
        id: "timeseries",
        label: "Time series",
        label_plural: "Time series",
        class: ElementClass::Collection,
        badge: "Ts",
    },
    ElementKind {
        id: "pattern",
        label: "Pattern",
        label_plural: "Patterns",
        class: ElementClass::Collection,
        badge: "Pa",
    },
    ElementKind {
        id: "rule",
        label: "Control rule",
        label_plural: "Control rules",
        class: ElementClass::Collection,
        badge: "Ru",
    },
];

// ── Quantities (spec §5) ──────────────────────────────────────────────────────

/// The engine's quantity catalog. SI display units follow the SWMM
/// convention (CMS-family), US-customary the CFS family; temperature is
/// the affine case the contract's offset exists for.
pub const QUANTITIES: &[QuantityDescriptor] = &[
    q("length", "m", "ft", 3.280_84, 0.0, 1, 1),
    q("elevation", "m", "ft", 3.280_84, 0.0, 2, 2),
    q("depth", "m", "ft", 3.280_84, 0.0, 2, 2),
    q("flow", "m³/s", "cfs", 35.314_7, 0.0, 3, 2),
    q("velocity", "m/s", "ft/s", 3.280_84, 0.0, 2, 2),
    q("rainfall", "mm/hr", "in/hr", 0.039_370_1, 0.0, 1, 2),
    q("infiltration", "mm/hr", "in/hr", 0.039_370_1, 0.0, 2, 3),
    q("area", "ha", "ac", 2.471_05, 0.0, 2, 2),
    q("volume", "m³", "ft³", 35.314_7, 0.0, 1, 0),
    q("concentration", "mg/L", "mg/L", 1.0, 0.0, 2, 2),
    q("percent", "%", "%", 1.0, 0.0, 1, 1),
    q("temperature", "°C", "°F", 1.8, 32.0, 1, 1),
];

/// Shorthand constructor keeping the table above readable.
const fn q(
    key: &'static str,
    si: &'static str,
    us: &'static str,
    scale: f64,
    offset: f64,
    si_dec: u8,
    us_dec: u8,
) -> QuantityDescriptor {
    QuantityDescriptor {
        key,
        si_label: si,
        us_label: us,
        si_to_us_scale: scale,
        si_to_us_offset: offset,
        si_decimals: si_dec,
        us_decimals: us_dec,
    }
}

// ── Attribute schemas (spec §4.3) ─────────────────────────────────────────────

/// The display attributes of one element kind, or empty for an unknown id —
/// advisory, like a block-options description.
pub fn attribute_schema(kind_id: &str) -> Vec<AttributeDescriptor> {
    match kind_id {
        "subcatchment" => vec![
            attr("raingage", "Rain gage", text(), None),
            attr("outlet", "Outlet", text(), None),
            attr("area", "Area", num(), Some("area")),
            attr("width", "Width", num(), Some("length")),
            attr("slope", "Slope", num(), Some("percent")),
            attr("imperviousness", "Imperviousness", num(), Some("percent")),
        ],
        "junction" => vec![
            attr("invert", "Invert elevation", num(), Some("elevation")),
            attr("maxDepth", "Maximum depth", num(), Some("depth")),
            attr("initDepth", "Initial depth", num(), Some("depth")),
        ],
        "outfall" => vec![
            attr("invert", "Invert elevation", num(), Some("elevation")),
            attr(
                "boundary",
                "Boundary",
                OptionKind::Choice {
                    default: None,
                    items: ["FREE", "NORMAL", "FIXED", "TIDAL", "TIMESERIES"]
                        .iter()
                        .map(|v| hydra_common::ChoiceItem {
                            value: (*v).to_string(),
                            label: (*v).to_string(),
                        })
                        .collect(),
                },
                None,
            ),
            attr(
                "gated",
                "Flap gated",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "divider" => vec![
            attr("invert", "Invert elevation", num(), Some("elevation")),
            attr("divertedLink", "Diverted link", text(), None),
        ],
        "storage" => vec![
            attr("invert", "Invert elevation", num(), Some("elevation")),
            attr("maxDepth", "Maximum depth", num(), Some("depth")),
            attr("shape", "Shape", text(), None),
        ],
        "raingage" => vec![
            attr("format", "Data format", text(), None),
            attr("interval", "Recording interval", text(), None),
            attr("source", "Data source", text(), None),
        ],
        "conduit" => vec![
            attr("length", "Length", num(), Some("length")),
            attr("roughness", "Roughness", num(), None),
            attr("shape", "Cross-section", text(), None),
            attr("maxDepth", "Full depth", num(), Some("depth")),
        ],
        "pump" => vec![
            attr("curve", "Pump curve", text(), None),
            attr(
                "initStatus",
                "Initial status",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "orifice" => vec![
            attr(
                "orientation",
                "Orientation",
                OptionKind::Choice {
                    default: None,
                    items: ["SIDE", "BOTTOM"]
                        .iter()
                        .map(|v| hydra_common::ChoiceItem {
                            value: (*v).to_string(),
                            label: (*v).to_string(),
                        })
                        .collect(),
                },
                None,
            ),
            attr("height", "Opening height", num(), Some("depth")),
            attr("dischargeCoeff", "Discharge coefficient", num(), None),
        ],
        "weir" => vec![
            attr("crestHeight", "Crest height", num(), Some("depth")),
            attr("dischargeCoeff", "Discharge coefficient", num(), None),
        ],
        "outlet" => vec![
            attr("outletCurve", "Rating curve", text(), None),
            attr(
                "gated",
                "Flap gated",
                OptionKind::Boolean { default: None },
                None,
            ),
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
        // Symbols are the domain's standard notation: y flow depth, H head,
        // V volume, q lateral inflow, ΣQ summed inflow, Q discharge,
        // v velocity, y/D the partial-depth capacity ratio, i rainfall
        // intensity (rational method), f infiltration rate (Horton).
        ElementClass::Point => vec![
            var("depth", "Depth", "y", Some("depth"), RampHint::Sequential),
            var(
                "head",
                "Hydraulic head",
                "H",
                Some("elevation"),
                RampHint::Sequential,
            ),
            var(
                "volume",
                "Stored volume",
                "V",
                Some("volume"),
                RampHint::Sequential,
            ),
            var(
                "lateralInflow",
                "Lateral inflow",
                "qL",
                Some("flow"),
                RampHint::Sequential,
            ),
            var(
                "totalInflow",
                "Total inflow",
                "ΣQ",
                Some("flow"),
                RampHint::Sequential,
            ),
            var(
                "flooding",
                "Flooding",
                "Qf",
                Some("flow"),
                RampHint::Sequential,
            ),
        ],
        ElementClass::Polyline => vec![
            var("flow", "Flow", "Q", Some("flow"), RampHint::Diverging),
            var("depth", "Depth", "y", Some("depth"), RampHint::Sequential),
            var(
                "velocity",
                "Velocity",
                "v",
                Some("velocity"),
                RampHint::Banded,
            ),
            var(
                "capacity",
                "Capacity used",
                "y/D",
                Some("percent"),
                RampHint::Banded,
            ),
        ],
        ElementClass::Region => vec![
            var(
                "rainfall",
                "Rainfall",
                "i",
                Some("rainfall"),
                RampHint::Sequential,
            ),
            var("runoff", "Runoff", "Q", Some("flow"), RampHint::Sequential),
            var(
                "infiltration",
                "Infiltration",
                "f",
                Some("infiltration"),
                RampHint::Sequential,
            ),
        ],
        ElementClass::Collection => Vec::new(),
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

#[cfg(test)]
mod tests {
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
    fn the_subcatchment_is_a_region_discharging_results() {
        let sub = ELEMENT_KINDS
            .iter()
            .find(|k| k.id == "subcatchment")
            .unwrap();
        assert_eq!(sub.class, ElementClass::Region);
        assert!(!result_variables(ElementClass::Region).is_empty());
    }

    #[test]
    fn every_referenced_quantity_is_in_the_catalog() {
        let keys: HashSet<&str> = QUANTITIES.iter().map(|q| q.key).collect();
        for kind in ELEMENT_KINDS {
            for a in attribute_schema(kind.id) {
                if let Some(q) = &a.quantity {
                    assert!(keys.contains(q.as_str()), "{}.{}: {q}", kind.id, a.key);
                }
            }
        }
        for class in [
            ElementClass::Point,
            ElementClass::Polyline,
            ElementClass::Region,
            ElementClass::Collection,
        ] {
            for v in result_variables(class) {
                if let Some(q) = v.quantity {
                    assert!(keys.contains(q), "variable {}: {q}", v.id);
                }
            }
        }
    }

    #[test]
    fn temperature_conversion_is_affine() {
        let t = QUANTITIES.iter().find(|q| q.key == "temperature").unwrap();
        assert_eq!(t.si_to_us(0.0), 32.0);
        assert_eq!(t.si_to_us(100.0), 212.0);
    }
}
