//! The uds viewer snapshot: geometry for a read-only urban-drainage canvas.
//!
//! The engine stores no parsed display geometry — its interop contract
//! (§14.5) preserves the `[COORDINATES]`, `[VERTICES]`, and `[POLYGONS]`
//! sections verbatim precisely so applications may consume them. This
//! module is that consumer: it resolves the preserved lines against the
//! parsed network into classed viewer elements (points, polylines,
//! regions — the hydra-common §4.1 element classes) and encodes them as
//! **snapshot layout v4**, the generic sibling of the wds-specific v3.
//!
//! The leading `u32` is how the frontend decoder tells the two layouts
//! apart, so it answers "which format" and "which version of it" at once
//! and the two numbers share one namespace. See
//! [`GENERIC_SNAPSHOT_VERSION`].
//!
//! Layout version 4:
//!
//! ```text
//! offset  size            content
//! 0       4               version      (u32 LE, = 4)
//! 4       4               flags        (u32 LE; bit 0 = snapshot present)
//! 8       4               n_points     (u32 LE)
//! 12      4               n_polylines  (u32 LE)
//! 16      4               n_regions    (u32 LE)
//! 20      4               n_kinds      (u32 LE; kind string table entries)
//! 24      4               total_bends  (u32 LE; Σ bends over polylines)
//! 28      4               total_ring   (u32 LE; Σ ring points over regions)
//! 32      16              reserved     (u32 LE × 4, all 0)
//! 48      8·n_points      point x                    (f64 LE)
//! …       8·n_points      point y                    (f64 LE)
//! …       8·total_bends   bend x                     (f64 LE; polyline order)
//! …       8·total_bends   bend y                     (f64 LE; polyline order)
//! …       8·total_ring    ring x                     (f64 LE; region order)
//! …       8·total_ring    ring y                     (f64 LE; region order)
//! …       4·n_polylines   polyline from point index  (i32 LE; -1 = none)
//! …       4·n_polylines   polyline to point index    (i32 LE; -1 = none)
//! …       4·n_regions     region outlet point index  (i32 LE; -1 = none)
//! …       4·n_polylines   polyline bend count        (u32 LE)
//! …       4·n_regions     region ring count          (u32 LE)
//! …       1·n_points      point kind     (u8; index into kind table)
//! …       1·n_polylines   polyline kind  (u8)
//! …       1·n_regions     region kind    (u8)
//! then 4 string columns, each `u32 LE byte_len` + newline-joined UTF-8:
//!   kind table | point ids | polyline ids | region ids
//! ```
//!
//! Kind entries are the engine's element-kind ids (`uds::descriptors`), so
//! the frontend resolves labels and badges from the catalog rather than
//! from anything baked in here. Elements without resolvable geometry are
//! omitted — a model authored without a map simply renders empty.

use std::collections::HashMap;

use hydra::uds::model::{LinkKind, Network, ParcelOutlet, VertexKind};

/// Leading `u32` of a generic viewer snapshot.
///
/// It is a format discriminator as much as a version: the frontend reads
/// this word to choose between this layout and the water-distribution one,
/// so it must never equal
/// [`NETWORK_SNAPSHOT_VERSION`](super::binary_codec::NETWORK_SNAPSHOT_VERSION)
/// — bumping either for a new column would hand every snapshot of one kind
/// to the other's decoder. `a_snapshot_word_names_one_layout` holds that
/// line here; `network.test.ts` holds it on the far side, because neither
/// compiler can see the other's constant.
pub(crate) const GENERIC_SNAPSHOT_VERSION: u32 = 4;

/// One located element of the viewer snapshot.
pub(crate) struct ViewPoint {
    pub kind: &'static str,
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// One connecting element, referencing point indices.
pub(crate) struct ViewPolyline {
    pub kind: &'static str,
    pub id: String,
    pub from: i32,
    pub to: i32,
    pub bends: Vec<[f64; 2]>,
}

/// One areal element with its polygon ring.
pub(crate) struct ViewRegion {
    pub kind: &'static str,
    pub id: String,
    pub ring: Vec<[f64; 2]>,
    pub outlet: i32,
}

pub(crate) struct UdsView {
    pub points: Vec<ViewPoint>,
    pub polylines: Vec<ViewPolyline>,
    pub regions: Vec<ViewRegion>,
}

/// Parse `id x y` triples from a preserved display section, tolerating and
/// skipping malformed lines (the sections are display metadata — a bad line
/// costs one element its geometry, never the load).
/// The map positions the model carries, for callers asking only where the
/// network sits — the import wizard's coordinate-system question, which is
/// asked before any viewer snapshot exists.
pub(crate) fn model_coordinates(net: &Network) -> impl Iterator<Item = (f64, f64)> + '_ {
    parse_xy_lines(net, "[COORDINATES]").map(|(_, x, y)| (x, y))
}

