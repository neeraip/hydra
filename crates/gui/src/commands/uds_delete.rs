//! Removing a drainage element, and the referential surgery it entails.
//!
//! A drainage model refers to its elements by *position*: a link names its
//! ends as indices into the vertex list, an inflow names the vertex it
//! arrives at the same way, and so on through a dozen collections. So
//! removing one vertex is never one removal — every index above it shifts,
//! and every record holding one has to be found and moved with it. A
//! missed holder does not fail: it silently comes to name a different
//! element, which is the worst outcome available.
//!
//! Two rules decide what happens to whatever pointed at the element.
//!
//! **Cascade** what has no meaning without it. A link needs two ends; an
//! inflow, a treatment, a sewer-inflow assignment and an inlet placement
//! each exist *at* one element and describe nothing once it is gone.
//! These are removed with it and named in the result, so the removal is
//! reported rather than discovered.
//!
//! **Refuse** what would need a choice. A parcel drains somewhere, and
//! "somewhere" cannot be inferred — a parcel whose outlet is deleted has
//! to be given a new one, and picking silently is picking wrong. Control
//! rules are the same: they are retained as their author's text, so a
//! rule naming a deleted element cannot be rewritten without deciding
//! what the author meant.
//!
//! The refusals are collected and reported together rather than one at a
//! time, because a user who has to detach three things wants to know that
//! before they start.
//!
//! This mirrors what the water-distribution side does in `mutations.rs` —
//! cascade the attached links, refuse while a control still names it —
//! with the wider set of attachments a drainage model carries.

use hydra::uds::model::{Network, ParcelOutlet, VertexKind};

use super::mutations::Removed;

/// Which class of thing an id names, or `None` when the model has no such
/// element.
fn locate(net: &Network, id: &str) -> Option<(&'static str, usize)> {
    if let Some(i) = net
        .vertices
        .iter()
        .position(|v| v.id.eq_ignore_ascii_case(id))
    {
        return Some(("vertex", i));
    }
    if let Some(i) = net.links.iter().position(|l| l.id.eq_ignore_ascii_case(id)) {
        return Some(("link", i));
    }
    net.parcels
        .iter()
        .position(|p| p.id.eq_ignore_ascii_case(id))
        .map(|i| ("parcel", i))
}

/// The control rules naming any of `ids` as an object.
///
/// Only the token *after* an object keyword counts, for the reason
/// `rename_in_controls` gives: drainage identifiers are routinely
/// numeric, and a vertex named `5` must not match the `5` in
/// `DEPTH > 5`.
fn rules_naming(net: &Network, ids: &[&str]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for rule in &net.controls.rules {
        let names = rule.lines.iter().any(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            (1..tokens.len()).any(|i| {
                super::uds_view::names_object(tokens[i - 1])
                    && ids.iter().any(|id| tokens[i].eq_ignore_ascii_case(id))
            })
        });
        if names && !found.contains(&rule.name) {
            found.push(rule.name.clone());
        }
    }
    found
}

/// A position map for one index space: `new[old]`, `None` where the entry
/// was removed.
struct Shift(Vec<Option<usize>>);

impl Shift {
    /// Build the map for a list of `len` entries with `removed` taken out.
    fn new(len: usize, removed: &[usize]) -> Self {
        let mut next = 0;
        Shift(
            (0..len)
                .map(|i| {
                    if removed.contains(&i) {
                        None
                    } else {
                        next += 1;
                        Some(next - 1)
                    }
                })
                .collect(),
        )
    }

    fn get(&self, old: usize) -> Option<usize> {
        self.0.get(old).copied().flatten()
    }

    /// Move an optional reference, clearing it when its target went.
    fn shift_opt(&self, slot: &mut Option<usize>) {
        if let Some(old) = *slot {
            *slot = self.get(old);
        }
    }
}

