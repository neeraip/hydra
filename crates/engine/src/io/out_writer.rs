// out_writer — EPANET-compatible binary output file writer.
//
// Produces a `.out` file byte-for-byte compatible with EPANET 2.3.
// All values are little-endian. Floating-point values are REAL4 (f32).
// Integers are INT4 (i32). String fields are fixed-width, zero-padded:
//   IDs: 32 bytes (MAXID)   title lines: 80 bytes   filenames: 260 bytes
//
// File layout — five consecutive sections:
//
//  ┌───────────────────────────────────────────────────────────────────────────┐
//  │ 1. PROLOG                                                                 │
//  │   15 × INT4 header (60 bytes):                                            │
//  │     magic (516114521), version (20012), n_nodes, n_tanks, n_links,        │
//  │     n_pumps, n_valves, quality_flag (0-3), trace_node (1-based),          │
//  │     flow_units (0-10), pressure_units (0=PSI,1=kPa,2=m),                 │
//  │     report_statistic (0=Series), report_start, report_step, duration      │
//  │   3 × 80 bytes: title lines                                               │
//  │   2 × 260 bytes: input filename, report filename                          │
//  │   2 × 32 bytes: chemical name, chemical units                             │
//  │   n_nodes × 32 bytes: node IDs                                            │
//  │   n_links × 32 bytes: link IDs                                            │
//  │   n_links × INT4: from-node indices (1-based)                             │
//  │   n_links × INT4: to-node indices (1-based)                               │
//  │   n_links × INT4: link type codes                                         │
//  │     (0=CV, 1=Pipe, 2=Pump, 3=PRV, 4=PSV, 5=PBV, 6=FCV, 7=TCV,           │
//  │      8=GPV, 9=PCV)                                                        │
//  │   n_tanks × INT4: tank/reservoir node indices (1-based)                   │
//  │   n_tanks × REAL4: tank cross-section areas (m², internal units)          │
//  │   n_nodes × REAL4: node elevations (output length units)                  │
//  │   n_links × REAL4: link lengths (output length units)                     │
//  │   n_links × REAL4: link diameters (output diameter units; 0 for pumps)    │
//  ├───────────────────────────────────────────────────────────────────────────┤
//  │ 2. ENERGY   (28 × n_pumps + 4 bytes)                                      │
//  │   Per pump: INT4 link_index, REAL4 pct_online, avg_eff,                   │
//  │             avg_kwh_per_flow, avg_kw, peak_kw, avg_cost                   │
//  │   Trailing REAL4: demand charge                                            │
//  ├───────────────────────────────────────────────────────────────────────────┤
//  │ 3. DYNAMIC RESULTS   (one record per reporting period)                     │
//  │   Column-major: all values for one variable, then the next.               │
//  │   Node vars (4 × n_nodes REAL4): demand, head, pressure, quality          │
//  │   Link vars (8 × n_links REAL4): flow, velocity, headloss, quality,       │
//  │     status (cast REAL4), setting, reaction_rate, friction_factor          │
//  │   Headloss: pipes = 1000|Δh|/L; pumps = signed Δh; valves = |Δh|         │
//  │   Bytes per period: (4×n_nodes + 8×n_links) × 4                           │
//  ├───────────────────────────────────────────────────────────────────────────┤
//  │ 4. NETWORK REACTIONS   (16 bytes)                                          │
//  │   4 × REAL4: avg bulk rate, wall rate, tank rate, source rate (mass/hr)   │
//  ├───────────────────────────────────────────────────────────────────────────┤
//  │ 5. EPILOG   (12 bytes)                                                     │
//  │   3 × INT4: n_periods, warn_flag (0=no warnings), magic (516114521)        │
//  └───────────────────────────────────────────────────────────────────────────┘

use std::io::{Seek, Write};

use super::units::{make_ucf, Ucf};
use super::WritableSimulation;
use crate::{FlowUnits, LinkKind, LinkStatus, NodeKind, QualityMode, ValveType};

// ── Constants ─────────────────────────────────────────────────────────────────

const MAGIC: i32 = 516114521;
const VERSION: i32 = 20012;
const MAXID: usize = 32; // MAXID+1 = 32 bytes per ID
const TITLELEN: usize = 80; // TITLELEN+1 = 80 bytes per title line
const MAXFNAME: usize = 260; // MAXFNAME+1 = 260 bytes per filename

/// Incremental EPANET-compatible `.out` writer.
///
/// This writer emits static sections up-front, appends dynamic period blocks as
/// hydraulic snapshots become available, and patches final sections (`energy`,
/// reactions, epilog) on `finish`.
pub struct OutStreamWriter<W: Write + Seek> {
    writer: W,
    ucf: Ucf,
    output_units: FlowUnits,
    energy_offset: u64,
    report_step: i64,
    next_rtime: i64,
    next_snapshot_index: usize,
    n_periods: i32,
}

