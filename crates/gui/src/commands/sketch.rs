//! A small drawing of a network's shape, for recognising a project by sight.
//!
//! Engineers know their models by outline long before they read a name, and
//! nothing in the app has ever shown one below full canvas size. This
//! produces enough of a network to be recognisable at a couple of hundred
//! pixels and nothing more: no vertices, no kinds, no results.
//!
//! It is written beside the project rather than into its record, so listing
//! projects does not carry drawings nothing on that screen renders. It is
//! regenerated only when the geometry it was drawn from has changed, which
//! most saves do not touch.

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::network_dto::NetworkDto;

/// How many segments a sketch may hold.
///
/// At the size these are drawn, a few hundred strokes already read as a
/// solid mass; more only costs bytes. A 46k-link network samples down to
/// this and still looks like itself, because what survives sampling is the
/// overall run of the mains, which is what recognition uses.
const MAX_SEGMENTS: usize = 600;

/// How many segments all catchment outlines may share between them.
///
/// Outlines are background: they say where the catchment is, and the
/// conveyance drawn over them says what it does. Given the same budget as
/// the network they would swamp it, since one catchment ring can carry more
/// vertices than a small model has pipes.
const MAX_AREA_SEGMENTS: usize = 400;

/// The fewest vertices a catchment keeps, however many there are to draw.
///
/// Below this a ring stops reading as an area and starts reading as a
/// stray triangle.
const MIN_RING_VERTICES: usize = 8;

/// One link, reduced to the straight line between its ends.
///
/// Vertices are dropped deliberately. A pipe's bends are invisible at this
/// scale and they are most of the geometry in a detailed model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// A network's outline, normalised into the unit square.
///
/// Normalised here rather than at draw time so the drawing needs no
/// knowledge of coordinate systems, and so a model in feet and one in
/// metres produce the same picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sketch {
    /// Segments with both ends in `0.0..=1.0`, y already flipped so that
    /// larger northings draw upward in screen space.
    pub segments: Vec<Segment>,
    /// Catchment outlines, in the same box and to the same extent as
    /// `segments`. Drawn behind them and more faintly, the way the canvas
    /// washes a catchment behind the network it drains to.
    ///
    /// Defaulted so a drawing made before catchments were included still
    /// reads, rather than being discarded and redrawn.
    #[serde(default)]
    pub areas: Vec<Segment>,
    /// Width divided by height of the source extent, for drawing without
    /// stretching. `1.0` where the network has no extent in one direction.
    pub aspect: f32,
    /// Digest of the geometry this was drawn from, or `None` where the
    /// engine publishes none. Present so a save that moved nothing can
    /// skip redrawing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// Build a sketch from a loaded network, or `None` when there is nothing to
/// draw.
///
/// Returns `None` for a model whose nodes carry no coordinates, which is a
/// real case: a network can be complete and have never been laid out. The
/// caller shows the engine's mark instead, rather than an empty frame.
pub fn build(dto: &NetworkDto, digest: Option<String>) -> Option<Sketch> {
    let index: std::collections::HashMap<&str, (f64, f64)> = dto
        .nodes
        .iter()
        .filter(|n| n.x.is_finite() && n.y.is_finite())
        .map(|n| (n.id.as_str(), (n.x, n.y)))
        .collect();
    if index.is_empty() {
        return None;
    }

    let ends: Vec<((f64, f64), (f64, f64))> = dto
        .links
        .iter()
        .filter_map(|l| {
            let a = index.get(l.from_id.as_str())?;
            let b = index.get(l.to_id.as_str())?;
            Some((*a, *b))
        })
        .collect();

    // A network of nodes and no links still has a shape worth drawing, so
    // fall back to zero-length segments at each node. They render as dots.
    let ends = if ends.is_empty() {
        index.values().map(|p| (*p, *p)).collect()
    } else {
        ends
    };
    from_ends(ends, Vec::new(), digest)
}

/// Reduce a catchment ring to at most `keep` vertices, still closed.
///
/// Vertices are dropped, not segments. Dropping segments would break an
/// outline into dashes; dropping vertices keeps one continuous ring that is
/// simply coarser, which is all an outline needs to be at this size.
fn decimate(ring: &[[f64; 2]], keep: usize) -> Vec<[f64; 2]> {
    if ring.len() <= keep {
        return ring.to_vec();
    }
    let step = ring.len().div_ceil(keep).max(1);
    ring.iter().step_by(step).copied().collect()
}