/// Remove `id` and everything that has no meaning without it.
///
/// Fails without touching the model when something would be left needing
/// a choice — the message names what, so the caller can say so.
pub(crate) fn delete_uds_element(net: &mut Network, id: &str) -> Result<Removed, String> {
    let Some((class, index)) = locate(net, id) else {
        // A container answers to the name and cannot be removed yet, and
        // saying "not found" about a thing plainly on the screen is the
        // worst of the two wrong answers. Every one of them is referenced
        // by *index* — a storage unit points at curve 3 — so removing one
        // moves every reference past it across a dozen index spaces, and
        // a removal that got that wrong would silently repoint a model at
        // the wrong curve rather than fail.
        return Err(if super::uds_create::taken(net, id) {
            format!(
                "'{id}' can be renamed and edited but not removed yet — \
                 the model refers to it by position"
            )
        } else {
            format!("element '{id}' not found")
        });
    };
    match class {
        "vertex" => delete_vertex(net, index),
        "link" => delete_link(net, index),
        _ => delete_parcel(net, index),
    }
}

fn delete_vertex(net: &mut Network, v: usize) -> Result<Removed, String> {
    let id = net.vertices[v].id.clone();
    // Links first: they cascade, so what a rule may not name includes
    // them, and so does what the display sections lose.
    let doomed_links: Vec<usize> = net
        .links
        .iter()
        .enumerate()
        .filter(|(_, l)| l.from == v || l.to == v)
        .map(|(i, _)| i)
        .collect();
    let link_ids: Vec<String> = doomed_links
        .iter()
        .map(|&i| net.links[i].id.clone())
        .collect();

    let mut blockers: Vec<String> = Vec::new();
    // A parcel draining here — or drawing groundwater here — needs a new
    // target, and only its author knows which.
    for p in &net.parcels {
        if p.outlet == ParcelOutlet::Vertex(v) {
            blockers.push(format!("{} drains to it", p.id));
        }
        if p.groundwater.as_ref().is_some_and(|g| g.vertex == v) {
            blockers.push(format!("{}'s groundwater discharges to it", p.id));
        }
    }
    let mut named: Vec<&str> = vec![id.as_str()];
    named.extend(link_ids.iter().map(String::as_str));
    for rule in rules_naming(net, &named) {
        blockers.push(format!("rule {rule} names it"));
    }
    if !blockers.is_empty() {
        return Err(refusal(&id, &blockers));
    }

    let mut removed = Removed {
        id: id.clone(),
        links: link_ids.clone(),
        ..Removed::default()
    };
    let vshift = Shift::new(net.vertices.len(), &[v]);
    let lshift = Shift::new(net.links.len(), &doomed_links);
    net.vertices.remove(v);
    retain_indexed(&mut net.links, &doomed_links);
    apply_shifts(
        net,
        &vshift,
        &lshift,
        &Shift::new(net.parcels.len(), &[]),
        &mut removed,
    );

    let mut gone: Vec<&str> = vec![id.as_str()];
    gone.extend(link_ids.iter().map(String::as_str));
    super::uds_view::remove_from_display(net, &gone);
    Ok(removed)
}

fn delete_link(net: &mut Network, l: usize) -> Result<Removed, String> {
    let id = net.links[l].id.clone();
    let blockers: Vec<String> = rules_naming(net, &[id.as_str()])
        .into_iter()
        .map(|rule| format!("rule {rule} names it"))
        .collect();
    if !blockers.is_empty() {
        return Err(refusal(&id, &blockers));
    }

    let mut removed = Removed {
        id: id.clone(),
        ..Removed::default()
    };
    let lshift = Shift::new(net.links.len(), &[l]);
    net.links.remove(l);
    apply_shifts(
        net,
        &Shift::new(net.vertices.len(), &[]),
        &lshift,
        &Shift::new(net.parcels.len(), &[]),
        &mut removed,
    );
    super::uds_view::remove_from_display(net, &[id.as_str()]);
    Ok(removed)
}