impl<W: Write + Seek> OutStreamWriter<W> {
    /// Begin a streaming `.out` write by emitting the prolog and a placeholder
    /// energy section.
    pub fn begin(
        mut writer: W,
        session: &impl WritableSimulation,
        input_file: &str,
        report_file: &str,
        output_units: FlowUnits,
    ) -> std::io::Result<Self> {
        let network = session.net();
        let options = &network.options;
        let ucf = make_ucf(output_units, options.specific_gravity);

        write_prolog(
            &mut writer,
            session,
            &ucf,
            input_file,
            report_file,
            output_units,
        )?;
        let energy_offset = writer.stream_position()?;
        write_energy_placeholder(&mut writer, network)?;

        Ok(Self {
            writer,
            ucf,
            output_units,
            energy_offset,
            report_step: options.report_step.round() as i64,
            next_rtime: options.report_start.round() as i64,
            next_snapshot_index: 0,
            n_periods: 0,
        })
    }

    /// Append newly available report-boundary snapshots.
    pub fn append_available(&mut self, session: &impl WritableSimulation) -> std::io::Result<()> {
        let network = session.net();
        let snapshots = session.snapshots();

        for snapshot in snapshots.iter().skip(self.next_snapshot_index) {
            self.next_snapshot_index += 1;
            let snapshot_time = snapshot.t.round() as i64;

            if snapshot_time < self.next_rtime {
                continue;
            }

            while snapshot_time >= self.next_rtime + self.report_step && self.report_step > 0 {
                self.next_rtime += self.report_step;
            }

            write_dynamic_snapshot(&mut self.writer, network, snapshot, &self.ucf)?;
            self.n_periods += 1;

            if self.report_step > 0 {
                self.next_rtime += self.report_step;
            }
        }

        Ok(())
    }

