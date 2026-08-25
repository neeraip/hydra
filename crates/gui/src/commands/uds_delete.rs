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
        // Not a vertex, a link or a parcel: one of the collections a
        // model keeps beside its network, removed by the same surgery
        // and a different policy — see `remove_container`.
        if let Some((space, at)) = container_at(net, id) {
            return remove_container(net, space, at).map(|()| Removed::default());
        }
        // A rule is the one collection nothing points at: it names other
        // elements and no element names it, so it goes without any
        // shifting at all.
        if let Some(at) = net
            .controls
            .rules
            .iter()
            .position(|r| r.name.eq_ignore_ascii_case(id))
        {
            net.controls.rules.remove(at);
            return Ok(Removed::default());
        }
        return Err(format!("element '{id}' not found"));
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

// ── Removing a container ─────────────────────────────────────────────────────
//
// The collections a model keeps beside its network — its curves, its
// patterns, its pollutants — are referred to by *position*, exactly as
// its vertices are. Removing one is the same surgery: take the entry
// out, then move every index above it down, everywhere one is held.
//
// The policy is different, and it is the one the water-distribution side
// already applies to its own curves and patterns: **refuse while
// anything still points at it**, naming what does. That is right here
// for a reason beyond consistency. A vertex cascades because the things
// attached to one describe nothing without it — an inflow *at* a deleted
// vertex is not a decision, it is debris. A curve is the opposite: a
// storage unit whose curve is deleted still exists and still needs a
// geometry, and picking one for it is picking wrong.
//
// **`refs_into` is the single place that knows where a reference can
// be.** The check and the shift are both built on it, so they cannot
// come to disagree about a holder — which is the failure that matters,
// because a shift that misses one does not fail. It leaves an index
// naming a different curve, and the model still runs.

/// A collection a model refers to by position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Space {
    Curve,
    Series,
    Pattern,
    Gage,
    Constituent,
    LandUse,
    Transect,
    Aquifer,
    Snowpack,
    Hydrograph,
    LidControl,
    Street,
    Inlet,
}

impl Space {
    /// What the engine calls one of these, for a refusal that reads.
    fn label(self) -> &'static str {
        match self {
            Space::Curve => "curve",
            Space::Series => "time series",
            Space::Pattern => "pattern",
            Space::Gage => "rain gage",
            Space::Constituent => "pollutant",
            Space::LandUse => "land use",
            Space::Transect => "transect",
            Space::Aquifer => "aquifer",
            Space::Snowpack => "snow pack",
            Space::Hydrograph => "unit hydrograph",
            Space::LidControl => "LID control",
            Space::Street => "street",
            Space::Inlet => "inlet design",
        }
    }
}

/// Which collection an id names, and where in it.
fn container_at(net: &Network, id: &str) -> Option<(Space, usize)> {
    macro_rules! look {
        ($space:expr, $list:expr) => {
            if let Some(i) = $list.iter().position(|x| x.id.eq_ignore_ascii_case(id)) {
                return Some(($space, i));
            }
        };
    }
    look!(Space::Curve, net.curves);
    look!(Space::Series, net.timeseries);
    look!(Space::Pattern, net.patterns);
    look!(Space::Gage, net.gages);
    look!(Space::Constituent, net.constituents);
    look!(Space::LandUse, net.land_uses);
    look!(Space::Transect, net.transects);
    look!(Space::Aquifer, net.aquifers);
    look!(Space::Snowpack, net.snowpacks);
    look!(Space::Hydrograph, net.unit_hydrographs);
    look!(Space::LidControl, net.lid_controls);
    look!(Space::Street, net.streets);
    look!(Space::Inlet, net.inlets);
    None
}

