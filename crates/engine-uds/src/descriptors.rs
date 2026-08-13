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
        id: "outfall",
        group: Some("Nodes"),
        label: "Outfall",
        label_plural: "Outfalls",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "OF",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "divider",
        group: Some("Nodes"),
        label: "Divider",
        label_plural: "Dividers",
        class: ElementClass::Point,
        role: Some(ElementRole::Control),
        badge: "FD",
        // Creatable, despite the file format making it look otherwise:
        // under the one routing form this engine solves, a divider *is*
        // an ordinary junction (§7.5), and its split rule is carried for
        // the import record rather than evaluated. So a new one needs
        // nothing invented — no diverted link is what `*` means, and the
        // overflow rule takes no parameters.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "storage",
        group: Some("Nodes"),
        label: "Storage unit",
        label_plural: "Storage units",
        class: ElementClass::Point,
        role: Some(ElementRole::Boundary),
        badge: "SU",
        // Creatable as a prismatic tank, whose depth and area are asked
        // for. A tabulated or fitted relation is still authored by
        // pointing the unit at a curve afterwards.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "conduit",
        group: Some("Links"),
        label: "Conduit",
        label_plural: "Conduits",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Conveyance),
        badge: "C",
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
        // Creatable for the same reason: its curve is a declared
        // reference now, so it is asked for rather than invented.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "orifice",
        group: Some("Links"),
        label: "Orifice",
        label_plural: "Orifices",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "OR",
        // Creatable since its opening became editable: the size is
        // asked for, and the coefficient the catalog declares.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "weir",
        group: Some("Links"),
        label: "Weir",
        label_plural: "Weirs",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "W",
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "outlet",
        group: Some("Links"),
        label: "Outlet",
        label_plural: "Outlets",
        class: ElementClass::Polyline,
        role: Some(ElementRole::Control),
        badge: "OL",
        creatable: false,
        not_creatable_because: Some(
            "an outlet's rating sets how much flow it passes, and no value for that \
             is more defensible than another",
        ),
    },
    ElementKind {
        id: "subcatchment",
        group: Some("Rainfall and runoff"),
        label: "Subcatchment",
        label_plural: "Subcatchments",
        class: ElementClass::Region,
        role: Some(ElementRole::Boundary),
        badge: "SC",
        // Creatable, and its refusal was wrong twice over: an area is a
        // plain number here and the polygon is optional display
        // geometry. What a new one really needs is a gage and an outlet,
        // and both are references the create can now be given.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "raingage",
        group: Some("Rainfall and runoff"),
        label: "Rain gage",
        label_plural: "Rain gages",
        class: ElementClass::Point,
        role: None,
        badge: "RG",
        // Creatable since the catalog says which kind its source names:
        // a form can ask for the series, which is the thing that had to
        // exist first.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "curve",
        group: Some("Curves and patterns"),
        label: "Curve",
        label_plural: "Curves",
        class: ElementClass::Collection,
        role: None,
        badge: "Cv",
        creatable: true,
        // Creatable since a form can ask for a choice. The role is the
        // whole difficulty — it decides what units the two columns are
        // read in — so it is chosen rather than defaulted, and there is
        // nothing else a new curve needs.
        not_creatable_because: None,
    },
    ElementKind {
        id: "pattern",
        group: Some("Curves and patterns"),
        label: "Pattern",
        label_plural: "Patterns",
        class: ElementClass::Collection,
        role: None,
        badge: "Pa",
        // Creatable since its multipliers became editable. Its kind
        // decides the pattern's *length* rather than what its numbers
        // mean, so a flat hourly one is complete rather than a guess.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "timeseries",
        group: Some("Rainfall records"),
        label: "Time series",
        label_plural: "Time series",
        class: ElementClass::Collection,
        role: None,
        badge: "Ts",
        // Creatable since its values became editable. A new one is two
        // readings of nothing an hour apart, which is a series rather
        // than a placeholder — and not an empty one, which the writer
        // would drop at the next save.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "hydrograph",
        group: Some("Rainfall records"),
        label: "Unit hydrograph",
        label_plural: "Unit hydrographs",
        class: ElementClass::Collection,
        role: None,
        badge: "Uh",
        creatable: false,
        not_creatable_because: Some("a unit hydrograph is its per-month responses"),
    },
    ElementKind {
        id: "pollutant",
        group: Some("Water quality"),
        label: "Pollutant",
        label_plural: "Pollutants",
        class: ElementClass::Collection,
        role: None,
        badge: "Po",
        // Creatable, and its refusal named the wrong thing: buildup and
        // washoff are kept on the *land uses*, not here. A new one is a
        // constituent that exists and which nothing yet generates —
        // every value a genuine zero.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "landuse",
        group: Some("Water quality"),
        label: "Land use",
        label_plural: "Land uses",
        class: ElementClass::Collection,
        role: None,
        badge: "Lu",
        // Creatable: a land use with no accumulation covers ground and
        // contributes nothing, which is a state a model may hold rather
        // than a value standing in for one.
        creatable: true,
        not_creatable_because: None,
    },
    ElementKind {
        id: "aquifer",
        group: Some("Ground and climate"),
        label: "Aquifer",
        label_plural: "Aquifers",
        class: ElementClass::Collection,
        role: None,
        badge: "Aq",
        creatable: false,
        not_creatable_because: Some(
            "an aquifer sits at a bottom elevation and a water table, and putting \
             those at datum would bury it silently",
        ),
    },
    ElementKind {
        id: "snowpack",
        group: Some("Ground and climate"),
        label: "Snow pack",
        label_plural: "Snow packs",
        class: ElementClass::Collection,
        role: None,
        badge: "Sn",
        creatable: false,
        not_creatable_because: Some(
            "a snow pack of no surfaces melts nothing and is written as no lines \
             at all, so a new one would vanish at the next save",
        ),
    },
    ElementKind {
        id: "rule",
        group: Some("Controls"),
        label: "Control rule",
        label_plural: "Control rules",
        class: ElementClass::Collection,
        role: None,
        badge: "Ru",
        creatable: false,
        not_creatable_because: Some(
            "a rule is a statement about the network, which has to be written out",
        ),
    },
    ElementKind {
        id: "lidcontrol",
        group: Some("Runoff controls"),
        label: "LID control",
        label_plural: "LID controls",
        class: ElementClass::Collection,
        role: None,
        badge: "Li",
        creatable: false,
        not_creatable_because: Some("a control measure is defined by its layers"),
    },
    ElementKind {
        id: "transect",
        group: Some("Street drainage"),
        label: "Transect",
        label_plural: "Transects",
        class: ElementClass::Collection,
        role: None,
        badge: "Tr",
        // Creatable since those pairs became editable: a new one is two
        // flat survey points, and its shape is entered afterwards.
        creatable: true,
        not_creatable_because: None,
    },
    // A street cross-section: the roadway geometry a conduit routes surface
    // flow across in a dual-drainage model. A named registry like transects
    // and curves, referenced by conduits rather than placed on the map.
    ElementKind {
        id: "street",
        group: Some("Street drainage"),
        label: "Street",
        label_plural: "Streets",
        class: ElementClass::Collection,
        role: None,
        badge: "St",
        // Creatable since its geometry became editable. Every dimension
        // is asked for, because each describes one particular street.
        creatable: true,
        not_creatable_because: None,
    },
    // An inlet design: the grate, curb opening or slotted drain through
    // which a street captures flow into the sewer below. One design serves
    // any number of streets, which is why it is a registry entry and not a
    // property of the conduit that uses it.
    ElementKind {
        id: "inlet",
        group: Some("Street drainage"),
        label: "Inlet design",
        label_plural: "Inlet designs",
        class: ElementClass::Collection,
        role: None,
        badge: "In",
        // Creatable since that geometry became editable. A design may
        // carry a grate, a curb opening and a slot at once, so the form
        // offers all three and builds whichever were given a size —
        // which is conditional in what it produces without being
        // conditional in what it asks.
        creatable: true,
        not_creatable_because: None,
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
    // Accumulated depth of precipitation or loss (§13.3) — a distinct
    // quantity from the rates above and from node "depth" (metres):
    // storm totals read in millimetres and inches.
    q("precipitation", "mm", "in", 0.039_370_1, 0.0, 1, 2),
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
    let mut schema = own_attributes(kind_id);
    // Last, and after every one of the model's own values: a tag is a
    // note the modeller keeps beside the element, not something the
    // solver reads. Offered for every kind the tag section can name —
    // its three object words are subcatchment, node and link, which is
    // the three spatial classes.
    if taggable(kind_id) {
        schema.push(rw("tag", "Tag", text(), None));
    }
    schema
}