    /// Finalize the file by patching energy and appending reactions+epilog.
    pub fn finish(mut self, session: &impl WritableSimulation) -> std::io::Result<W> {
        let dynamic_end = self.writer.stream_position()?;

        self.writer
            .seek(std::io::SeekFrom::Start(self.energy_offset))?;
        write_energy(&mut self.writer, session, self.output_units)?;

        self.writer.seek(std::io::SeekFrom::Start(dynamic_end))?;
        write_network_reactions(&mut self.writer, session)?;
        write_epilog(&mut self.writer, self.n_periods, epanet_warn_flag(session))?;

        Ok(self.writer)
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Write an EPANET-compatible binary output file.
///
/// `output_units` controls the unit system used for all numeric values in the
/// file.  Pass `session.net().options.flow_units` to use the model's
/// declared units (the default behaviour when no `--output-units` flag is given).
///
/// `input_file` and `report_file` are written into the prolog as fixed-width
/// strings (up to 259 chars each).  The caller is responsible for managing the
/// writer and flushing it after this function returns.
pub fn write_binary_output<W: Write + Seek>(
    w: &mut W,
    session: &impl WritableSimulation,
    input_file: &str,
    report_file: &str,
    output_units: FlowUnits,
) -> std::io::Result<()> {
    let mut stream = OutStreamWriter::begin(w, session, input_file, report_file, output_units)?;
    stream.append_available(session)?;
    let _ = stream.finish(session)?;

    Ok(())
}

// ── Prolog (crates/interface/cli/spec.md §4.1.1) ──────────────────────────────────────────────

fn write_prolog<W: Write>(
    w: &mut W,
    session: &impl WritableSimulation,
    ucf: &Ucf,
    input_file: &str,
    report_file: &str,
    output_units: FlowUnits,
) -> std::io::Result<()> {
    let network = session.net();
    let options = &network.options;

    let n_nodes = network.nodes.len() as i32;
    let n_links = network.links.len() as i32;

    // Count reservoirs + tanks.
    let n_tanks: i32 = network
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Reservoir(_) | NodeKind::Tank(_)))
        .count() as i32;
    let n_pumps: i32 = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
        .count() as i32;
    let n_valves: i32 = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Valve(_)))
        .count() as i32;

    let quality_flag: i32 = match options.quality_mode {
        QualityMode::None => 0,
        QualityMode::Chemical => 1,
        QualityMode::Age => 2,
        QualityMode::Trace => 3,
    };

    let trace_node_idx: i32 = if options.quality_mode == QualityMode::Trace {
        options
            .trace_node
            .as_ref()
            .and_then(|id| network.nodes.iter().position(|n| n.base.id == *id))
            .map(|i| (i + 1) as i32)
            .unwrap_or(0)
    } else {
        0
    };

    let flow_units_code: i32 = flow_units_to_code(output_units);
    let pressure_units_code: i32 = if is_si(output_units) { 2 } else { 0 };
    let report_statistic: i32 = 0; // Series (always)

    // 15 × INT4 header
    write_i32(w, MAGIC)?;
    write_i32(w, VERSION)?;
    write_i32(w, n_nodes)?;
    write_i32(w, n_tanks)?;
    write_i32(w, n_links)?;
    write_i32(w, n_pumps)?;
    write_i32(w, n_valves)?;
    write_i32(w, quality_flag)?;
    write_i32(w, trace_node_idx)?;
    write_i32(w, flow_units_code)?;
    write_i32(w, pressure_units_code)?;
    write_i32(w, report_statistic)?;
    write_i32(w, options.report_start as i32)?;
    write_i32(w, options.report_step as i32)?;
    write_i32(w, options.duration as i32)?;

    // 3 title lines × 80 bytes
    for i in 0..3 {
        let line = network.title.get(i).map(|s| s.as_str()).unwrap_or("");
        write_fixed_str(w, line, TITLELEN)?;
    }

    // 2 filenames × 260 bytes
    write_fixed_str(w, input_file, MAXFNAME)?;
    write_fixed_str(w, report_file, MAXFNAME)?;

    // Chemical name + units × 32 bytes each
    write_fixed_str(w, &network.options.chem_name, MAXID)?;
    write_fixed_str(w, &network.options.chem_units, MAXID)?;

    // Node IDs
    for node in &network.nodes {
        write_fixed_str(w, &node.base.id, MAXID)?;
    }

    // Link IDs
    for link in &network.links {
        write_fixed_str(w, &link.base.id, MAXID)?;
    }

    // Link from-node indices (1-based)
    for link in &network.links {
        write_i32(w, link.base.from_node as i32)?;
    }

    // Link to-node indices (1-based)
    for link in &network.links {
        write_i32(w, link.base.to_node as i32)?;
    }

    // Link type codes
    for link in &network.links {
        write_i32(w, link_type_code(link))?;
    }

    // Tank/reservoir node indices (1-based)
    for node in &network.nodes {
        if matches!(node.kind, NodeKind::Reservoir(_) | NodeKind::Tank(_)) {
            write_i32(w, node.base.index as i32)?;
        }
    }

    // Tank cross-section areas (sq m — internal units, NOT unit-converted)
    for node in &network.nodes {
        match &node.kind {
            NodeKind::Tank(t) => {
                let area = if let Some(ref cv_id) = t.volume_curve {
                    // EPANET inittanks + convertunits: nominal area = average dV/dh
                    // from volume curve's first to last point.
                    if let Some(curve) = network.curves.iter().find(|c| c.id == *cv_id) {
                        let pts = &curve.points;
                        let last = pts.len() - 1;
                        let dx = pts[last].x - pts[0].x;
                        if dx > 0.0 {
                            (pts[last].y - pts[0].y) / dx
                        } else {
                            0.0
                        }
                    } else {
                        std::f64::consts::PI * (t.diameter / 2.0).powi(2)
                    }
                } else {
                    std::f64::consts::PI * (t.diameter / 2.0).powi(2)
                };
                write_f32(w, area as f32)?;
            }
            NodeKind::Reservoir(_) => {
                write_f32(w, 0.0)?;
            }
            _ => {}
        }
    }

    // Node elevations (output length units).
    // Tanks: internal elevation = bottom + min_level; recover original by
    // subtracting min_level before converting to output units.
    for node in &network.nodes {
        let elev = match &node.kind {
            NodeKind::Tank(t) => (node.base.elevation - t.min_level) * ucf.elev,
            _ => node.base.elevation * ucf.elev,
        };
        write_f32(w, elev as f32)?;
    }

    // Link lengths (output length units)
    for link in &network.links {
        let length = match &link.kind {
            LinkKind::Pipe(p) => p.length * ucf.elev,
            _ => 0.0,
        };
        write_f32(w, length as f32)?;
    }

    // Link diameters (output diameter units; 0 for pumps)
    for link in &network.links {
        let diam = match &link.kind {
            LinkKind::Pipe(p) => p.diameter * ucf.diam,
            LinkKind::Valve(v) => v.diameter * ucf.diam,
            LinkKind::Pump(_) => 0.0,
        };
        write_f32(w, diam as f32)?;
    }

    Ok(())
}

// ── Energy (crates/interface/cli/spec.md §4.1.2) ──────────────────────────────────────────────

