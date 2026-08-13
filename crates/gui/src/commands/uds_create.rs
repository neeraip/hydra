//! Adding a drainage element.
//!
//! The mirror of `uds_delete`, and the harder half of what used to be one
//! "structure" capability. Deleting asks *what points at this*; creating
//! asks *what does a new one of these need*, and the answer has to be a
//! complete, valid element — a model where a conduit has no cross-section
//! or a storage unit no stage-area relation is not a model that can run.
//!
//! So the same two rules apply, in the same order.
//!
//! **Default** what has a defensible value. Most of a new element's
//! fields do: a junction's maximum depth of zero means "raise it to the
//! crown of the highest connecting conduit", which is the predecessor's
//! own convention and the right answer for a junction nobody has
//! surveyed. An initial depth of zero is a dry network at the start of a
//! run. These are not placeholders — they are what a modeller would
//! type.
//!
//! **Refuse** what would have to be invented. A storage unit's geometry
//! is a curve or a fitted shape, a pump's characteristic is a curve, an
//! outlet's rating is a curve or a power relation, and a divider needs to
//! be told which link the flow leaves by. There is no defensible default
//! for any of them, and a made-up one is worse than a refusal: it
//! produces a model that runs and is wrong. Those kinds are named in the
//! refusal so it reads as "not this way" rather than "not supported".
//!
//! What can be created is what a sewer network is mostly made of —
//! junctions, outfalls, and the conduits between them.

use hydra::uds::model::{
    CrossSection, DividerRule, Link, LinkKind, Network, Offset, OutfallStage, Vertex, VertexKind,
    XsectShape,
};

/// The Manning roughness a new conduit gets: concrete pipe, the value
/// every drainage text prints and every model uses until someone has a
/// reason not to.
const DEFAULT_ROUGHNESS: f64 = 0.013;

/// The bore a new conduit gets, in metres: 300 mm, the smallest pipe
/// most standards allow in a public sewer and the size a modeller is
/// least surprised to have to change.
///
/// A default rather than something the caller supplies, because a
/// cross-section is more than a bore — a shape, a barrel count, a
/// culvert code — and none of the rest is editable anywhere yet. Asking
/// for one number of it at creation while the others stay out of reach
/// answers a fraction of the question and makes the fraction look like
/// the whole.
const DEFAULT_DIAMETER_M: f64 = 0.3;

/// Whether a name is already taken.
///
/// Vertices, links and parcels share one namespace here, as they do for
/// renaming: the reader registers them in one table, so a duplicate
/// across classes is a duplicate.
fn taken(net: &Network, id: &str) -> bool {
    net.vertices.iter().any(|v| v.id.eq_ignore_ascii_case(id))
        || net.links.iter().any(|l| l.id.eq_ignore_ascii_case(id))
        || net.parcels.iter().any(|p| p.id.eq_ignore_ascii_case(id))
}

/// Why this kind cannot be created, in the engine's own words.
///
/// The reason is catalog data since the editing contract landed
/// (hydra-common §4.5.3), so it is one sentence in one place rather than
/// one here and another wherever an application explains itself. A kind
/// this engine does not publish at all is a caller error, not a refusal.
fn refuse_kind(kind: &str) -> String {
    hydra::uds::descriptors::ELEMENT_KINDS
        .iter()
        .find(|k| k.id == kind)
        .map_or_else(
            || format!("unknown element kind '{kind}'"),
            |k| {
                k.not_creatable_because.map_or_else(
                    || format!("a {} cannot be added here", k.label.to_lowercase()),
                    |why| format!("{why}, so it cannot be added from the map yet"),
                )
            },
        )
}

/// Whether the engine's catalog says this kind can be created at all.
fn creatable(kind: &str) -> bool {
    hydra::uds::descriptors::ELEMENT_KINDS
        .iter()
        .any(|k| k.id == kind && k.creatable)
}

/// Add a vertex at `(x, y)` in the model's own coordinate system.
///
/// `invert` is the invert elevation in metres, as every other numeric
/// value crossing this boundary is.
pub(crate) fn create_uds_vertex(
    net: &mut Network,
    kind: &str,
    id: &str,
    x: f64,
    y: f64,
    invert: f64,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable(kind) {
        return Err(refuse_kind(kind));
    }
    let vertex_kind = match kind {
        "junction" => VertexKind::Junction {
            // Zero is not "no depth": §14.7 raises a zero maximum depth
            // to the crown of the highest connecting conduit at
            // validation, which is the right answer for a junction whose
            // rim nobody has surveyed.
            max_depth: 0.0,
            init_depth: 0.0,
            surcharge_depth: 0.0,
            ponded_area: 0.0,
        },
        "outfall" => VertexKind::Outfall {
            // The only boundary condition that needs nothing said about
            // it: free outfall takes the smaller of critical and normal
            // depth at the connecting channel. Fixed needs a stage,
            // tidal and series need a referent.
            stage: OutfallStage::Free,
            flap_gate: false,
            route_to_parcel: None,
        },
        "divider" => VertexKind::Divider {
            // Nothing invented, which is why this kind is creatable at
            // all. Under the one routing form this engine solves a
            // divider is an ordinary junction (§7.5) and the rule is
            // never read — it travels with the model for the import
            // record. `None` is what the file writes as `*`, and the
            // overflow rule is the one that takes no parameters, so a
            // new divider diverts nothing until it is told where to.
            diverted_link: None,
            rule: DividerRule::Overflow,
            // A junction's defaults, because that is what this is.
            max_depth: 0.0,
            init_depth: 0.0,
            surcharge_depth: 0.0,
            ponded_area: 0.0,
        },
        // Every other kind was refused above, by the catalog. This arm
        // is reached only by a kind that is creatable and has no
        // constructor here, which is a gap in this file rather than a
        // refusal to report.
        other => return Err(format!("no constructor for vertex kind '{other}'")),
    };
    net.vertices.push(Vertex {
        id: id.to_string(),
        invert,
        kind: vertex_kind,
    });
    super::uds_view::set_display_point(net, "[COORDINATES]", id, x, y);
    Ok(())
}

