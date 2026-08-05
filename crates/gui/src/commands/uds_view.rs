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
pub(crate) fn build_view(net: &Network) -> UdsView {
    // [COORDINATES]: vertex id → map position.
    let coords: HashMap<&str, (f64, f64)> = parse_xy_lines(net, "[COORDINATES]")
        .map(|(id, x, y)| (id, (x, y)))
        .collect();

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
    let mut regions = Vec::new();
    for p in &net.parcels {
        let Some(ring) = rings.get(p.id.as_str()) else {
            continue;
        };
        if ring.len() < 3 {
            continue;
        }
        let outlet = match p.outlet {
            ParcelOutlet::Vertex(vi) => {
                *point_index.get(net.vertices[vi].id.as_str()).unwrap_or(&-1)
            }
            ParcelOutlet::Parcel(_) => -1,
        };
        regions.push(ViewRegion {
            kind: "subcatchment",
            id: p.id.clone(),
            ring: ring.clone(),
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
        4u32,
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

    #[test]
    fn a_model_without_a_map_renders_empty_not_broken() {
        let bare = "[OPTIONS]\nFLOW_UNITS CFS\n[JUNCTIONS]\nJ1 100 4\n\
                    [OUTFALLS]\nO1 98 FREE\n[CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                    [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (net, _) = hydra::uds::io::objects::parse_network(bare);
        let view = build_view(&net);
        assert!(view.points.is_empty());
        assert!(view.polylines.is_empty());
        assert!(view.regions.is_empty());
        assert!(encode_uds_snapshot(&view).len() > 48);
    }
}
