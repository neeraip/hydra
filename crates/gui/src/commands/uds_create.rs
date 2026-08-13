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
        // A divider used to be here too, and is not: its rule is never
        // read (§7.5), so nothing about a new one has to be invented —
        // which the test below asserts instead.
        let err =
            create_uds_vertex(&mut net, "storage", "X", 0.0, 0.0, 90.0).expect_err("should refuse");
        assert!(err.contains("stage-area"), "unhelpful for storage: {err}");
        for (kind, expect) in [
            ("pump", "characteristic curve"),
            ("outlet", "rating"),
            ("weir", "discharge coefficient"),
        ] {
            let err = create_uds_link(&mut net, kind, "X", "J1", "O1", 10.0, Some(0.3))
                .expect_err("should refuse");
            assert!(err.contains(expect), "unhelpful for {kind}: {err}");
        }
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