fn delete_parcel(net: &mut Network, p: usize) -> Result<Removed, String> {
    let id = net.parcels[p].id.clone();
    let mut blockers: Vec<String> = Vec::new();
    for other in &net.parcels {
        if other.id != id && other.outlet == ParcelOutlet::Parcel(p) {
            blockers.push(format!("{} drains to it", other.id));
        }
        if other
            .snowpack
            .and_then(|s| net.snowpacks.get(s))
            .and_then(|s| s.removal.as_ref())
            .and_then(|r| r.to_parcel)
            == Some(p)
        {
            blockers.push(format!("{}'s plowing transfers to it", other.id));
        }
    }
    for v in &net.vertices {
        if let VertexKind::Outfall {
            route_to_parcel: Some(target),
            ..
        } = &v.kind
        {
            if *target == p {
                blockers.push(format!("{} returns its discharge to it", v.id));
            }
        }
    }
    if !blockers.is_empty() {
        return Err(refusal(&id, &blockers));
    }

    let mut removed = Removed {
        id: id.clone(),
        ..Removed::default()
    };
    let pshift = Shift::new(net.parcels.len(), &[p]);
    net.parcels.remove(p);
    apply_shifts(
        net,
        &Shift::new(net.vertices.len(), &[]),
        &Shift::new(net.links.len(), &[]),
        &pshift,
        &mut removed,
    );
    super::uds_view::remove_from_display(net, &[id.as_str()]);
    Ok(removed)
}

fn refusal(id: &str, blockers: &[String]) -> String {
    format!("'{id}' cannot be deleted: {}", blockers.join("; "))
}

/// Drop the entries at `removed` from a vector, by position.
fn retain_indexed<T>(items: &mut Vec<T>, removed: &[usize]) {
    let mut i = 0;
    items.retain(|_| {
        let keep = !removed.contains(&i);
        i += 1;
        keep
    });
}

/// Move every stored index through the three maps, dropping the records
/// whose subject went.
///
/// One function over the whole model rather than a line at each removal
/// site: the failure this guards against is *forgetting* a holder, and a
/// list of them in one place can be read against the model's own fields.
/// Every collection holding a vertex, link or parcel index appears below
/// — adding a field to the model without adding it here is what the
/// round-trip test catches.
fn apply_shifts(net: &mut Network, vs: &Shift, ls: &Shift, ps: &Shift, removed: &mut Removed) {
    let mut note = |what: &str, n: usize| {
        if n > 0 {
            removed
                .attachments
                .push(format!("{n} {what}{}", if n == 1 { "" } else { "s" }));
        }
    };

    // Links' ends. A link whose end went is already out of the vector.
    for link in &mut net.links {
        if let (Some(from), Some(to)) = (vs.get(link.from), vs.get(link.to)) {
            link.from = from;
            link.to = to;
        }
    }
    // A divider's diverted link may simply stop existing: `None` is the
    // model's own "none named", so this is a clear rather than a refusal.
    for vertex in &mut net.vertices {
        match &mut vertex.kind {
            VertexKind::Divider { diverted_link, .. } => ls.shift_opt(diverted_link),
            VertexKind::Outfall {
                route_to_parcel, ..
            } => ps.shift_opt(route_to_parcel),
            _ => {}
        }
    }
    for pack in &mut net.snowpacks {
        if let Some(removal) = &mut pack.removal {
            ps.shift_opt(&mut removal.to_parcel);
        }
    }

    let before = net.inflows.len();
    net.inflows.retain_mut(|i| match vs.get(i.vertex) {
        Some(v) => {
            i.vertex = v;
            true
        }
        None => false,
    });
    note("inflow", before - net.inflows.len());

    let before = net.dry_weather.len();
    net.dry_weather.retain_mut(|i| match vs.get(i.vertex) {
        Some(v) => {
            i.vertex = v;
            true
        }
        None => false,
    });
    note("dry-weather inflow", before - net.dry_weather.len());

    let before = net.rdii.len();
    net.rdii.retain_mut(|r| match vs.get(r.vertex) {
        Some(v) => {
            r.vertex = v;
            true
        }
        None => false,
    });
    note("sewer-inflow assignment", before - net.rdii.len());

    let before = net.treatments.len();
    net.treatments.retain_mut(|t| match vs.get(t.vertex) {
        Some(v) => {
            t.vertex = v;
            true
        }
        None => false,
    });
    note("treatment", before - net.treatments.len());

    let before = net.inlet_usage.len();
    net.inlet_usage
        .retain_mut(|u| match (ls.get(u.link), vs.get(u.capture_vertex)) {
            (Some(link), Some(vertex)) => {
                u.link = link;
                u.capture_vertex = vertex;
                true
            }
            _ => false,
        });
    note("inlet placement", before - net.inlet_usage.len());

    let before = net.lid_usage.len();
    net.lid_usage.retain_mut(|u| match ps.get(u.parcel) {
        Some(parcel) => {
            u.parcel = parcel;
            true
        }
        None => false,
    });
    note("control-measure deployment", before - net.lid_usage.len());

    for parcel in &mut net.parcels {
        parcel.outlet = match parcel.outlet {
            ParcelOutlet::Vertex(v) => ParcelOutlet::Vertex(vs.get(v).unwrap_or(v)),
            ParcelOutlet::Parcel(p) => ParcelOutlet::Parcel(ps.get(p).unwrap_or(p)),
        };
        // A groundwater connection to a removed vertex is a refusal, not
        // a shift, so the target here is always still present.
        if let Some(g) = &mut parcel.groundwater {
            g.vertex = vs.get(g.vertex).unwrap_or(g.vertex);
        }
    }

    shift_selection(&mut net.report.vertices, vs);
    shift_selection(&mut net.report.links, ls);
    shift_selection(&mut net.report.parcels, ps);
}