/// Add an orifice or a weir between two existing vertices.
///
/// Both are an opening of a given size, and the size is what the caller
/// supplies: there is no conventional height for an orifice or crest for
/// a weir, so nothing here invents one. Their discharge coefficients are
/// the opposite case — 0.65 and 1.84 are the values every text prints,
/// and the catalog declares them so a form starts there.
pub(crate) fn create_uds_opening(
    net: &mut Network,
    kind: &str,
    id: &str,
    from_id: &str,
    to_id: &str,
    height: f64,
    width: f64,
) -> Result<(), String> {
    use hydra::uds::model::{OrificeOrientation, WeirForm};
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable(kind) {
        return Err(refuse_kind(kind));
    }
    let find = |name: &str| {
        net.vertices
            .iter()
            .position(|v| v.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("'{name}' is not a node in this model"))
    };
    let from = find(from_id)?;
    let to = find(to_id)?;
    if from == to {
        return Err("a link needs two different ends".into());
    }
    for (what, v) in [("height", height), ("width", width)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(format!("an opening needs a positive {what}"));
        }
    }
    let link_kind = match kind {
        "orifice" => LinkKind::Orifice {
            // A side orifice flush with the invert, which is what an
            // opening drawn between two nodes means before anyone says
            // otherwise — the same stance the conduit's offsets take.
            orientation: OrificeOrientation::Side,
            offset: Offset::Depth(0.0),
            discharge_coeff: 0.65,
            flap_gate: false,
            open_close_time: 0.0,
        },
        "weir" => LinkKind::Weir {
            form: WeirForm::Transverse,
            offset: Offset::Depth(0.0),
            discharge_coeff: 1.84,
            flap_gate: false,
            end_contractions: 0.0,
            end_coeff: 0.0,
            can_surcharge: false,
            road_width: 0.0,
            road_surface: hydra::uds::model::RoadSurface::Unspecified,
            coeff_curve: None,
        },
        other => return Err(format!("no constructor for opening kind '{other}'")),
    };
    let per_unit = net.options.flow_units.m_per_length_unit();
    net.links.push(Link {
        id: id.to_string(),
        from,
        to,
        kind: link_kind,
        // §5 carries geometry in the file's own units, so the two
        // dimensions convert on the way in through the mapping the file
        // was read under.
        cross_section: Some(CrossSection {
            shape: if kind == "orifice" {
                XsectShape::RectClosed
            } else {
                XsectShape::RectOpen
            },
            geom_user: [height / per_unit, width / per_unit, 0.0, 0.0],
            barrels: 1,
            culvert_code: 0,
            referent: None,
        }),
    });
    Ok(())
}

/// Add a storage unit at `(x, y)`.
///
/// Prismatic: its area does not vary with depth, which is the one shape
/// a single number can describe. A tabulated or fitted relation is a
/// curve or three coefficients, and neither is something a create can be
/// handed as one value.
pub(crate) fn create_uds_storage(
    net: &mut Network,
    id: &str,
    x: f64,
    y: f64,
    invert: f64,
    max_depth: f64,
    area: f64,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("storage") {
        return Err(refuse_kind("storage"));
    }
    for (what, v) in [("depth", max_depth), ("surface area", area)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(format!("a storage unit needs a positive {what}"));
        }
    }
    net.vertices.push(Vertex {
        id: id.to_string(),
        invert,
        kind: VertexKind::Storage {
            max_depth,
            init_depth: 0.0,
            // A = c + a·y^b with a = 0: the depth term vanishes and the
            // constant *is* the area, which is what a prismatic tank is.
            geometry: hydra::uds::model::StorageGeometry::Functional {
                coeff: 0.0,
                exponent: 0.0,
                constant: area,
            },
            surcharge_depth: 0.0,
            evap_fraction: 0.0,
            seepage: None,
        },
    });
    super::uds_view::set_display_point(net, "[COORDINATES]", id, x, y);
    Ok(())
}

