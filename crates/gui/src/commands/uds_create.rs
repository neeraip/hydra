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
pub(crate) fn taken(net: &Network, id: &str) -> bool {
    macro_rules! any_named {
        ($($list:expr),+ $(,)?) => {
            $($list.iter().any(|x| x.id.eq_ignore_ascii_case(id)))||+
        };
    }
    any_named!(
        net.vertices,
        net.links,
        net.parcels,
        // The containers too. Without them a curve could be created
        // beside another of the same name: the reader registers every
        // object in one table, so a duplicate across kinds is a
        // duplicate, and two curves called ST1 are a model whose storage
        // units point at whichever the reader saw last.
        net.curves,
        net.patterns,
        net.timeseries,
        net.constituents,
        net.land_uses,
        net.aquifers,
        net.snowpacks,
        net.unit_hydrographs,
        net.lid_controls,
        net.transects,
        net.streets,
        net.inlets,
        net.gages,
    )
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

/// The grate families, in the predecessor's own spelling and order.
///
/// The same pair the reader keeps, restated here because the create
/// takes a keyword rather than a parsed kind — and a spelling that
/// drifted would write a family the reader cannot read back.
const GRATE_TYPES: [(&str, hydra::uds::model::GrateKind); 8] = {
    use hydra::uds::model::GrateKind as G;
    [
        ("P_BAR-50x100", G::PBar50x100),
        ("P_BAR-50", G::PBar50),
        ("P_BAR-30", G::PBar30),
        ("CURVED_VANE", G::CurvedVane),
        ("TILT_BAR-45", G::TiltBar45),
        ("TILT_BAR-30", G::TiltBar30),
        ("RETICULINE", G::Reticuline),
        ("GENERIC", G::Generic),
    ]
};

/// The curve roles, in the predecessor's own keywords.
///
/// Beside the writer's table rather than derived from it, because a
/// create takes a keyword and a spelling that drifted would write a role
/// the reader cannot read back.
const CURVE_ROLES: [(&str, hydra::uds::model::CurveKind); 12] = {
    use hydra::uds::model::CurveKind as C;
    [
        ("STORAGE", C::Storage),
        ("DIVERSION", C::Diversion),
        ("TIDAL", C::Tidal),
        ("RATING", C::Rating),
        ("CONTROL", C::Control),
        ("SHAPE", C::Shape),
        ("WEIR", C::WeirCoeff),
        ("PUMP1", C::Pump1),
        ("PUMP2", C::Pump2),
        ("PUMP3", C::Pump3),
        ("PUMP4", C::Pump4),
        ("PUMP5", C::Pump5),
    ]
};

/// The `CurveKind` a keyword names, or `None` for one no reader knows.
pub(crate) fn curve_kind(name: &str) -> Option<hydra::uds::model::CurveKind> {
    CURVE_ROLES
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// The `LidKind` a file keyword names, or `None` for one no reader
/// knows.
///
/// The eight the reader accepts, in its own order — the same list the
/// catalog offers, so a type this dialog shows is a type the file can be
/// written with.
const LID_KINDS: [(&str, hydra::uds::model::LidKind); 8] = {
    use hydra::uds::model::LidKind as K;
    [
        ("BC", K::BioRetention),
        ("RG", K::RainGarden),
        ("GR", K::GreenRoof),
        ("IT", K::InfiltrationTrench),
        ("PP", K::PermeablePavement),
        ("RB", K::RainBarrel),
        ("VS", K::VegetativeSwale),
        ("RD", K::RooftopDisconnection),
    ]
};

pub(crate) fn lid_kind(name: &str) -> Option<hydra::uds::model::LidKind> {
    LID_KINDS
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// The file's keyword for a kind — the inverse of [`lid_kind`], from the
/// same table so the two cannot drift. The read serves this, the choice
/// offers it, and the write takes it back; the enum's own spelling
/// ("BioRetention") is for programmers and appears nowhere a modeller
/// reads.
pub(crate) fn lid_keyword(kind: hydra::uds::model::LidKind) -> &'static str {
    LID_KINDS
        .iter()
        .find(|(_, v)| *v == kind)
        .map(|(k, _)| *k)
        .unwrap_or("BC")
}

/// The `GrateKind` a keyword names, or `None` for one no reader knows.
pub(crate) fn grate_kind(name: &str) -> Option<hydra::uds::model::GrateKind> {
    GRATE_TYPES
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

/// Add an inlet design.
///
/// A design may carry a grate, a curb opening and a slot at once, so all
/// three pairs of dimensions are offered and an opening given no size is
/// simply absent — which is what the file says too, a line per opening
/// the design has. A design with none of the three is refused: it
/// captures nothing, and the writer would emit no line for it, so it
/// would vanish at the next save.
pub(crate) fn create_uds_inlet(
    net: &mut Network,
    id: &str,
    grate: Option<(f64, f64)>,
    grate_type: &str,
    curb: Option<(f64, f64)>,
    slotted: Option<(f64, f64)>,
) -> Result<(), String> {
    use hydra::uds::model::{CurbInlet, GrateInlet, InletDesign, SlottedInlet, ThroatAngle};
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("inlet") {
        return Err(refuse_kind("inlet"));
    }
    if grate.is_none() && curb.is_none() && slotted.is_none() {
        return Err("an inlet needs at least one opening with a size".into());
    }
    let kind =
        grate_kind(grate_type).ok_or_else(|| format!("'{grate_type}' is not a grate type"))?;
    net.inlets.push(InletDesign {
        id: id.to_string(),
        grate: grate.map(|(length, width)| GrateInlet {
            length,
            width,
            grate: kind,
            // Only a generic grate reads these; a named family carries
            // its own published capture curve, so zero here is "not
            // stated" rather than "no open area".
            area_ratio: 0.0,
            splash_velocity: 0.0,
        }),
        curb: curb.map(|(length, height)| CurbInlet {
            length,
            height,
            throat: ThroatAngle::Horizontal,
        }),
        slotted: slotted.map(|(length, width)| SlottedInlet { length, width }),
        custom_curve: None,
        drop_grate: false,
        drop_curb: false,
    });
    Ok(())
}

/// Add a curve of a given role.
///
/// The role is the caller's and cannot be defaulted: it decides what
/// units the two columns are *read* in, so a storage curve created as a
/// rating one is not a curve set to the wrong thing, it is two numbers
/// read in the wrong units.
pub(crate) fn create_uds_curve(net: &mut Network, id: &str, role: &str) -> Result<(), String> {
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("curve") {
        return Err(refuse_kind("curve"));
    }
    let kind = curve_kind(role).ok_or_else(|| format!("'{role}' is not a curve role"))?;
    net.curves.push(hydra::uds::model::Curve {
        id: id.to_string(),
        kind,
        // Two points, as a curve of one is a value and every evaluation
        // of it an extrapolation — and not none, which the writer would
        // drop at the next save. The shape is entered afterwards.
        points: vec![(0.0, 0.0), (1.0, 1.0)],
    });
    Ok(())
}

/// Add a time pattern of a given period.
///
/// The period decides the length — twelve months, seven days,
/// twenty-four hours — so it is taken up front for the reason a curve's
/// role is: a pattern built under a guessed one is the wrong number of
/// multipliers, and the write that would correct it refuses precisely
/// because the length no longer suits.
pub(crate) fn create_uds_pattern(net: &mut Network, id: &str, period: &str) -> Result<(), String> {
    use hydra::uds::model::PatternKind as K;
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("pattern") {
        return Err(refuse_kind("pattern"));
    }
    let kind = match period.to_ascii_uppercase().as_str() {
        "MONTHLY" => K::Monthly,
        "DAILY" => K::Daily,
        "HOURLY" => K::Hourly,
        "WEEKEND" => K::Weekend,
        other => return Err(format!("'{other}' is not a pattern type")),
    };
    let n = match kind {
        K::Monthly => 12,
        K::Daily => 7,
        K::Hourly | K::Weekend => 24,
    };
    net.patterns.push(hydra::uds::model::TimePattern {
        id: id.to_string(),
        kind,
        // A period of no variation, which is what a pattern of ones
        // means rather than a value standing in for one nobody supplied.
        factors: vec![1.0; n],
    });
    Ok(())
}

/// Add an outlet between two existing vertices.
///
/// Its rating is either a tabulated curve or a power relation $Q = aH^b$,
/// so both are offered and whichever was given decides which the outlet
/// has — the same shape an inlet's three openings take. Neither is
/// invented: no value for a rating coefficient is more defensible than
/// another, which is a reason to ask for one and was never a reason to
/// refuse the kind.
pub(crate) fn create_uds_outlet(
    net: &mut Network,
    id: &str,
    from_id: &str,
    to_id: &str,
    curve_id: Option<&str>,
    functional: Option<(f64, f64)>,
) -> Result<(), String> {
    use hydra::uds::model::{OutletHeadBasis, OutletRating};
    if taken(net, id) {
        return Err(format!("ID '{id}' is already in use"));
    }
    if !creatable("outlet") {
        return Err(refuse_kind("outlet"));
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
    // The curve wins where both were given: a named relation is a
    // statement, and two numbers left at whatever a form defaulted them
    // to are not.
    let rating = if let Some(name) = curve_id.filter(|n| !n.trim().is_empty()) {
        OutletRating::Tabular {
            curve: net
                .curves
                .iter()
                .position(|c| c.id.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| format!("'{name}' is not a curve in this model"))?,
        }
    } else {
        let (coeff, exponent) = functional
            .ok_or_else(|| "an outlet needs a rating curve or a coefficient".to_string())?;
        if !(coeff.is_finite() && coeff > 0.0) {
            return Err("a rating coefficient has to be greater than zero".into());
        }
        if !exponent.is_finite() {
            return Err("a rating exponent has to be a number".into());
        }
        OutletRating::Functional { coeff, exponent }
    };
    net.links.push(Link {
        id: id.to_string(),
        from,
        to,
        kind: LinkKind::Outlet {
            offset: Offset::Depth(0.0),
            rating,
            // Depth above the outlet, which is what a rating is written
            // against unless the model says otherwise.
            head_basis: OutletHeadBasis::Depth,
            flap_gate: false,
        },
        cross_section: None,
    });
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
        position: None,
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
        "aquifer" => {
            net.aquifers.push(hydra::uds::model::Aquifer {
                id: id.to_string(),
                // Every one of these is asked for by the form and
                // arrives as an ordinary attribute write. They open at
                // zero because the engine declares no default for any of
                // them, and a groundwater parameter has no conventional
                // value the way a roughness does — which is why the form
                // asks rather than this inventing.
                porosity: 0.0,
                wilting_point: 0.0,
                field_capacity: 0.0,
                conductivity: 0.0,
                conductivity_slope: 0.0,
                tension_slope: 0.0,
                upper_evap_frac: 0.0,
                lower_evap_depth: 0.0,
                lower_loss_coeff: 0.0,
                bottom_elev: 0.0,
                water_table_elev: 0.0,
                upper_moisture: 0.0,
                // No monthly pattern on the evaporation fraction, which
                // is a state the file writes as an absent column.
                evap_pattern: None,
            });
            Ok(())
        }
        "lidcontrol" => {
            net.lid_controls.push(hydra::uds::model::LidControl {
                id: id.to_string(),
                // The type is the one thing a control measure is before
                // any layer is entered, and the form asks for it — the
                // catalog offers the eight the file can be written with.
                kind: None,
                // No layers, which is a state the file writes and reads:
                // a measure named with none is one nobody has described
                // yet, and each is entered afterwards in its own table.
                // It was the absence of *those* that made this kind
                // uncreatable, not the absence of a default for them.
                surface: None,
                soil: None,
                pavement: None,
                storage: None,
                drain: None,
                drain_mat: None,
                removals: Vec::new(),
            });
            Ok(())
        }
        "curve" => Err("a curve needs its role, which decides what its columns mean".into()),
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
        // A pattern is created by `create_uds_pattern`, which takes the
        // period: it decides how many multipliers the pattern has, so a
        // new one built under a guessed period is a table of the wrong
        // length rather than a table nobody has filled in.
        "pattern" => {
            Err("a pattern needs its period, which decides how many multipliers it has".into())
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

    /// A pattern and a curve are both created by saying what they are
    /// for, and neither goes through the container path.
    ///
    /// The difference between them is the data model's rather than the
    /// editor's — a pattern's period decides how many multipliers it
    /// has, a curve's role decides what units its two columns are read
    /// in — but the consequence is the same: a value guessed here is a
    /// table of the wrong length or numbers read in the wrong units.
    #[test]
    fn a_pattern_and_a_curve_are_created_by_what_they_are_for() {
        let mut net = model();
        for (kind, id) in [("pattern", "PX"), ("curve", "CX")] {
            let err = create_uds_container(&mut net, kind, id).expect_err("should refuse");
            assert!(
                err.contains("period") || err.contains("role"),
                "unhelpful for {kind}: {err}"
            );
        }

        // The period decides the length, which is the whole reason it is
        // taken up front rather than corrected afterwards: the write that
        // would correct it refuses precisely because the length no longer
        // suits the type.
        create_uds_pattern(&mut net, "PM", "MONTHLY").expect("a monthly pattern");
        assert_eq!(
            net.patterns
                .iter()
                .find(|p| p.id == "PM")
                .expect("PM")
                .factors,
            vec![1.0; 12]
        );
        create_uds_pattern(&mut net, "PH", "HOURLY").expect("an hourly pattern");
        assert_eq!(
            net.patterns
                .iter()
                .find(|p| p.id == "PH")
                .expect("PH")
                .factors
                .len(),
            24
        );
        assert!(create_uds_pattern(&mut net, "PZ", "FORTNIGHTLY").is_err());

        // And they survive the round trip, which is what makes one an
        // element the user added rather than one that vanishes on save.
        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        assert!(again.patterns.iter().any(|p| p.id == "PM"));
    }

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

    /// The kind that looked like it needed a form changing shape with a
    /// choice, and did not. A design may carry a grate, a curb opening
    /// and a slot at once, so all three pairs are offered and an opening
    /// given no size is absent — conditional in what it produces without
    /// being conditional in what it asks.
    #[test]
    fn an_inlet_carries_whichever_openings_were_given_a_size() {
        let mut net = gaged_model();
        create_uds_inlet(&mut net, "IN1", Some((0.6, 0.4)), "P_BAR-50", None, None)
            .expect("a grate alone");
        let made = net.inlets.iter().find(|i| i.id == "IN1").expect("IN1");
        assert!(made.grate.is_some());
        assert!(made.curb.is_none() && made.slotted.is_none());

        // And more than one at a time, which is what a combination inlet
        // is and what no single-choice form could have expressed.
        create_uds_inlet(
            &mut net,
            "IN2",
            Some((0.6, 0.4)),
            "CURVED_VANE",
            Some((1.0, 0.12)),
            None,
        )
        .expect("a combination");
        let combo = net.inlets.iter().find(|i| i.id == "IN2").expect("IN2");
        assert!(combo.grate.is_some() && combo.curb.is_some());

        // A design with no opening captures nothing and the writer would
        // emit no line for it, so it would vanish at the next save.
        assert!(create_uds_inlet(&mut net, "IN3", None, "P_BAR-50", None, None).is_err());
        // A family the reader could not read back is refused here.
        assert!(create_uds_inlet(&mut net, "IN3", Some((1.0, 1.0)), "NOPE", None, None).is_err());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        let back = again.inlets.iter().find(|i| i.id == "IN2").expect("IN2");
        assert!(back.grate.is_some() && back.curb.is_some());
    }

    /// The kind whose blocker was a choice nobody could make. A role
    /// decides what units the two columns are *read* in, so it cannot be
    /// defaulted — and until a form could ask for a choice, that made
    /// the kind unreachable rather than merely awkward.
    #[test]
    fn a_curve_is_created_under_the_role_it_is_given() {
        let mut net = gaged_model();
        create_uds_curve(&mut net, "ST9", "STORAGE").expect("a storage curve");
        let made = net.curves.iter().find(|c| c.id == "ST9").expect("ST9");
        assert_eq!(made.kind, hydra::uds::model::CurveKind::Storage);
        assert_eq!(made.points.len(), 2, "a curve of one point is a value");

        // A role no reader knows is refused rather than written.
        assert!(create_uds_curve(&mut net, "X", "NOPE").is_err());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        // The role has to survive the round trip, because it is the one
        // thing that says how to read the two numbers beside it.
        let back = again.curves.iter().find(|c| c.id == "ST9").expect("ST9");
        assert_eq!(back.kind, hydra::uds::model::CurveKind::Storage);
    }

    /// The last kind refused for wanting a value, and the refusal was
    /// stale by the standard the weir set: no value for a rating
    /// coefficient is more defensible than another, which is a reason to
    /// ask for one rather than to refuse the kind.
    #[test]
    fn an_outlet_is_rated_by_whichever_of_the_two_it_was_given() {
        let mut net = gaged_model();
        create_uds_outlet(&mut net, "OU1", "J1", "O1", None, Some((10.0, 0.5)))
            .expect("a power relation");
        let made = net.links.iter().find(|l| l.id == "OU1").expect("OU1");
        assert!(matches!(
            made.kind,
            LinkKind::Outlet {
                rating: hydra::uds::model::OutletRating::Functional { .. },
                ..
            }
        ));

        net.curves.push(hydra::uds::model::Curve {
            id: "RC1".into(),
            kind: hydra::uds::model::CurveKind::Rating,
            points: vec![(0.0, 0.0), (1.0, 0.05)],
        });
        create_uds_outlet(&mut net, "OU2", "J1", "O1", Some("RC1"), None).expect("a curve");
        let tabulated = net.links.iter().find(|l| l.id == "OU2").expect("OU2");
        assert!(matches!(
            tabulated.kind,
            LinkKind::Outlet {
                rating: hydra::uds::model::OutletRating::Tabular { .. },
                ..
            }
        ));

        // Neither given is refused: an outlet with no rating passes no
        // flow, and that is not a state a model may hold.
        assert!(create_uds_outlet(&mut net, "X", "J1", "O1", None, None).is_err());
        // Nor a coefficient of nothing, nor a curve that is not there.
        assert!(create_uds_outlet(&mut net, "X", "J1", "O1", None, Some((0.0, 0.5))).is_err());
        assert!(create_uds_outlet(&mut net, "X", "J1", "O1", Some("NOPE"), None).is_err());

        let written = write_inp(&net).expect("write");
        let (again, diags) = parse_network(&written);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "{diags:?}\n{written}"
        );
        for id in ["OU1", "OU2"] {
            assert!(
                again.links.iter().any(|l| l.id == id),
                "{id} did not survive"
            );
        }
    }

    /// One namespace for everything the reader registers, containers
    /// included. Without them in the check a curve could be created
    /// beside another of the same name, and a model with two curves
    /// called ST1 has storage units pointing at whichever the reader saw
    /// last.
    #[test]
    fn a_container_name_already_in_use_is_refused() {
        let mut net = gaged_model();
        create_uds_curve(&mut net, "C9", "STORAGE").expect("curve");
        assert!(create_uds_curve(&mut net, "C9", "RATING").is_err());
        assert!(create_uds_curve(&mut net, "c9", "RATING").is_err(), "§14.2");
        // And across kinds, since they share the one table.
        assert!(create_uds_container(&mut net, "pattern", "C9").is_err());
        assert!(create_uds_vertex(&mut net, "junction", "C9", 0.0, 0.0, 9.0).is_err());
        assert_eq!(net.curves.iter().filter(|c| c.id == "C9").count(), 1);
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