fn parse_xy_lines<'a>(
    net: &'a Network,
    header: &'a str,
) -> impl Iterator<Item = (&'a str, f64, f64)> + 'a {
    net.display
        .iter()
        .filter(move |s| s.header.eq_ignore_ascii_case(header))
        .flat_map(|s| s.lines.iter())
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let id = it.next()?;
            let x: f64 = it.next()?.parse().ok()?;
            let y: f64 = it.next()?.parse().ok()?;
            (x.is_finite() && y.is_finite()).then_some((id, x, y))
        })
}

fn vertex_kind(kind: &VertexKind) -> &'static str {
    match kind {
        VertexKind::Junction { .. } => "junction",
        VertexKind::Outfall { .. } => "outfall",
        VertexKind::Storage { .. } => "storage",
        VertexKind::Divider { .. } => "divider",
    }
}

fn link_kind(kind: &LinkKind) -> &'static str {
    match kind {
        LinkKind::Channel { .. } => "conduit",
        LinkKind::Pump { .. } => "pump",
        LinkKind::Orifice { .. } => "orifice",
        LinkKind::Weir { .. } => "weir",
        LinkKind::Outlet { .. } => "outlet",
    }
}

/// Resolve the preserved display sections against the network into viewer
/// elements. Elements without geometry are omitted; identifiers resolve
/// case-insensitively nowhere — display sections quote ids as written,
/// exactly as the model stores them.
/// A deterministic schematic for a model that places nothing (see
/// `build_view`): vertices in layers by hop distance from the outfalls —
/// water at the bottom, headwaters at the top — spread evenly within
/// each layer, unconnected components continuing beyond the deepest
/// layer. Distances are arbitrary drawing units; the frontend fits the
/// view to whatever bounds arrive.
fn synthesize_positions(net: &Network) -> Vec<(f64, f64)> {
    const SPACING: f64 = 100.0;
    let n = net.vertices.len();
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    for l in &net.links {
        neighbours[l.from].push(l.to);
        neighbours[l.to].push(l.from);
    }
    // Layer 0: the outfalls — or, in a network without one, vertex 0.
    let mut depth = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    for (vi, v) in net.vertices.iter().enumerate() {
        if matches!(v.kind, VertexKind::Outfall { .. }) {
            depth[vi] = 0;
            queue.push_back(vi);
        }
    }
    if queue.is_empty() {
        depth[0] = 0;
        queue.push_back(0);
    }
    let mut deepest = 0;
    while let Some(vi) = queue.pop_front() {
        for &next in &neighbours[vi] {
            if depth[next] == usize::MAX {
                depth[next] = depth[vi] + 1;
                deepest = deepest.max(depth[next]);
                queue.push_back(next);
            }
        }
    }
    // Components the outfall search never reached: stacked in their own
    // layers beyond the deepest, registration order.
    for vi in 0..n {
        if depth[vi] == usize::MAX {
            deepest += 1;
            depth[vi] = deepest;
            let mut q = std::collections::VecDeque::from([vi]);
            while let Some(u) = q.pop_front() {
                for &next in &neighbours[u] {
                    if depth[next] == usize::MAX {
                        depth[next] = depth[u] + 1;
                        deepest = deepest.max(depth[next]);
                        q.push_back(next);
                    }
                }
            }
        }
    }
    // Within each layer, registration order, centred about x = 0; depth
    // grows upward so outfalls sit at the bottom of the drawing.
    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); deepest + 1];
    for vi in 0..n {
        by_layer[depth[vi]].push(vi);
    }
    let mut out = vec![(0.0, 0.0); n];
    for (layer, members) in by_layer.iter().enumerate() {
        let width = (members.len().saturating_sub(1)) as f64 * SPACING;
        for (slot, &vi) in members.iter().enumerate() {
            // Offset off the origin: (0, 0) is the placeholder the
            // frontend reads as "no coordinates" — a vertex landing there
            // loses its zoom affordance, drops out of the canvas bounds,
            // and is counted as unplaced. A centred layer-0 member lands
            // exactly there without this.
            out[vi] = (
                slot as f64 * SPACING - width / 2.0 + SPACING / 2.0,
                layer as f64 * SPACING + SPACING / 2.0,
            );
        }
    }
    out
}