/// Move a `[REPORT]` selection, dropping what is gone.
///
/// A selection that empties becomes `None` rather than an empty list —
/// "report these, and there are none" and "report none" are the same
/// instruction, and the writer only has a spelling for the second.
fn shift_selection(selection: &mut hydra::uds::model::ReportSelection, shift: &Shift) {
    use hydra::uds::model::ReportSelection as S;
    let S::Ids(ids) = selection else { return };
    let moved: Vec<usize> = ids.iter().filter_map(|&i| shift.get(i)).collect();
    *selection = if moved.is_empty() {
        S::None
    } else {
        S::Ids(moved)
    };
}

#[cfg(test)]
mod tests {

    /// Every kind the Editor offers Delete on either deletes, or says
    /// why not in words about the model.
    ///
    /// The button is offered on every row of every kind, because the
    /// Editor's row actions are generic — it does not know which kinds
    /// the removal path has arms for. So the promise this holds is not
    /// "everything can be removed", which is not true yet; it is that
    /// pressing it never answers with something about the code. "Not
    /// found" and "unknown element kind" are both statements about a
    /// thing plainly on the screen, and both read as a bug rather than
    /// as a limit.
    ///
    /// It also counts the two populations, so neither can quietly
    /// change: a kind that stops being removable, or a refusal that
    /// starts, fails here rather than in someone's hands.
    #[test]
    fn every_kind_either_deletes_or_says_why_not() {
        let (net, _) = parse_network(FULL);
        let mut removable = Vec::new();
        let mut refused = Vec::new();

        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let Some(id) = crate::commands::uds_attrs::kind_elements(&net, kind.id)
                .ids
                .first()
                .cloned()
            else {
                continue;
            };
            let mut draft = net.clone();
            match delete_uds_element(&mut draft, &id) {
                Ok(_) => removable.push(kind.id),
                Err(e) => {
                    // The two answers that describe the program rather
                    // than the model.
                    assert!(
                        !e.contains("not found") && !e.contains("unknown"),
                        "{} refuses with {e:?}, which reads as the element being missing",
                        kind.id
                    );
                    refused.push((kind.id, e));
                }
            }
        }

        // Three outcomes, and only two of them are about the model.
        //
        // A spatial element goes. A subcatchment draining into another
        // refuses for a reason the modeller can act on — that is a
        // statement about the network, not a limit. The containers
        // refuse because removing one is not built: the model refers to
        // each by position, so a removal that got the shift wrong would
        // repoint the model at the wrong curve rather than fail, and
        // failing is the better of the two until it is right.
        assert_eq!(
            removable,
            vec!["junction", "outfall", "divider", "conduit"],
            "the set of removable kinds changed"
        );