/// Every reference into `space`, as a description of what holds it and
/// the slot itself.
///
/// The one enumeration. Both the refusal and the shift walk this, so a
/// holder added to the model is either in both or in neither — and a
/// holder in neither is caught by the test that removes one of every kind
/// from a model that references all of them.
#[allow(clippy::too_many_lines)] // one arm per index space; splitting hides the map
fn refs_into(net: &mut Network, space: Space) -> Vec<(String, &mut usize)> {
    use hydra::uds::model::{
        DividerRule, LinkKind, OutfallStage, OutletRating, StorageGeometry, XsectReferent,
    };
    let Network {
        vertices,
        links,
        parcels,
        gages,
        inflows,
        dry_weather,
        aquifers,
        unit_hydrographs,
        rdii,
        treatments,
        land_uses,
        lid_controls,
        lid_usage,
        inlets,
        inlet_usage,
        constituents,
        climate,
        ..
    } = net;
    let mut out: Vec<(String, &mut usize)> = Vec::new();
    match space {
        Space::Curve => {
            for v in vertices.iter_mut() {
                let id = &v.id;
                match &mut v.kind {
                    VertexKind::Outfall {
                        stage: OutfallStage::Tidal { curve },
                        ..
                    } => out.push((format!("outfall {id}"), curve)),
                    VertexKind::Storage {
                        geometry: StorageGeometry::Tabular { curve },
                        ..
                    } => out.push((format!("storage unit {id}"), curve)),
                    VertexKind::Divider {
                        rule: DividerRule::Tabular { curve },
                        ..
                    } => out.push((format!("divider {id}"), curve)),
                    _ => {}
                }
            }
            for l in links.iter_mut() {
                let id = &l.id;
                match &mut l.kind {
                    LinkKind::Pump {
                        curve: Some(curve), ..
                    } => out.push((format!("pump {id}"), curve)),
                    LinkKind::Weir {
                        coeff_curve: Some(curve),
                        ..
                    } => out.push((format!("weir {id}"), curve)),
                    LinkKind::Outlet {
                        rating: OutletRating::Tabular { curve },
                        ..
                    } => out.push((format!("outlet {id}"), curve)),
                    _ => {}
                }
                if let Some(Some(XsectReferent::Curve(i))) =
                    l.cross_section.as_mut().map(|c| &mut c.referent)
                {
                    out.push((format!("{id}'s cross-section"), i));
                }
            }
            for c in lid_controls.iter_mut() {
                let id = &c.id;
                if let Some(curve) = c.drain.as_mut().and_then(|d| d.curve.as_mut()) {
                    out.push((format!("LID control {id}"), curve));
                }
            }
            for d in inlets.iter_mut() {
                let id = &d.id;
                if let Some(curve) = d.custom_curve.as_mut() {
                    out.push((format!("inlet design {id}"), curve));
                }
            }
        }
        Space::Series => {
            for v in vertices.iter_mut() {
                let id = &v.id;
                if let VertexKind::Outfall {
                    stage: OutfallStage::Series { series },
                    ..
                } = &mut v.kind
                {
                    out.push((format!("outfall {id}"), series));
                }
            }
            for g in gages.iter_mut() {
                let id = &g.id;
                if let hydra::uds::model::GageSource::Series { series } = &mut g.source {
                    out.push((format!("rain gage {id}"), series));
                }
            }
            for u in land_uses.iter_mut() {
                let id = &u.id;
                for b in u.buildup.iter_mut().flatten() {
                    if let Some(series) = b.series.as_mut() {
                        out.push((format!("land use {id}"), series));
                    }
                }
            }
            for (n, f) in inflows.iter_mut().enumerate() {
                if let Some(series) = f.series.as_mut() {
                    out.push((format!("inflow {}", n + 1), series));
                }
            }
        }
        Space::Pattern => {
            for p in parcels.iter_mut() {
                let id = p.id.clone();
                for slot in [
                    &mut p.n_perv_pattern,
                    &mut p.dstore_pattern,
                    &mut p.infil_pattern,
                ] {
                    if let Some(i) = slot.as_mut() {
                        out.push((format!("subcatchment {id}"), i));
                    }
                }
            }
            for (n, f) in inflows.iter_mut().enumerate() {
                if let Some(i) = f.base_pattern.as_mut() {
                    out.push((format!("inflow {}", n + 1), i));
                }
            }
            for (n, d) in dry_weather.iter_mut().enumerate() {
                for slot in d.patterns.iter_mut() {
                    if let Some(i) = slot.as_mut() {
                        out.push((format!("dry weather inflow {}", n + 1), i));
                    }
                }
            }
            for a in aquifers.iter_mut() {
                let id = &a.id;
                if let Some(i) = a.evap_pattern.as_mut() {
                    out.push((format!("aquifer {id}"), i));
                }
            }
            if let Some(i) = climate.recovery_pattern.as_mut() {
                out.push(("the climate options".to_string(), i));
            }
        }
        Space::Gage => {
            for p in parcels.iter_mut() {
                let id = &p.id;
                out.push((format!("subcatchment {id}"), &mut p.gage));
            }
            for g in unit_hydrographs.iter_mut() {
                let id = &g.id;
                if let Some(i) = g.gage.as_mut() {
                    out.push((format!("unit hydrograph {id}"), i));
                }
            }
        }
        Space::Constituent => {
            for c in constituents.iter_mut() {
                let id = &c.id;
                if let Some(i) = c.co_constituent.as_mut() {
                    out.push((format!("pollutant {id}"), i));
                }
            }
            for (n, f) in inflows.iter_mut().enumerate() {
                if let Some(i) = f.constituent.as_mut() {
                    out.push((format!("inflow {}", n + 1), i));
                }
            }
            for (n, d) in dry_weather.iter_mut().enumerate() {
                if let Some(i) = d.constituent.as_mut() {
                    out.push((format!("dry weather inflow {}", n + 1), i));
                }
            }
            for (n, t) in treatments.iter_mut().enumerate() {
                out.push((format!("treatment {}", n + 1), &mut t.constituent));
            }
            for p in parcels.iter_mut() {
                let id = p.id.clone();
                for (i, _) in p.init_buildup.iter_mut() {
                    out.push((format!("subcatchment {id}"), i));
                }
            }
            for c in lid_controls.iter_mut() {
                let id = c.id.clone();
                for (i, _) in c.removals.iter_mut() {
                    out.push((format!("LID control {id}"), i));
                }
            }
        }
        Space::LandUse => {
            for p in parcels.iter_mut() {
                let id = p.id.clone();
                for (i, _) in p.land_cover.iter_mut() {
                    out.push((format!("subcatchment {id}"), i));
                }
            }
        }
        Space::Transect => {
            for l in links.iter_mut() {
                let id = &l.id;
                if let Some(Some(XsectReferent::Transect(i))) =
                    l.cross_section.as_mut().map(|c| &mut c.referent)
                {
                    out.push((format!("{id}'s cross-section"), i));
                }
            }
        }
        Space::Street => {
            for l in links.iter_mut() {
                let id = &l.id;
                if let Some(Some(XsectReferent::Street(i))) =
                    l.cross_section.as_mut().map(|c| &mut c.referent)
                {
                    out.push((format!("{id}'s cross-section"), i));
                }
            }
        }
        Space::Aquifer => {
            for p in parcels.iter_mut() {
                let id = &p.id;
                if let Some(g) = p.groundwater.as_mut() {
                    out.push((format!("subcatchment {id}"), &mut g.aquifer));
                }
            }
        }
        Space::Snowpack => {
            for p in parcels.iter_mut() {
                let id = &p.id;
                if let Some(i) = p.snowpack.as_mut() {
                    out.push((format!("subcatchment {id}"), i));
                }
            }
        }
        Space::Hydrograph => {
            for (n, r) in rdii.iter_mut().enumerate() {
                out.push((format!("sewer inflow {}", n + 1), &mut r.group));
            }
        }
        Space::LidControl => {
            for (n, u) in lid_usage.iter_mut().enumerate() {
                out.push((format!("LID deployment {}", n + 1), &mut u.control));
            }
        }
        Space::Inlet => {
            for (n, u) in inlet_usage.iter_mut().enumerate() {
                out.push((format!("inlet placement {}", n + 1), &mut u.design));
            }
        }
    }
    out
}