pub(crate) fn build_view(net: &Network) -> UdsView {
    // [COORDINATES]: vertex id → map position.
    let mut coords: HashMap<&str, (f64, f64)> = parse_xy_lines(net, "[COORDINATES]")
        .map(|(id, x, y)| (id, (x, y)))
        .collect();

    // A model may carry no map placement at all — display sections are
    // optional, and a hand-written or tutorial file often omits them. An
    // element the viewer drops is an element the network list, canvas,
    // and details cannot reach, so a placement is synthesized instead:
    // vertices layered by hop distance from the outfalls, which reads as
    // the schematic of a drainage network draining downward. Only the
    // all-missing case is synthesized — a model that places *some*
    // vertices has an authored frame, and inventing positions inside it
    // would look like data.
    let synthesized: Vec<(f64, f64)>;
    let synthesized_frame = coords.is_empty() && !net.vertices.is_empty();
    if synthesized_frame {
        synthesized = synthesize_positions(net);
        for (vi, xy) in synthesized.iter().enumerate() {
            coords.insert(net.vertices[vi].id.as_str(), *xy);
        }
    }

    let mut points = Vec::new();
    let mut point_index: HashMap<&str, i32> = HashMap::new();
    for v in &net.vertices {
        if let Some(&(x, y)) = coords.get(v.id.as_str()) {
            point_index.insert(v.id.as_str(), points.len() as i32);
            points.push(ViewPoint {
                kind: vertex_kind(&v.kind),
                id: v.id.clone(),
                x,
                y,
            });
        }
    }

    // [VERTICES]: link id → intermediate bend, in file order.
    let mut bends: HashMap<&str, Vec<[f64; 2]>> = HashMap::new();
    for (id, x, y) in parse_xy_lines(net, "[VERTICES]") {
        bends.entry(id).or_default().push([x, y]);
    }
    let mut polylines = Vec::new();
    for l in &net.links {
        let from = *point_index
            .get(net.vertices[l.from].id.as_str())
            .unwrap_or(&-1);
        let to = *point_index
            .get(net.vertices[l.to].id.as_str())
            .unwrap_or(&-1);
        if from < 0 && to < 0 {
            continue;
        }
        polylines.push(ViewPolyline {
            kind: link_kind(&l.kind),
            id: l.id.clone(),
            from,
            to,
            bends: bends.get(l.id.as_str()).cloned().unwrap_or_default(),
        });
    }

    // [POLYGONS]: subcatchment id → boundary ring, in file order.
    let mut rings: HashMap<&str, Vec<[f64; 2]>> = HashMap::new();
    for (id, x, y) in parse_xy_lines(net, "[POLYGONS]") {
        rings.entry(id).or_default().push([x, y]);
    }
    // The same optional-section rule as coordinates: a model that draws
    // no polygons at all gets each subcatchment as a small square beside
    // its (possibly transitive) outlet, stacked when several share one —
    // reachable and selectable rather than invisible. A model that draws
    // some keeps its authored frame.
    // Rings are synthesized only inside a synthesized frame. The square's
    // dimensions are drawing units of *this* layout; emitted into an
    // authored frame they are meaningless — in a degrees model a 60-unit
    // box is 60° wide, pushing the ring past longitude −180 and wrecking
    // the canvas fit. One frame per view: either the model placed things
    // or this did.
    let synthesize_rings = synthesized_frame && rings.is_empty() && !net.parcels.is_empty();
    let mut stacked_at: HashMap<i32, usize> = HashMap::new();
    let mut regions = Vec::new();
    for p in &net.parcels {
        // Cascades resolve to the vertex the chain finally drains to.
        let outlet_vertex = {
            let mut hops = 0;
            let mut current = p;
            loop {
                match current.outlet {
                    ParcelOutlet::Vertex(vi) => break Some(vi),
                    ParcelOutlet::Parcel(pi) => {
                        hops += 1;
                        if hops > net.parcels.len() {
                            break None;
                        }
                        current = &net.parcels[pi];
                    }
                }
            }
        };
        let outlet = outlet_vertex
            .and_then(|vi| point_index.get(net.vertices[vi].id.as_str()).copied())
            .unwrap_or(-1);
        let ring: Vec<[f64; 2]> = match rings.get(p.id.as_str()) {
            Some(ring) if ring.len() >= 3 => ring.clone(),
            Some(_) => continue,
            None if synthesize_rings && outlet >= 0 => {
                let (cx, cy) = {
                    let anchor = &points[outlet as usize];
                    let stack = stacked_at.entry(outlet).or_insert(0);
                    let slot = *stack;
                    *stack += 1;
                    (anchor.x - 70.0 - 75.0 * slot as f64, anchor.y + 55.0)
                };
                let r = 30.0;
                vec![
                    [cx - r, cy - r],
                    [cx + r, cy - r],
                    [cx + r, cy + r],
                    [cx - r, cy + r],
                ]
            }
            None => continue,
        };
        regions.push(ViewRegion {
            kind: "subcatchment",
            id: p.id.clone(),
            ring,
            outlet,
        });
    }

    UdsView {
        points,
        polylines,
        regions,
    }
}