/// Whether `[TAGS]` can name this kind. A curve, a pattern or a time
/// series is a container the section has no grammar for.
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
        "subcatchment" => vec![
            rw("raingage", "Rain gage", text(), None),
            rw("outlet", "Outlet", text(), None),
            rw("area", "Area", num(), Some("area")),
            rw("width", "Width", num(), Some("length")),
            rw("slope", "Slope", num(), Some("percent")),
            rw("imperviousness", "Imperviousness", num(), Some("percent")),
        ],
        "junction" => vec![
            rw("invert", "Invert elevation", num(), Some("elevation")),
            rw("maxDepth", "Maximum depth", num(), Some("depth")),
            rw("initDepth", "Initial depth", num(), Some("depth")),
        ],
        "outfall" => vec![
            rw("invert", "Invert elevation", num(), Some("elevation")),
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
            rw("invert", "Invert elevation", num(), Some("elevation")),
            // A divider is a junction under the one solver (§7.5) and
            // carries a junction's depths; the schema simply never
            // published them, so a divider's table showed one column
            // where a junction's showed three.
            rw("maxDepth", "Maximum depth", num(), Some("depth")),
            rw("initDepth", "Initial depth", num(), Some("depth")),
            rw("divertedLink", "Diverted link", text(), None),
        ],
        "storage" => vec![
            rw("invert", "Invert elevation", num(), Some("elevation")),
            rw("maxDepth", "Maximum depth", num(), Some("depth")),
            attr("shape", "Shape", text(), None),
            // Only for a storage unit whose area does not vary with
            // depth. One number cannot describe a tabulated or fitted
            // relation, and an element that has no such value carries no
            // row for it (§4.5.1) rather than a misleading one.
            rw("surfaceArea", "Surface area", num(), Some("area")),
        ],
        "raingage" => vec![
            attr("format", "Data format", text(), None),
            attr("interval", "Recording interval", text(), None),
            // The record it reads. Writable and declared as a reference,
            // which is what lets a gage be created at all: it names a
            // series that has to exist first, and nothing could ask for
            // one until this said which kind it names.
            rw("source", "Data source", text(), None),
        ],
        "conduit" => vec![
            rw("length", "Length", num(), Some("length")),
            rw("roughness", "Roughness", num(), None),
            attr("shape", "Cross-section", text(), None),
            attr("maxDepth", "Full depth", num(), Some("depth")),
        ],
        "pump" => vec![
            rw("curve", "Pump curve", text(), None),
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
            // The opening. Editable since it became servable at all —
            // the schema declared a height for the life of this catalog
            // and no value was ever attached to it, so the row was
            // dropped and an orifice's size was unreadable.
            rw("height", "Opening height", num(), Some("depth")),
            rw("width", "Opening width", num(), Some("length")),
            rw(
                "dischargeCoeff",
                "Discharge coefficient",
                // The sharp-edged orifice value every text prints, and
                // dimensionless, so it needs no unit interpretation.
                numd(0.65),
                None,
            ),
        ],
        "weir" => vec![
            rw("crestHeight", "Crest height", num(), Some("depth")),
            // Not "length": a conduit's `length` is how far it runs, and
            // one key meaning two things across two kinds is how a reader
            // comes to believe they are the same measurement.
            rw("crestLength", "Crest length", num(), Some("length")),
            rw(
                "dischargeCoeff",
                "Discharge coefficient",
                // Declared, so a form creating one starts from the value
                // every text prints for a transverse weir rather than
                // from zero. 1.84 in SI; the file's own units convert.
                numd(1.84),
                None,
            ),
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
            // The role, which is the whole of what a new curve needs and
            // the reason the kind was unreachable: it decides what units
            // the two columns are *read* in, so no default is defensible
            // and the choice has to be made rather than supplied.
            rw("curveType", "Type", curve_roles(), None),
            // A count; the points themselves are the element's contents
            // (§4.5.2.2), edited in their own panel.
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
            rw("nChannel", "Channel roughness", num(), None),
            rw("nLeft", "Left-bank roughness", num(), None),
            rw("nRight", "Right-bank roughness", num(), None),
            // A count, not a value: the stations themselves are the
            // element's contents (§4.5.2.2), edited in their own panel.
            attr("stations", "Stations", num(), None),
        ],
        "street" => vec![
            rw("crownWidth", "Crown width", num(), Some("length")),
            rw("curbHeight", "Curb height", num(), Some("length")),
            rw("crossSlope", "Cross slope", num(), Some("percent")),
            rw("roughness", "Roughness", num(), None),
            rw("gutterWidth", "Gutter width", num(), Some("length")),
            rw(
                "gutterDepression",
                "Gutter depression",
                num(),
                Some("length"),
            ),
            rw("sides", "Sides", num(), None),
        ],
        "inlet" => vec![
            // What the design *is* — a combination inlet carries more than
            // one, so this is a summary rather than a single type.
            attr("openings", "Openings", text(), None),
            // One design may carry a grate, a curb opening and a slot at
            // once, so all three pairs are offered and a size given to
            // none of them means that opening is absent. That is what the
            // file says too: a line per opening the design has.
            rw("grateLength", "Grate length", num(), Some("length")),
            rw("grateWidth", "Grate width", num(), Some("length")),
            rw(
                "grateType",
                "Grate type",
                OptionKind::Choice {
                    // The family the predecessor's own examples reach for
                    // first, and the one this list starts with.
                    default: Some("P_BAR-50".to_string()),
                    // The predecessor's own spellings, in its own order —
                    // these are what the file carries, so a value picked
                    // here writes back verbatim.
                    items: [
                        "P_BAR-50x100",
                        "P_BAR-50",
                        "P_BAR-30",
                        "CURVED_VANE",
                        "TILT_BAR-45",
                        "TILT_BAR-30",
                        "RETICULINE",
                        "GENERIC",
                    ]
                    .iter()
                    .map(|v| hydra_common::ChoiceItem {
                        value: (*v).to_string(),
                        label: (*v).to_string(),
                    })
                    .collect(),
                },
                None,
            ),
            rw("curbLength", "Curb length", num(), Some("length")),
            rw("curbHeight", "Curb opening height", num(), Some("length")),
            rw("slottedLength", "Slotted length", num(), Some("length")),
            rw("slottedWidth", "Slotted width", num(), Some("length")),
        ],
        _ => Vec::new(),
    }
}