/// Add a rain gage reading a time series.
///
/// The series has to exist, which is what the refusal this replaces was
/// about — and now that the catalog declares which kind the source names,
/// a form can ask for one rather than the kind being unreachable.
pub(crate) fn create_uds_gage(net: &mut Network, id: &str, series_id: &str) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("raingage") {
        return Err(refuse_kind("raingage"));
    }
    let series = net
        .timeseries
        .iter()
        .position(|t| t.id.eq_ignore_ascii_case(series_id))
        .ok_or_else(|| format!("'{series_id}' is not a time series in this model"))?;
    net.gages.push(hydra::uds::model::Gage {
        id: id.to_string(),
        // Intensity at an hourly interval, which is how a rainfall
        // record is most often written and what the series it reads
        // will be interpreted as. Both are ordinary values the modeller
        // changes; neither decides what any number *means* the way a
        // curve's role does.
        form: hydra::uds::model::RainForm::Intensity,
        interval: 3600.0,
        catch_factor: 1.0,
        source: hydra::uds::model::GageSource::Series { series },
    });
    Ok(())
}

/// Add a pump following a characteristic curve.
pub(crate) fn create_uds_pump(
    net: &mut Network,
    id: &str,
    from_id: &str,
    to_id: &str,
    curve_id: &str,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("pump") {
        return Err(refuse_kind("pump"));
    }
    let find = |name: &str| {
        net.vertices
            .iter()
            .position(|v| v.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("'{name}' is not a node in this model"))
    };
    let from = find(from_id)?;
    let to = find(to_id)?;
    if from == to {
        return Err("a link needs two different ends".into());
    }
    let curve = net
        .curves
        .iter()
        .position(|c| c.id.eq_ignore_ascii_case(curve_id))
        .ok_or_else(|| format!("'{curve_id}' is not a curve in this model"))?;
    net.links.push(Link {
        id: id.to_string(),
        from,
        to,
        kind: LinkKind::Pump {
            curve: Some(curve),
            // Running at the start, and no shutoff band — a pump whose
            // depths are both zero runs whenever the wet well has
            // anything in it, which is what a pump with nothing said
            // about its controls does.
            initial_on: true,
            startup_depth: 0.0,
            shutoff_depth: 0.0,
        },
        // A pump is not a conduit: it has no cross-section, and the
        // writer emits none for it.
        cross_section: None,
    });
    Ok(())
}

/// Add a street section.
///
/// Every dimension is the caller's: a crown width, a curb height and a
/// cross slope describe one particular street, and no value for any of
/// them is more defensible than another. Only the roughness has a
/// convention, and it is the one this engine already defaults a channel
/// to.
pub(crate) fn create_uds_street(
    net: &mut Network,
    id: &str,
    crown_width: f64,
    curb_height: f64,
    cross_slope: f64,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("street") {
        return Err(refuse_kind("street"));
    }
    for (what, v) in [
        ("crown width", crown_width),
        ("curb height", curb_height),
        ("cross slope", cross_slope),
    ] {
        if !(v.is_finite() && v > 0.0) {
            return Err(format!("a street needs a positive {what}"));
        }
    }
    net.streets.push(hydra::uds::model::Street {
        id: id.to_string(),
        crown_width,
        curb_height,
        // Described as a percentage and stored as a fraction, the same
        // pair a subcatchment's slope makes.
        cross_slope: cross_slope / 100.0,
        roughness: DEFAULT_ROUGHNESS,
        gutter_depression: 0.0,
        gutter_width: 0.0,
        sides: 2,
        // No backing behind the curb, which is what a street with none
        // is written as rather than a value standing in for one.
        backing_width: 0.0,
        backing_slope: 0.0,
        backing_roughness: 0.0,
    });
    Ok(())
}