/// Take a container out, once nothing points at it.
///
/// The refusal names every holder, and names them together: a modeller
/// who has to detach three things wants to know that before they start,
/// which is the same reason the vertex path collects its refusals.
fn remove_container(net: &mut Network, space: Space, index: usize) -> Result<(), String> {
    let id = container_id(net, space, index);
    let mut attached: Vec<String> = refs_into(net, space)
        .into_iter()
        .filter(|(_, at)| **at == index)
        .map(|(what, _)| what)
        .collect();
    // A pollutant is also named by *position* in every land use's
    // accumulation lists, which are one slot per pollutant rather than a
    // list of indices — so a land use that has something to say about
    // this one holds it just as surely.
    if space == Space::Constituent {
        for u in &net.land_uses {
            let holds = u.buildup.get(index).is_some_and(Option::is_some)
                || u.washoff.get(index).is_some_and(Option::is_some);
            if holds {
                attached.push(format!("land use {}", u.id));
            }
        }
    }
    attached.dedup();
    if !attached.is_empty() {
        return Err(format!(
            "{} '{id}' is still attached to {}; detach it first",
            space.label(),
            attached.join(", ")
        ));
    }

    remove_at(net, space, index);
    // Everything above it moves down by one. Nothing is left pointing at
    // the hole, because nothing pointed at it.
    for (_, at) in refs_into(net, space) {
        if *at > index {
            *at -= 1;
        }
    }
    Ok(())
}