/// A read-only attribute: shown, never offered for editing.
///
/// The bulk of this engine's schema. Most of what a drainage element
/// publishes either names a referent — a cross-section shape, a storage
/// curve, an outfall's boundary condition — or is a choice, and setting
/// one of those is a different operation from typing a number over it.
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
    match key {
        "raingage" => vec!["raingage".to_string()],
        // A gage reads a time series; a pump follows a curve. Both name
        // something that has to exist first, which is why declaring the
        // kind is what makes the kind creatable.
        "source" => vec!["timeseries".to_string()],
        "curve" => vec!["curve".to_string()],
        // Every kind of conveyance node, *and* another subcatchment:
        // runoff either enters the network or cascades overland. This is
        // the attribute §4.5.1.1 was widened for — one kind id could not
        // say it, so the outlet went unwritable while it stayed one.
        "outlet" => kinds_of_class(ElementClass::Point)
            .chain(std::iter::once("subcatchment".to_string()))
            .collect(),
        // A link of any kind, so the divider knows which way the
        // diverted flow leaves.
        "divertedLink" => kinds_of_class(ElementClass::Polyline).collect(),
        _ => Vec::new(),
    }
}

/// The ids of every kind in a class, in catalog order — which is
/// presentation order, so an application offering them offers them the
/// way this engine would list them.
fn kinds_of_class(class: ElementClass) -> impl Iterator<Item = String> {
    ELEMENT_KINDS
        .iter()
        .filter(move |k| k.class == class)
        .map(|k| k.id.to_string())
}