/// Add a container element — a pattern (§4.5.3).
///
/// A curve is deliberately absent. Its role decides the *units* its two
/// columns are read in — a storage curve is depth against area, a rating
/// curve head against discharge — so a curve created with a guessed role
/// stores numbers under an interpretation nobody chose, which is the
/// "runs and is wrong" outcome this file refuses. The role is not
/// editable anywhere yet, so there is nowhere to correct it either.
///
/// A pattern's kind decides its *length*, not its meaning, and a flat
/// hourly pattern is a complete answer rather than a placeholder.
pub(crate) fn create_uds_container(net: &mut Network, kind: &str, id: &str) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable(kind) {
        return Err(refuse_kind(kind));
    }
    match kind {
        "pollutant" => {
            net.constituents.push(hydra::uds::model::Constituent {
                id: id.to_string(),
                units: hydra::uds::model::ConcentrationUnits::MgPerL,
                // Every one of these is a real zero rather than a value
                // standing in for one nobody supplied: a constituent that
                // exists and which nothing yet generates. What accumulates
                // it lives on the land uses, not here — the refusal this
                // replaces said otherwise and was simply wrong about
                // where buildup and washoff are kept.
                c_rain: 0.0,
                c_groundwater: 0.0,
                c_rdii: 0.0,
                decay: 0.0,
                snow_only: false,
                co_constituent: None,
                co_fraction: 0.0,
                c_dwf: 0.0,
                c_init: 0.0,
            });
            Ok(())
        }
        "landuse" => {
            net.land_uses.push(hydra::uds::model::LandUse {
                id: id.to_string(),
                // No street cleaning, and no accumulation for any
                // constituent — a land use that covers ground and
                // contributes nothing until its relations are given. Both
                // are states a model may hold rather than placeholders.
                sweep_interval: 0.0,
                sweep_removal: 0.0,
                sweep_days_since: 0.0,
                buildup: Vec::new(),
                washoff: Vec::new(),
            });
            Ok(())
        }
        "transect" => {
            net.transects.push(hydra::uds::model::Transect {
                id: id.to_string(),
                // The roughnesses arrive with the rest of the create as
                // ordinary attribute writes; these are the values the
                // engine already defaults a channel to, so a transect
                // nobody has surveyed conveys like the pipes around it.
                n_left: DEFAULT_ROUGHNESS,
                n_right: DEFAULT_ROUGHNESS,
                n_channel: DEFAULT_ROUGHNESS,
                x_left: 0.0,
                x_right: 0.0,
                meander_factor: 1.0,
                // Two survey points, because a section of one has no
                // width — and not none, which the writer would drop.
                // Flat, and the shape is the modeller's to enter.
                stations: vec![(0.0, 0.0), (0.0, 1.0)],
            });
            Ok(())
        }
        "timeseries" => {
            net.timeseries.push(hydra::uds::model::TimeSeries {
                id: id.to_string(),
                // Two readings of nothing, an hour apart. Not an empty
                // series: the writer emits a line per point, so one with
                // none would vanish at the next save — which is the
                // failure this whole file is written against.
                source: hydra::uds::model::TimeSeriesSource::Points(vec![
                    hydra::uds::model::TimeSeriesPoint {
                        time: hydra::uds::model::SeriesTime::Elapsed(0.0),
                        value: 0.0,
                    },
                    hydra::uds::model::TimeSeriesPoint {
                        time: hydra::uds::model::SeriesTime::Elapsed(3600.0),
                        value: 0.0,
                    },
                ]),
            });
            Ok(())
        }
        "pattern" => {
            net.patterns.push(hydra::uds::model::TimePattern {
                id: id.to_string(),
                kind: hydra::uds::model::PatternKind::Hourly,
                // Twenty-four hours of no variation, which is what a
                // pattern of ones means rather than a value standing in
                // for one nobody supplied.
                factors: vec![1.0; 24],
            });
            Ok(())
        }
        other => Err(format!("no constructor for container kind '{other}'")),
    }
}

/// Add a link between two existing vertices.
///
/// `length` and `diameter` are metres. The diameter reaches the model as
/// a cross-section geometry parameter, which §5 carries **in the file's
/// own units** — so it converts on the way in through the same mapping
/// the file was read under, asked of the engine rather than restated
/// here.
pub(crate) fn create_uds_link(
    net: &mut Network,
    kind: &str,
    id: &str,
    from_id: &str,
    to_id: &str,
    length: f64,
    // Metres; `None` takes `DEFAULT_DIAMETER_M`.
    diameter: Option<f64>,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable(kind) {
        return Err(refuse_kind(kind));
    }
    let find = |name: &str| {
        net.vertices
            .iter()
            .position(|v| v.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("'{name}' is not a node in this model"))
    };
    let from = find(from_id)?;
    let to = find(to_id)?;
    if from == to {
        return Err("a link needs two different ends".into());
    }
    if !(length.is_finite() && length > 0.0) {
        return Err("a conduit needs a positive length".into());
    }
    let diameter = diameter.unwrap_or(DEFAULT_DIAMETER_M);
    if !(diameter.is_finite() && diameter > 0.0) {
        return Err("a conduit needs a positive diameter".into());
    }
    if kind != "conduit" {
        return Err(format!("no constructor for link kind '{kind}'"));
    }
    let per_unit = net.options.flow_units.m_per_length_unit();
    net.links.push(Link {
        id: id.to_string(),
        from,
        to,
        kind: LinkKind::Channel {
            length,
            roughness: DEFAULT_ROUGHNESS,
            // Both ends flush with their node inverts, which is what a
            // conduit drawn between two nodes means before anyone says
            // otherwise.
            offset1: Offset::Depth(0.0),
            offset2: Offset::Depth(0.0),
            init_flow: 0.0,
            max_flow: 0.0,
            reversed: false,
            loss_inlet: 0.0,
            loss_outlet: 0.0,
            loss_avg: 0.0,
            flap_gate: false,
            seepage_rate: 0.0,
        },
        cross_section: Some(CrossSection {
            shape: XsectShape::Circular,
            geom_user: [diameter / per_unit, 0.0, 0.0, 0.0],
            barrels: 1,
            culvert_code: 0,
            referent: None,
        }),
    });
    Ok(())
}