fn write_energy<W: Write>(
    w: &mut W,
    session: &impl WritableSimulation,
    output_units: FlowUnits,
) -> std::io::Result<()> {
    let network = session.net();
    let options = &network.options;
    let duration = options.duration;

    for (link_index, link) in network.links.iter().enumerate() {
        if !matches!(link.kind, LinkKind::Pump(_)) {
            continue;
        }

        let pe = session
            .pump_energy_at(link_index)
            .filter(|pe| pe.time_online > 0.0);

        // 1-based link index
        write_i32(w, (link_index + 1) as i32)?;

        if let Some(pe) = pe {
            // EPANET (output.c writeenergy): when Dur==0, time normalisation
            // uses 1 hour (the synthetic dt from addenergy).  Hydra accumulated
            // with dt=3600 s, so dividing by 3600 reproduces the same result.
            let pct_online = if duration > 0.0 {
                (pe.time_online / duration * 100.0) as f32
            } else {
                (pe.time_online / 3600.0 * 100.0) as f32
            };
            let avg_eff = (pe.avg_efficiency() * 100.0) as f32;
            // kWh per unit of flow: EPANET reports kWh/Mgal (US) or kWh/m³ (SI).
            // kwh_per_flow is accumulated as (kW / flow_CFS) * dt; divide by
            // time_online to get average kW/CFS, then convert to output units.
            let avg_kwh_per_flow = if pe.time_online > 0.0 {
                let raw = pe.kwh_per_flow / pe.time_online;
                // GPMperCFS = 448.831, LPSperCFS = 28.317
                if is_si(output_units) {
                    (raw * (1000.0 / 28.317 / 3600.0)) as f32
                } else {
                    (raw * (1.0e6 / 448.831 / 60.0)) as f32
                }
            } else {
                0.0
            };
            let avg_kw = if pe.time_online > 0.0 {
                (pe.kwh * 3600.0 / pe.time_online) as f32
            } else {
                0.0
            };
            let peak_kw = pe.max_kw as f32;
            let avg_cost = if duration > 0.0 {
                (pe.total_cost / (duration / 86400.0)) as f32
            } else {
                (pe.total_cost * 24.0) as f32
            };

            write_f32(w, pct_online)?;
            write_f32(w, avg_eff)?;
            write_f32(w, avg_kwh_per_flow)?;
            write_f32(w, avg_kw)?;
            write_f32(w, peak_kw)?;
            write_f32(w, avg_cost)?;
        } else {
            // No energy data — write zeroes.
            for _ in 0..6 {
                write_f32(w, 0.0)?;
            }
        }
    }

    // Trailing REAL4: demand charge
    let demand_charge = session.peak_demand_kw() * network.options.peak_demand_charge;
    write_f32(w, demand_charge as f32)?;

    Ok(())
}

fn write_energy_placeholder<W: Write>(w: &mut W, network: &crate::Network) -> std::io::Result<()> {
    let n_pumps = network
        .links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Pump(_)))
        .count();
    let bytes = 28 * n_pumps + 4;
    if bytes > 0 {
        w.write_all(&vec![0u8; bytes])?;
    }
    Ok(())
}

// ── Dynamic Results (crates/interface/cli/spec.md §4.1.3) ─────────────────────────────────────

#[allow(dead_code)]
fn write_dynamic_results<W: Write + Seek>(
    w: &mut W,
    session: &impl WritableSimulation,
    ucf: &Ucf,
) -> std::io::Result<i32> {
    let network = session.net();
    let options = &network.options;
    let snapshots = session.snapshots();

    // Filter snapshots to report boundaries using integer time tracking
    // (matches EPANET's approach: Rtime starts at Rstart, advances by Rstep).
    let report_start = options.report_start.round() as i64;
    let report_step = options.report_step.round() as i64;
    let mut next_rtime: i64 = report_start;

    let mut n_periods: i32 = 0;

    for snapshot in snapshots {
        let snapshot_time = snapshot.t.round() as i64;

        // Only emit snapshots at or past the next report boundary.
        if snapshot_time < next_rtime {
            continue;
        }
        // Advance report boundary past current snapshot time.
        while snapshot_time >= next_rtime + report_step && report_step > 0 {
            next_rtime += report_step;
        }

        write_dynamic_snapshot(w, network, snapshot, ucf)?;

        n_periods += 1;
        if report_step > 0 {
            next_rtime += report_step;
        }
    }

    Ok(n_periods)
}