/// A number the engine has a default for — what a form creating an
/// element should start the field at, rather than zero.
fn numd(default: f64) -> OptionKind {
    OptionKind::Number {
        default: Some(default),
        min: None,
        max: None,
    }
}

/// The curve roles, in the predecessor's own keywords.
///
/// No default: the role decides what the two columns *mean*, so a
/// storage curve created as a rating one is not a curve set to the wrong
/// thing — it is two numbers read in the wrong units.
fn curve_roles() -> OptionKind {
    OptionKind::Choice {
        default: None,
        items: [
            "STORAGE",
            "DIVERSION",
            "TIDAL",
            "RATING",
            "CONTROL",
            "SHAPE",
            "WEIR",
            "PUMP1",
            "PUMP2",
            "PUMP3",
            "PUMP4",
            "PUMP5",
        ]
        .iter()
        .map(|v| hydra_common::ChoiceItem {
            value: (*v).to_string(),
            label: (*v).to_string(),
        })
        .collect(),
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
                RampHint::Banded {
                    criterion: "velocity",
                },
            ),
            var(
                "capacity",
                "Capacity used",
                "y/D",
                Some("percent"),
                RampHint::Banded {
                    criterion: "capacity",
                },
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

    /// An attribute is marked editable here only if a cell can hold it.
    ///
    /// A number or free text — which since the tag and the outlet became
    /// writable includes text that names another element. Never a shape:
    /// a cross-section carries four geometry values behind one label, so
    /// an editor offering a field for it would refuse every use of it.
    ///
    /// This asserts the *shape* an editable attribute may have. That the
    /// write actually accepts each one is asserted where the write is,
    /// by reading every editable key back after setting it — neither
    /// test would catch what the other does.
    /// A group is a name on kinds already adjacent in the catalog, not
    /// an instruction to gather kinds that are not (§4.2.1) — so an
    /// application draws a heading wherever the group changes and never
    /// reorders. A group appearing twice would draw two headings with
    /// the same words, which reads as a duplicate rather than as two
    /// runs of one thing.
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

    /// No group spans the rule the rail draws between what is on the map
    /// and what is not. A heading with a horizontal line through it is
    /// two half-headings, and the reader cannot tell which one they are
    /// under.
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

    #[test]
    fn only_values_are_marked_editable() {
        for kind in ELEMENT_KINDS {
            for a in attribute_schema(kind.id) {
                if a.editable {
                    assert!(
                        matches!(
                            a.kind,
                            OptionKind::Number { .. }
                                | OptionKind::Integer { .. }
                                | OptionKind::Text { .. }
                                | OptionKind::Choice { .. }
                                | OptionKind::Boolean { .. }
                        ),
                        "{}.{} is editable but is not a value a cell can hold",
                        kind.id,
                        a.key
                    );
                }
            }
        }
    }
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