        let unbuilt: Vec<&str> = refused
            .iter()
            .filter(|(_, e)| e.contains("not removed yet"))
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            unbuilt,
            vec![
                "raingage",
                "timeseries",
                "hydrograph",
                "pollutant",
                "aquifer",
                "snowpack",
                "lidcontrol",
                "street",
                "inlet",
            ],
            "the set of kinds that cannot yet be removed changed"
        );

        // The rest refused for a reason about the network itself, which
        // is a refusal a modeller can do something about.
        let by_the_model: Vec<&str> = refused
            .iter()
            .filter(|(_, e)| !e.contains("not removed yet"))
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(by_the_model, vec!["subcatchment"]);
    }

    /// A container answers to its name and cannot be removed yet, so the
    /// refusal says that rather than "not found" — which is the worst of
    /// the two wrong answers about a thing plainly on the screen.
    #[test]
    fn removing_a_container_refuses_by_naming_the_limit() {
        let (mut net, _) = parse_network(FULL);
        net.curves.push(hydra::uds::model::Curve {
            id: "CURVE9".into(),
            kind: hydra::uds::model::CurveKind::Storage,
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        });
        let err = delete_uds_element(&mut net, "CURVE9").expect_err("not yet");
        assert!(!err.contains("not found"), "{err}");
        assert!(err.contains("by position"), "{err}");
        // And a name nothing answers to still says so.
        let err = delete_uds_element(&mut net, "NOPE").expect_err("absent");
        assert!(err.contains("not found"), "{err}");
    }
    use super::*;
    use hydra::uds::io::inp_writer::write_inp;
    use hydra::uds::io::objects::parse_network;

    /// A model with every index-holding collection populated, and with
    /// its references pointing *past* the elements the tests remove — so
    /// a holder that is never shifted comes to name the wrong element
    /// rather than merely surviving.
    ///
    /// The three deletable elements sit in the middle of their lists on
    /// purpose. J2 is the second vertex, C2 the second link (so the
    /// divider's diverted C3 has to move), and S3 the second parcel (so
    /// S2 has to move, and four separate references point at S2).
    const FULL: &str = "\
[TITLE]
reference model
[OPTIONS]
FLOW_UNITS CFS
INFILTRATION HORTON
[RAINGAGES]
RG1 INTENSITY 1:00 1.0 TIMESERIES TS1
[JUNCTIONS]
J1 100 4 0 0 0
J2 90 4 0 0 0
J3 80 4 0 0 0
SEW 60 8 0 0 0
[OUTFALLS]
O1 70 FREE NO
O2 50 FREE NO S2
[DIVIDERS]
D1 90 C3 CUTOFF 1.0 4 0 0 0
[CONDUITS]
C1 J1 J2 400 0.013 0 0 0 0
C2 J1 D1 300 0.013 0 0 0 0
C3 D1 J3 200 0.013 0 0 0 0
C4 J3 O1 150 0.013 0 0 0 0
GUT1 J1 J3 120 0.016 0 0 0 0
SEW1 SEW O2 100 0.013 0 0 0 0
[XSECTIONS]
C1 CIRCULAR 1.5 0 0 0
C2 CIRCULAR 1.5 0 0 0
C3 CIRCULAR 1.5 0 0 0
C4 CIRCULAR 1.5 0 0 0
GUT1 STREET ST1
SEW1 CIRCULAR 1.5 0 0 0
[STREETS]
ST1 20 0.5 2 0.016 0.1 2 1 10 4 0.02
[INLETS]
CB1 GRATE 2 2 P_BAR-50
[INLET_USAGE]
GUT1 CB1 SEW 1 0 0 0 0 ON_GRADE
[SUBCATCHMENTS]
S1 RG1 J3 5 40 500 0.5 0 SP1
S3 RG1 J3 2 20 300 0.5 0
S2 RG1 S1 3 30 400 0.5 0
[SUBAREAS]
S1 0.01 0.1 0.05 0.05 25 OUTLET
S3 0.01 0.1 0.05 0.05 25 OUTLET
S2 0.01 0.1 0.05 0.05 25 OUTLET
[INFILTRATION]
S1 3.0 0.5 4 7 0
S3 3.0 0.5 4 7 0
S2 3.0 0.5 4 7 0
[SNOWPACKS]
SP1 PLOWABLE 0.001 0.003 32 0.10 0 0 0.5
SP1 IMPERV 0.001 0.003 32 0.10 1.0 0.5 4.0
SP1 REMOVAL 6.0 0.3 0.2 0.1 0.1 0.3 S2
[AQUIFERS]
AQ1 0.5 0.15 0.30 0.5 10 15 0.35 14 0.002 0 10 0.30
[GROUNDWATER]
S2 AQ1 J3 6 0.001 2 0 0 0 0 * 0.4
[POLLUTANTS]
TSS MG/L 0 0 0 0 NO
[LID_CONTROLS]
GRF GR
GRF SURFACE 3 0.1 0.15 1.0 5
GRF DRAINMAT 2 0.5 0.1
[LID_USAGE]
S2 GRF 1 200 8 0 0 0
[INFLOWS]
J2 FLOW TS1 FLOW 1.0
J3 FLOW TS1 FLOW 1.0
[DWF]
J2 FLOW 0.02
J3 FLOW 0.01
[HYDROGRAPHS]
UH1 RG1
UH1 All SHORT 0.5 1.0 2.0
[RDII]
J2 UH1 50
J3 UH1 100
[TREATMENT]
J2 TSS R = 0.9
J3 TSS R = 0.5
[TIMESERIES]
TS1 0:00 1.0
TS1 1:00 2.0
[REPORT]
NODES J2 J3 SEW
LINKS C2 C3 C4 SEW1
SUBCATCHMENTS S1 S3 S2
[COORDINATES]
J1 0 0
J2 100 50
J3 200 0
D1 100 0
O1 300 0
SEW 200 -50
O2 300 -50
[Polygons]
S1 0 10
S1 10 10
S3 0 30
S3 10 30
S2 0 20
S2 10 20
";

    /// The same model with `ids` never written into it.
    ///
    /// A drainage file keys almost every line by the element it belongs
    /// to — a coordinate, a cross-section, an inflow and a treatment all
    /// begin with the identifier — so dropping the lines that *start*
    /// with a name removes that element and everything written about it,
    /// which is exactly the file a modeller would have authored had they
    /// never added it. The report selections are the one place a name
    /// appears mid-line, and are handled by name too.
    ///
    /// This is what makes the comparison meaningful: the test is not
    /// asserting that a delete produces some plausible model, it is
    /// asserting it produces *the model that never had the element*.
    fn model_without(ids: &[&str]) -> String {
        let names = |token: &str| ids.iter().any(|id| token.eq_ignore_ascii_case(id));
        let mut out = String::new();
        for line in FULL.lines() {
            let mut tokens = line.split_whitespace();
            let Some(first) = tokens.next() else {
                out.push('\n');
                continue;
            };
            if names(first) {
                continue;
            }
            if matches!(first, "NODES" | "LINKS" | "SUBCATCHMENTS") {
                let kept: Vec<&str> = tokens.filter(|t| !names(t)).collect();
                out.push_str(&format!("{first} {}\n", kept.join(" ")));
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// The fixture is only as good as what it actually contains: a
    /// holder left empty is a holder the round-trip test cannot check,
    /// and an empty collection looks exactly like a passing one.
    #[test]
    fn the_reference_model_populates_every_index_holder() {
        let (net, diags) = parse_network(FULL);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| format!("{d:?}").contains("Error"))
            .collect();
        assert!(errors.is_empty(), "the fixture does not parse: {errors:?}");
        for (what, n) in [
            ("vertices", net.vertices.len()),
            ("links", net.links.len()),
            ("parcels", net.parcels.len()),
            ("inflows", net.inflows.len()),
            ("dry weather", net.dry_weather.len()),
            ("rdii", net.rdii.len()),
            ("treatments", net.treatments.len()),
            ("inlet usage", net.inlet_usage.len()),
            ("lid usage", net.lid_usage.len()),
            ("snowpacks", net.snowpacks.len()),
        ] {
            assert!(n > 0, "the fixture has no {what}");
        }
        // The references that only exist in *some* models, and which a
        // shift would otherwise never be asked about.
        assert!(
            net.vertices.iter().any(|v| matches!(
                &v.kind,
                VertexKind::Divider {
                    diverted_link: Some(_),
                    ..
                }
            )),
            "no divider names a diverted link",
        );
        assert!(
            net.vertices.iter().any(|v| matches!(
                &v.kind,
                VertexKind::Outfall {
                    route_to_parcel: Some(_),
                    ..
                }
            )),
            "no outfall returns its discharge to a parcel",
        );
        assert!(
            net.parcels
                .iter()
                .any(|p| matches!(p.outlet, ParcelOutlet::Parcel(_))),
            "no parcel cascades to another parcel",
        );
        assert!(
            net.parcels.iter().any(|p| p.groundwater.is_some()),
            "no parcel has a groundwater connection",
        );
        assert!(
            net.snowpacks
                .iter()
                .any(|s| s.removal.as_ref().is_some_and(|r| r.to_parcel.is_some())),
            "no plowing rule transfers to a parcel",
        );
    }

    /// The invariant the whole module rests on: after a removal, every
    /// surviving reference names the element it named before.
    ///
    /// Asserted through the writer rather than by walking the model,
    /// because walking the model to check it would use the same list of
    /// holders that might be missing one. The writer resolves every
    /// index back to an identifier independently — so a reference left
    /// pointing at the wrong position shows up as the wrong *name* in
    /// the output, in whichever section holds it.
    ///
    /// The comparison model is the same file with the victim left out,
    /// so what is being asserted is precisely "deleting it produces the
    /// model that never had it".
    ///
    /// Run for one element of each index space, because they shift
    /// different holders: a vertex moves nine kinds of reference, a link
    /// three, a parcel five.
    #[test]
    fn a_removal_leaves_every_surviving_reference_naming_what_it_named() {
        // `also` is what cascades with the element, which the twin model
        // must therefore be missing too.
        for (id, also) in [
            ("J2", vec!["C1"]),
            ("C2", vec![]),
            // S3 rather than S1 or S2: both of those are referred to by
            // something and refuse (asserted separately).
            ("S3", vec![]),
        ] {
            let (mut net, _) = parse_network(FULL);
            let removed =
                delete_uds_element(&mut net, id).unwrap_or_else(|e| panic!("delete {id}: {e}"));
            assert_eq!(removed.links, also, "wrong cascade for {id}");

            let mut gone = vec![id];
            gone.extend(also);
            let (twin, diags) = parse_network(&model_without(&gone));
            assert!(
                !diags.iter().any(|d| format!("{d:?}").contains("Error")),
                "the twin without {id} does not parse: {diags:?}"
            );
            assert_eq!(
                write_inp(&net).expect("write after delete"),
                write_inp(&twin).expect("write the twin"),
                "deleting {id} did not produce the model that never had it",
            );
        }
    }

    /// The `[REPORT]` selections hold indices too, and they are the one
    /// holder the twin comparison cannot see: an index left pointing
    /// past the end of a list is dropped on the way out, so a selection
    /// that was never shifted writes the same text as one that was, and
    /// only reveals itself as the wrong element being reported on the
    /// next run.
    #[test]
    fn the_report_selections_still_name_the_same_elements() {
        use hydra::uds::model::ReportSelection as S;
        let names = |sel: &S, ids: Vec<String>| match sel {
            S::Ids(v) => v.iter().map(|&i| ids[i].clone()).collect::<Vec<_>>(),
            other => panic!("expected a list, got {other:?}"),
        };
        for (id, parcels, vertices, links) in [
            (
                "S3",
                vec!["S1", "S2"],
                vec!["J2", "J3", "SEW"],
                vec!["C2", "C3", "C4", "SEW1"],
            ),
            (
                "C2",
                vec!["S1", "S3", "S2"],
                vec!["J2", "J3", "SEW"],
                vec!["C3", "C4", "SEW1"],
            ),
            // J2 takes C1 with it, and neither was in a selection — but
            // every survivor's index moved down by one.
            (
                "J2",
                vec!["S1", "S3", "S2"],
                vec!["J3", "SEW"],
                vec!["C2", "C3", "C4", "SEW1"],
            ),
        ] {
            let (mut net, _) = parse_network(FULL);
            delete_uds_element(&mut net, id).unwrap_or_else(|e| panic!("delete {id}: {e}"));
            let pid: Vec<String> = net.parcels.iter().map(|p| p.id.clone()).collect();
            let vid: Vec<String> = net.vertices.iter().map(|v| v.id.clone()).collect();
            let lid: Vec<String> = net.links.iter().map(|l| l.id.clone()).collect();
            assert_eq!(
                names(&net.report.parcels, pid),
                parcels,
                "parcels after {id}"
            );
            assert_eq!(
                names(&net.report.vertices, vid),
                vertices,
                "vertices after {id}"
            );
            assert_eq!(names(&net.report.links, lid), links, "links after {id}");
        }
    }

    #[test]
    fn what_only_described_the_element_goes_with_it() {
        let (mut net, _) = parse_network(FULL);
        let removed = delete_uds_element(&mut net, "J2").expect("delete J2");
        // Reported, not silent: the user asked to remove one vertex and
        // four other records went.
        for expected in [
            "1 inflow",
            "1 dry-weather inflow",
            "1 sewer-inflow assignment",
            "1 treatment",
        ] {
            assert!(
                removed.attachments.iter().any(|a| a == expected),
                "{expected} not reported in {:?}",
                removed.attachments
            );
        }
    }

    #[test]
    fn the_display_lines_go_too() {
        let (mut net, _) = parse_network(FULL);
        delete_uds_element(&mut net, "J2").expect("delete J2");
        let written = write_inp(&net).expect("write");
        assert!(
            !written.contains("J2"),
            "a display line still names J2:\n{written}"
        );
    }

    #[test]
    fn a_parcel_draining_to_it_refuses_the_delete() {
        // S1 drains to J3. Deleting J3 would leave S1 discharging
        // nowhere, and only its author can say where instead.
        let (mut net, _) = parse_network(FULL);
        let before = write_inp(&net).expect("write");
        let err = delete_uds_element(&mut net, "J3").expect_err("should refuse");
        assert!(err.contains("S1 drains to it"), "unhelpful message: {err}");
        // Refusing has to mean *nothing happened*: a delete that removed
        // half of what it was going to before noticing the blocker would
        // leave a model no one asked for and no error to explain it.
        assert_eq!(
            write_inp(&net).expect("write"),
            before,
            "a refused delete still changed the model",
        );
    }

    #[test]
    fn a_rule_naming_it_refuses_the_delete() {
        let text = format!(
            "{}[CONTROLS]\nRULE R1\nIF NODE J1 DEPTH > 2\nTHEN CONDUIT C2 STATUS = CLOSED\n",
            FULL
        );
        let (mut net, _) = parse_network(&text);
        let err = delete_uds_element(&mut net, "J1").expect_err("should refuse");
        assert!(err.contains("R1"), "the rule is not named: {err}");

        // And the guard covers what *cascades*, not only what was asked
        // for: C2 is a link the delete would have taken with it.
        let (mut net, _) = parse_network(&text);
        let err = delete_uds_element(&mut net, "D1").expect_err("should refuse");
        assert!(err.contains("R1"), "the cascade was not guarded: {err}");
    }

    /// A numeric identifier is ordinary in a drainage model, and a rule
    /// comparing against a number must not read as a reference to the
    /// element that shares its spelling.
    #[test]
    fn a_number_in_a_rule_is_not_a_reference() {
        let text = format!(
            "{}[CONTROLS]\nRULE R1\nIF NODE J1 DEPTH > 2\nTHEN CONDUIT C2 STATUS = CLOSED\n",
            FULL.replace("J3", "2")
        );
        let (mut net, _) = parse_network(&text);
        // "2" is a junction here, and `DEPTH > 2` mentions it. Deleting
        // it is refused only for the reason that is actually true — the
        // parcel draining to it — never for the comparison.
        let err = delete_uds_element(&mut net, "2").expect_err("should refuse");
        assert!(
            !err.contains("R1"),
            "a numeric comparison was read as a reference: {err}"
        );
    }

    #[test]
    fn deleting_something_that_is_not_there_is_an_error_not_a_no_op() {
        let (mut net, _) = parse_network(FULL);
        let before = write_inp(&net).expect("write");
        assert!(delete_uds_element(&mut net, "NOPE").is_err());
        assert_eq!(write_inp(&net).expect("write"), before);
    }
}