/// Add a subcatchment at `(x, y)`.
///
/// Its rain gage and its outlet are taken up front rather than left to
/// the attribute writes that follow, because the model holds both as
/// indices and there is no value meaning "not yet chosen" — a parcel
/// exists pointing at something or it does not exist. The refusal names
/// whichever is missing, so a caller learns which of the two it got
/// wrong.
pub(crate) fn create_uds_parcel(
    net: &mut Network,
    id: &str,
    x: f64,
    y: f64,
    gage_id: &str,
    outlet_id: &str,
) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("subcatchment") {
        return Err(refuse_kind("subcatchment"));
    }
    let gage = net
        .gages
        .iter()
        .position(|g| g.id.eq_ignore_ascii_case(gage_id))
        .ok_or_else(|| format!("'{gage_id}' is not a rain gage in this model"))?;
    let outlet = if let Some(v) = net
        .vertices
        .iter()
        .position(|v| v.id.eq_ignore_ascii_case(outlet_id))
    {
        hydra::uds::model::ParcelOutlet::Vertex(v)
    } else if let Some(p) = net
        .parcels
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(outlet_id))
    {
        hydra::uds::model::ParcelOutlet::Parcel(p)
    } else {
        return Err(format!(
            "'{outlet_id}' is not a node or a subcatchment in this model"
        ));
    };
    net.parcels.push(hydra::uds::model::Parcel {
        id: id.to_string(),
        gage,
        outlet,
        // Everything below is an ordinary editable attribute and arrives
        // with the rest of the create. These are the engine's defaults
        // for a catchment nobody has surveyed, not placeholders: a
        // hectare of half-impervious ground at a one-percent slope is
        // what a modeller sketching a catchment starts from.
        area: 10_000.0,
        frac_imperv: 0.5,
        width: 100.0,
        slope: 0.01,
        curb_length: 0.0,
        snowpack: None,
        land_cover: Vec::new(),
        init_buildup: Vec::new(),
        // Absent, not defaulted. Each of these is a whole parameter set
        // the file supplies in its own section, and an engine reading a
        // model without them applies its own defaults — inventing one
        // here would put values in the written file that the source
        // never had.
        subareas: None,
        infiltration: None,
        groundwater: None,
        n_perv_pattern: None,
        dstore_pattern: None,
        infil_pattern: None,
    });
    super::uds_view::set_display_point(net, "[Polygons]", id, x, y);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra::uds::io::inp_writer::write_inp;
    use hydra::uds::io::objects::parse_network;

    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS CFS