/// The id at a position, for the refusal message.
fn container_id(net: &Network, space: Space, index: usize) -> String {
    macro_rules! id_at {
        ($list:expr) => {
            $list.get(index).map(|x| x.id.clone()).unwrap_or_default()
        };
    }
    match space {
        Space::Curve => id_at!(net.curves),
        Space::Series => id_at!(net.timeseries),
        Space::Pattern => id_at!(net.patterns),
        Space::Gage => id_at!(net.gages),
        Space::Constituent => id_at!(net.constituents),
        Space::LandUse => id_at!(net.land_uses),
        Space::Transect => id_at!(net.transects),
        Space::Aquifer => id_at!(net.aquifers),
        Space::Snowpack => id_at!(net.snowpacks),
        Space::Hydrograph => id_at!(net.unit_hydrographs),
        Space::LidControl => id_at!(net.lid_controls),
        Space::Street => id_at!(net.streets),
        Space::Inlet => id_at!(net.inlets),
    }
}

/// Take the entry out of its own list.
fn remove_at(net: &mut Network, space: Space, index: usize) {
    match space {
        Space::Curve => drop(net.curves.remove(index)),
        Space::Series => drop(net.timeseries.remove(index)),
        Space::Pattern => drop(net.patterns.remove(index)),
        Space::Gage => drop(net.gages.remove(index)),
        Space::Constituent => {
            net.constituents.remove(index);
            // The per-pollutant slots go with it, or every land use's
            // accumulation would shift onto the next pollutant along.
            for u in &mut net.land_uses {
                if index < u.buildup.len() {
                    u.buildup.remove(index);
                }
                if index < u.washoff.len() {
                    u.washoff.remove(index);
                }
            }
        }
        Space::LandUse => drop(net.land_uses.remove(index)),
        Space::Transect => drop(net.transects.remove(index)),
        Space::Aquifer => drop(net.aquifers.remove(index)),
        Space::Snowpack => drop(net.snowpacks.remove(index)),
        Space::Hydrograph => drop(net.unit_hydrographs.remove(index)),
        Space::LidControl => drop(net.lid_controls.remove(index)),
        Space::Street => drop(net.streets.remove(index)),
        Space::Inlet => drop(net.inlets.remove(index)),
    }
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

        let mut absent: Vec<&str> = Vec::new();
        for kind in hydra::uds::descriptors::ELEMENT_KINDS {
            let Some(id) = crate::commands::uds_attrs::kind_elements(&net, kind.id)
                .ids
                .first()
                .cloned()
            else {
                // Recorded, not skipped — a silent skip is how a kind's
                // removal goes unverified for the life of a build.
                absent.push(kind.id);
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

        assert!(
            absent.is_empty(),
            "the fixture has no element of: {absent:?} — those kinds' removals \
             are unverified"
        );

        // Two outcomes now, and both are about the model.
        //
        // Nothing refuses because a removal is unbuilt. What refuses,
        // refuses because something in the network still points at it —
        // a subcatchment another drains into, a curve a storage unit
        // needs — which is a refusal a modeller can act on, and the
        // message names what to detach.
        assert!(
            refused.iter().all(|(_, e)| e.contains("detach it first")
                || e.contains("drains to it")
                || e.contains("names it")),
            "a refusal that is not about the network: {refused:?}"
        );
        // J1 is named by rule R1 now, so the first junction refuses — the
        // spatial kinds' plain removals are carried by the outfall and
        // the conduit. The rule itself goes: a control is deletable even
        // though its text is not otherwise editable.
        assert!(
            removable.contains(&"outfall")
                && removable.contains(&"conduit")
                && removable.contains(&"rule"),
            "the spatial kinds still go: {removable:?}"
        );
        // Every container in this model is attached to something, which
        // is why none of them is in `removable` — the removal itself is
        // covered by the three tests below, on models where one is not.
        assert!(
            refused.len() >= 9,
            "only {} kinds were exercised for a refusal",
            refused.len()
        );
    }

    /// A container that nothing points at is removed, and everything
    /// above it moves down.
    ///
    /// The shift is the part that cannot be seen from outside: a
    /// reference that was never moved still resolves, still writes, and
    /// still runs — it has simply come to name a different curve. So
    /// this asserts what the survivors *resolve to*, by id, rather than
    /// that the removal returned Ok.
    #[test]
    fn removing_a_container_moves_every_reference_above_it_down() {
        const THREE_CURVES: &str = "[OPTIONS]\nFLOW_UNITS CMS\n\
             [JUNCTIONS]\nJ1 10 3 0 0 0\n\
             [OUTFALLS]\nO1 8 FREE NO\n\
             [STORAGE]\nST1 5 12 0 TABULAR SC3\n\
             [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n\
             [CURVES]\nSC1 STORAGE 0 10\nSC1 1 12\n\
             SC2 STORAGE 0 20\nSC2 1 22\n\
             SC3 STORAGE 0 30\nSC3 1 32\n";
        let (mut net, _) = parse_network(THREE_CURVES);
        assert_eq!(net.curves.len(), 3);

        // SC2 is attached to nothing; SC3 is the storage unit's geometry
        // and sits above it, so it is the one that has to move.
        delete_uds_element(&mut net, "SC2").expect("SC2 is attached to nothing");
        assert_eq!(
            net.curves.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["SC1", "SC3"]
        );

        let storage = net.vertices.iter().find(|v| v.id == "ST1").expect("ST1");
        let hydra::uds::model::VertexKind::Storage {
            geometry: hydra::uds::model::StorageGeometry::Tabular { curve },
            ..
        } = &storage.kind
        else {
            panic!("ST1 is not a tabular storage unit");
        };
        assert_eq!(
            net.curves[*curve].id, "SC3",
            "the storage unit came to name a different curve"
        );
    }

    /// A container something points at refuses, and says what points at
    /// it.
    ///
    /// The rule the water-distribution side already applies to its own
    /// curves. A vertex cascades what is attached to it, because an
    /// inflow at a deleted vertex is debris rather than a decision — but
    /// a storage unit whose curve is deleted still exists and still
    /// needs a geometry, and picking one for it is picking wrong.
    #[test]
    fn removing_an_attached_container_refuses_and_names_the_holder() {
        const ONE_CURVE: &str = "[OPTIONS]\nFLOW_UNITS CMS\n\
             [JUNCTIONS]\nJ1 10 3 0 0 0\n\
             [OUTFALLS]\nO1 8 FREE NO\n\
             [STORAGE]\nST1 5 12 0 TABULAR SC1\n\
             [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n\
             [CURVES]\nSC1 STORAGE 0 10\nSC1 1 12\n";
        let (mut net, _) = parse_network(ONE_CURVE);
        let err = delete_uds_element(&mut net, "SC1").expect_err("still attached");
        assert!(err.contains("storage unit ST1"), "{err}");
        assert!(err.contains("detach it first"), "{err}");
        assert_eq!(net.curves.len(), 1, "a refusal removed nothing");
    }

    /// A pollutant is named by position twice over: by the indices other
    /// records hold, and by *where it sits* in every land use's
    /// accumulation lists, which are one slot per pollutant.
    ///
    /// The second is the one an index shift does not touch. Remove the
    /// pollutant and leave the slots alone and every land use's buildup
    /// moves onto the next pollutant along — a model that still runs and
    /// washes off the wrong thing.
    #[test]
    fn removing_a_pollutant_takes_its_slot_out_of_every_land_use() {
        const TWO: &str = "[OPTIONS]\nFLOW_UNITS CMS\n\
             [JUNCTIONS]\nJ1 10 3 0 0 0\n\
             [POLLUTANTS]\n\
             LEAD MG/L 0 0 0 0 NO\n\
             TSS MG/L 0 0 0 0 NO\n\
             [LANDUSES]\nRESID 0 0 0\n\
             [WASHOFF]\nRESID TSS EXP 0.5 1.0 0 0\n";
        let (mut net, _) = parse_network(TWO);
        assert_eq!(net.constituents.len(), 2);

        // LEAD is first and nothing refers to it; TSS is second and the
        // land use washes it off.
        delete_uds_element(&mut net, "LEAD").expect("LEAD is attached to nothing");
        assert_eq!(
            net.constituents
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TSS"]
        );
        let use_ = &net.land_uses[0];
        assert!(
            use_.washoff.first().is_some_and(Option::is_some),
            "the washoff slot did not move down with the pollutant it belongs to"
        );
    }

    use super::*;
    use hydra::swmm::inp_writer::write_inp;
    use hydra::swmm::objects::parse_network;

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
[STORAGE]
SU1 55 4 0 FUNCTIONAL 1000 0 0
[PUMPS]
PU1 SU1 SEW PC1 ON 0 0
[ORIFICES]
OR1 J2 J3 SIDE 0 0.6 NO 0
[WEIRS]
W1 J2 J3 TRANSVERSE 0.5 3.33 NO 0 0
[CONDUITS]
C1 J1 J2 400 0.013 0 0 0 0
C2 J1 D1 300 0.013 0 0 0 0
C3 D1 J3 200 0.013 0 0 0 0
C4 J3 O1 150 0.013 0 0 0 0
GUT1 J1 J3 120 0.016 0 0 0 0
SEW1 SEW O2 100 0.013 0 0 0 0
[OUTLETS]
OL1 J2 J3 0 TABULAR/DEPTH RC1 NO
[XSECTIONS]
C1 CIRCULAR 1.5 0 0 0
C2 CIRCULAR 1.5 0 0 0
C3 CIRCULAR 1.5 0 0 0
C4 IRREGULAR TR1
GUT1 STREET ST1
SEW1 CIRCULAR 1.5 0 0 0
OR1 CIRCULAR 0.5 0 0 0
W1 RECT_OPEN 0.5 1 0 0
[TRANSECTS]
NC 0.02 0.02 0.016
X1 TR1 3 0 0 0 0 0 0 0
GR 10 0 0 5 10 10
[LANDUSES]
LU1 0 0 0
[COVERAGES]
S1 LU1 50
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
[PATTERNS]
DW1 HOURLY 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1
[DWF]
J2 FLOW 0.02 DW1
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
[CURVES]
RC1 RATING 0 0
RC1 1 5
PC1 PUMP4 0 10
PC1 1 8
[CONTROLS]
RULE R1
IF NODE J1 DEPTH > 2
THEN PUMP PU1 STATUS = ON
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
        without(FULL, ids)
    }

    /// `src` with every line naming one of `ids` taken out — the model
    /// that never had them.
    ///
    /// Most sections name their element in the first token, which is one
    /// rule. Two do not, and both are elements this file can remove, so
    /// both are handled rather than left as a hole in the check:
    ///
    ///  - a **transect** is named on its `X1` line and on none of the
    ///    `GR` lines that belong to it, so striking one means dropping
    ///    its header and every survey line until the next transect
    ///    begins;
    ///  - a **coverage** names its parcel first and the land use second.
    fn without(src: &str, ids: &[&str]) -> String {
        let names = |token: &str| ids.iter().any(|id| token.eq_ignore_ascii_case(id));
        let mut out = String::new();
        let mut section = String::new();
        // Set while the survey lines of a struck transect are going past.
        let mut dropping_transect = false;
        for line in src.lines() {
            let mut tokens = line.split_whitespace();
            let Some(first) = tokens.next() else {
                out.push('\n');
                continue;
            };
            if first.starts_with('[') {
                section = first.to_ascii_uppercase();
                dropping_transect = false;
            }
            if section == "[TRANSECTS]" {
                match first {
                    "X1" => dropping_transect = tokens.next().is_some_and(names),
                    // `NC` opens a new roughness group rather than a
                    // transect, so it ends whatever was being dropped.
                    "NC" => dropping_transect = false,
                    _ => {}
                }
                if dropping_transect {
                    continue;
                }
            }
            // A coverage is the one line that names a land use second.
            if section == "[COVERAGES]" && tokens.next().is_some_and(names) {
                continue;
            }
            if names(first) {
                continue;
            }
            let mut tokens = line.split_whitespace();
            let first = tokens.next().unwrap_or_default();
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

    /// The reference model with an unreferenced entry added at the
    /// *front* of every collection it holds.
    ///
    /// At the front deliberately. A spare added at the end shifts
    /// nothing when it goes, so a removal that moved no reference at all
    /// would pass — the test would be measuring the fixture. Put first,
    /// every existing reference into that collection has to move down by
    /// one, and a holder nobody found stays behind pointing at its
    /// neighbour.
    fn with_spares() -> String {
        let spares = [
            ("[TIMESERIES]", "SPARETS 0:00 5.0"),
            ("[RAINGAGES]", "SPAREGAGE INTENSITY 1:00 1.0 TIMESERIES TS1"),
            ("[STREETS]", "SPAREST 20 0.5 2 0.016 0.1 2 1 10 4 0.02"),
            ("[INLETS]", "SPARECB GRATE 2 2 P_BAR-50"),
            (
                "[AQUIFERS]",
                "SPAREAQ 0.5 0.15 0.30 0.5 10 15 0.35 14 0.002 0 10 0.30",
            ),
            ("[POLLUTANTS]", "SPAREPOL MG/L 0 0 0 0 NO"),
            ("[HYDROGRAPHS]", "SPAREUH RG1"),
            ("[LID_CONTROLS]", "SPAREGR GR"),
            (
                "[SNOWPACKS]",
                "SPARESP PLOWABLE 0.001 0.003 32 0.10 0 0 0.5",
            ),
            (
                "[PATTERNS]",
                "SPAREPAT HOURLY 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1",
            ),
            ("[CURVES]", "SPARECV RATING 0 0"),
            ("[LANDUSES]", "SPARELU 0 0 0"),
            // After the roughness line rather than after the header: an
            // `X1` above its `NC` is a transect with no roughness group
            // to belong to.
            (
                "NC 0.02 0.02 0.016",
                "X1 SPARETR 2 0 0 0 0 0 0 0\nGR 5 0 0 5",
            ),
        ];
        let mut out = String::new();
        for line in FULL.lines() {
            out.push_str(line);
            out.push('\n');
            if let Some((_, extra)) = spares.iter().find(|(h, _)| *h == line.trim()) {
                out.push_str(extra);
                out.push('\n');
            }
        }
        out
    }

    /// Removing a container leaves the model that never had it.
    ///
    /// The strongest check available, and the one the focused tests
    /// cannot make: a reference the shift missed still resolves, still
    /// writes and still runs — it has merely come to name its neighbour.
    /// Writing the model out and comparing it against a twin parsed from
    /// source without the entry catches exactly that, because the writer
    /// resolves every index back to a name.
    ///
    /// So this is the test that would fail if the map in `refs_into` is
    /// incomplete, which is the one thing the map cannot check about
    /// itself. Verified by removing a holder and watching it fail.
    ///
    /// All thirteen collections. Two of them cost the twin builder a
    /// rule each, because neither names itself in the first token of
    /// every line it owns — a transect is named on its `X1` line and on
    /// none of its survey lines, and a coverage names its parcel first.
    #[test]
    fn removing_a_container_leaves_the_model_that_never_had_it() {
        let source = with_spares();
        let (base, diags) = parse_network(&source);
        assert!(
            !diags.iter().any(|d| format!("{d:?}").contains("Error")),
            "the spared fixture does not parse: {diags:?}"
        );

        for spare in [
            "SPARETS",
            "SPAREGAGE",
            "SPAREST",
            "SPARECB",
            "SPAREAQ",
            "SPAREPOL",
            "SPAREUH",
            "SPAREGR",
            "SPARESP",
            "SPAREPAT",
            "SPARECV",
            "SPARELU",
            "SPARETR",
        ] {
            // It really is in there, and really is first — otherwise the
            // removal shifts nothing and the comparison is vacuous.
            assert!(
                container_at(&base, spare).is_some_and(|(_, i)| i == 0),
                "{spare} is not the first entry of its collection"
            );

            let mut net = base.clone();
            delete_uds_element(&mut net, spare)
                .unwrap_or_else(|e| panic!("{spare} is attached to nothing, but: {e}"));

            let (twin, diags) = parse_network(&without(&source, &[spare]));
            assert!(
                !diags.iter().any(|d| format!("{d:?}").contains("Error")),
                "the twin without {spare} does not parse: {diags:?}"
            );
            assert_eq!(
                write_inp(&net).expect("write after delete"),
                write_inp(&twin).expect("write the twin"),
                "deleting {spare} did not produce the model that never had it",
            );
        }
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
            // The collections beside the network. Each is removed by the
            // same shift, and a collection the fixture leaves empty is a
            // shift the twin comparison is never asked to check — which
            // looks exactly like one that passed.
            ("gages", net.gages.len()),
            ("timeseries", net.timeseries.len()),
            ("constituents", net.constituents.len()),
            ("aquifers", net.aquifers.len()),
            ("unit hydrographs", net.unit_hydrographs.len()),
            ("lid controls", net.lid_controls.len()),
            ("streets", net.streets.len()),
            ("inlets", net.inlets.len()),
            ("curves", net.curves.len()),
            ("patterns", net.patterns.len()),
            ("transects", net.transects.len()),
            ("land uses", net.land_uses.len()),
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
            // J2 carries an orifice, a weir, a conduit and an outlet,
            // and all four go with it.
            ("J2", vec!["OR1", "W1", "C1", "OL1"]),
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
