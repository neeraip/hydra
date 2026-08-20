//! Binary results writer (§14.9): the predecessor's layout — magic
//! 516114522, version 52004, identifier and static-property tables,
//! result-variable code lists, fixed-size per-period records in user
//! units, and the six-integer epilog readers locate by seeking back from
//! the end. Per-object records appear only for objects the `[REPORT]`
//! selection flagged; the stored start date is backdated one period when
//! reporting starts after the simulation.

use std::io::{self, Write};

use crate::model::{ConcentrationUnits, LinkKind, Network, Offset, ReportSelection, VertexKind};
use crate::simulation::engine::Snapshot;

const MAGIC: i32 = 516_114_522;
const VERSION: i32 = 52_004;
/// Days between the predecessor's epoch (1899-12-30) and the civil epoch
/// (1970-01-01).
const EPOCH_OFFSET_DAYS: f64 = 25_569.0;

struct Cv {
    us: bool,
    flow: f64,
    len: f64,
}

impl Cv {
    fn rain(&self, v: f64) -> f32 {
        (v * 3600.0 / if self.us { 0.0254 } else { 1.0e-3 }) as f32
    }
    fn evap(&self, v: f64) -> f32 {
        (v * 86_400.0 / if self.us { 0.0254 } else { 1.0e-3 }) as f32
    }
    fn depth_small(&self, v: f64) -> f32 {
        (v / if self.us { 0.0254 } else { 1.0e-3 }) as f32
    }
    fn q(&self, v: f64) -> f32 {
        (v / self.flow) as f32
    }
    fn l(&self, v: f64) -> f32 {
        (v / self.len) as f32
    }
    fn vol(&self, v: f64) -> f32 {
        (v / self.len.powi(3)) as f32
    }
    fn temp(&self, c: f64) -> f32 {
        if self.us {
            (c * 1.8 + 32.0) as f32
        } else {
            c as f32
        }
    }
}