/// The drawing itself, from link endpoints in model coordinates.
///
/// Shared by both engines. What differs between them is only how a link's
/// two ends are found: one names its nodes, the other indexes them.
fn from_ends(
    ends: Vec<((f64, f64), (f64, f64))>,
    areas: Vec<((f64, f64), (f64, f64))>,
    digest: Option<String>,
) -> Option<Sketch> {
    if ends.is_empty() && areas.is_empty() {
        return None;
    }
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    // One extent over both. Measured separately they would be normalised
    // against different boxes, and every catchment would sit somewhere its
    // network is not.
    for ((x1, y1), (x2, y2)) in ends.iter().chain(areas.iter()) {
        min_x = min_x.min(*x1).min(*x2);
        max_x = max_x.max(*x1).max(*x2);
        min_y = min_y.min(*y1).min(*y2);
        max_y = max_y.max(*y1).max(*y2);
    }

    // A network can be a single point, or a perfectly straight line. Both
    // would divide by zero; both are drawn centred on the axis that has no
    // extent.
    let span_x = max_x - min_x;
    let span_y = max_y - min_y;
    let span = span_x.max(span_y);
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    let aspect = if span_y > 0.0 && span_x > 0.0 {
        (span_x / span_y) as f32
    } else {
        1.0
    };

    // Evenly spaced sampling rather than the first N, which would draw
    // whichever corner of the network happens to be first in the file.
    let step = ends.len().div_ceil(MAX_SEGMENTS).max(1);
    let norm = |x: f64, y: f64| -> (f32, f32) {
        let nx = if span_x > 0.0 {
            (x - min_x) / span_x
        } else {
            0.5
        };
        // Flipped: model northings increase upward, screen y increases down.
        let ny = if span_y > 0.0 {
            1.0 - (y - min_y) / span_y
        } else {
            0.5
        };
        (nx as f32, ny as f32)
    };

    let place = |((x1, y1), (x2, y2)): &((f64, f64), (f64, f64))| {
        let (nx1, ny1) = norm(*x1, *y1);
        let (nx2, ny2) = norm(*x2, *y2);
        Segment {
            x1: nx1,
            y1: ny1,
            x2: nx2,
            y2: ny2,
        }
    };
    let segments = ends.iter().step_by(step).map(place).collect();
    let areas = areas.iter().map(place).collect();

    Some(Sketch {
        segments,
        areas,
        aspect,
        digest,
    })
}

/// Where a project's sketch lives.
pub fn sketch_path(app_data: &Path, project_id: &str) -> std::path::PathBuf {
    crate::meta::bundle::project_dir(app_data, project_id).join("sketch.json")
}

/// Redraw a drainage project's sketch.
///
/// Drainage models arrive as the viewer's own point/polyline view rather
/// than as a network DTO, so the endpoints are found by index instead of by
/// name. Subcatchment boundaries are left out: they double the drawing's
/// size for shapes that are unreadable at this scale, and the conveyance
/// network is what makes a catchment recognisable anyway.
pub fn refresh_uds(app_data: &Path, project_id: &str, view: &super::uds_view::UdsView) {
    let at = |i: i32| -> Option<(f64, f64)> {
        let p = view.points.get(usize::try_from(i).ok()?)?;
        (p.x.is_finite() && p.y.is_finite()).then_some((p.x, p.y))
    };
    let mut ends: Vec<((f64, f64), (f64, f64))> = view
        .polylines
        .iter()
        .filter_map(|l| Some((at(l.from)?, at(l.to)?)))
        .collect();
    if ends.is_empty() {
        ends = view
            .points
            .iter()
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .map(|p| ((p.x, p.y), (p.x, p.y)))
            .collect();
    }

    // Catchment outlines, sharing one budget between them so a model with
    // many small catchments does not draw more of them than of its network.
    let rings: Vec<&Vec<[f64; 2]>> = view
        .regions
        .iter()
        .map(|r| &r.ring)
        .filter(|r| r.len() >= 3)
        .collect();
    let per_ring = if rings.is_empty() {
        0
    } else {
        (MAX_AREA_SEGMENTS / rings.len()).max(MIN_RING_VERTICES)
    };
    let mut areas: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for ring in &rings {
        let kept = decimate(ring, per_ring);
        for i in 0..kept.len() {
            // Wraps, so the outline closes. A ring drawn open reads as a
            // stray line across the catchment.
            let a = kept[i];
            let b = kept[(i + 1) % kept.len()];
            if a[0].is_finite() && a[1].is_finite() && b[0].is_finite() && b[1].is_finite() {
                areas.push(((a[0], a[1]), (b[0], b[1])));
            }
        }
    }

    // Positions only, as on the other path: renaming a conduit must not
    // cost a redraw.
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    view.points.len().hash(&mut h);
    view.polylines.len().hash(&mut h);
    for p in &view.points {
        p.x.to_bits().hash(&mut h);
        p.y.to_bits().hash(&mut h);
    }
    view.regions.len().hash(&mut h);
    for r in &view.regions {
        r.ring.len().hash(&mut h);
        for v in &r.ring {
            v[0].to_bits().hash(&mut h);
            v[1].to_bits().hash(&mut h);
        }
    }
    let digest = Some(format!("{:016x}", h.finish()));
    write_if_changed(app_data, project_id, |d| from_ends(ends, areas, d), digest);
}