fn write_dynamic_snapshot<W: Write>(
    w: &mut W,
    network: &crate::Network,
    snapshot: &crate::io::HydSnapshot,
    ucf: &Ucf,
) -> std::io::Result<()> {
    let n_nodes = network.nodes.len();
    let n_links = network.links.len();
    let snapshot_bytes = (n_nodes * 4 + n_links * 9) * 4;
    let mut buf: Vec<u8> = Vec::with_capacity(snapshot_bytes);

    // Demand
    for (i, node) in network.nodes.iter().enumerate() {
        let node_state = &snapshot.node_states[i];
        let demand = match &node.kind {
            NodeKind::Junction(_) => {
                node_state.demand_flow + node_state.emitter_flow + node_state.leakage_flow
            }
            NodeKind::Reservoir(_) | NodeKind::Tank(_) => node_state.net_flow,
        };
        buf.extend_from_slice(&((demand * ucf.flow) as f32).to_le_bytes());
    }

    // Head
    for (i, _node) in network.nodes.iter().enumerate() {
        let node_state = &snapshot.node_states[i];
        buf.extend_from_slice(&((node_state.head * ucf.elev) as f32).to_le_bytes());
    }

    // Pressure
    for (i, node) in network.nodes.iter().enumerate() {
        let node_state = &snapshot.node_states[i];
        let physical_elevation = match &node.kind {
            NodeKind::Tank(t) => node.base.elevation - t.min_level,
            _ => node.base.elevation,
        };
        let pressure_ft = node_state.head - physical_elevation;
        buf.extend_from_slice(&((pressure_ft * ucf.pressure) as f32).to_le_bytes());
    }

    // Node quality
    for (i, _node) in network.nodes.iter().enumerate() {
        let node_state = &snapshot.node_states[i];
        buf.extend_from_slice(&(node_state.quality as f32).to_le_bytes());
    }

    // Link flow
    for (i, _link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        let flow = if is_closed(link_state.status) {
            0.0
        } else {
            link_state.flow
        };
        buf.extend_from_slice(&((flow * ucf.flow) as f32).to_le_bytes());
    }

    // Velocity
    for (i, link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        let velocity = if is_closed(link_state.status) {
            0.0
        } else {
            match &link.kind {
                LinkKind::Pump(_) => 0.0,
                LinkKind::Pipe(p) => {
                    let area = std::f64::consts::PI * (p.diameter / 2.0).powi(2);
                    if area > 0.0 {
                        (link_state.flow.abs() / area) * ucf.elev
                    } else {
                        0.0
                    }
                }
                LinkKind::Valve(v) => {
                    let area = std::f64::consts::PI * (v.diameter / 2.0).powi(2);
                    if area > 0.0 {
                        (link_state.flow.abs() / area) * ucf.elev
                    } else {
                        0.0
                    }
                }
            }
        };
        buf.extend_from_slice(&(velocity as f32).to_le_bytes());
    }

    // Headloss
    for (i, link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        let from_node_index = link.base.from_idx();
        let to_node_index = link.base.to_idx();
        let headloss = if is_closed(link_state.status) {
            0.0
        } else {
            match &link.kind {
                LinkKind::Pipe(p) => {
                    let head_drop = (snapshot.node_states[from_node_index].head
                        - snapshot.node_states[to_node_index].head)
                        .abs();
                    if p.length > 0.0 {
                        1000.0 * head_drop / p.length
                    } else {
                        0.0
                    }
                }
                LinkKind::Pump(_) => {
                    let head_drop = snapshot.node_states[from_node_index].head
                        - snapshot.node_states[to_node_index].head;
                    head_drop * ucf.elev
                }
                LinkKind::Valve(_) => {
                    let head_drop = (snapshot.node_states[from_node_index].head
                        - snapshot.node_states[to_node_index].head)
                        .abs();
                    head_drop * ucf.elev
                }
            }
        };
        buf.extend_from_slice(&(headloss as f32).to_le_bytes());
    }

    // Link quality
    for (i, _link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        buf.extend_from_slice(&(link_state.quality as f32).to_le_bytes());
    }

    // Status
    for (i, _link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        buf.extend_from_slice(&status_to_f32(link_state.status).to_le_bytes());
    }

    // Setting
    for (i, link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        let setting = match &link.kind {
            LinkKind::Pipe(p) => p.roughness,
            LinkKind::Pump(_) => link_state.setting,
            LinkKind::Valve(v) => {
                if link_state.setting.is_nan() {
                    0.0
                } else {
                    match v.valve_type {
                        ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                            link_state.setting * ucf.pressure
                        }
                        ValveType::Fcv => link_state.setting * ucf.flow,
                        _ => link_state.setting,
                    }
                }
            }
        };
        buf.extend_from_slice(&(setting as f32).to_le_bytes());
    }

    // Reaction rate
    for (i, _link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        buf.extend_from_slice(&(link_state.reaction_rate as f32).to_le_bytes());
    }

    // Friction factor
    for (i, link) in network.links.iter().enumerate() {
        let link_state = &snapshot.link_states[i];
        let friction_factor = if let LinkKind::Pipe(p) = &link.kind {
            if link_state.flow.abs() > 1.0e-6 && p.length > 0.0 {
                let from_node_index = link.base.from_idx();
                let to_node_index = link.base.to_idx();
                let from_head = snapshot.node_states[from_node_index].head as f32 as f64;
                let to_head = snapshot.node_states[to_node_index].head as f32 as f64;
                let head_drop = (from_head - to_head).abs();
                let q = link_state.flow;
                // f = π²·2g·D⁵/(16·L·Q²) where D, L in m, Q in m³/s, g = 9.81 m/s².
                // Coefficient: π²·2·9.81/16 ≈ 12.106
                12.106 * head_drop * p.diameter.powi(5) / (p.length * q * q)
            } else {
                0.0
            }
        } else {
            0.0
        };
        buf.extend_from_slice(&(friction_factor as f32).to_le_bytes());
    }

    w.write_all(&buf)
}