fn put_i32(w: &mut impl Write, v: i32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_f32(w: &mut impl Write, v: f32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_f64(w: &mut impl Write, v: f64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn put_id(w: &mut impl Write, id: &str, n: &mut u64) -> io::Result<()> {
    put_i32(w, id.len() as i32)?;
    w.write_all(id.as_bytes())?;
    *n += 4 + id.len() as u64;
    Ok(())
}

fn selected(sel: &ReportSelection, count: usize) -> Vec<usize> {
    match sel {
        ReportSelection::None => Vec::new(),
        ReportSelection::All => (0..count).collect(),
        ReportSelection::Ids(ids) => ids.clone(),
    }
}

/// Write the §14.9 binary results for `snapshots` over `net`.
/// `start_epoch` is the run's absolute start (s); the caller owns where
/// the bytes go.
pub fn write_out(
    net: &Network,
    snapshots: &[Snapshot],
    start_epoch: f64,
    report_step: f64,
    w: &mut impl Write,
) -> io::Result<()> {
    let cv = Cv {
        us: net.options.flow_units.is_us(),
        flow: net.options.flow_units.m3s_per_unit(),
        len: if net.options.flow_units.is_us() {
            0.3048
        } else {
            1.0
        },
    };
    let subs = selected(&net.report.parcels, net.parcels.len());
    let nodes = selected(&net.report.vertices, net.vertices.len());
    let links = selected(&net.report.links, net.links.len());
    let np = net.constituents.len();
    let mut pos: u64 = 0;

    // ── Header ──────────────────────────────────────────────────────────
    put_i32(w, MAGIC)?;
    put_i32(w, VERSION)?;
    put_i32(w, net.options.flow_units as i32)?;
    put_i32(w, subs.len() as i32)?;
    put_i32(w, nodes.len() as i32)?;
    put_i32(w, links.len() as i32)?;
    put_i32(w, np as i32)?;
    pos += 28;

    // ── Identifier tables ───────────────────────────────────────────────
    let id_start = pos;
    for &pi in &subs {
        put_id(w, &net.parcels[pi].id, &mut pos)?;
    }
    for &vi in &nodes {
        put_id(w, &net.vertices[vi].id, &mut pos)?;
    }
    for &li in &links {
        put_id(w, &net.links[li].id, &mut pos)?;
    }
    for c in &net.constituents {
        put_id(w, &c.id, &mut pos)?;
    }
    for c in &net.constituents {
        put_i32(
            w,
            match c.units {
                ConcentrationUnits::MgPerL => 0,
                ConcentrationUnits::UgPerL => 1,
                ConcentrationUnits::CountPerL => 2,
            },
        )?;
        pos += 4;
    }

    // ── Static property tables ──────────────────────────────────────────
    let input_start = pos;
    let cv_area = if cv.us { 4_046.856_422_4 } else { 10_000.0 };
    put_i32(w, 1)?;
    put_i32(w, 1)?; // INPUT_AREA
    for &pi in &subs {
        put_f32(w, (net.parcels[pi].area / cv_area) as f32)?;
    }
    pos += 8 + 4 * subs.len() as u64;

    put_i32(w, 3)?;
    put_i32(w, 0)?; // INPUT_TYPE_CODE
    put_i32(w, 2)?; // INPUT_INVERT
    put_i32(w, 3)?; // INPUT_MAX_DEPTH
                    // The §5 section build takes the file's depth-like geometry in its
                    // own unit; both the outfall crowns below and the link table later
                    // need the same factor.
    let len_cv = if net.options.flow_units.is_us() {
        0.3048
    } else {
        1.0
    };
    for &vi in &nodes {
        let v = &net.vertices[vi];
        let (code, max_depth) = match &v.kind {
            VertexKind::Junction { max_depth, .. } => (0, *max_depth),
            // §14.9: derived, not stored — see `validate::outfall_crown`.
            VertexKind::Outfall { .. } => (1, crate::io::validate::outfall_crown(net, vi, len_cv)),
            VertexKind::Storage { max_depth, .. } => (2, *max_depth),
            VertexKind::Divider { max_depth, .. } => (3, *max_depth),
        };
        put_i32(w, code)?;
        put_f32(w, cv.l(v.invert))?;
        put_f32(w, cv.l(max_depth))?;
    }
    pos += 16 + 12 * nodes.len() as u64;

    put_i32(w, 5)?;
    put_i32(w, 0)?; // INPUT_TYPE_CODE
    put_i32(w, 4)?; // INPUT_OFFSET
    put_i32(w, 4)?; // INPUT_OFFSET
    put_i32(w, 3)?; // INPUT_MAX_DEPTH
    put_i32(w, 5)?; // INPUT_LENGTH
    for &li in &links {
        let l = &net.links[li];
        let off = |o: &Offset, invert: f64| match o {
            Offset::Depth(h) => *h,
            Offset::Elevation(e) => (e - invert).max(0.0),
            Offset::Missing => 0.0,
        };
        let (code, o1, o2, length) = match &l.kind {
            // §14.7: output keeps the user's orientation — a reversed
            // adverse-slope channel swaps its offsets back.
            LinkKind::Channel {
                length,
                offset1,
                offset2,
                reversed,
                ..
            } => {
                let (mut o1, mut o2) = (
                    off(offset1, net.vertices[l.from].invert),
                    off(offset2, net.vertices[l.to].invert),
                );
                if *reversed {
                    std::mem::swap(&mut o1, &mut o2);
                }
                (0, o1, o2, *length)
            }
            LinkKind::Pump { .. } => (1, 0.0, 0.0, 0.0),
            // §14.9: a regulator has one offset and the table has two
            // columns, so the predecessor writes it into both. A reader
            // taking the downstream column at face value would place
            // every weir and orifice at its downstream vertex's invert.
            LinkKind::Orifice { offset, .. } => {
                let o = off(offset, net.vertices[l.from].invert);
                (2, o, o, 0.0)
            }
            LinkKind::Weir { offset, .. } => {
                let o = off(offset, net.vertices[l.from].invert);
                (3, o, o, 0.0)
            }
            LinkKind::Outlet { offset, .. } => {
                let o = off(offset, net.vertices[l.from].invert);
                (4, o, o, 0.0)
            }
        };
        // Full depth from the §5 section build (pumps have none, as in
        // the predecessor's table).
        let y_full = crate::io::validate::build_for_link(net, li, len_cv)
            .and_then(|r| r.ok())
            .map_or(0.0, |b| b.section.y_full());
        put_i32(w, code)?;
        put_f32(w, cv.l(o1))?;
        put_f32(w, cv.l(o2))?;
        put_f32(w, cv.l(y_full))?;
        put_f32(w, cv.l(length))?;
        pos += 20;
    }
    pos += 24;

    // ── Result variable code lists ──────────────────────────────────────
    let n_sub_vars = 8 + np;
    let n_node_vars = 6 + np;
    let n_link_vars = 5 + np;
    put_i32(w, n_sub_vars as i32)?;
    for k in 0..n_sub_vars {
        put_i32(w, k as i32)?;
    }
    put_i32(w, n_node_vars as i32)?;
    for k in 0..n_node_vars {
        put_i32(w, k as i32)?;
    }
    put_i32(w, n_link_vars as i32)?;
    for k in 0..n_link_vars {
        put_i32(w, k as i32)?;
    }
    put_i32(w, 15)?;
    for k in 0..15 {
        put_i32(w, k)?;
    }
    pos += 4 * (4 + n_sub_vars + n_node_vars + n_link_vars + 15) as u64;

    // ── Reporting clock ─────────────────────────────────────────────────
    // The stored start date backdates one period when reporting starts
    // after the simulation (§14.9), so readers computing start + n·step
    // land on the true record times.
    let first_snap_epoch = snapshots
        .first()
        .map_or(start_epoch + report_step, |s| start_epoch + s.t);
    let stored_epoch = first_snap_epoch - report_step;
    let start_days = stored_epoch / 86_400.0 + EPOCH_OFFSET_DAYS;
    put_f64(w, start_days)?;
    put_i32(w, report_step as i32)?;
    pos += 12;

    // ── Per-period records ──────────────────────────────────────────────
    let output_start = pos;
    for snap in snapshots {
        // Record dates are the true instants, independent of the
        // backdated header start.
        put_f64(w, (start_epoch + snap.t) / 86_400.0 + EPOCH_OFFSET_DAYS)?;
        for &pi in &subs {
            let r = &snap.subcatch[pi];
            put_f32(w, cv.rain(r.rain))?;
            put_f32(w, cv.depth_small(r.snow_depth))?;
            put_f32(w, cv.evap(r.evap))?;
            put_f32(w, cv.rain(r.infil))?;
            put_f32(w, cv.q(r.runoff))?;
            put_f32(w, cv.q(r.gw_flow))?;
            put_f32(w, cv.l(r.gw_elev))?;
            put_f32(w, r.soil_moisture as f32)?;
            for p in 0..np {
                put_f32(w, r.washoff[p] as f32)?;
            }
        }
        for &vi in &nodes {
            put_f32(w, cv.l(snap.depths[vi]))?;
            put_f32(w, cv.l(snap.node_head[vi]))?;
            put_f32(w, cv.vol(snap.node_volume[vi]))?;
            put_f32(w, cv.q(snap.node_lateral[vi]))?;
            put_f32(w, cv.q(snap.node_inflow[vi]))?;
            put_f32(w, cv.q(snap.node_flooding[vi]))?;
            for p in 0..np {
                put_f32(w, snap.node_quality[p][vi] as f32)?;
            }
        }
        for &li in &links {
            put_f32(w, cv.q(snap.flows[li]))?;
            put_f32(w, cv.l(snap.link_depth[li]))?;
            put_f32(w, cv.l(snap.link_velocity[li]))?;
            put_f32(w, cv.vol(snap.link_volume[li]))?;
            put_f32(w, snap.link_capacity[li] as f32)?;
            for p in 0..np {
                put_f32(w, snap.link_quality[p][li] as f32)?;
            }
        }
        let s = &snap.system;
        put_f32(w, cv.temp(s[0]))?;
        put_f32(w, cv.rain(s[1]))?;
        put_f32(w, cv.depth_small(s[2]))?;
        // Infiltration is an area-weighted rate, in the rain unit.
        put_f32(w, cv.rain(s[3]))?;
        put_f32(w, cv.q(s[4]))?;
        put_f32(w, cv.q(s[5]))?;
        put_f32(w, cv.q(s[6]))?;
        put_f32(w, cv.q(s[7]))?;
        put_f32(w, cv.q(s[8]))?;
        put_f32(w, cv.q(s[9]))?;
        put_f32(w, cv.q(s[10]))?;
        put_f32(w, cv.q(s[11]))?;
        put_f32(w, cv.vol(s[12]))?;
        // Evaporation and PET are rates, in the evaporation unit.
        put_f32(w, cv.evap(s[13]))?;
        put_f32(w, cv.evap(s[14]))?;
    }

    // ── Epilog ──────────────────────────────────────────────────────────
    put_i32(w, id_start as i32)?;
    put_i32(w, input_start as i32)?;
    put_i32(w, output_start as i32)?;
    put_i32(w, snapshots.len() as i32)?;
    put_i32(w, 0)?; // error code
    put_i32(w, MAGIC)?;
    Ok(())
}