[JUNCTIONS]
J1 100 4 0 0 0
[OUTFALLS]
O1 90 FREE NO
[CONDUITS]
C1 J1 O1 400 0.013 0 0 0 0
[XSECTIONS]
C1 CIRCULAR 1.5 0 0 0
[COORDINATES]
J1 0 0
O1 100 0
";

    /// The fixture plus a rain gage, for the kinds that need one.
    fn gaged_model() -> Network {
        let (net, diags) = parse_network(&MODEL.replace(
            "[COORDINATES]",
            "[RAINGAGES]\nRG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
             [TIMESERIES]\nTS1 0:00 0.0\nTS1 1:00 0.0\n\
             [COORDINATES]",
        ));
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}"
        );
        net
    }

    fn model() -> Network {
        let (net, diags) = parse_network(MODEL);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}"
        );
        net
    }

    /// The test that matters most: a created element has to survive the
    /// round trip, because an element the writer cannot express or the
    /// reader cannot read back is not an element the user has added —
    /// it is one that vanishes at the next save.
    #[test]
    fn a_created_element_writes_and_reads_back() {
        let mut net = model();
        create_uds_vertex(&mut net, "junction", "J2", 50.0, 25.0, 95.0).expect("junction");
        create_uds_vertex(&mut net, "outfall", "O2", 200.0, 0.0, 80.0).expect("outfall");
        create_uds_link(&mut net, "conduit", "C2", "J1", "J2", 55.9, Some(0.4572))
            .expect("conduit");

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| format!("{d:?}").contains("Error"))
            .collect();
        assert!(
            errors.is_empty(),
            "the written model does not parse: {errors:?}\n{written}"
        );

        let junction = again
            .vertices
            .iter()
            .find(|v| v.id == "J2")
            .expect("J2 survived");
        assert!((junction.invert - 95.0).abs() < 1e-9);
        assert!(matches!(junction.kind, VertexKind::Junction { .. }));

        let conduit = again.links.iter().find(|l| l.id == "C2").expect("C2");
        assert_eq!(again.vertices[conduit.from].id, "J1");
        assert_eq!(again.vertices[conduit.to].id, "J2");
        let LinkKind::Channel {
            length, roughness, ..
        } = conduit.kind
        else {
            panic!("C2 is not a channel");
        };
        assert!((length - 55.9).abs() < 1e-6, "length drifted: {length}");
        assert!((roughness - DEFAULT_ROUGHNESS).abs() < 1e-12);
    }

    /// The diameter is the one value that does not cross this boundary
    /// in SI: §5 carries a cross-section's geometry in the file's own
    /// units, so an 18-inch pipe is `1.5` in a CFS file and `0.4572` in
    /// an SI one. Applying the mapping one way and not the other is the
    /// mistake that put a value three times out in the writer.
    #[test]
    fn a_diameter_is_written_in_the_files_own_units() {
        let mut us = model();
        create_uds_link(&mut us, "conduit", "C2", "J1", "O1", 100.0, Some(0.4572))
            .expect("conduit");
        let xs = us.links.last().unwrap().cross_section.as_ref().unwrap();
        assert!(
            (xs.geom_user[0] - 1.5).abs() < 1e-9,
            "0.4572 m should be 1.5 ft in a CFS file, got {}",
            xs.geom_user[0]
        );

        let (mut si, _) = parse_network(&MODEL.replace("FLOW_UNITS CFS", "FLOW_UNITS CMS"));
        create_uds_link(&mut si, "conduit", "C2", "J1", "O1", 100.0, Some(0.4572))
            .expect("conduit");
        let xs = si.links.last().unwrap().cross_section.as_ref().unwrap();
        assert!(
            (xs.geom_user[0] - 0.4572).abs() < 1e-12,
            "an SI file takes the metres unchanged, got {}",
            xs.geom_user[0]
        );
    }

    #[test]
    fn a_new_vertex_gets_a_coordinate() {
        let mut net = model();
        create_uds_vertex(&mut net, "junction", "J2", 50.0, 25.0, 95.0).expect("junction");
        // Written into the display section the engine preserves
        // verbatim, because that is where a drainage model keeps
        // geometry — a vertex without one is on no map.
        let written = write_inp(&net).expect("write");
        assert!(
            written.contains("J2 50 25"),
            "no coordinate for J2:\n{written}"
        );
    }

    #[test]
    fn a_name_already_in_use_is_refused() {
        let mut net = model();
        // Across classes, not just within one: the reader registers
        // vertices, links and parcels in a single table.
        assert!(create_uds_vertex(&mut net, "junction", "C1", 1.0, 1.0, 90.0).is_err());
        // And case-insensitively, per §14.2.
        assert!(create_uds_vertex(&mut net, "junction", "j1", 1.0, 1.0, 90.0).is_err());
        assert_eq!(net.vertices.len(), 2, "a refused create still added one");
    }

    #[test]
    fn a_kind_that_would_need_an_invented_value_is_refused_by_name() {
        let mut net = model();
        // Storage and the divider used to be here and are not: a
        // divider's rule is never read (§7.5), and a storage unit's
        // depth and area are asked for rather than invented. Both are
        // asserted by their own tests below.
        //
        // Storage, the divider and the rain gage all used to be here and
        // are not: what each needs is now asked for rather than invented,
        // and each has its own test below.
        // The one link kind still refused, and it refuses on principle
        // rather than for want of plumbing: a rating's coefficient sets
        // how much flow the outlet passes, and no value for that is more
        // defensible than another.
        let err = create_uds_link(&mut net, "outlet", "X", "J1", "O1", 10.0, Some(0.3))
            .expect_err("should refuse");
        assert!(err.contains("rating"), "unhelpful for outlet: {err}");
    }

    /// A conduit added from a table names its two ends and nothing about
    /// its bore, because a bore is one number out of a cross-section and
    /// the rest is not editable anywhere yet. The engine supplies one
    /// rather than refusing — and supplies it in the file's own units,
    /// like any other geometry parameter.
    #[test]
    fn a_conduit_with_nothing_said_about_its_size_gets_the_default_bore() {
        let mut net = model();
        create_uds_link(&mut net, "conduit", "C2", "J1", "O1", 100.0, None).expect("conduit");
        let xs = net.links.last().unwrap().cross_section.as_ref().unwrap();
        // A CFS file, so 300 mm is 0.984 ft on the page.
        let per_unit = net.options.flow_units.m_per_length_unit();
        assert!(
            (xs.geom_user[0] - DEFAULT_DIAMETER_M / per_unit).abs() < 1e-12,
            "got {}",
            xs.geom_user[0]
        );
    }

    /// The refusal this kind used to carry said a divider "needs the
    /// link its diverted flow leaves by". It does not: `*` is legal
    /// input, and under the one routing form this engine solves a
    /// divider is an ordinary junction whose rule is carried for the
    /// import record and never evaluated (§7.5). So the refusal was
    /// reading the file format as if the solver used it.
    #[test]
    fn a_divider_is_created_as_the_junction_this_engine_treats_it_as() {
        let mut net = model();
        create_uds_vertex(&mut net, "divider", "D1", 50.0, 25.0, 95.0).expect("divider");
        let made = net.vertices.iter().find(|v| v.id == "D1").expect("D1");
        assert!(matches!(
            made.kind,
            VertexKind::Divider {
                diverted_link: None,
                rule: DividerRule::Overflow,
                ..
            }
        ));

        // And it survives the writer, which is what makes it an element
        // the user has added rather than one that vanishes on save. The
        // diverted link writes as `*`, the shape the reader takes back
        // as "none named".
        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(matches!(
            again
                .vertices
                .iter()
                .find(|v| v.id == "D1")
                .map(|v| &v.kind),
            Some(VertexKind::Divider {
                diverted_link: None,
                ..
            })
        ));
    }

    /// A drainage pattern is creatable and a drainage curve is not, and
    /// the difference is the data model's rather than the editor's: a
    /// pattern's kind decides its length, a curve's role decides what
    /// units its two columns are read in. There is a defensible default
    /// for the first and none for the second.
    #[test]
    fn a_pattern_is_created_flat_and_a_curve_is_refused() {
        let mut net = model();
        create_uds_container(&mut net, "pattern", "PX").expect("pattern");
        let made = net.patterns.iter().find(|p| p.id == "PX").expect("PX");
        assert_eq!(made.factors, vec![1.0; 24]);

        let err = create_uds_container(&mut net, "curve", "CX").expect_err("should refuse");
        assert!(err.contains("units"), "unhelpful: {err}");

        // And it survives the round trip, which is what makes it an
        // element the user added rather than one that vanishes on save.
        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(again.patterns.iter().any(|p| p.id == "PX"));
    }

    /// The kind whose refusal was wrong twice over. It said a
    /// subcatchment "needs an area, which is its polygon rather than a
    /// number" — the area is a plain number and the polygon is optional
    /// display geometry. What it really needs is a gage and an outlet,
    /// and both are references a create can be given.
    #[test]
    fn a_subcatchment_is_created_from_a_gage_and_an_outlet() {
        let mut net = gaged_model();
        create_uds_parcel(&mut net, "S1", 10.0, 20.0, "RG1", "J1").expect("subcatchment");
        let made = net.parcels.iter().find(|p| p.id == "S1").expect("S1");
        assert_eq!(net.gages[made.gage].id, "RG1");
        assert!(matches!(
            made.outlet,
            hydra::uds::model::ParcelOutlet::Vertex(_)
        ));
        assert!(made.area > 0.0, "a catchment of no area drains nothing");

        // Both references are checked, and neither is invented.
        assert!(create_uds_parcel(&mut net, "S2", 0.0, 0.0, "NOPE", "J1").is_err());
        assert!(create_uds_parcel(&mut net, "S2", 0.0, 0.0, "RG1", "NOPE").is_err());
        assert_eq!(net.parcels.len(), 1, "a refused create still added one");

        // And it survives the writer, which is what makes it an element
        // the user added rather than one that vanishes on save.
        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        let back = again.parcels.iter().find(|p| p.id == "S1").expect("S1");
        assert_eq!(again.gages[back.gage].id, "RG1");
    }

    /// A time series with no points is not an empty series — it is a
    /// series the writer emits nothing for, which vanishes at the next
    /// save. So a new one carries two readings.
    #[test]
    fn a_new_time_series_survives_being_written() {
        let mut net = gaged_model();
        create_uds_container(&mut net, "timeseries", "TS9").expect("series");
        let written = write_inp(&net).expect("write");
        let (again, _) = parse_network(&written);
        assert!(
            again.timeseries.iter().any(|t| t.id == "TS9"),
            "the series did not survive the round trip:\n{written}"
        );
    }

    /// Two kinds whose every value is a genuine zero rather than one
    /// standing in for a number nobody supplied. A constituent that
    /// exists and which nothing generates, and a land use that covers
    /// ground and contributes nothing, are both states a model may hold.
    ///
    /// The pollutant's refusal also named the wrong thing: buildup and
    /// washoff are kept on the land uses, not on the constituent.
    #[test]
    fn a_pollutant_and_a_land_use_are_created_empty_and_survive_the_writer() {
        let mut net = gaged_model();
        create_uds_container(&mut net, "pollutant", "TSS").expect("pollutant");
        create_uds_container(&mut net, "landuse", "Residential").expect("land use");

        let made = net
            .constituents
            .iter()
            .find(|c| c.id == "TSS")
            .expect("TSS");
        assert_eq!(made.c_rain, 0.0);
        assert_eq!(made.decay, 0.0);
        assert!(made.co_constituent.is_none());
        let use_ = net
            .land_uses
            .iter()
            .find(|l| l.id == "Residential")
            .expect("Residential");
        assert!(use_.buildup.is_empty() && use_.washoff.is_empty());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(again.constituents.iter().any(|c| c.id == "TSS"));
        assert!(again.land_uses.iter().any(|l| l.id == "Residential"));
    }

    /// The three kinds whose refusal was plumbing rather than
    /// judgement. Each needs one or two plain numbers that no convention
    /// supplies, so the caller supplies them — and each coefficient that
    /// *does* have a conventional value is the engine's own.
    #[test]
    fn an_opening_and_a_tank_are_created_from_the_sizes_they_are_given() {
        let mut net = gaged_model();
        create_uds_opening(&mut net, "orifice", "OR1", "J1", "O1", 0.5, 0.4).expect("orifice");
        create_uds_opening(&mut net, "weir", "W1", "J1", "O1", 1.2, 3.0).expect("weir");
        create_uds_storage(&mut net, "ST1", 5.0, 5.0, 90.0, 4.0, 250.0).expect("storage");

        let orifice = net.links.iter().find(|l| l.id == "OR1").expect("OR1");
        let LinkKind::Orifice {
            discharge_coeff, ..
        } = orifice.kind
        else {
            panic!("OR1 is not an orifice");
        };
        assert!((discharge_coeff - 0.65).abs() < 1e-12);
        // A CFS file, so the opening is stored in feet.
        let per_unit = net.options.flow_units.m_per_length_unit();
        let xs = orifice.cross_section.as_ref().expect("a cross-section");
        assert!((xs.geom_user[0] - 0.5 / per_unit).abs() < 1e-12);

        let store = net.vertices.iter().find(|v| v.id == "ST1").expect("ST1");
        let VertexKind::Storage { geometry, .. } = &store.kind else {
            panic!("ST1 is not a storage unit");
        };
        // Prismatic: the depth term vanishes and the constant is the area.
        assert!(matches!(
            geometry,
            hydra::uds::model::StorageGeometry::Functional {
                coeff, constant, ..
            } if *coeff == 0.0 && (*constant - 250.0).abs() < 1e-12
        ));

        // A size nobody gave is refused rather than invented.
        assert!(create_uds_opening(&mut net, "orifice", "X", "J1", "O1", 0.0, 0.4).is_err());
        assert!(create_uds_storage(&mut net, "X", 0.0, 0.0, 90.0, 4.0, 0.0).is_err());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        for id in ["OR1", "W1"] {
            assert!(
                again.links.iter().any(|l| l.id == id),
                "{id} did not survive"
            );
        }
        assert!(again.vertices.iter().any(|v| v.id == "ST1"));
    }

    /// The last two flat records. A street's dimensions describe one
    /// particular street, so all three are asked for; a transect's shape
    /// is its survey points, which became editable as contents — so a new
    /// one starts flat and is surveyed afterwards, exactly as a curve is.
    #[test]
    fn a_street_and_a_transect_are_created_and_survive_the_writer() {
        let mut net = gaged_model();
        create_uds_street(&mut net, "ST9", 12.0, 0.15, 4.0).expect("street");
        let made = net.streets.iter().find(|s| s.id == "ST9").expect("ST9");
        assert!((made.crown_width - 12.0).abs() < 1e-12);
        // Described as a percentage, stored as a fraction.
        assert!((made.cross_slope - 0.04).abs() < 1e-12);
        // A dimension nobody gave is refused rather than invented.
        assert!(create_uds_street(&mut net, "X", 0.0, 0.15, 4.0).is_err());

        create_uds_container(&mut net, "transect", "TR9").expect("transect");
        let t = net.transects.iter().find(|t| t.id == "TR9").expect("TR9");
        assert_eq!(t.stations.len(), 2, "a section of one station has no width");
        assert!(t.n_channel > 0.0, "a roughness of nought divides by zero");

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(again.streets.iter().any(|s| s.id == "ST9"));
        assert!(again.transects.iter().any(|t| t.id == "TR9"));
    }

    /// The two kinds whose blocker was a referent, reached by the same
    /// path the subcatchment took: the catalog says which kind the
    /// reference names, so a form can ask for one.
    #[test]
    fn a_gage_and_a_pump_are_created_from_what_they_name() {
        let mut net = gaged_model();
        create_uds_gage(&mut net, "RG2", "TS1").expect("gage");
        let made = net.gages.iter().find(|g| g.id == "RG2").expect("RG2");
        assert!(matches!(
            made.source,
            hydra::uds::model::GageSource::Series { .. }
        ));
        // The series has to exist — that was the whole refusal.
        assert!(create_uds_gage(&mut net, "RG3", "NOPE").is_err());

        net.curves.push(hydra::uds::model::Curve {
            id: "PC1".into(),
            kind: hydra::uds::model::CurveKind::Pump4,
            points: vec![(0.0, 0.0), (1.0, 0.05)],
        });
        create_uds_pump(&mut net, "PU1", "J1", "O1", "PC1").expect("pump");
        let pump = net.links.iter().find(|l| l.id == "PU1").expect("PU1");
        assert!(matches!(pump.kind, LinkKind::Pump { curve: Some(_), .. }));
        // A pump is not a conduit and carries no cross-section.
        assert!(pump.cross_section.is_none());
        assert!(create_uds_pump(&mut net, "PU2", "J1", "O1", "NOPE").is_err());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(again.gages.iter().any(|g| g.id == "RG2"));
        assert!(again.links.iter().any(|l| l.id == "PU1"));
    }

    #[test]
    fn a_link_needs_two_different_ends_that_exist() {
        let mut net = model();
        assert!(create_uds_link(&mut net, "conduit", "C2", "J1", "J1", 10.0, Some(0.3)).is_err());
        assert!(create_uds_link(&mut net, "conduit", "C2", "J1", "NOPE", 10.0, Some(0.3)).is_err());
        assert_eq!(net.links.len(), 1, "a refused create still added one");
    }

    #[test]
    fn a_conduit_with_no_size_is_refused_rather_than_written() {
        // Zero and NaN both reach here from a field someone cleared, and
        // a zero-diameter conduit is a model that runs and is wrong.
        let mut net = model();
        for (length, diameter) in [(0.0, 0.3), (10.0, 0.0), (f64::NAN, 0.3), (10.0, f64::NAN)] {
            assert!(
                create_uds_link(
                    &mut net,
                    "conduit",
                    "C2",
                    "J1",
                    "O1",
                    length,
                    Some(diameter)
                )
                .is_err(),
                "accepted length {length} diameter {diameter}",
            );
        }
    }
}