// ── Network Reactions (crates/interface/cli/spec.md §4.1.4) ───────────────────────────────────

fn write_network_reactions<W: Write>(
    w: &mut W,
    session: &impl WritableSimulation,
) -> std::io::Result<()> {
    // §6.9.1: rates = accumulated mass / duration_hours.
    // All four accumulators are in mg/L × m³ (SI): concentration change × segment
    // volume. Since 1 m³ = 1000 L, each unit of accumulation equals 1000 mg.
    // Multiply by 1000 to convert to mg, then divide by duration_hours to get mg/hr.
    const L_PER_M3: f64 = 1000.0;
    let duration = session.net().options.duration;
    let duration_hours = if duration > 0.0 {
        duration / 3600.0
    } else {
        1.0
    };

    let (bulk, wall, tank, source) = match session.mass_balance() {
        Some(mb) => (
            (mb.reacted_bulk * L_PER_M3 / duration_hours) as f32,
            (mb.reacted_wall * L_PER_M3 / duration_hours) as f32,
            (mb.reacted_tank * L_PER_M3 / duration_hours) as f32,
            (mb.source * L_PER_M3 / duration_hours) as f32,
        ),
        None => (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32),
    };

    write_f32(w, bulk)?;
    write_f32(w, wall)?;
    write_f32(w, tank)?;
    write_f32(w, source)?;
    Ok(())
}

// ── Epilog (crates/interface/cli/spec.md §4.1.5) ──────────────────────────────────────────────

fn write_epilog<W: Write>(w: &mut W, n_periods: i32, warn_flag: i32) -> std::io::Result<()> {
    write_i32(w, n_periods)?;
    write_i32(w, warn_flag)?;
    write_i32(w, MAGIC)?;
    Ok(())
}

