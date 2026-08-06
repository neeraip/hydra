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
    AttributeDescriptor, ElementClass, ElementKind, ElementRole, OptionKind, QuantityDescriptor,
    RampHint, VariableDescriptor,
};

// ── Element kinds (spec §4.2) ─────────────────────────────────────────────────

/// The engine's element kinds, in presentation order.
///
/// Grouped by element class, in the order the contract declares them —
/// points, then polylines, then regions, then collections. An application
/// that lays a tab or a chip out per kind then reads network before
/// catchment, which is the order someone inspecting a model works in even
/// though the water runs the other way.
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
        id: "outfall",
        label: "Outfall",
        label_plural: "Outfalls",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "OF",
    },
    ElementKind {
        id: "divider",
        label: "Divider",
        label_plural: "Dividers",
        class: ElementClass::Point,
        role: Some(ElementRole::Control),
        badge: "FD",
    },
    ElementKind {
        id: "storage",
        label: "Storage unit",
        label_plural: "Storage units",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "SU",
    },
    ElementKind {
        id: "raingage",
        label: "Rain gage",
        label_plural: "Rain gages",
        class: ElementClass::Point,
        role: None,
        badge: "RG",
    },
    ElementKind {
        id: "conduit",
        label: "Conduit",
        label_plural: "Conduits",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Conveyance),
        badge: "C",
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
        id: "orifice",
        label: "Orifice",
        label_plural: "Orifices",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "OR",
    },
    ElementKind {
        id: "weir",
        label: "Weir",
        label_plural: "Weirs",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "W",
    },
    ElementKind {
        id: "outlet",
        label: "Outlet",
        label_plural: "Outlets",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "OL",
    },
    ElementKind {
        id: "subcatchment",
        label: "Subcatchment",
        label_plural: "Subcatchments",
        class: ElementClass::Region,
        role: Some(ElementRole::Boundary),
        badge: "SC",
    },
    ElementKind {
        id: "pollutant",
        label: "Pollutant",
        label_plural: "Pollutants",
        class: ElementClass::Collection,
        role: None,
        badge: "Po",
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
        id: "timeseries",
        label: "Time series",
        label_plural: "Time series",
        class: ElementClass::Collection,
        role: None,
        badge: "Ts",
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
        id: "rule",
        label: "Control rule",
        label_plural: "Control rules",
        class: ElementClass::Collection,
        role: None,
        badge: "Ru",
    },
    ElementKind {
        id: "landuse",
        label: "Land use",
        label_plural: "Land uses",
        class: ElementClass::Collection,
        role: None,
        badge: "Lu",
    },
    ElementKind {
        id: "aquifer",
        label: "Aquifer",
        label_plural: "Aquifers",
        class: ElementClass::Collection,
        role: None,
        badge: "Aq",
    },
    ElementKind {
        id: "snowpack",
        label: "Snow pack",
        label_plural: "Snow packs",
        class: ElementClass::Collection,
        role: None,
        badge: "Sn",
    },
    ElementKind {
        id: "hydrograph",
        label: "Unit hydrograph",
        label_plural: "Unit hydrographs",
        class: ElementClass::Collection,
        role: None,
        badge: "Uh",
    },
    ElementKind {
        id: "lidcontrol",
        label: "LID control",
        label_plural: "LID controls",
        class: ElementClass::Collection,
        role: None,
        badge: "Li",
    },
    ElementKind {
        id: "transect",
        label: "Transect",
        label_plural: "Transects",
        class: ElementClass::Collection,
        role: None,
        badge: "Tr",
    },
    // A street cross-section: the roadway geometry a conduit routes surface
    // flow across in a dual-drainage model. A named registry like transects
    // and curves, referenced by conduits rather than placed on the map.
    ElementKind {
        id: "street",
        label: "Street",
        label_plural: "Streets",
        class: ElementClass::Collection,
        role: None,
        badge: "St",
    },
    // An inlet design: the grate, curb opening or slotted drain through
    // which a street captures flow into the sewer below. One design serves
    // any number of streets, which is why it is a registry entry and not a
    // property of the conduit that uses it.
    ElementKind {
        id: "inlet",
        label: "Inlet design",
        label_plural: "Inlet designs",
        class: ElementClass::Collection,
        role: None,
        badge: "In",
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
        // ── Collections (§4.1 `collection` class) ─────────────────────
        //
        // These carry no geometry, so an application lists them the same
        // way it lists a junction: by id, with the columns declared here.
        // The point of each is a table or a rule body — too large for a
        // cell — so the schemas describe *what a row is* and how big,
        // leaving the contents to a dedicated editor.
        "pollutant" => vec![
            attr("units", "Units", text(), None),
            attr("rainConc", "Rainfall conc.", num(), Some("concentration")),
            attr(
                "groundwaterConc",
                "Groundwater conc.",
                num(),
                Some("concentration"),
            ),
            attr("rdiiConc", "RDII conc.", num(), Some("concentration")),
            attr("dwfConc", "Dry-weather conc.", num(), Some("concentration")),
            // First-order decay, per day: dimensionless in the §5 catalog
            // because a reciprocal-time quantity has no entry there.
            attr("decay", "Decay (1/day)", num(), None),
            attr(
                "snowOnly",
                "Snow only",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "curve" => vec![
            attr("curveType", "Type", text(), None),
            attr("points", "Points", num(), None),
        ],
        "timeseries" => vec![
            attr("source", "Source", text(), None),
            attr("points", "Points", num(), None),
        ],
        "pattern" => vec![
            attr("patternType", "Type", text(), None),
            attr("factors", "Factors", num(), None),
        ],
        "rule" => vec![attr("clauses", "Clauses", num(), None)],
        // ── Process parameter sets ────────────────────────────────────
        //
        // Named registries the model references by id: a subcatchment
        // names its land uses, an aquifer, a snow pack; a storage vertex
        // names a unit hydrograph group. Each is a parameter set rather
        // than a table, so the columns are the parameters a modeller
        // compares across the set, and the deeper structure (per-pollutant
        // buildup curves, per-month responses, per-layer LID geometry)
        // stays behind the row.
        "landuse" => vec![
            attr("sweepInterval", "Sweep interval (days)", num(), None),
            // A fraction of available load, not a percentage: the parser
            // rejects anything outside 0–1, so labelling it `percent`
            // would render 0.5 as "0.5 %" and mean 50.
            attr("sweepRemoval", "Sweep removal (fraction)", num(), None),
            attr("sweepDaysSince", "Days since swept", num(), None),
            attr("buildupFor", "Buildup defined", num(), None),
            attr("washoffFor", "Washoff defined", num(), None),
        ],
        "aquifer" => vec![
            attr("porosity", "Porosity", num(), None),
            attr("wiltingPoint", "Wilting point", num(), None),
            attr("fieldCapacity", "Field capacity", num(), None),
            attr("conductivity", "Conductivity", num(), Some("infiltration")),
            attr("upperEvapFrac", "Upper evap. fraction", num(), None),
            attr("lowerEvapDepth", "Lower evap. depth", num(), Some("depth")),
        ],
        "snowpack" => vec![
            attr("surfaces", "Surfaces defined", num(), None),
            attr("plowFraction", "Plowable fraction", num(), None),
            attr(
                "removal",
                "Removal defined",
                OptionKind::Boolean { default: None },
                None,
            ),
        ],
        "hydrograph" => vec![
            attr("raingage", "Rain gage", text(), None),
            attr("responses", "Responses defined", num(), None),
        ],
        "lidcontrol" => vec![
            attr("lidType", "Type", text(), None),
            attr("layers", "Layers defined", num(), None),
            attr("removals", "Pollutant removals", num(), None),
        ],
        "transect" => vec![
            attr("nChannel", "Channel roughness", num(), None),
            attr("nLeft", "Left-bank roughness", num(), None),
            attr("nRight", "Right-bank roughness", num(), None),
            attr("stations", "Stations", num(), None),
        ],
        "street" => vec![
            attr("crownWidth", "Crown width", num(), Some("length")),
            attr("curbHeight", "Curb height", num(), Some("length")),
            attr("crossSlope", "Cross slope", num(), Some("percent")),
            attr("roughness", "Roughness", num(), None),
            attr("gutterWidth", "Gutter width", num(), Some("length")),
            attr(
                "gutterDepression",
                "Gutter depression",
                num(),
                Some("length"),
            ),
            attr("sides", "Sides", num(), None),
        ],
        "inlet" => vec![
            // What the design *is* — a combination inlet carries more than
            // one, so this is a summary rather than a single type.
            attr("openings", "Openings", text(), None),
            attr("grateLength", "Grate length", num(), Some("length")),
            attr("grateWidth", "Grate width", num(), Some("length")),
            attr("grateType", "Grate type", text(), None),
            attr("curbLength", "Curb length", num(), Some("length")),
            attr("curbHeight", "Curb opening height", num(), Some("length")),
            attr("slottedLength", "Slotted length", num(), Some("length")),
            attr("slottedWidth", "Slotted width", num(), Some("length")),
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
            // Both are signed: a lateral can be a loss, and a total is a
            // *net* — laterals plus arriving link flows minus leaving ones —
            // so a node that sheds more than it takes reads negative. A
            // sequential ramp hides that: the sign crossing becomes just
            // another shade, and water entering looks like water leaving.
            var(
                "lateralInflow",
                "Lateral inflow",
                "qL",
                Some("flow"),
                RampHint::Diverging,
            ),
            var(
                "totalInflow",
                "Total inflow",
                "ΣQ",
                Some("flow"),
                RampHint::Diverging,
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
    /// The kinds placed on a map come first, and the ones that are not
    /// come last — with no interleaving.
    ///
    /// Applications lean on this. The drainage editor's rail draws one
    /// rule between the two groups rather than offering a second level of
    /// navigation, and it finds that rule by looking for the first
    /// collection: a spatial kind declared after the collections would put
    /// the rule in the middle of them, and the rail would read as though
    /// a curve belonged on the canvas.
    ///
    /// Ordering a catalog is this crate's business, so the guarantee is
    /// asserted here rather than assumed there.
    #[test]
    fn spatial_kinds_precede_the_collections() {
        use super::ElementClass;
        let first_collection = super::ELEMENT_KINDS
            .iter()
            .position(|k| k.class == ElementClass::Collection)
            .expect("the catalog declares collections");
        assert!(
            super::ELEMENT_KINDS[..first_collection]
                .iter()
                .all(|k| k.class != ElementClass::Collection),
            "a collection precedes the first one found"
        );
        assert!(
            super::ELEMENT_KINDS[first_collection..]
                .iter()
                .all(|k| k.class == ElementClass::Collection),
            "a kind placed on the map is declared after the collections, \
             which would put the editor rail's divider inside them"
        );
    }

    /// A signed quantity needs a ramp with a middle. Node inflows are net
    /// figures — a node can shed more than it takes — and link flow already
    /// says so; declaring the same physical quantity sequential at a node
    /// and diverging in a conduit told an application two different things
    /// about one measurement.
    #[test]
    fn signed_quantities_declare_a_diverging_ramp() {
        let hint = |class, id: &str| {
            result_variables(class)
                .into_iter()
                .find(|v| v.id == id)
                .unwrap()
                .ramp
        };
        for id in ["lateralInflow", "totalInflow"] {
            assert!(
                matches!(hint(ElementClass::Point, id), RampHint::Diverging),
                "{id} is signed and must diverge"
            );
        }
        assert!(matches!(
            hint(ElementClass::Polyline, "flow"),
            RampHint::Diverging
        ));

        // Quantities that cannot go negative keep a one-way ramp.
        for id in ["depth", "volume", "flooding"] {
            assert!(
                matches!(hint(ElementClass::Point, id), RampHint::Sequential),
                "{id} is unsigned"
            );
        }
    }

    /// Every declared kind must be listable, collections included.
    ///
    /// A kind with no attribute schema has no columns, so an application
    /// building a table from the catalog gets an entry that opens onto
    /// nothing — which is why the drainage editor hid its five collection
    /// kinds outright rather than show five empty tables.
    #[test]
    fn every_kind_has_attributes_to_show() {
        for kind in ELEMENT_KINDS {
            assert!(
                !attribute_schema(kind.id).is_empty(),
                "{} declares no attribute schema, so nothing can list it",
                kind.id
            );
        }
    }

    /// Role is the engine's judgement, not a lookup, so it is pinned here.
    /// An application draws an unsimulated model from these alone, and a
    /// kind silently sliding from boundary to conveyance would change what
    /// a reader sees without changing anything they can point at.
    #[test]
    fn every_kind_declares_the_role_it_plays() {
        use ElementRole::*;
        let role = |id: &str| ELEMENT_KINDS.iter().find(|k| k.id == id).unwrap().role;

        // Where flow enters or leaves the routed network.
        assert_eq!(role("outfall"), Some(Boundary));
        assert_eq!(role("storage"), Some(Boundary));
        // Runoff is where water enters the network from outside it.
        assert_eq!(role("subcatchment"), Some(Boundary));

        // Structures act on the flow; a divider splits it, so it acts too.
        for id in ["pump", "orifice", "weir", "outlet", "divider"] {
            assert_eq!(role(id), Some(Control), "{id}");
        }

        assert_eq!(role("junction"), Some(Conveyance));
        assert_eq!(role("conduit"), Some(Conveyance));

        // A gage is located but conveys nothing, and a collection is not in
        // the flow network at all.
        assert_eq!(role("raingage"), None);
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