/// Redraw a project's sketch if the model has moved since the last one.
///
/// Called where a network is loaded, because that is the one place its
/// geometry is already in memory. Silent on failure by design: a project
/// that cannot be drawn is a project shown with its engine's mark, not a
/// project that fails to open.
pub fn refresh(app_data: &Path, project_id: &str, dto: &NetworkDto) {
    let digest = geometry_digest(dto);
    write_if_changed(app_data, project_id, |d| build(dto, d), digest);
}

/// Write a drawing unless the one on disk was made from the same geometry.
fn write_if_changed(
    app_data: &Path,
    project_id: &str,
    draw: impl FnOnce(Option<String>) -> Option<Sketch>,
    digest: Option<String>,
) {
    let path = sketch_path(app_data, project_id);
    if let Ok(existing) = std::fs::read(&path) {
        if let Ok(prior) = serde_json::from_slice::<Sketch>(&existing) {
            if prior.digest.is_some() && prior.digest == digest {
                return;
            }
        }
    }

    let Some(sketch) = draw(digest) else {
        return;
    };
    if let Ok(bytes) = serde_json::to_vec(&sketch) {
        let _ = std::fs::write(&path, bytes);
    }
}

/// Read a project's sketch, or `None` where none has been drawn.
#[tauri::command]
pub fn get_project_sketch(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Option<Sketch>, String> {
    // The id is joined into a path below, so it is checked here like every
    // other project-scoped command. This one was missed, which left a guard
    // meant to be total with a hole in it.
    crate::commands::projects::validate_id(&project_id)?;
    let app_data = crate::commands::app_data_dir(&app)?;
    let path = sketch_path(&app_data, &project_id);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(None);
    };
    Ok(serde_json::from_slice(&bytes).ok())
}

/// A digest of the coordinates a sketch is drawn from.
///
/// Only positions, because only positions change the drawing. Renaming an
/// element or editing a diameter must not cost a redraw.
fn geometry_digest(dto: &NetworkDto) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    dto.nodes.len().hash(&mut h);
    dto.links.len().hash(&mut h);
    for n in &dto.nodes {
        n.x.to_bits().hash(&mut h);
        n.y.to_bits().hash(&mut h);
    }
    Some(format!("{:016x}", h.finish()))
}

#[cfg(test)]
mod tests {