/// Compute the EPANET-compatible warning flag from the session's warnings.
///
/// EPANET checks warnings per time step in this order (later overrides earlier):
///   2 = system unstable (converged but barely)
///   6 = pressure deficient nodes (DDA negative pressure)
///   5 = abnormal valve condition (XFCV/XPRESSURE)
///   4 = pump out of range (XHEAD/XFLOW)
///   1 = system unbalanced (solver didn't converge)
///
/// The global flag is overwritten each time step that has any warning,
/// so the final flag comes from the last time step with a warning.
fn epanet_warn_flag(session: &impl WritableSimulation) -> i32 {
    use super::WarningKind;
    let warnings = session.warnings();
    if warnings.is_empty() {
        return 0;
    }

    // Group by approximate time step and compute per-step flag using
    // EPANET's priority order: unstable(2) < pressure(6) < valve(5)
    // < pump(4) < unbalanced(1). Later checks override earlier ones.
    let mut global_flag: i32 = 0;
    let mut current_t = f64::NEG_INFINITY;
    let mut has_unbalanced = false;
    let mut has_pressure = false;
    let mut has_pump = false;

    let flush = |unbal: &mut bool, press: &mut bool, pump: &mut bool| -> i32 {
        // EPANET checking order: 2→6→5→4→1 (last overrides first).
        let mut f: i32 = 0;
        if *press {
            f = 6;
        }
        if *pump {
            f = 4;
        }
        if *unbal {
            f = 1;
        }
        *unbal = false;
        *press = false;
        *pump = false;
        f
    };

    for w in warnings {
        if (w.t - current_t).abs() > 0.5 {
            let f = flush(&mut has_unbalanced, &mut has_pressure, &mut has_pump);
            if f > 0 {
                global_flag = f;
            }
            current_t = w.t;
        }
        match &w.kind {
            WarningKind::NegativePressure { .. } => {
                has_pressure = true;
            }
            WarningKind::PumpXHead { .. } => {
                has_pump = true;
            }
            WarningKind::UnbalancedHydraulics => {
                has_unbalanced = true;
            }
        }
    }
    let f = flush(&mut has_unbalanced, &mut has_pressure, &mut has_pump);
    if f > 0 {
        global_flag = f;
    }
    global_flag
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn write_i32<W: Write>(w: &mut W, v: i32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

fn write_f32<W: Write>(w: &mut W, v: f32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Write a string into a fixed-width zero-padded field.
fn write_fixed_str<W: Write>(w: &mut W, s: &str, width: usize) -> std::io::Result<()> {
    let bytes = s.as_bytes();
    let n = bytes.len().min(width - 1); // leave room for at least one null
    w.write_all(&bytes[..n])?;
    // Zero-pad the remainder
    let padding = width - n;
    if padding > 0 {
        const ZEROS: [u8; 256] = [0u8; 256];
        let mut rem = padding;
        while rem > 0 {
            let chunk = rem.min(ZEROS.len());
            w.write_all(&ZEROS[..chunk])?;
            rem -= chunk;
        }
    }
    Ok(())
}

fn is_si(fu: FlowUnits) -> bool {
    matches!(
        fu,
        FlowUnits::Lps
            | FlowUnits::Lpm
            | FlowUnits::Mld
            | FlowUnits::Cmh
            | FlowUnits::Cmd
            | FlowUnits::Cms
    )
}

fn flow_units_to_code(fu: FlowUnits) -> i32 {
    match fu {
        FlowUnits::Cfs => 0,
        FlowUnits::Gpm => 1,
        FlowUnits::Mgd => 2,
        FlowUnits::Imgd => 3,
        FlowUnits::Afd => 4,
        FlowUnits::Lps => 5,
        FlowUnits::Lpm => 6,
        FlowUnits::Mld => 7,
        FlowUnits::Cmh => 8,
        FlowUnits::Cmd => 9,
        FlowUnits::Cms => 10,
    }
}

fn link_type_code(link: &crate::Link) -> i32 {
    match &link.kind {
        LinkKind::Pipe(p) => {
            if p.check_valve {
                0
            } else {
                1
            }
        }
        LinkKind::Pump(_) => 2,
        LinkKind::Valve(v) => match v.valve_type {
            ValveType::Prv => 3,
            ValveType::Psv => 4,
            ValveType::Pbv => 5,
            ValveType::Fcv => 6,
            ValveType::Tcv => 7,
            ValveType::Gpv => 8,
            ValveType::Pcv => 1, // PCV treated as pipe-like for output
        },
    }
}

fn is_closed(status: LinkStatus) -> bool {
    matches!(
        status,
        LinkStatus::Closed | LinkStatus::XHead | LinkStatus::TempClosed
    )
}

/// Map Hydra `LinkStatus` to EPANET `StatusType` enum value (0–10).
fn status_to_f32(status: LinkStatus) -> f32 {
    match status {
        LinkStatus::XHead => 0.0,
        LinkStatus::TempClosed => 1.0,
        LinkStatus::Closed => 2.0,
        LinkStatus::Open => 3.0,
        LinkStatus::Active => 4.0,
        LinkStatus::XFcv => 6.0,
        LinkStatus::XPressure => 7.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::parse;
    use std::io::Cursor;
    use std::path::Path;

    struct MockSession {
        network: crate::Network,
        snapshots: Vec<crate::io::HydSnapshot>,
        warnings: Vec<crate::io::SimWarning>,
        begun: Option<std::time::SystemTime>,
        ended: Option<std::time::SystemTime>,
    }

    impl crate::io::WritableSimulation for MockSession {
        fn net(&self) -> &crate::Network {
            &self.network
        }
        fn snapshots(&self) -> &[crate::io::HydSnapshot] {
            &self.snapshots
        }
        fn pump_energy_at(&self, _link_index: usize) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn peak_demand_kw(&self) -> f64 {
            0.0
        }
        fn mass_balance(&self) -> Option<&crate::io::MassBalance> {
            None
        }
        fn warnings(&self) -> &[crate::io::SimWarning] {
            &self.warnings
        }
        fn pump_energy_by_id(&self, _pump_id: &str) -> Option<&crate::io::PumpEnergy> {
            None
        }
        fn analysis_times(&self) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
            (self.begun, self.ended)
        }
        fn flow_balance(&self) -> Option<&crate::io::FlowBalance> {
            None
        }
        fn flow_balance_summary(&self) -> Option<crate::io::FlowBalanceSummary> {
            None
        }
    }

    fn load_fixture_network(name: &str) -> crate::Network {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures")
            .join(name);
        let bytes = std::fs::read(path).expect("read fixture");
        parse(&bytes).expect("parse fixture")
    }

    fn mock_session(name: &str) -> MockSession {
        let network = load_fixture_network(name);
        let node_states = network
            .nodes
            .iter()
            .map(|node| crate::NodeState {
                head: node.base.elevation,
                ..crate::NodeState::default()
            })
            .collect();
        let link_states = network
            .links
            .iter()
            .map(|_| crate::LinkState::default())
            .collect();

        MockSession {
            network,
            snapshots: vec![crate::io::HydSnapshot {
                t: 0.0,
                node_states,
                link_states,
            }],
            warnings: Vec::new(),
            begun: None,
            ended: None,
        }
    }

    #[test]
    fn write_fixed_str_zero_pads_and_truncates() {
        let mut buf = Vec::new();
        write_fixed_str(&mut buf, "abcdef", 5).expect("write fixed string");
        assert_eq!(&buf[..4], b"abcd");
        assert_eq!(buf[4], 0);
    }

    #[test]
    fn flow_units_to_code_matches_epanet_codes() {
        assert_eq!(flow_units_to_code(FlowUnits::Cfs), 0);
        assert_eq!(flow_units_to_code(FlowUnits::Gpm), 1);
        assert_eq!(flow_units_to_code(FlowUnits::Lps), 5);
        assert_eq!(flow_units_to_code(FlowUnits::Cms), 10);
    }

    #[test]
    fn status_to_f32_matches_epanet_status_enum() {
        assert_eq!(status_to_f32(LinkStatus::Closed), 2.0);
        assert_eq!(status_to_f32(LinkStatus::Open), 3.0);
        assert_eq!(status_to_f32(LinkStatus::Active), 4.0);
        assert_eq!(status_to_f32(LinkStatus::XPressure), 7.0);
    }

    #[test]
    fn epanet_warn_flag_prefers_last_step_warning_with_epanet_priority() {
        let mut session = mock_session("single_pipe_hw.inp");
        let warnings = vec![
            crate::io::SimWarning {
                t: 0.0,
                kind: crate::io::WarningKind::NegativePressure { node_index: 0 },
            },
            crate::io::SimWarning {
                t: 0.0,
                kind: crate::io::WarningKind::PumpXHead { link_index: 0 },
            },
            crate::io::SimWarning {
                t: 3600.0,
                kind: crate::io::WarningKind::UnbalancedHydraulics,
            },
        ];
        session.warnings = warnings;
        assert_eq!(epanet_warn_flag(&session), 1);
    }

    #[test]
    fn write_binary_output_writes_expected_magic_and_version() {
        let session = mock_session("single_pipe_hw.inp");
        let mut buf = Cursor::new(Vec::new());
        write_binary_output(&mut buf, &session, "test.inp", "test.rpt", FlowUnits::Gpm)
            .expect("write binary output");
        let data = buf.into_inner();
        assert_eq!(i32::from_le_bytes(data[0..4].try_into().unwrap()), MAGIC);
        assert_eq!(i32::from_le_bytes(data[4..8].try_into().unwrap()), VERSION);
        assert_eq!(
            i32::from_le_bytes(data[data.len() - 4..].try_into().unwrap()),
            MAGIC
        );
    }

    /// Verifies that `write_network_reactions` converts accumulators using 1000
    /// L/m³ rather than 28.317 L/ft³.
    ///
    /// A `reacted_bulk` of 1.0 mg/L × m³ over a 1-hour simulation should
    /// produce a rate of 1.0 × 1000 / 1.0 = 1000.0 mg/hr in the binary output.
    /// With the old factor 28.317 the output would be ≈ 28.317 mg/hr (35× smaller).
    #[test]
    fn network_reactions_use_l_per_m3_conversion_factor() {
        struct MbSession {
            network: crate::Network,
            mb: crate::io::MassBalance,
        }
        impl crate::io::WritableSimulation for MbSession {
            fn net(&self) -> &crate::Network {
                &self.network
            }
            fn snapshots(&self) -> &[crate::io::HydSnapshot] {
                &[]
            }
            fn pump_energy_at(&self, _: usize) -> Option<&crate::io::PumpEnergy> {
                None
            }
            fn peak_demand_kw(&self) -> f64 {
                0.0
            }
            fn mass_balance(&self) -> Option<&crate::io::MassBalance> {
                Some(&self.mb)
            }
            fn warnings(&self) -> &[crate::io::SimWarning] {
                &[]
            }
            fn pump_energy_by_id(&self, _: &str) -> Option<&crate::io::PumpEnergy> {
                None
            }
            fn analysis_times(
                &self,
            ) -> (Option<std::time::SystemTime>, Option<std::time::SystemTime>) {
                (None, None)
            }
            fn flow_balance(&self) -> Option<&crate::io::FlowBalance> {
                None
            }
            fn flow_balance_summary(&self) -> Option<crate::io::FlowBalanceSummary> {
                None
            }
        }

        let mut network = load_fixture_network("dead_end.inp");
        network.options.duration = 3600.0; // 1 hour → duration_hours = 1.0

        let mb = crate::io::MassBalance {
            reacted_bulk: 1.0, // 1.0 mg/L × m³
            ..crate::io::MassBalance::default()
        };
        let session = MbSession { network, mb };

        let mut buf = Vec::new();
        write_network_reactions(&mut buf, &session).unwrap();

        // First f32 written is the bulk reaction rate.
        let bulk_rate = f32::from_le_bytes(buf[0..4].try_into().unwrap());
        // Expected: 1.0 × 1000 L/m³ / 1.0 hr = 1000.0 mg/hr
        assert!(
            (bulk_rate - 1000.0_f32).abs() < 0.001_f32,
            "expected 1000.0 mg/hr but got {bulk_rate:.3} (old code with 28.317 factor would give ≈28.317)"
        );
    }
}