/// Encode a [`UdsView`] as snapshot layout v4 (module docs).
pub(crate) fn encode_uds_snapshot(view: &UdsView) -> Vec<u8> {
    let mut kinds: Vec<&'static str> = Vec::new();
    let kind_idx = |k: &'static str, kinds: &mut Vec<&'static str>| -> u8 {
        match kinds.iter().position(|x| *x == k) {
            Some(i) => i as u8,
            None => {
                kinds.push(k);
                (kinds.len() - 1) as u8
            }
        }
    };
    let point_kinds: Vec<u8> = view
        .points
        .iter()
        .map(|p| kind_idx(p.kind, &mut kinds))
        .collect();
    let polyline_kinds: Vec<u8> = view
        .polylines
        .iter()
        .map(|p| kind_idx(p.kind, &mut kinds))
        .collect();
    let region_kinds: Vec<u8> = view
        .regions
        .iter()
        .map(|r| kind_idx(r.kind, &mut kinds))
        .collect();

    let total_bends: usize = view.polylines.iter().map(|p| p.bends.len()).sum();
    let total_ring: usize = view.regions.iter().map(|r| r.ring.len()).sum();

    let mut buf = Vec::new();
    for v in [
        GENERIC_SNAPSHOT_VERSION,
        1, // flags: present
        view.points.len() as u32,
        view.polylines.len() as u32,
        view.regions.len() as u32,
        kinds.len() as u32,
        total_bends as u32,
        total_ring as u32,
        0,
        0,
        0,
        0,
    ] {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    let push_f64s = |buf: &mut Vec<u8>, it: &mut dyn Iterator<Item = f64>| {
        for v in it {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    };
    push_f64s(&mut buf, &mut view.points.iter().map(|p| p.x));
    push_f64s(&mut buf, &mut view.points.iter().map(|p| p.y));
    push_f64s(
        &mut buf,
        &mut view.polylines.iter().flat_map(|p| &p.bends).map(|b| b[0]),
    );
    push_f64s(
        &mut buf,
        &mut view.polylines.iter().flat_map(|p| &p.bends).map(|b| b[1]),
    );
    push_f64s(
        &mut buf,
        &mut view.regions.iter().flat_map(|r| &r.ring).map(|b| b[0]),
    );
    push_f64s(
        &mut buf,
        &mut view.regions.iter().flat_map(|r| &r.ring).map(|b| b[1]),
    );
    for p in &view.polylines {
        buf.extend_from_slice(&p.from.to_le_bytes());
    }
    for p in &view.polylines {
        buf.extend_from_slice(&p.to.to_le_bytes());
    }
    for r in &view.regions {
        buf.extend_from_slice(&r.outlet.to_le_bytes());
    }
    for p in &view.polylines {
        buf.extend_from_slice(&(p.bends.len() as u32).to_le_bytes());
    }
    for r in &view.regions {
        buf.extend_from_slice(&(r.ring.len() as u32).to_le_bytes());
    }
    buf.extend_from_slice(&point_kinds);
    buf.extend_from_slice(&polyline_kinds);
    buf.extend_from_slice(&region_kinds);

    let push_strings = |buf: &mut Vec<u8>, items: &mut dyn Iterator<Item = &str>| {
        let joined = items.collect::<Vec<_>>().join("\n");
        buf.extend_from_slice(&(joined.len() as u32).to_le_bytes());
        buf.extend_from_slice(joined.as_bytes());
    };
    push_strings(&mut buf, &mut kinds.iter().copied());
    push_strings(&mut buf, &mut view.points.iter().map(|p| p.id.as_str()));
    push_strings(&mut buf, &mut view.polylines.iter().map(|p| p.id.as_str()));
    push_strings(&mut buf, &mut view.regions.iter().map(|r| r.id.as_str()));
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One word, one layout.
    ///
    /// The frontend picks a decoder by reading the first `u32`, so these two
    /// constants are not independent version counters — they are names in a
    /// shared namespace. Bumping either to the other's value would route
    /// every snapshot of one engine into the other's decoder, and neither
    /// constant's own file would look wrong while it happened.
    #[test]
    fn a_snapshot_word_names_one_layout() {
        assert_ne!(
            GENERIC_SNAPSHOT_VERSION,
            super::super::binary_codec::NETWORK_SNAPSHOT_VERSION,
            "the generic and water-distribution snapshots must stay \
             distinguishable by their leading word"
        );
    }

    const MODEL: &str = "[OPTIONS]\nFLOW_UNITS CFS\nFLOW_ROUTING DYNWAVE\n\
        [RAINGAGES]\nG1 INTENSITY 0:15 1.0 TIMESERIES R1\n\
        [SUBCATCHMENTS]\nS1 G1 J1 10 25 500 0.5 0\n\
        [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
        [INFILTRATION]\nS1 3.0 0.5 4 7 0\n\
        [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
        [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
        [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
        [TIMESERIES]\nR1 0:00 1.0\n\
        [COORDINATES]\nJ1 10 20\nO1 30 40\n\
        [VERTICES]\nC1 15 25\nC1 20 30\n\
        [POLYGONS]\nS1 0 0\nS1 10 0\nS1 10 10\nS1 0 10\n";

    #[test]
    fn preserved_display_sections_become_viewer_geometry() {
        let (net, diags) = hydra::uds::io::objects::parse_network(MODEL);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "test model must parse: {diags:?}"
        );
        let view = build_view(&net);

        assert_eq!(view.points.len(), 2);
        assert_eq!(view.points[0].id, "J1");
        assert_eq!((view.points[0].x, view.points[0].y), (10.0, 20.0));
        assert_eq!(view.points[0].kind, "junction");
        assert_eq!(view.points[1].kind, "outfall");

        assert_eq!(view.polylines.len(), 1);
        assert_eq!(view.polylines[0].kind, "conduit");
        assert_eq!(view.polylines[0].from, 0);
        assert_eq!(view.polylines[0].to, 1);
        assert_eq!(view.polylines[0].bends, vec![[15.0, 25.0], [20.0, 30.0]]);

        assert_eq!(view.regions.len(), 1);
        assert_eq!(view.regions[0].kind, "subcatchment");
        assert_eq!(view.regions[0].ring.len(), 4);
        assert_eq!(view.regions[0].outlet, 0, "S1 discharges to J1");
    }

    #[test]
    fn v4_encoding_is_self_consistent() {
        let (net, _) = hydra::uds::io::objects::parse_network(MODEL);
        let view = build_view(&net);
        let bytes = encode_uds_snapshot(&view);

        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(u32_at(0), 4, "version");
        assert_eq!(u32_at(8), 2, "points");
        assert_eq!(u32_at(12), 1, "polylines");
        assert_eq!(u32_at(16), 1, "regions");
        assert_eq!(u32_at(24), 2, "bends");
        assert_eq!(u32_at(28), 4, "ring points");
        // First f64 block starts 8-aligned at the 48-byte header boundary.
        let x0 = f64::from_le_bytes(bytes[48..56].try_into().unwrap());
        assert_eq!(x0, 10.0);
    }

    /// A model that places nothing still views completely.
    ///
    /// This reverses this module's earlier stance ("renders empty, not
    /// broken"): in practice an empty view *was* broken — no canvas, no
    /// network list, no way to reach any element — for a perfectly valid
    /// model whose author never opened a map. Display sections are
    /// optional; the elements are not.
    #[test]
    fn a_model_without_a_map_views_via_a_synthesized_schematic() {
        let bare = "[OPTIONS]\nFLOW_UNITS CFS\n[JUNCTIONS]\nJ1 100 4\nJ2 99 4\n\
                    [OUTFALLS]\nO1 98 FREE\n[CONDUITS]\nC1 J1 J2 400 0.013 0 0\n\
                    C2 J2 O1 400 0.013 0 0\n\
                    [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\nC2 CIRCULAR 1.5 0 0 0\n";
        let (net, diags) = hydra::uds::io::objects::parse_network(bare);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        let view = build_view(&net);
        assert_eq!(view.points.len(), 3, "every vertex is placed");
        assert_eq!(view.polylines.len(), 2, "every link is drawable");
        // The layout is a schematic: the outfall at the bottom, its
        // upstream chain layered above it.
        let y_of = |id: &str| view.points.iter().find(|p| p.id == id).unwrap().y;
        assert!(y_of("O1") < y_of("J2"), "outfall below its feeder");
        assert!(y_of("J2") < y_of("J1"), "headwater on top");
        // Nothing lands on the frontend's "no coordinates" placeholder:
        // a vertex there loses its zoom affordance, drops out of the
        // canvas bounds, and is counted as unplaced.
        assert!(
            !view.points.iter().any(|p| p.x == 0.0 && p.y == 0.0),
            "a synthesized vertex sits on the (0,0) sentinel: {:?}",
            view.points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()
        );
        // And deterministic: the same model draws the same picture.
        let again = build_view(&net);
        assert_eq!(
            view.points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(),
            again.points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(),
        );
    }

    /// Subcatchments of a polygon-less model appear as squares beside
    /// their outlets — reachable and selectable rather than invisible —
    /// including through a cascade, which anchors to the vertex the
    /// chain finally drains to.
    #[test]
    fn polygonless_subcatchments_are_placed_beside_their_outlets() {
        let inp = "[OPTIONS]\nFLOW_UNITS CFS\n\
            [RAINGAGES]\nG1 INTENSITY 0:15 1.0 TIMESERIES R1\n\
            [SUBCATCHMENTS]\nS1 G1 J1 10 25 500 0.5 0\nS2 G1 S1 5 25 300 0.5 0\n\
            [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
            S2 0.01 0.1 0.05 0.05 25 OUTLET\n\
            [INFILTRATION]\nS1 3.0 0.5 4 7 0\nS2 3.0 0.5 4 7 0\n\
            [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
            [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
            [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
            [TIMESERIES]\nR1 0:00 1.0\n";
        let (net, diags) = hydra::uds::io::objects::parse_network(inp);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        let view = build_view(&net);
        assert_eq!(view.regions.len(), 2, "both subcatchments visible");
        for region in &view.regions {
            assert_eq!(region.ring.len(), 4, "a square each");
            let anchor = &view.points[region.outlet as usize];
            assert_eq!(anchor.id, "J1", "anchored to the draining vertex");
        }
        // Sharing an outlet stacks rather than overlaps.
        assert_ne!(view.regions[0].ring, view.regions[1].ring);
    }

    /// Synthesized rings never enter an authored frame.
    ///
    /// The square's dimensions are drawing units of the synthesized
    /// layout; emitted into a degrees model they are 60° wide, putting
    /// the ring past longitude −180 and collapsing the canvas fit to a
    /// dot. One frame per view: either the model placed things or the
    /// synthesis did.
    #[test]
    fn polygonless_subcatchments_stay_out_of_an_authored_frame() {
        let authored = "[OPTIONS]\nFLOW_UNITS CFS\n\
            [RAINGAGES]\nG1 INTENSITY 0:15 1.0 TIMESERIES R1\n\
            [SUBCATCHMENTS]\nS1 G1 J1 10 25 500 0.5 0\n\
            [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
            [INFILTRATION]\nS1 3.0 0.5 4 7 0\n\
            [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
            [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
            [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
            [TIMESERIES]\nR1 0:00 1.0\n\
            [MAP]\nUNITS DEGREES\n\
            [COORDINATES]\nJ1 -87.63 41.88\nO1 -87.62 41.87\n";
        let (net, _) = hydra::uds::io::objects::parse_network(authored);
        let view = build_view(&net);
        assert_eq!(view.points.len(), 2, "the authored placements stand");
        assert!(
            view.regions.is_empty(),
            "no square is invented inside an authored map: {:?}",
            view.regions.iter().map(|r| &r.ring).collect::<Vec<_>>()
        );
        // Nothing escapes the geographic range the model declared.
        for p in &view.points {
            assert!((-180.0..=180.0).contains(&p.x) && (-90.0..=90.0).contains(&p.y));
        }
    }

    /// A model that places some vertices keeps its authored frame — the
    /// synthesis covers only the nothing-placed case, because invented
    /// positions inside an authored map would look like data.
    #[test]
    fn authored_coordinates_are_never_mixed_with_synthesis() {
        let partial = "[OPTIONS]\nFLOW_UNITS CFS\n[JUNCTIONS]\nJ1 100 4\n\
                    [OUTFALLS]\nO1 98 FREE\n[CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                    [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                    [COORDINATES]\nJ1 100 200\n";
        let (net, _) = hydra::uds::io::objects::parse_network(partial);
        let view = build_view(&net);
        assert_eq!(view.points.len(), 1, "only the authored placement");
        assert_eq!(view.points[0].id, "J1");
    }
}

// ── Editing the preserved display sections ──────────────────────────────

// Maintaining the identifier-keyed display metadata under mutation.
//
// The engine keeps `[COORDINATES]`, `[VERTICES]` and `[POLYGONS]` as
// opaque text (§14.5) — it has no use for geometry, and the writer emits
// them back verbatim. That is invisible while an application only reads
// them, and it is why the reader above exists.
//
// It stops being invisible the moment a model can be edited, because
// those lines are keyed by identifier: renaming an element orphans its
// line, moving one has to rewrite it, creating one has to append it, and
// deleting one leaves a line naming nothing. The water-distribution
// engine never poses the question — a node there carries its own `x` and
// `y`, so moving it is a field assignment.
//
// This is where that difference is handled, once, rather than at each
// mutation site. A "remember to also update the display sections" rule
// spread across every command is the shape of defect that gets written
// about later; concentrating it here makes it one named thing with its
// own tests.
//
// Positions are written in the model's own coordinate system, whatever
// that is: these numbers are never converted on the way in, so they are
// never converted on the way out.

/// Move or place `id`'s point in a display section, appending the line —
/// and the section — when either is absent.
pub(crate) fn set_display_point(net: &mut Network, header: &str, id: &str, x: f64, y: f64) {
    let line = format!("{id} {} {}", fmt_coord(x), fmt_coord(y));
    let Some(section) = net
        .display
        .iter_mut()
        .find(|s| s.header.eq_ignore_ascii_case(header))
    else {
        net.display.push(hydra::uds::model::DisplaySection {
            header: header.to_string(),
            lines: vec![line],
        });
        return;
    };
    match section.lines.iter_mut().find(|l| line_names(l, id)) {
        Some(existing) => *existing = line,
        None => section.lines.push(line),
    }
}

/// Rename every display line naming `old`, in every section.
///
/// All of them, not coordinates alone: a link's intermediate vertices
/// and a subcatchment's polygon are keyed the same way, so a rename that
/// fixed only the point would strand the rest of the geometry.
pub(crate) fn rename_in_display(net: &mut Network, old: &str, new: &str) {
    for section in &mut net.display {
        for line in &mut section.lines {
            if line_names(line, old) {
                let rest = line
                    .split_whitespace()
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(" ");
                *line = if rest.is_empty() {
                    new.to_string()
                } else {
                    format!("{new} {rest}")
                };
            }
        }
    }
}

/// Remove every display line naming any of `ids`, in every section.
///
/// The counterpart of `rename_in_display`, and for the same reason: a
/// deleted element's coordinate, polygon and intermediate vertices are
/// three lines in three sections, and a delete that took only the first
/// would leave the writer emitting geometry for something the model no
/// longer has.
///
/// An emptied section goes with them. `[POLYGONS]` with no polygons
/// under it is not wrong, but it is a heading the file did not have
/// before the edit, and the writer's output is compared against the
/// import.
pub(crate) fn remove_from_display(net: &mut Network, ids: &[&str]) {
    for section in &mut net.display {
        section
            .lines
            .retain(|line| !ids.iter().any(|id| line_names(line, id)));
    }
    net.display.retain(|s| !s.lines.is_empty());
}

/// Whether `token` is a keyword a control rule names an object after
/// (§9.1) — so the token following it is an identifier, not a value.
///
/// Shared by the rename and the delete guard so they cannot come to
/// disagree about what counts as naming an element: one would rewrite a
/// reference the other refused to see.
pub(crate) fn names_object(token: &str) -> bool {
    /// The keywords a rule names an object after (§9.1).
    const OBJECTS: [&str; 8] = [
        "GAGE", "NODE", "LINK", "CONDUIT", "PUMP", "ORIFICE", "WEIR", "OUTLET",
    ];
    OBJECTS.iter().any(|k| token.eq_ignore_ascii_case(k))
}

/// Rename `old` where control-rule text names it as an object.
///
/// Rules are retained as their author's text (§9.1 compiles them later),
/// so an element's name appears in them as a bare token — and a
/// rename that ignored them would leave a rule pointing at an element
/// that no longer exists.
///
/// Only the token *after* an object keyword is replaced. Substituting
/// every token equal to the old id would be shorter and wrong: drainage
/// identifiers are routinely numeric, so a node named `5` would rewrite
/// the `5` in `DEPTH > 5` and silently change what the rule tests.
pub(crate) fn rename_in_controls(net: &mut Network, old: &str, new: &str) {
    let rewrite = |line: &mut String| {
        let mut tokens: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        for i in 1..tokens.len() {
            if names_object(&tokens[i - 1]) && tokens[i].eq_ignore_ascii_case(old) {
                tokens[i] = new.to_string();
            }
        }
        *line = tokens.join(" ");
    };
    for rule in &mut net.controls.rules {
        for line in &mut rule.lines {
            rewrite(line);
        }
    }
}

/// Whether a display line's first token is `id`.
///
/// Compared case-insensitively because §14.2 matches identifiers that way:
/// a model referring to `Node1` and a coordinate line spelling it `NODE1`
/// are the same element, and a rename that missed one would silently
/// strand its geometry.
fn line_names(line: &str, id: &str) -> bool {
    line.split_whitespace()
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case(id))
}

/// A coordinate in the shortest form that reads back as the same number.
///
/// The same rule the engine's writer applies, for the same reason: these
/// are the model's own numbers in the model's own system, and a fixed
/// precision would move an element every time the file was saved.
fn fmt_coord(v: f64) -> String {
    if v == 0.0 {
        "0".into()
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod display_edit_tests {
    use super::*;

    const INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS

[JUNCTIONS]
J1  10  3  0  0  0
J2  9   3  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  J2  100  0.01  0  0  0  0

[COORDINATES]
J1  100  200
J2  300  400
O1  500  600

[VERTICES]
C1  150  250

[POLYGONS]
S1  0  0
";

    fn model() -> Network {
        let (net, diags) = hydra::uds::io::objects::parse_network(INP);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        net
    }

    fn coords(net: &Network) -> Vec<(String, f64, f64)> {
        parse_xy_lines(net, "[COORDINATES]")
            .map(|(id, x, y)| (id.to_string(), x, y))
            .collect()
    }

    #[test]
    fn moving_a_node_rewrites_its_line_and_leaves_the_others() {
        let mut net = model();
        set_display_point(&mut net, "[COORDINATES]", "J2", 999.5, -1.25);
        let after = coords(&net);
        assert_eq!(after.len(), 3, "a move must not add or drop a line");
        assert!(after.contains(&("J2".into(), 999.5, -1.25)), "{after:?}");
        assert!(after.contains(&("J1".into(), 100.0, 200.0)), "{after:?}");
    }

    #[test]
    fn placing_a_node_with_no_line_yet_appends_one() {
        let mut net = model();
        set_display_point(&mut net, "[COORDINATES]", "NEW", 1.0, 2.0);
        assert!(coords(&net).contains(&("NEW".into(), 1.0, 2.0)));
    }

    #[test]
    fn placing_into_a_section_the_model_lacks_creates_it() {
        // A model authored with no map has no `[COORDINATES]` at all, so
        // the first placement has nowhere to go unless one is made.
        let (mut net, _) = hydra::uds::io::objects::parse_network(
            "[OPTIONS]\nFLOW_UNITS CMS\n\n[JUNCTIONS]\nJ1 10 3 0 0 0\n",
        );
        assert!(coords(&net).is_empty());
        set_display_point(&mut net, "[COORDINATES]", "J1", 7.0, 8.0);
        assert_eq!(coords(&net), vec![("J1".into(), 7.0, 8.0)]);
    }

    #[test]
    fn a_move_matches_the_spelling_the_reader_would() {
        // §14.2 matches identifiers case-insensitively, so a model naming
        // `J1` and a coordinate line spelling it `j1` are one element.
        // Matching by exact text would append a second line for the same
        // node, and the reader takes the first — so the node would appear
        // not to have moved at all.
        let mut net = model();
        net.display
            .iter_mut()
            .find(|s| s.header == "[COORDINATES]")
            .unwrap()
            .lines[0] = "j1 100 200".into();
        set_display_point(&mut net, "[COORDINATES]", "J1", 5.0, 6.0);
        let after = coords(&net);
        assert_eq!(after.len(), 3, "a duplicate line was appended: {after:?}");
        assert!(after.contains(&("J1".into(), 5.0, 6.0)), "{after:?}");
    }

    #[test]
    fn a_moved_position_survives_a_write_and_a_re_read() {
        // The point of the section: the engine writes display metadata
        // back verbatim, so an edit here has to reach the file and come
        // back as itself.
        let mut net = model();
        set_display_point(&mut net, "[COORDINATES]", "J1", 12.5, -7.25);
        let text = hydra::uds::io::inp_writer::write_inp(&net).expect("export");
        let (again, diags) = hydra::uds::io::objects::parse_network(&text);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        assert!(
            coords(&again).contains(&("J1".into(), 12.5, -7.25)),
            "{:?}",
            coords(&again)
        );
    }
}

#[cfg(test)]
mod rename_tests {
    use super::*;

    fn model(controls: &str) -> Network {
        let inp = format!(
            "\
[OPTIONS]
FLOW_UNITS    CMS

[JUNCTIONS]
5   10  3  0  0  0
J2  9   3  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  5  J2  100  0.01  0  0  0  0
C2  J2  O1  100  0.01  0  0  0  0

[COORDINATES]
5   100  200
J2  300  400
O1  500  600

{controls}"
        );
        let (net, diags) = hydra::uds::io::objects::parse_network(&inp);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        net
    }

    fn rule_lines(net: &Network) -> Vec<String> {
        net.controls
            .rules
            .iter()
            .flat_map(|r| r.lines.iter().cloned())
            .collect()
    }

    #[test]
    fn a_rename_follows_the_element_into_a_control_rule() {
        let net =
            &mut model("[CONTROLS]\nRULE R1\nIF NODE J2 DEPTH > 2\nTHEN LINK C2 SETTING = 0\n");
        rename_in_controls(net, "J2", "OUTLET_NODE");
        let lines = rule_lines(net);
        assert!(
            lines.iter().any(|l| l.contains("NODE OUTLET_NODE")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_numeric_name_is_not_confused_with_a_threshold() {
        // The reason only the token after an object keyword is replaced.
        // Drainage identifiers are routinely numeric, and this model has a
        // node named `5` alongside a rule testing `> 5`. Substituting
        // every matching token would rewrite the threshold and silently
        // change what the rule asks.
        let net =
            &mut model("[CONTROLS]\nRULE R1\nIF NODE 5 DEPTH > 5\nTHEN LINK C1 SETTING = 0\n");
        rename_in_controls(net, "5", "INLET");
        let lines = rule_lines(net);
        let premise = lines
            .iter()
            .find(|l| l.to_uppercase().starts_with("IF"))
            .expect("premise");
        assert!(premise.contains("NODE INLET"), "{premise}");
        assert!(
            premise.contains("> 5"),
            "the threshold was rewritten: {premise}"
        );
    }

    #[test]
    fn a_rename_leaves_rules_that_never_named_the_element() {
        let net =
            &mut model("[CONTROLS]\nRULE R1\nIF NODE J2 DEPTH > 2\nTHEN LINK C2 SETTING = 0\n");
        let before = rule_lines(net);
        rename_in_controls(net, "O1", "SEA");
        assert_eq!(rule_lines(net), before);
    }
}