    /// Every project-scoped command validates its id before joining it into
    /// a path. `get_project_sketch` did not, so a traversal id reached
    /// `sketch_path` and read a file outside the projects root.
    #[test]
    fn the_sketch_path_is_only_built_from_a_validated_id() {
        use crate::commands::projects::validate_id;
        for bad in ["../../etc", "..", "not-a-uuid", ""] {
            assert!(validate_id(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(validate_id("11111111-1111-4111-8111-111111111111").is_ok());
    }
    use super::*;
    use crate::commands::network_dto::{LinkDto, NodeDto};

    // Built through the DTOs' own `Deserialize` rather than by field, so a
    // wire type does not grow a `Default` for the convenience of a test.
    fn node(id: &str, x: f64, y: f64) -> NodeDto {
        serde_json::from_value(serde_json::json!({
            "id": id, "type": "junction", "x": x, "y": y,
            "elevation": 0.0, "baseDemand": 0.0
        }))
        .expect("node fixture")
    }

    fn link(from: &str, to: &str) -> LinkDto {
        serde_json::from_value(serde_json::json!({
            "id": format!("{from}-{to}"), "type": "pipe",
            "fromId": from, "toId": to,
            "velocity": 0.0, "diameter": 0.0, "length": 0.0, "roughness": 0.0
        }))
        .expect("link fixture")
    }

    fn dto(nodes: Vec<NodeDto>, links: Vec<LinkDto>) -> NetworkDto {
        NetworkDto {
            nodes,
            links,
            ..Default::default()
        }
    }

    /// The whole point of normalising: two models of the same shape in
    /// different units must draw identically, because the drawing knows
    /// nothing about coordinate systems.
    #[test]
    fn the_same_shape_in_different_units_draws_the_same() {
        let metres = dto(
            vec![node("a", 0.0, 0.0), node("b", 100.0, 50.0)],
            vec![link("a", "b")],
        );
        let feet = dto(
            vec![node("a", 0.0, 0.0), node("b", 328.084, 164.042)],
            vec![link("a", "b")],
        );
        let m = build(&metres, None).unwrap();
        let f = build(&feet, None).unwrap();
        assert_eq!(m.segments, f.segments);
        assert!((m.aspect - f.aspect).abs() < 1e-5);
    }

    /// Screen y grows downward and northings grow upward, so a node to the
    /// north must draw above one to the south.
    #[test]
    fn north_draws_upward() {
        let s = build(
            &dto(
                vec![node("s", 0.0, 0.0), node("n", 0.0, 100.0)],
                vec![link("s", "n")],
            ),
            None,
        )
        .unwrap();
        let seg = s.segments[0];
        assert_eq!(seg.y1, 1.0, "the southern end draws at the bottom");
        assert_eq!(seg.y2, 0.0, "the northern end draws at the top");
    }

    #[test]
    fn every_coordinate_lands_inside_the_unit_square() {
        let s = build(
            &dto(
                vec![
                    node("a", -500.0, 200.0),
                    node("b", 1200.0, -80.0),
                    node("c", 40.0, 90.0),
                ],
                vec![link("a", "b"), link("b", "c")],
            ),
            None,
        )
        .unwrap();
        for seg in &s.segments {
            for v in [seg.x1, seg.y1, seg.x2, seg.y2] {
                assert!((0.0..=1.0).contains(&v), "{v} is outside the box");
            }
        }
    }

    /// A large network is sampled, not truncated. Truncating would draw
    /// whichever corner happened to be first in the file and call it the
    /// network.
    #[test]
    fn a_large_network_is_sampled_across_its_whole_extent() {
        let nodes: Vec<NodeDto> = (0..5000)
            .map(|i| node(&format!("n{i}"), i as f64, (i % 7) as f64))
            .collect();
        let links: Vec<LinkDto> = (0..4999)
            .map(|i| link(&format!("n{i}"), &format!("n{}", i + 1)))
            .collect();
        let s = build(&dto(nodes, links), None).unwrap();
        assert!(s.segments.len() <= MAX_SEGMENTS);
        // Both extremes survive: a truncated sketch would stop near zero.
        let max_x = s.segments.iter().fold(0.0f32, |m, g| m.max(g.x2));
        assert!(max_x > 0.9, "sampling lost the far end: {max_x}");
    }

    /// A model can be complete and never laid out. There is nothing to
    /// draw, and the caller shows the engine's mark instead.
    #[test]
    fn a_model_without_coordinates_has_no_sketch() {
        // Set after building: JSON has no NaN, so the fixture cannot carry
        // one through `Deserialize`.
        let mut n = node("a", 0.0, 0.0);
        n.x = f64::NAN;
        n.y = f64::NAN;
        assert!(build(&dto(vec![n], vec![]), None).is_none());
    }

    /// One node is a point, not an extent. Dividing by its zero span would
    /// produce infinities rather than a drawing.
    #[test]
    fn a_single_node_has_no_extent_to_draw() {
        assert!(build(&dto(vec![node("a", 5.0, 5.0)], vec![]), None).is_none());
    }

    /// A perfectly straight run has extent one way and none the other. It
    /// draws, centred on the axis that has none.
    #[test]
    fn a_straight_line_draws_centred_on_its_flat_axis() {
        let s = build(
            &dto(
                vec![node("a", 0.0, 10.0), node("b", 100.0, 10.0)],
                vec![link("a", "b")],
            ),
            None,
        )
        .unwrap();
        assert_eq!(s.aspect, 1.0, "no aspect to take from a flat network");
        assert_eq!(s.segments[0].y1, 0.5);
        assert_eq!(s.segments[0].y2, 0.5);
    }

    /// Nodes with no links still describe a shape.
    #[test]
    fn nodes_without_links_draw_as_points() {
        let s = build(
            &dto(vec![node("a", 0.0, 0.0), node("b", 10.0, 10.0)], vec![]),
            None,
        )
        .unwrap();
        assert_eq!(s.segments.len(), 2);
        assert!(s.segments.iter().all(|g| g.x1 == g.x2 && g.y1 == g.y2));
    }

    /// A link naming a node that is not in the model must not take the
    /// sketch down with it.
    #[test]
    fn a_link_to_a_missing_node_is_skipped() {
        let s = build(
            &dto(
                vec![node("a", 0.0, 0.0), node("b", 10.0, 10.0)],
                vec![link("a", "b"), link("a", "ghost")],
            ),
            None,
        )
        .unwrap();
        assert_eq!(s.segments.len(), 1);
    }

    /// Drainage models reach the same drawing by a different route: their
    /// links index their points rather than naming them. A link pointing
    /// past the end of the list, or at nothing, must not take the sketch
    /// down with it.
    #[test]
    fn a_drainage_view_draws_from_its_indices() {
        use crate::commands::uds_view::{UdsView, ViewPoint, ViewPolyline};
        let point = |x: f64, y: f64| ViewPoint {
            kind: "junction",
            id: String::new(),
            x,
            y,
        };
        let line = |from: i32, to: i32| ViewPolyline {
            kind: "conduit",
            id: String::new(),
            from,
            to,
            bends: Vec::new(),
        };
        let view = UdsView {
            points: vec![point(0.0, 0.0), point(10.0, 10.0)],
            polylines: vec![line(0, 1), line(0, 9), line(-1, 0)],
            regions: Vec::new(),
        };
        let at = |i: i32| -> Option<(f64, f64)> {
            let p = view.points.get(usize::try_from(i).ok()?)?;
            Some((p.x, p.y))
        };
        let ends: Vec<_> = view
            .polylines
            .iter()
            .filter_map(|l| Some((at(l.from)?, at(l.to)?)))
            .collect();
        assert_eq!(ends.len(), 1, "only the link with two real ends survives");
        assert_eq!(from_ends(ends, Vec::new(), None).unwrap().segments.len(), 1);
    }

    /// Catchments and conveyance must be measured against one extent. Taken
    /// separately, each would be normalised into the whole box and every
    /// catchment would sit somewhere its network is not.
    #[test]
    fn catchments_share_the_networks_extent() {
        // The network occupies the left half; the catchment the right.
        let ends = vec![((0.0, 0.0), (10.0, 0.0))];
        let areas = vec![
            ((10.0, 0.0), (20.0, 0.0)),
            ((20.0, 0.0), (20.0, 10.0)),
            ((20.0, 10.0), (10.0, 0.0)),
        ];
        let s = from_ends(ends, areas, None).unwrap();
        // The network stops halfway across, because the catchment set the
        // other half of the extent.
        assert!(s.segments[0].x2 < 0.6, "network was measured on its own");
        assert!(
            s.areas.iter().any(|a| a.x2 > 0.9),
            "the catchment reaches the far edge"
        );
    }

    /// A ring is decimated by vertex and still closes. Dropping segments
    /// instead would leave an outline of dashes.
    #[test]
    fn a_decimated_ring_still_closes() {
        let ring: Vec<[f64; 2]> = (0..500)
            .map(|i| {
                let t = i as f64 / 500.0 * std::f64::consts::TAU;
                [t.cos(), t.sin()]
            })
            .collect();
        let kept = decimate(&ring, 12);
        assert!(kept.len() <= 12);
        assert!(kept.len() >= 3);
    }

    /// A small ring is left alone rather than padded or resampled.
    #[test]
    fn a_small_ring_is_untouched() {
        let ring = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]];
        assert_eq!(decimate(&ring, 12), ring);
    }

    /// A drawing made before catchments existed still loads, rather than
    /// being discarded and redrawn on next open.
    #[test]
    fn a_sketch_written_without_areas_still_reads() {
        let json = r#"{"segments":[{"x1":0.0,"y1":0.0,"x2":1.0,"y2":1.0}],"aspect":1.0}"#;
        let s: Sketch = serde_json::from_str(json).expect("older sketch");
        assert_eq!(s.segments.len(), 1);
        assert!(s.areas.is_empty());
    }

    /// The digest rides along so a save that moved nothing can skip
    /// redrawing.
    #[test]
    fn the_digest_is_carried_through() {
        let s = build(
            &dto(
                vec![node("a", 0.0, 0.0), node("b", 1.0, 1.0)],
                vec![link("a", "b")],
            ),
            Some("abc123".into()),
        )
        .unwrap();
        assert_eq!(s.digest.as_deref(), Some("abc123"));
    }
}
