//! Routing interface files (§14.8): text files carrying dated flow and
//! concentration series at outlet vertices — inflow files are read-only
//! boundary inflows, outflow files are written from outlet vertices, one
//! file never serving both roles in a run. Values interpolate between
//! bracketing periods, unmatched pollutants read as zero, and flows
//! convert from the *file's* declared units.

use std::io::{self, Write};

use crate::io::lex::FiniteParse;
use crate::model::Network;
use crate::simulation::engine::Snapshot;

/// Declared-count bounds (§14.8): generous against any real model, small
/// against an allocation attack.
const MAX_IFACE_CONSTITUENTS: usize = 100;
const MAX_IFACE_NODES: usize = 100_000;

const FLOW_WORDS: [(&str, f64); 6] = [
    ("CFS", 0.028_316_846_592),
    ("GPM", 6.309_019_64e-5),
    ("MGD", 0.043_812_636_4),
    ("CMS", 1.0),
    ("LPS", 1.0e-3),
    ("MLD", 1.0 / 86.4),
];

/// A parsed routing interface file, resolved against a model.
pub struct RoutingInterface {
    /// The file's reporting step (s).
    pub step: f64,
    /// Receiving vertex per file node column (`None` = unknown vertex,
    /// carried but unused).
    pub vertices: Vec<Option<usize>>,
    /// Model constituent per file constituent column (§14.8: unmatched
    /// pollutants read as zero).
    pub constituents: Vec<Option<usize>>,
    /// m³/s per file flow unit.
    pub flow_cv: f64,
    /// Dated records: (epoch s, per-file-node rows of `flow, qual…` in
    /// file units).
    pub records: Vec<(f64, Vec<Vec<f64>>)>,
}

impl RoutingInterface {
    /// The interpolated (flow m³/s, per-model-constituent concentration)
    /// additions at epoch `t`, per resolved vertex.
    pub fn inflows_at(&self, epoch: f64, np: usize) -> Vec<(usize, f64, Vec<f64>)> {
        let mut out = Vec::new();
        if self.records.is_empty() {
            return out;
        }
        // Bracketing records: the series' own span is served inclusive
        // of both end instants, nothing beyond (§14.8).
        let first_t = self.records[0].0;
        let last_t = self.records[self.records.len() - 1].0;
        if epoch < first_t - 1e-9 || epoch > last_t + 1e-9 {
            return out;
        }
        if self.records.len() == 1 {
            let (_, rows) = &self.records[0];
            for (col, v) in self.vertices.iter().enumerate() {
                let Some(vi) = v else { continue };
                let a = &rows[col];
                let q = a.first().copied().unwrap_or(0.0) * self.flow_cv;
                let mut conc = vec![0.0; np];
                for (fc, m) in self.constituents.iter().enumerate() {
                    if let Some(p) = m {
                        conc[*p] = a.get(fc + 1).copied().unwrap_or(0.0);
                    }
                }
                out.push((*vi, q, conc));
            }
            return out;
        }
        let e = epoch.clamp(first_t, last_t);
        let i = self
            .records
            .iter()
            .position(|(t, _)| *t >= e)
            .unwrap_or(self.records.len() - 1)
            .clamp(1, self.records.len() - 1);
        let (t0, r0) = &self.records[i - 1];
        let (t1, r1) = &self.records[i];
        let f = if t1 > t0 {
            (epoch - t0) / (t1 - t0)
        } else {
            1.0
        };
        for (col, v) in self.vertices.iter().enumerate() {
            let Some(vi) = v else { continue };
            let (a, b) = (&r0[col], &r1[col]);
            let at = |r: &Vec<f64>, i: usize| r.get(i).copied().unwrap_or(0.0);
            let q = ((1.0 - f) * at(a, 0) + f * at(b, 0)) * self.flow_cv;
            let mut conc = vec![0.0; np];
            for (fc, m) in self.constituents.iter().enumerate() {
                if let Some(p) = m {
                    conc[*p] = (1.0 - f) * at(a, fc + 1) + f * at(b, fc + 1);
                }
            }
            out.push((*vi, q, conc));
        }
        out
    }
}

/// Parse a routing interface file against the model: nodes and
/// constituents resolve by identity, the flow unit converts from the
/// file's declaration. `start_epoch` anchors record times (s).
pub fn parse_routing_file(text: &str, net: &Network) -> Result<RoutingInterface, String> {
    let mut lines = text.lines();
    let head = lines.next().ok_or("empty interface file")?;
    if !head.trim_start().starts_with("SWMM5") {
        return Err("not a SWMM5 interface file".into());
    }
    let _title = lines.next().ok_or("truncated interface file")?;
    let step: f64 = first_number(lines.next().ok_or("missing time step")?)?;
    let n_con = first_number(lines.next().ok_or("missing constituent count")?)? as usize;
    if n_con < 1 {
        return Err("interface file declares no FLOW column".into());
    }
    // §14.8: declared counts are bounded — each period allocates an
    // n_nodes × n_con matrix, so unbounded counts let a kilobyte-scale
    // file demand gigabytes.
    if n_con > 1 + MAX_IFACE_CONSTITUENTS {
        return Err(format!(
            "interface file declares {n_con} constituents (limit {})",
            1 + MAX_IFACE_CONSTITUENTS
        ));
    }
    // First constituent line must be FLOW with its unit.
    let flow_line = lines.next().ok_or("missing FLOW line")?;
    let mut it = flow_line.split_whitespace();
    if it.next().map(str::to_ascii_uppercase).as_deref() != Some("FLOW") {
        return Err("first interface constituent must be FLOW".into());
    }
    let unit = it.next().unwrap_or("CMS").to_ascii_uppercase();
    let flow_cv = FLOW_WORDS
        .iter()
        .find(|(w, _)| *w == unit)
        .map(|(_, cv)| *cv)
        .ok_or_else(|| format!("unknown interface flow unit '{unit}'"))?;
    let mut constituents = Vec::new();
    for _ in 1..n_con {
        let line = lines.next().ok_or("truncated constituent list")?;
        let name = line.split_whitespace().next().unwrap_or("");
        constituents.push(net.constituents.iter().position(|c| c.id == name));
    }
    let n_nodes = first_number(lines.next().ok_or("missing node count")?)? as usize;
    if n_nodes > MAX_IFACE_NODES {
        return Err(format!(
            "interface file declares {n_nodes} nodes (limit {MAX_IFACE_NODES})"
        ));
    }
    let mut vertices = Vec::new();
    let mut row_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..n_nodes {
        let line = lines.next().ok_or("truncated node list")?;
        let name = line.split_whitespace().next().unwrap_or("");
        row_of.insert(name.to_string(), i);
        vertices.push(net.vertices.iter().position(|v| v.id == name));
    }
    // Dated records: node rows repeat per period in node-list order.
    let mut records: Vec<(f64, Vec<Vec<f64>>)> = Vec::new();
    for line in lines {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 8 {
            continue;
        }
        let Some(&row) = row_of.get(t[0]) else {
            continue;
        };
        let date = crate::io::options::Date {
            year: t[1].parse().map_err(|_| "bad year")?,
            month: t[2].parse().map_err(|_| "bad month")?,
            day: t[3].parse().map_err(|_| "bad day")?,
        };
        let secs: f64 = t[4].finite_f64().map_err(|_| "bad hour")? * 3600.0
            + t[5].finite_f64().map_err(|_| "bad minute")? * 60.0
            + t[6].finite_f64().map_err(|_| "bad second")?;
        let epoch = crate::simulation::time::days_from_civil(date) as f64 * 86_400.0 + secs;
        let mut values: Vec<f64> = t[7..]
            .iter()
            .map(|s| s.finite_f64().unwrap_or(0.0))
            .collect();
        values.resize(n_con, 0.0);
        match records.last_mut() {
            Some((te, rows)) if (*te - epoch).abs() < 1e-6 => rows[row] = values,
            _ => {
                let mut rows = vec![vec![0.0; n_con]; n_nodes];
                rows[row] = values;
                records.push((epoch, rows));
            }
        }
    }
    Ok(RoutingInterface {
        step,
        vertices,
        constituents,
        flow_cv,
        records,
    })
}

fn first_number(line: &str) -> Result<f64, String> {
    line.split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("expected a number in '{line}'"))
}

/// Write a routing interface file from the reporting snapshots: outlet
/// (outfall) vertices' inflows and concentrations per period, in the
/// model's flow units.
pub fn write_routing_file(
    net: &Network,
    snapshots: &[Snapshot],
    start_epoch: f64,
    report_step: f64,
    w: &mut impl Write,
) -> io::Result<()> {
    let unit_word = match net.options.flow_units {
        crate::io::options::FlowUnits::Cfs => "CFS",
        crate::io::options::FlowUnits::Gpm => "GPM",
        crate::io::options::FlowUnits::Mgd => "MGD",
        crate::io::options::FlowUnits::Cms => "CMS",
        crate::io::options::FlowUnits::Lps => "LPS",
        crate::io::options::FlowUnits::Mld => "MLD",
    };
    let flow_cv = FLOW_WORDS
        .iter()
        .find(|(word, _)| *word == unit_word)
        .map(|(_, cv)| *cv)
        .unwrap_or(1.0);
    let outlets: Vec<usize> = net
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| matches!(v.kind, crate::model::VertexKind::Outfall { .. }))
        .map(|(i, _)| i)
        .collect();
    writeln!(w, "SWMM5 Interface File")?;
    writeln!(w, "{}", net.title.first().map(String::as_str).unwrap_or(""))?;
    writeln!(w, "{:<4} - reporting time step in sec", report_step as i64)?;
    writeln!(
        w,
        "{:<4} - number of constituents as listed below:",
        1 + net.constituents.len()
    )?;
    writeln!(w, "FLOW {unit_word}")?;
    for c in &net.constituents {
        let u = match c.units {
            crate::model::ConcentrationUnits::MgPerL => "MG/L",
            crate::model::ConcentrationUnits::UgPerL => "UG/L",
            crate::model::ConcentrationUnits::CountPerL => "#/L",
        };
        writeln!(w, "{} {u}", c.id)?;
    }
    writeln!(w, "{:<4} - number of nodes as listed below:", outlets.len())?;
    for &vi in &outlets {
        writeln!(w, "{}", net.vertices[vi].id)?;
    }
    let np = net.constituents.len();
    for snap in snapshots {
        let epoch = start_epoch + snap.t;
        let days = (epoch / 86_400.0).floor() as i64;
        let d = crate::simulation::time::civil_from_days(days);
        let secs = epoch - days as f64 * 86_400.0;
        let (hh, mm, ss) = (
            (secs / 3600.0) as u32,
            ((secs % 3600.0) / 60.0) as u32,
            (secs % 60.0) as u32,
        );
        for &vi in &outlets {
            write!(
                w,
                "{:<16} {:04} {:02}  {:02}  {:02}  {:02}  {:02} ",
                net.vertices[vi].id, d.year, d.month, d.day, hh, mm, ss
            )?;
            write!(w, " {:<10.6}", snap.node_inflow[vi] / flow_cv)?;
            for p in 0..np {
                write!(w, " {:<10.6}", snap.node_quality[p][vi])?;
            }
            writeln!(w)?;
        }
    }
    Ok(())
}

// ── RDII interface files (§14.8.1) ────────────────────────────────────────────

/// Days between the predecessor's date origin and the Unix epoch.
///
/// Its `DateDelta` is 693594 days before 01/01/0000, which its own
/// `datetime_encodeDate` puts exactly this many days before 1970-01-01.
/// Getting it wrong shifts every hydrograph by decades rather than failing.
const SWMM_EPOCH_DAYS: f64 = 25_569.0;

/// How far off an instant may be and still count as the same one (s).
///
/// The predecessor dates records as a decimal day, which cannot hold a
/// whole number of seconds exactly: an hour after midnight reads back as
/// 3600.000000104774 s. A record therefore begins a fraction of a
/// microsecond after the instant it means, and a query at exactly that
/// instant would otherwise fall into the gap and read as no flow. A
/// millisecond is far above the encoding's noise and far below any step
/// a model would use.
const DATE_TOL: f64 = 1e-3;

/// A parsed RDII interface file, resolved against a model.
#[derive(Debug)]
pub struct RdiiInterface {
    /// The file's step (s): how long each record's flows apply.
    pub step: f64,
    /// Model vertex per file column, in the file's own column order.
    pub vertices: Vec<usize>,
    /// Dated records: (epoch s, flow per column in m³/s).
    pub records: Vec<(f64, Vec<f64>)>,
}

impl RdiiInterface {
    /// The (vertex, flow m³/s) additions at epoch `t`.
    ///
    /// Piecewise constant, never interpolated: a record's flows apply from
    /// its own instant until `step` later, and the hydrograph is zero
    /// before the first record, after the last, and in any gap between
    /// them. An RDII hydrograph is a volume already apportioned to a step,
    /// so interpolating would move water between the steps the unit
    /// hydrographs put it in.
    pub fn inflows_at(&self, epoch: f64) -> Vec<(usize, f64)> {
        // The last record at or before `epoch`, within the encoding's own
        // precision. Records are written in time order and read in it.
        let target = epoch + DATE_TOL;
        let idx = match self
            .records
            .binary_search_by(|(t, _)| t.partial_cmp(&target).unwrap_or(std::cmp::Ordering::Less))
        {
            Ok(i) => i,
            Err(0) => return Vec::new(),
            Err(i) => i - 1,
        };
        let (t, flows) = &self.records[idx];
        if epoch >= *t + self.step - DATE_TOL {
            return Vec::new();
        }
        self.vertices
            .iter()
            .zip(flows)
            .map(|(v, q)| (*v, *q))
            .collect()
    }
}

/// m³/s per unit of a model's declared flow unit.
///
/// Matched by name rather than by position: the two lists happen to be in
/// the same order today, and a reordering of either would otherwise
/// silently rescale every flow read from a binary file.
pub fn flow_cv_of(units: crate::io::options::FlowUnits) -> f64 {
    use crate::io::options::FlowUnits::*;
    let word = match units {
        Cfs => "CFS",
        Gpm => "GPM",
        Mgd => "MGD",
        Cms => "CMS",
        Lps => "LPS",
        Mld => "MLD",
    };
    FLOW_WORDS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, cv)| *cv)
        .unwrap_or(1.0)
}

/// Parse an RDII interface file against the model.
///
/// The encoding is chosen by the first ten bytes, as the predecessor
/// chooses it: `SWMM5-RDII` begins the binary form and anything else is
/// read as text.
pub fn parse_rdii_file(
    bytes: &[u8],
    net: &Network,
    model_flow_cv: f64,
) -> Result<RdiiInterface, String> {
    if bytes.starts_with(b"SWMM5-RDII") {
        parse_rdii_binary(&bytes[10..], net, model_flow_cv)
    } else {
        let text = String::from_utf8_lossy(bytes);
        parse_rdii_text(&text, net)
    }
}

/// The binary form: stamp, step, count, that many vertex *positions*, then
/// one record per period.
fn parse_rdii_binary(
    body: &[u8],
    net: &Network,
    model_flow_cv: f64,
) -> Result<RdiiInterface, String> {
    let i32_at = |o: usize| -> Result<i32, String> {
        body.get(o..o + 4)
            .and_then(|s| s.try_into().ok())
            .map(i32::from_le_bytes)
            .ok_or_else(|| "truncated RDII interface file".to_string())
    };
    let step = i32_at(0)?;
    if step <= 0 {
        return Err(format!("RDII interface file declares a step of {step}s"));
    }
    let count = i32_at(4)?;
    if count <= 0 {
        return Err(format!("RDII interface file declares {count} vertices"));
    }
    let count = count as usize;
    if count > MAX_IFACE_NODES {
        return Err(format!(
            "RDII interface file declares {count} vertices (limit {MAX_IFACE_NODES})"
        ));
    }
    // §14.8.1: the format stores positions in the *writing* model's vertex
    // array, not names, so a file is readable only against a model ordered
    // as the writer's was. The predecessor checks only that the vertex at
    // each position happens to have RDII defined; both checks are applied
    // here, and a failure is a refusal naming the file rather than a
    // hydrograph silently landing on the wrong vertex.
    let mut vertices = Vec::with_capacity(count);
    for i in 0..count {
        let raw = i32_at(8 + 4 * i)?;
        let v = usize::try_from(raw)
            .ok()
            .filter(|v| *v < net.vertices.len())
            .ok_or_else(|| {
                format!(
                    "RDII interface file names vertex position {raw}, which this \
                     model does not have: the file was written against a model \
                     with a different vertex order"
                )
            })?;
        if !net.rdii.iter().any(|r| r.vertex == v) {
            return Err(format!(
                "RDII interface file names vertex '{}', which has no RDII \
                 assignment in this model: the file was written against a \
                 different model",
                net.vertices[v].id
            ));
        }
        vertices.push(v);
    }
    // Records: a date as the predecessor's decimal day, then one 32-bit
    // float of flow per vertex. Binary files carry no units, so the flows
    // are in those of the model that wrote it, which can only be assumed
    // to be this one's.
    let mut records = Vec::new();
    let mut o = 8 + 4 * count;
    let record = 8 + 4 * count;
    while o + record <= body.len() {
        let date = f64::from_le_bytes(
            body[o..o + 8]
                .try_into()
                .map_err(|_| "truncated RDII record")?,
        );
        let mut flows = Vec::with_capacity(count);
        for i in 0..count {
            let b = o + 8 + 4 * i;
            let q = f32::from_le_bytes(
                body[b..b + 4]
                    .try_into()
                    .map_err(|_| "truncated RDII record")?,
            );
            flows.push(f64::from(q) * model_flow_cv);
        }
        records.push(((date - SWMM_EPOCH_DAYS) * 86_400.0, flows));
        o += record;
    }
    Ok(RdiiInterface {
        step: f64::from(step),
        vertices,
        records,
    })
}

/// The text form: a `SWMM5` line, a title, the step, a constituent count,
/// the flow units, a vertex count, that many named vertices, a heading
/// line, then one line per vertex per period.
fn parse_rdii_text(text: &str, net: &Network) -> Result<RdiiInterface, String> {
    let mut lines = text.lines();
    let head = lines.next().ok_or("empty RDII interface file")?;
    if head.split_whitespace().next() != Some("SWMM5") {
        return Err("not a SWMM5 RDII interface file".into());
    }
    let _title = lines.next().ok_or("truncated RDII interface file")?;
    let step = first_number(lines.next().ok_or("missing RDII time step")?)?;
    if step <= 0.0 {
        return Err(format!("RDII interface file declares a step of {step}s"));
    }
    let _constituents = lines.next().ok_or("missing RDII constituent count")?;
    let unit_line = lines.next().ok_or("missing RDII flow units")?;
    let unit = unit_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("CMS")
        .to_ascii_uppercase();
    let flow_cv = FLOW_WORDS
        .iter()
        .find(|(w, _)| *w == unit)
        .map(|(_, cv)| *cv)
        .ok_or_else(|| format!("unknown RDII flow unit '{unit}'"))?;
    let count = first_number(lines.next().ok_or("missing RDII vertex count")?)? as usize;
    if count > MAX_IFACE_NODES {
        return Err(format!(
            "RDII interface file declares {count} vertices (limit {MAX_IFACE_NODES})"
        ));
    }
    let mut vertices = Vec::with_capacity(count);
    let mut column_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..count {
        let line = lines.next().ok_or("truncated RDII vertex list")?;
        let name = line.split_whitespace().next().unwrap_or("");
        let v = net
            .vertices
            .iter()
            .position(|v| v.id == name)
            .ok_or_else(|| format!("RDII interface file names unknown vertex '{name}'"))?;
        column_of.insert(name.to_string(), i);
        vertices.push(v);
    }
    let _heading = lines.next().ok_or("truncated RDII interface file")?;
    // §14.8.1: rows are matched by the name each row carries. The
    // predecessor reads that name into a variable its own comment marks
    // "not used" and matches by position instead, so a file whose rows are
    // ordered differently from its header is read there without complaint
    // and every hydrograph is misassigned.
    let mut records: Vec<(f64, Vec<f64>)> = Vec::new();
    for line in lines {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 8 {
            continue;
        }
        let column = *column_of
            .get(t[0])
            .ok_or_else(|| format!("RDII interface file row names unlisted vertex '{}'", t[0]))?;
        let date = crate::io::options::Date {
            year: t[1].parse().map_err(|_| "bad RDII year")?,
            month: t[2].parse().map_err(|_| "bad RDII month")?,
            day: t[3].parse().map_err(|_| "bad RDII day")?,
        };
        let secs = t[4].finite_f64().map_err(|_| "bad RDII hour")? * 3600.0
            + t[5].finite_f64().map_err(|_| "bad RDII minute")? * 60.0
            + t[6].finite_f64().map_err(|_| "bad RDII second")?;
        let epoch = crate::simulation::time::days_from_civil(date) as f64 * 86_400.0 + secs;
        let q = t[7].finite_f64().map_err(|_| "bad RDII flow")? * flow_cv;
        match records.last_mut() {
            Some((te, flows)) if (*te - epoch).abs() < 1e-6 => flows[column] = q,
            _ => {
                let mut flows = vec![0.0; count];
                flows[column] = q;
                records.push((epoch, flows));
            }
        }
    }
    Ok(RdiiInterface {
        step,
        vertices,
        records,
    })
}

/// Write an RDII interface file in the text form (§14.8.1).
///
/// `records` are `(epoch s, flow m³/s per assignment)` as the run
/// convolved them, and `step` the longer of the model's hydrology steps,
/// which bounds every gap between them.
pub fn write_rdii_file(
    net: &Network,
    vertices: &[usize],
    step: f64,
    records: &[(f64, Vec<f64>)],
    w: &mut impl Write,
) -> io::Result<()> {
    let unit = match net.options.flow_units {
        crate::io::options::FlowUnits::Cfs => "CFS",
        crate::io::options::FlowUnits::Gpm => "GPM",
        crate::io::options::FlowUnits::Mgd => "MGD",
        crate::io::options::FlowUnits::Cms => "CMS",
        crate::io::options::FlowUnits::Lps => "LPS",
        crate::io::options::FlowUnits::Mld => "MLD",
    };
    let cv = flow_cv_of(net.options.flow_units);
    writeln!(w, "SWMM5")?;
    writeln!(w, "RDII hydrographs")?;
    writeln!(w, "{}", step.round() as i64)?;
    // One constituent: the flow itself.
    writeln!(w, "1")?;
    writeln!(w, "FLOW {unit}")?;
    writeln!(w, "{}", vertices.len())?;
    for v in vertices {
        writeln!(w, "{}", net.vertices[*v].id)?;
    }
    writeln!(w, "Node             Year Mon Day Hr Min Sec Flow")?;
    for (epoch, flows) in records {
        let days = (*epoch / 86_400.0).floor();
        let date = crate::simulation::time::civil_from_days(days as i64);
        let secs = *epoch - days * 86_400.0;
        let (hr, min, sec) = (
            (secs / 3600.0) as u32,
            ((secs % 3600.0) / 60.0) as u32,
            (secs % 60.0).round() as u32,
        );
        for (v, q) in vertices.iter().zip(flows) {
            writeln!(
                w,
                "{} {} {} {} {} {} {} {:.6}",
                net.vertices[*v].id,
                date.year,
                date.month,
                date.day,
                hr,
                min,
                sec,
                q / cv
            )?;
        }
    }
    Ok(())
}

// ── Runoff interface files (§14.8.2) ─────────────────────────────────────────

/// The predecessor's flow-unit enumeration, as a runoff file records it.
/// Its order is the file format, not an implementation detail.
const FILE_FLOW_UNITS: [&str; 6] = ["CFS", "GPM", "MGD", "CMS", "LPS", "MLD"];

/// One parcel's replayed results for one step, in the file's own order.
///
/// Values are as the file holds them, in the writing model's user units,
/// and are not converted here: the flow unit is checked on parse, so the
/// units are this model's, but which physical unit each field carries
/// depends on the quantity and is the caller's to apply (§14.8.2).
#[derive(Debug, Clone)]
pub struct ParcelReplay {
    /// Rainfall intensity.
    pub rainfall: f64,
    /// Snow depth.
    pub snow_depth: f64,
    /// Evaporation loss.
    pub evap: f64,
    /// Infiltration loss.
    pub infil: f64,
    /// Runoff flow, in the file's flow unit.
    pub runoff: f64,
    /// Groundwater flow to the vertex, in the file's flow unit.
    pub gw_flow: f64,
    /// Saturated groundwater elevation.
    pub gw_elev: f64,
    /// Soil moisture (dimensionless).
    pub soil_moisture: f64,
    /// Washoff concentration per constituent.
    pub washoff: Vec<f64>,
}

impl ParcelReplay {
    /// This record in the engine's own units.
    ///
    /// The file holds the writing model's user units, and which unit each
    /// field carries differs by quantity: rainfall and infiltration are
    /// depth per *hour* where evaporation is depth per *day*, and the
    /// depth itself is inches or millimetres by the model's system. These
    /// invert `out_writer`'s conversions exactly, since the predecessor
    /// writes one result vector to both files.
    pub fn to_si(&self, us: bool, flow_cv: f64) -> ParcelReplay {
        let depth = if us { 0.0254 } else { 1.0e-3 };
        let length = if us { 0.3048 } else { 1.0 };
        ParcelReplay {
            rainfall: self.rainfall * depth / 3600.0,
            snow_depth: self.snow_depth * depth,
            evap: self.evap * depth / 86_400.0,
            infil: self.infil * depth / 3600.0,
            runoff: self.runoff * flow_cv,
            gw_flow: self.gw_flow * flow_cv,
            gw_elev: self.gw_elev * length,
            soil_moisture: self.soil_moisture,
            washoff: self.washoff.clone(),
        }
    }

    /// This record in the writing model's user units, the inverse of
    /// [`ParcelReplay::to_si`].
    ///
    /// A file is written in the units of the model that wrote it, so this
    /// is what a writer applies on the way out. The two directions are
    /// stated separately rather than one deriving from the other: a single
    /// shared table would round-trip perfectly while being wrong in both
    /// directions, and the units are pinned against the predecessor's own
    /// conversion factors in the tests.
    pub fn from_si(&self, us: bool, flow_cv: f64) -> ParcelReplay {
        let depth = if us { 0.0254 } else { 1.0e-3 };
        let length = if us { 0.3048 } else { 1.0 };
        ParcelReplay {
            rainfall: self.rainfall * 3600.0 / depth,
            snow_depth: self.snow_depth / depth,
            evap: self.evap * 86_400.0 / depth,
            infil: self.infil * 3600.0 / depth,
            runoff: self.runoff / flow_cv,
            gw_flow: self.gw_flow / flow_cv,
            gw_elev: self.gw_elev / length,
            soil_moisture: self.soil_moisture,
            washoff: self.washoff.clone(),
        }
    }
}

/// A parsed runoff interface file, checked against a model.
#[derive(Debug)]
pub struct RunoffInterface {
    /// Steps: `(step length s, per-parcel results in model order)`.
    pub steps: Vec<(f64, Vec<ParcelReplay>)>,
}

/// Parse a runoff interface file against the model (§14.8.2).
///
/// The format holds no names: parcels are positional, and the parcel
/// count, constituent count and flow unit agreeing is the whole of the
/// check available. The caller reports that the match was positional.
pub fn parse_runoff_file(bytes: &[u8], net: &Network) -> Result<RunoffInterface, String> {
    const STAMP: &[u8] = b"SWMM5-RUNOFF";
    if !bytes.starts_with(STAMP) {
        return Err("not a SWMM5 runoff interface file".into());
    }
    let body = &bytes[STAMP.len()..];
    let i32_at = |o: usize| -> Result<i32, String> {
        body.get(o..o + 4)
            .and_then(|s| s.try_into().ok())
            .map(i32::from_le_bytes)
            .ok_or_else(|| "truncated runoff interface file".to_string())
    };
    let parcels = i32_at(0)?;
    let constituents = i32_at(4)?;
    let units = i32_at(8)?;
    let steps_declared = i32_at(12)?;
    if parcels as usize != net.parcels.len() {
        return Err(format!(
            "runoff interface file carries {parcels} parcels and this model has {}",
            net.parcels.len()
        ));
    }
    if constituents as usize != net.constituents.len() {
        return Err(format!(
            "runoff interface file carries {constituents} constituents and this \
             model has {}",
            net.constituents.len()
        ));
    }
    let want = FILE_FLOW_UNITS
        .iter()
        .position(|w| *w == flow_unit_word(net.options.flow_units))
        .unwrap_or(usize::MAX) as i32;
    if units != want {
        let named = |i: i32| {
            usize::try_from(i)
                .ok()
                .and_then(|i| FILE_FLOW_UNITS.get(i))
                .copied()
                .unwrap_or("an unknown unit")
        };
        // The unit word also fixes the unit system, so this is the check
        // that stops a US file being read into an SI model.
        return Err(format!(
            "runoff interface file is in {} and this model is in {}",
            named(units),
            named(want)
        ));
    }
    if steps_declared <= 0 {
        return Err(format!(
            "runoff interface file declares {steps_declared} steps"
        ));
    }
    let np = net.parcels.len();
    let nc = net.constituents.len();
    let per_parcel = 8 + nc;
    let record = 4 + np * per_parcel * 4;
    let mut steps = Vec::new();
    let mut o = 16;
    while o + record <= body.len() {
        let f32_at = |b: usize| -> f64 {
            f64::from(f32::from_le_bytes(
                body[b..b + 4].try_into().unwrap_or([0; 4]),
            ))
        };
        let dt = f32_at(o);
        let mut row = Vec::with_capacity(np);
        for pi in 0..np {
            let base = o + 4 + pi * per_parcel * 4;
            row.push(ParcelReplay {
                rainfall: f32_at(base),
                snow_depth: f32_at(base + 4),
                evap: f32_at(base + 8),
                infil: f32_at(base + 12),
                runoff: f32_at(base + 16),
                gw_flow: f32_at(base + 20),
                gw_elev: f32_at(base + 24),
                soil_moisture: f32_at(base + 28),
                washoff: (0..nc).map(|c| f32_at(base + 32 + c * 4)).collect(),
            });
        }
        steps.push((dt, row));
        o += record;
    }
    Ok(RunoffInterface { steps })
}

/// Write a runoff interface file (§14.8.2).
///
/// `records` are `(step length in seconds, one row per parcel in model
/// order)` in engine units, one record per hydrology step in the order the
/// run produced them. The header's step count is taken from what is
/// actually here, so a run that stopped early still describes itself.
pub fn write_runoff_file(
    net: &Network,
    records: &[(f64, Vec<ParcelReplay>)],
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    let us = net.options.flow_units.is_us();
    let cv = flow_cv_of(net.options.flow_units);
    let nc = net.constituents.len();
    let unit = FILE_FLOW_UNITS
        .iter()
        .position(|u| *u == flow_unit_word(net.options.flow_units))
        .unwrap_or(0) as i32;
    w.write_all(b"SWMM5-RUNOFF")?;
    for v in [
        net.parcels.len() as i32,
        nc as i32,
        unit,
        records.len() as i32,
    ] {
        w.write_all(&v.to_le_bytes())?;
    }
    for (dt, rows) in records {
        w.write_all(&(*dt as f32).to_le_bytes())?;
        for row in rows {
            let u = row.from_si(us, cv);
            for x in [
                u.rainfall,
                u.snow_depth,
                u.evap,
                u.infil,
                u.runoff,
                u.gw_flow,
                u.gw_elev,
                u.soil_moisture,
            ] {
                w.write_all(&(x as f32).to_le_bytes())?;
            }
            // Every declared constituent gets a slot whether the row
            // carries one or not: the record length is fixed by the
            // header, and a short row would misalign every parcel after it.
            for ci in 0..nc {
                let c = u.washoff.get(ci).copied().unwrap_or(0.0);
                w.write_all(&(c as f32).to_le_bytes())?;
            }
        }
    }
    Ok(())
}

/// The predecessor's word for a model's flow unit.
fn flow_unit_word(units: crate::io::options::FlowUnits) -> &'static str {
    use crate::io::options::FlowUnits::*;
    match units {
        Cfs => "CFS",
        Gpm => "GPM",
        Mgd => "MGD",
        Cms => "CMS",
        Lps => "LPS",
        Mld => "MLD",
    }
}

#[cfg(test)]
mod rdii_tests {
    use super::*;
    use crate::io::objects::parse_network;

    /// A model with two RDII vertices, so a file can name them in either
    /// order and the difference is visible.
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS  CMS

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  TS1

[JUNCTIONS]
J1  100  4
J2  99   4

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  J2  400  0.013  0  0
C2  J2  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0
C2  CIRCULAR  1.5  0  0  0

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT  0.5  1.0  2.0

[RDII]
J1  UH1  12.5
J2  UH1  12.5

[TIMESERIES]
TS1  0:00  1.0
TS1  1:00  0.0
";

    fn model() -> Network {
        let (net, diags) = parse_network(MODEL);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        net
    }

    fn epoch_of(y: i32, m: u32, d: u32) -> f64 {
        crate::simulation::time::days_from_civil(crate::io::options::Date {
            year: y,
            month: m,
            day: d,
        }) as f64
            * 86_400.0
    }

    /// The binary form dates records as the predecessor's decimal day. The
    /// origin is 25569 days before the Unix epoch, and a wrong constant
    /// does not fail — it silently moves every hydrograph by decades.
    #[test]
    fn the_binary_date_origin_matches_the_predecessors() {
        let net = model();
        // 1970-01-01 is day 25569 in the predecessor's encoding.
        let bytes = binary(&[0], 3600, &[(25_569.0, &[1.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0).expect("parse");
        assert_eq!(epoch_of(1970, 1, 1), f.records[0].0);
        // And a real date well away from the origin.
        let bytes = binary(&[0], 3600, &[(45_000.0, &[1.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0).expect("parse");
        assert_eq!(epoch_of(2023, 3, 15), f.records[0].0);
    }

    /// Build a binary file: stamp, step, count, positions, then records.
    fn binary(positions: &[i32], step: i32, records: &[(f64, &[f32])]) -> Vec<u8> {
        let mut b = b"SWMM5-RDII".to_vec();
        b.extend_from_slice(&step.to_le_bytes());
        b.extend_from_slice(&(positions.len() as i32).to_le_bytes());
        for p in positions {
            b.extend_from_slice(&p.to_le_bytes());
        }
        for (date, flows) in records {
            b.extend_from_slice(&date.to_le_bytes());
            for q in *flows {
                b.extend_from_slice(&q.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn a_binary_file_reads_its_vertices_and_flows() {
        let net = model();
        let bytes = binary(&[0, 1], 3600, &[(25_569.0, &[2.0, 3.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0).expect("parse");
        assert_eq!(3600.0, f.step);
        assert_eq!(vec![0, 1], f.vertices);
        assert_eq!(vec![2.0, 3.0], f.records[0].1);
    }

    #[test]
    fn a_binary_file_converts_from_the_models_units() {
        let net = model();
        // Read as if the writing model used L/s: 1000 L/s is 1 m³/s.
        let bytes = binary(&[0], 3600, &[(25_569.0, &[1000.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0e-3).expect("parse");
        assert!(
            (f.records[0].1[0] - 1.0).abs() < 1e-9,
            "{:?}",
            f.records[0].1
        );
    }

    /// The format stores positions in the writing model's vertex array, so
    /// a file written against a different model can name a position this
    /// one does not have. The predecessor would index past its own array.
    #[test]
    fn a_binary_position_outside_the_model_is_refused_by_name() {
        let net = model();
        let bytes = binary(&[99], 3600, &[(25_569.0, &[1.0])]);
        let err = parse_rdii_file(&bytes, &net, 1.0).unwrap_err();
        assert!(err.contains("99"), "{err}");
        assert!(err.contains("vertex order"), "{err}");
    }

    /// The one check the format does allow, which the predecessor makes
    /// too: the named position must have an RDII assignment. Position 2 is
    /// the outfall.
    #[test]
    fn a_binary_position_without_rdii_is_refused() {
        let net = model();
        let bytes = binary(&[2], 3600, &[(25_569.0, &[1.0])]);
        let err = parse_rdii_file(&bytes, &net, 1.0).unwrap_err();
        assert!(err.contains("no RDII assignment"), "{err}");
    }

    #[test]
    fn a_non_positive_binary_step_or_count_is_refused() {
        let net = model();
        assert!(parse_rdii_file(&binary(&[0], 0, &[]), &net, 1.0).is_err());
        let mut b = b"SWMM5-RDII".to_vec();
        b.extend_from_slice(&3600i32.to_le_bytes());
        b.extend_from_slice(&(-1i32).to_le_bytes());
        assert!(parse_rdii_file(&b, &net, 1.0).is_err());
    }

    fn text(rows: &str) -> String {
        format!(
            "SWMM5\nA title\n3600\n1\nFLOW CMS\n2\nJ1\nJ2\nNode Year Mon Day Hr Min Sec Flow\n{rows}"
        )
    }

    #[test]
    fn a_text_file_reads_its_vertices_flows_and_units() {
        let net = model();
        let f = parse_rdii_file(
            text("J1 1970 1 1 0 0 0 2.0\nJ2 1970 1 1 0 0 0 3.0\n").as_bytes(),
            &net,
            1.0,
        )
        .expect("parse");
        assert_eq!(3600.0, f.step);
        assert_eq!(vec![0, 1], f.vertices);
        assert_eq!(epoch_of(1970, 1, 1), f.records[0].0);
        assert_eq!(vec![2.0, 3.0], f.records[0].1);
    }

    #[test]
    fn a_text_file_converts_from_its_declared_units() {
        let net = model();
        let body =
            text("J1 1970 1 1 0 0 0 1000\nJ2 1970 1 1 0 0 0 0\n").replace("FLOW CMS", "FLOW LPS");
        let f = parse_rdii_file(body.as_bytes(), &net, 1.0).expect("parse");
        assert!(
            (f.records[0].1[0] - 1.0).abs() < 1e-9,
            "{:?}",
            f.records[0].1
        );
    }

    /// The predecessor reads the row's vertex name into a variable its own
    /// comment marks "not used" and matches by position, so a file whose
    /// rows are ordered differently from its header is misassigned there
    /// without complaint. Here the name on the row decides.
    #[test]
    fn text_rows_are_matched_by_the_name_they_carry_not_their_order() {
        let net = model();
        let f = parse_rdii_file(
            text("J2 1970 1 1 0 0 0 3.0\nJ1 1970 1 1 0 0 0 2.0\n").as_bytes(),
            &net,
            1.0,
        )
        .expect("parse");
        // Column order follows the header, so J1's 2.0 stays in column 0
        // however the rows were ordered.
        assert_eq!(vec![2.0, 3.0], f.records[0].1);
    }

    #[test]
    fn a_text_row_naming_an_unlisted_vertex_is_refused() {
        let net = model();
        let err =
            parse_rdii_file(text("O1 1970 1 1 0 0 0 1.0\n").as_bytes(), &net, 1.0).unwrap_err();
        assert!(err.contains("O1"), "{err}");
    }

    #[test]
    fn a_text_header_naming_an_unknown_vertex_is_refused() {
        let net = model();
        let body = text("").replace("J2\n", "NOPE\n");
        let err = parse_rdii_file(body.as_bytes(), &net, 1.0).unwrap_err();
        assert!(err.contains("NOPE"), "{err}");
    }

    #[test]
    fn the_encoding_is_chosen_by_the_stamp() {
        let net = model();
        assert!(parse_rdii_file(b"SWMM5-RDIInot really", &net, 1.0).is_err());
        // Text that does not open with SWMM5 is refused as text, not read
        // as binary.
        let err = parse_rdii_file(b"HELLO\n", &net, 1.0).unwrap_err();
        assert!(err.contains("not a SWMM5"), "{err}");
    }

    /// A record's flows hold until the next record, and the hydrograph is
    /// zero outside the file's own span. Routing interface files
    /// interpolate; these do not.
    #[test]
    fn flows_are_piecewise_constant_and_zero_outside_the_span() {
        let net = model();
        let t0 = 25_569.0;
        let bytes = binary(&[0], 3600, &[(t0, &[1.0]), (t0 + 1.0 / 24.0, &[2.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0).expect("parse");
        let start = epoch_of(1970, 1, 1);
        assert!(
            f.inflows_at(start - 1.0).is_empty(),
            "before the first record"
        );
        assert_eq!(vec![(0, 1.0)], f.inflows_at(start));
        // Held, not ramped, right up to the next record.
        assert_eq!(vec![(0, 1.0)], f.inflows_at(start + 3599.0));
        assert_eq!(vec![(0, 2.0)], f.inflows_at(start + 3600.0));
        assert!(
            f.inflows_at(start + 7200.0).is_empty(),
            "after the last record's own step"
        );
    }

    #[test]
    fn a_gap_between_records_carries_no_flow() {
        let net = model();
        let t0 = 25_569.0;
        // Two records an hour apart in a file whose step is a minute.
        let bytes = binary(&[0], 60, &[(t0, &[1.0]), (t0 + 1.0 / 24.0, &[2.0])]);
        let f = parse_rdii_file(&bytes, &net, 1.0).expect("parse");
        let start = epoch_of(1970, 1, 1);
        assert_eq!(vec![(0, 1.0)], f.inflows_at(start));
        assert!(f.inflows_at(start + 600.0).is_empty(), "inside the gap");
        assert_eq!(vec![(0, 2.0)], f.inflows_at(start + 3600.0));
    }
}

#[cfg(test)]
mod runoff_iface_tests {
    use super::*;
    use crate::io::objects::parse_network;

    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS  CMS

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  50  500  0.01  0
S2  G1  J1  5   50  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET
S2  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0
S2  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
TS1  0:00  1.0
TS1  1:00  0.0
";

    fn model() -> Network {
        let (net, diags) = parse_network(MODEL);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        net
    }

    /// `parcels`, `constituents`, `units` and `steps` as the header holds
    /// them, then `steps` records of `dt` and 8+c floats per parcel.
    fn file(parcels: i32, constituents: i32, units: i32, steps: &[(f32, Vec<f32>)]) -> Vec<u8> {
        let mut b = b"SWMM5-RUNOFF".to_vec();
        for v in [parcels, constituents, units, steps.len().max(1) as i32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for (dt, row) in steps {
            b.extend_from_slice(&dt.to_le_bytes());
            for x in row {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        b
    }

    /// Eight results per parcel plus one per constituent; this model has
    /// two parcels and no constituents.
    fn row(runoff_a: f32, runoff_b: f32) -> Vec<f32> {
        let mut v = vec![0.0; 16];
        v[4] = runoff_a;
        v[12] = runoff_b;
        v
    }

    #[test]
    fn a_runoff_file_reads_its_steps_and_parcels() {
        let net = model();
        let bytes = file(2, 0, 3, &[(300.0, row(1.5, 2.5)), (300.0, row(1.0, 2.0))]);
        let f = parse_runoff_file(&bytes, &net).expect("parse");
        assert_eq!(2, f.steps.len());
        assert_eq!(300.0, f.steps[0].0);
        assert_eq!(1.5, f.steps[0].1[0].runoff);
        assert_eq!(2.5, f.steps[0].1[1].runoff);
        assert_eq!(1.0, f.steps[1].1[0].runoff);
    }

    #[test]
    fn the_stamp_is_required() {
        let net = model();
        let err = parse_runoff_file(b"SWMM5-RDII\0\0\0\0", &net).unwrap_err();
        assert!(err.contains("not a SWMM5 runoff"), "{err}");
    }

    /// The counts agreeing is the whole of the identity check the format
    /// allows, so each half of it has to actually be made.
    #[test]
    fn a_parcel_count_that_differs_is_refused() {
        let net = model();
        let err = parse_runoff_file(&file(3, 0, 3, &[]), &net).unwrap_err();
        assert!(err.contains("3 parcels"), "{err}");
        assert!(err.contains("this model has 2"), "{err}");
    }

    #[test]
    fn a_constituent_count_that_differs_is_refused() {
        let net = model();
        let err = parse_runoff_file(&file(2, 1, 3, &[]), &net).unwrap_err();
        assert!(err.contains("1 constituents"), "{err}");
    }

    /// The unit word fixes the unit *system* too, so this is what stops a
    /// US file being read into an SI model.
    #[test]
    fn a_file_in_another_unit_system_is_refused_by_name() {
        let net = model();
        // 0 is CFS; the model is CMS.
        let err = parse_runoff_file(&file(2, 0, 0, &[]), &net).unwrap_err();
        assert!(err.contains("CFS"), "{err}");
        assert!(err.contains("CMS"), "{err}");
    }

    #[test]
    fn a_non_positive_step_count_is_refused() {
        let net = model();
        let mut b = b"SWMM5-RUNOFF".to_vec();
        for v in [2i32, 0, 3, 0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let err = parse_runoff_file(&b, &net).unwrap_err();
        assert!(err.contains("0 steps"), "{err}");
    }

    #[test]
    fn a_truncated_record_is_dropped_rather_than_read_short() {
        let net = model();
        let mut bytes = file(2, 0, 3, &[(300.0, row(1.5, 2.5))]);
        bytes.truncate(bytes.len() - 8);
        let f = parse_runoff_file(&bytes, &net).expect("the header still parses");
        assert!(f.steps.is_empty(), "a partial record must not be served");
    }

    #[test]
    fn every_field_lands_in_its_own_slot() {
        let net = model();
        // Distinct values so a transposed field shows up.
        let mut r = vec![0.0f32; 16];
        for (i, v) in r.iter_mut().enumerate().take(8) {
            *v = (i + 1) as f32;
        }
        let f = parse_runoff_file(&file(2, 0, 3, &[(60.0, r)]), &net).expect("parse");
        let p = &f.steps[0].1[0];
        assert_eq!(1.0, p.rainfall);
        assert_eq!(2.0, p.snow_depth);
        assert_eq!(3.0, p.evap);
        assert_eq!(4.0, p.infil);
        assert_eq!(5.0, p.runoff);
        assert_eq!(6.0, p.gw_flow);
        assert_eq!(7.0, p.gw_elev);
        assert_eq!(8.0, p.soil_moisture);
    }

    /// The nine conversions, each against the predecessor's own factor
    /// rather than against `to_si`. A shared table would let one mistake
    /// round-trip unnoticed in both directions, so these numbers are
    /// computed from `Ucf` by hand: rainfall 43 200 (in/hr), evaporation
    /// 1 036 800 (in/day), rain depth 12 (in), length 1 (ft), all as
    /// ft-per-second or ft, and CFS for flow.
    #[test]
    fn the_written_units_are_the_predecessors() {
        // One inch per hour is 1/43200 ft/s, and a foot is 0.3048 m.
        let in_per_hour = 0.3048 / 43_200.0;
        let in_per_day = 0.3048 / 1_036_800.0;
        let inch = 0.3048 / 12.0;
        let cfs = 0.3048_f64.powi(3);
        let si = ParcelReplay {
            rainfall: in_per_hour,
            snow_depth: inch,
            evap: in_per_day,
            infil: in_per_hour,
            runoff: cfs,
            gw_flow: 2.0 * cfs,
            gw_elev: 0.3048,
            soil_moisture: 0.31,
            washoff: vec![4.0],
        };
        let u = si.from_si(true, cfs);
        let close = |a: f64, b: f64, what: &str| {
            assert!((a - b).abs() < 1e-9, "{what}: {a} not {b}");
        };
        close(u.rainfall, 1.0, "rainfall is inches per hour");
        close(u.snow_depth, 1.0, "snow depth is inches");
        close(u.evap, 1.0, "evaporation is inches per DAY, not per hour");
        close(u.infil, 1.0, "infiltration is inches per hour");
        close(u.runoff, 1.0, "runoff is the model's flow unit");
        close(u.gw_flow, 2.0, "groundwater flow is the model's flow unit");
        close(u.gw_elev, 1.0, "water-table elevation is feet");
        close(u.soil_moisture, 0.31, "soil moisture is dimensionless");
        assert_eq!(vec![4.0], u.washoff, "washoff is a concentration");
    }

    /// Metric models carry the same distinction: millimetres, and per day
    /// for evaporation alone.
    #[test]
    fn the_written_units_are_the_predecessors_in_metric() {
        let si = ParcelReplay {
            rainfall: 1.0e-3 / 3600.0,
            snow_depth: 1.0e-3,
            evap: 1.0e-3 / 86_400.0,
            infil: 1.0e-3 / 3600.0,
            runoff: 1.0,
            gw_flow: 1.0,
            gw_elev: 1.0,
            soil_moisture: 0.2,
            washoff: vec![],
        };
        let u = si.from_si(false, 1.0);
        for (got, what) in [
            (u.rainfall, "rainfall mm/hr"),
            (u.snow_depth, "snow depth mm"),
            (u.evap, "evaporation mm/day"),
            (u.infil, "infiltration mm/hr"),
            (u.runoff, "runoff CMS"),
            (u.gw_elev, "water table m"),
        ] {
            assert!((got - 1.0).abs() < 1e-9, "{what}: {got}");
        }
    }

    #[test]
    fn a_written_file_reads_back_as_what_was_written() {
        let net = model();
        let cv = flow_cv_of(net.options.flow_units);
        let us = net.options.flow_units.is_us();
        let mk = |runoff: f64| ParcelReplay {
            rainfall: 1.0e-6,
            snow_depth: 0.02,
            evap: 3.0e-8,
            infil: 2.0e-6,
            runoff,
            gw_flow: 0.01,
            gw_elev: 9.5,
            soil_moisture: 0.28,
            washoff: vec![],
        };
        let records = vec![
            (300.0, vec![mk(0.5), mk(1.5)]),
            (600.0, vec![mk(0.25), mk(0.75)]),
        ];
        let mut bytes = Vec::new();
        write_runoff_file(&net, &records, &mut bytes).expect("write");
        let back = parse_runoff_file(&bytes, &net).expect("read back");
        assert_eq!(2, back.steps.len(), "step count");
        for (i, (dt, rows)) in back.steps.iter().enumerate() {
            assert_eq!(records[i].0, *dt, "step {i} length");
            for (j, row) in rows.iter().enumerate() {
                let si = row.to_si(us, cv);
                let want = &records[i].1[j];
                for (got, want, what) in [
                    (si.rainfall, want.rainfall, "rainfall"),
                    (si.snow_depth, want.snow_depth, "snow depth"),
                    (si.evap, want.evap, "evaporation"),
                    (si.infil, want.infil, "infiltration"),
                    (si.runoff, want.runoff, "runoff"),
                    (si.gw_flow, want.gw_flow, "groundwater flow"),
                    (si.gw_elev, want.gw_elev, "water-table elevation"),
                    (si.soil_moisture, want.soil_moisture, "soil moisture"),
                ] {
                    // Single precision on the way through the file.
                    assert!(
                        (got - want).abs() <= want.abs() * 1e-6,
                        "step {i} parcel {j} {what}: {got} not {want}"
                    );
                }
            }
        }
    }

    /// The header is the predecessor's, byte for byte.
    #[test]
    fn a_written_file_has_the_predecessors_header() {
        let net = model();
        let mut bytes = Vec::new();
        write_runoff_file(&net, &[(60.0, vec![])], &mut bytes).expect("write");
        assert_eq!(b"SWMM5-RUNOFF", &bytes[..12], "stamp");
        let word = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(2, word(12), "parcel count");
        assert_eq!(0, word(16), "constituent count");
        // CMS is the fourth of the six, and the enumeration is the format.
        assert_eq!(3, word(20), "flow unit");
        assert_eq!(1, word(24), "step count is what the run produced");
    }

    /// A file the reference implementation itself wrote, read by this
    /// engine. The literal assertions above encode what this engine
    /// believes the format to be; only this one encodes what the format
    /// actually is.
    #[test]
    fn reads_a_file_the_predecessor_wrote() {
        let dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/uds");
        let text = std::fs::read_to_string(dir.join("runoff_interface.inp")).expect("model");
        let (net, diags) = parse_network(&text);
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        let bytes = std::fs::read(dir.join("runoff_interface.rff")).expect("reference file");
        let f = parse_runoff_file(&bytes, &net).expect("parse the reference file");
        // Three hours at a fifteen-minute wet step.
        assert_eq!(12, f.steps.len(), "step count");
        assert!(f.steps.iter().all(|(dt, _)| *dt == 900.0), "step lengths");
        assert!(f.steps.iter().all(|(_, r)| r.len() == 1), "one parcel each");
        // The reference run's fifth record, in the model's own units.
        let p = &f.steps[4].1[0];
        assert!((p.runoff - 2.260_708).abs() < 1e-4, "runoff {}", p.runoff);
        assert!((p.infil - 0.75).abs() < 1e-6, "infiltration {}", p.infil);
        // 2.26 cfs is 0.064 m³/s, and 0.75 in/hr is 5.29e-6 m/s.
        let cv = flow_cv_of(net.options.flow_units);
        let si = p.to_si(net.options.flow_units.is_us(), cv);
        assert!((si.runoff - 0.064_016).abs() < 1e-5, "runoff {}", si.runoff);
        assert!(
            (si.infil - 5.291_67e-6).abs() < 1e-10,
            "infiltration {}",
            si.infil
        );
    }
}

// ── Rainfall interface files (§14.8.3) ───────────────────────────────────────

/// Bytes of the rainfall format's file stamp.
const RAIN_STAMP: &[u8] = b"SWMM5-RAIN";
/// Bytes the station identifier occupies in a gage header.
const RAIN_STATION_FIELD: usize = 1025;
/// Bytes of one gage header: the station, then three 32-bit values.
const RAIN_GAGE_HEADER: usize = RAIN_STATION_FIELD + 12;
/// Bytes of one reading: a decimal day and a depth.
const RAIN_READING: usize = 12;

/// One station's cached record (§14.8.3).
#[derive(Debug, Clone, PartialEq)]
pub struct RainGageRecord {
    /// The recording station's identifier, which is what a gage is
    /// matched by. Not the gage's own name.
    pub station: String,
    /// The gage's recording interval (s).
    pub interval: f64,
    /// Readings as `(decimal day, depth in inches over the interval)`.
    /// Zero depths are present and mean a dry interval, not a gap.
    pub readings: Vec<(f64, f64)>,
}

/// A parsed rainfall interface file.
#[derive(Debug, Clone, PartialEq)]
pub struct RainInterface {
    /// The stations the file caches, in the order it holds them.
    pub gages: Vec<RainGageRecord>,
}

impl RainInterface {
    /// The record for a station, compared without case as every other
    /// identifier in this engine is.
    pub fn station(&self, station: &str) -> Option<&RainGageRecord> {
        self.gages
            .iter()
            .find(|g| g.station.eq_ignore_ascii_case(station))
    }
}

/// Parse a rainfall interface file (§14.8.3).
///
/// The file carries its own identity, so unlike the runoff format it is
/// read without a model: a gage finds its station afterwards.
pub fn parse_rain_iface(bytes: &[u8]) -> Result<RainInterface, String> {
    if !bytes.starts_with(RAIN_STAMP) {
        return Err("not a SWMM5 rainfall interface file".into());
    }
    let i32_at = |o: usize| -> Result<i32, String> {
        bytes
            .get(o..o + 4)
            .and_then(|s| s.try_into().ok())
            .map(i32::from_le_bytes)
            .ok_or_else(|| "truncated rainfall interface file".to_string())
    };
    let count = i32_at(RAIN_STAMP.len())?;
    if count < 0 {
        return Err(format!("rainfall interface file declares {count} gages"));
    }
    let count = count as usize;
    // The declared count sizes the header, so a file claiming more gages
    // than it has room for is refused before anything is allocated.
    let header_end = RAIN_STAMP.len() + 4 + count * RAIN_GAGE_HEADER;
    if header_end > bytes.len() {
        return Err(format!(
            "rainfall interface file declares {count} gages, which needs \
             {header_end} bytes of header and the file holds {}",
            bytes.len()
        ));
    }
    let mut gages = Vec::with_capacity(count);
    for g in 0..count {
        let o = RAIN_STAMP.len() + 4 + g * RAIN_GAGE_HEADER;
        let field = &bytes[o..o + RAIN_STATION_FIELD];
        let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
        let station = String::from_utf8_lossy(&field[..end]).trim().to_string();
        let interval = i32_at(o + RAIN_STATION_FIELD)?;
        let start = i32_at(o + RAIN_STATION_FIELD + 4)?;
        let stop = i32_at(o + RAIN_STATION_FIELD + 8)?;
        // The offsets are absolute byte positions into this file, so every
        // one of them is a way to read somewhere it should not.
        let (start, stop) = match (usize::try_from(start), usize::try_from(stop)) {
            (Ok(a), Ok(b)) if a <= b && b <= bytes.len() && a >= header_end => (a, b),
            _ => {
                return Err(format!(
                    "rainfall interface file: station {station:?} claims bytes \
                     {start}..{stop} of a {}-byte file",
                    bytes.len()
                ))
            }
        };
        if (stop - start) % RAIN_READING != 0 {
            return Err(format!(
                "rainfall interface file: station {station:?} claims {} bytes, \
                 which is not a whole number of readings",
                stop - start
            ));
        }
        let readings = (start..stop)
            .step_by(RAIN_READING)
            .map(|q| {
                let day = f64::from_le_bytes(bytes[q..q + 8].try_into().unwrap_or([0; 8]));
                let depth = f32::from_le_bytes(bytes[q + 8..q + 12].try_into().unwrap_or([0; 4]));
                (day, f64::from(depth))
            })
            .collect();
        gages.push(RainGageRecord {
            station,
            interval: f64::from(interval),
            readings,
        });
    }
    Ok(RainInterface { gages })
}

/// Write a rainfall interface file (§14.8.3).
///
/// Assembled whole before anything is written, so each gage's byte range
/// is known rather than patched in a second pass.
pub fn write_rain_iface(
    gages: &[RainGageRecord],
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    let header_end = RAIN_STAMP.len() + 4 + gages.len() * RAIN_GAGE_HEADER;
    let total = header_end
        + gages
            .iter()
            .map(|g| g.readings.len() * RAIN_READING)
            .sum::<usize>();
    // The offsets are signed 32-bit, so the format cannot describe a file
    // this large. Refusing beats writing offsets that have wrapped.
    if i32::try_from(total).is_err() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "a rainfall interface file of {total} bytes cannot be \
                 addressed by the format's 32-bit offsets (§14.8.3)"
            ),
        ));
    }
    w.write_all(RAIN_STAMP)?;
    w.write_all(&(gages.len() as i32).to_le_bytes())?;
    let mut at = header_end;
    for g in gages {
        let mut field = [0u8; RAIN_STATION_FIELD];
        let id = g.station.as_bytes();
        let n = id.len().min(RAIN_STATION_FIELD - 1);
        field[..n].copy_from_slice(&id[..n]);
        w.write_all(&field)?;
        let stop = at + g.readings.len() * RAIN_READING;
        for v in [g.interval as i32, at as i32, stop as i32] {
            w.write_all(&v.to_le_bytes())?;
        }
        at = stop;
    }
    for g in gages {
        for (day, depth) in &g.readings {
            w.write_all(&day.to_le_bytes())?;
            w.write_all(&(*depth as f32).to_le_bytes())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod rain_iface_tests {
    use super::*;

    fn record(station: &str, readings: &[(f64, f64)]) -> RainGageRecord {
        RainGageRecord {
            station: station.to_string(),
            interval: 900.0,
            readings: readings.to_vec(),
        }
    }

    /// A file the reference implementation itself wrote. The assertions
    /// below this one encode what this engine believes the format to be;
    /// only this one encodes what it is.
    #[test]
    fn reads_a_rain_file_the_predecessor_wrote() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/uds/rainfall_interface.rain");
        let bytes = std::fs::read(&path).expect("reference file");
        let f = parse_rain_iface(&bytes).expect("parse the reference file");
        assert_eq!(2, f.gages.len(), "gage count");
        assert_eq!("STA01", f.gages[0].station);
        assert_eq!("STAB", f.gages[1].station);
        assert!(f.gages.iter().all(|g| g.interval == 900.0), "interval");
        // The zero at 00:30 is in the file: a dry interval is recorded,
        // not omitted, whatever the predecessor's own description says.
        let a: Vec<f64> = f.gages[0].readings.iter().map(|(_, v)| *v).collect();
        assert_eq!(vec![0.10, 0.25, 0.0, 0.05], round4(&a), "STA01 depths");
        let b: Vec<f64> = f.gages[1].readings.iter().map(|(_, v)| *v).collect();
        assert_eq!(vec![0.40, 0.20, 0.30], round4(&b), "STAB depths");
        // 2020-01-01 is day 43831 of the predecessor's calendar, and the
        // readings are a quarter of an hour apart.
        assert_eq!(43_831.0, f.gages[0].readings[0].0, "first instant");
        let step = f.gages[0].readings[1].0 - f.gages[0].readings[0].0;
        assert!((step - 900.0 / 86_400.0).abs() < 1e-9, "spacing {step}");
        // Both stations start at the same instant, so a reader that had
        // run the two blocks together would show STAB continuing STA01.
        assert_eq!(f.gages[0].readings[0].0, f.gages[1].readings[0].0);
    }

    fn round4(v: &[f64]) -> Vec<f64> {
        v.iter().map(|x| (x * 1e4).round() / 1e4).collect()
    }

    /// The layout, against the reference file's own byte positions.
    #[test]
    fn the_header_is_the_predecessors() {
        let gages = [
            record("STA01", &[(43_831.0, 0.1), (43_831.010_416_667, 0.25)]),
            record("STAB", &[(43_831.0, 0.4)]),
        ];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        assert_eq!(b"SWMM5-RAIN", &b[..10], "the stamp is ten bytes, no more");
        let word = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        assert_eq!(2, word(10), "gage count follows the stamp");
        // 14 + 2 x 1037.
        assert_eq!(2088, 14 + 2 * 1037, "the gage header is 1037 bytes");
        assert_eq!(900, word(14 + 1025), "interval");
        assert_eq!(
            2088,
            word(14 + 1025 + 4),
            "first gage starts after the header"
        );
        assert_eq!(2088 + 24, word(14 + 1025 + 8), "and ends one past its last");
        assert_eq!(
            2088 + 24,
            word(14 + 1037 + 1025 + 4),
            "the second follows it"
        );
        assert_eq!(
            2088 + 36,
            word(14 + 1037 + 1025 + 8),
            "to the end of the file"
        );
        assert_eq!(2088 + 36, b.len(), "nothing after the readings");
        // The identifier is padded with zero bytes, not spaces.
        assert_eq!(b"STA01", &b[14..19]);
        assert!(b[19..14 + 1025].iter().all(|c| *c == 0), "padding");
    }

    #[test]
    fn a_written_file_reads_back_as_what_was_written() {
        let gages = vec![
            record("A", &[(43_831.0, 0.1), (43_831.25, 0.0), (43_831.5, 2.5)]),
            record("B", &[]),
            record("C", &[(43_900.125, 0.75)]),
        ];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        let back = parse_rain_iface(&b).expect("read back");
        assert_eq!(gages.len(), back.gages.len(), "gage count");
        for (want, got) in gages.iter().zip(&back.gages) {
            assert_eq!(want.station, got.station, "station");
            assert_eq!(want.interval, got.interval, "interval");
            assert_eq!(
                want.readings.len(),
                got.readings.len(),
                "{}: reading count",
                want.station
            );
            for (a, b) in want.readings.iter().zip(&got.readings) {
                // The instant is a 64-bit double and survives exactly. The
                // depth is a 32-bit float, so 0.1 comes back as 0.100000001
                // and only its precision can be asserted.
                assert_eq!(a.0, b.0, "{}: instant", want.station);
                assert!(
                    (a.1 - b.1).abs() <= a.1.abs() * 1e-7,
                    "{}: depth {} came back {}",
                    want.station,
                    a.1,
                    b.1
                );
            }
        }
    }

    #[test]
    fn a_station_is_found_without_case() {
        let gages = [record("Sta7", &[(43_831.0, 0.1)])];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        let f = parse_rain_iface(&b).expect("parse");
        assert!(f.station("STA7").is_some(), "STA7 must find Sta7");
        assert!(f.station("sta7").is_some(), "sta7 must find Sta7");
        assert!(f.station("sta8").is_none(), "sta8 must find nothing");
    }

    #[test]
    fn the_stamp_is_required() {
        let err = parse_rain_iface(b"SWMM5-RUNOFF\0\0\0\0").unwrap_err();
        assert!(err.contains("not a SWMM5 rainfall"), "{err}");
    }

    /// A declared count is a length claim about a file that may not hold
    /// it, and is checked before anything is sized from it.
    #[test]
    fn a_gage_count_larger_than_the_file_is_refused() {
        let mut b = RAIN_STAMP.to_vec();
        b.extend_from_slice(&5000i32.to_le_bytes());
        let err = parse_rain_iface(&b).unwrap_err();
        assert!(err.contains("declares 5000 gages"), "{err}");
    }

    /// The offsets address the file, so each is a way to read outside it.
    #[test]
    fn an_offset_outside_the_file_is_refused() {
        let gages = [record("A", &[(43_831.0, 0.1)])];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        let mut past = b.clone();
        past[14 + 1025 + 8..14 + 1025 + 12].copy_from_slice(&99_999i32.to_le_bytes());
        let err = parse_rain_iface(&past).unwrap_err();
        assert!(err.contains("claims bytes"), "{err}");
        // An offset inside the header would let a gage read the station
        // names as if they were readings.
        let mut into_header = b.clone();
        into_header[14 + 1025 + 4..14 + 1025 + 8].copy_from_slice(&20i32.to_le_bytes());
        let err = parse_rain_iface(&into_header).unwrap_err();
        assert!(err.contains("claims bytes"), "{err}");
        // A backwards range is not an empty one.
        let mut backwards = b.clone();
        backwards[14 + 1025 + 4..14 + 1025 + 8].copy_from_slice(&(b.len() as i32).to_le_bytes());
        backwards[14 + 1025 + 8..14 + 1025 + 12].copy_from_slice(&2088i32.to_le_bytes());
        let err = parse_rain_iface(&backwards).unwrap_err();
        assert!(err.contains("claims bytes"), "{err}");
    }

    #[test]
    fn a_partial_reading_is_refused() {
        let gages = [record("A", &[(43_831.0, 0.1)])];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        b.truncate(b.len() - 4);
        let end = b.len() as i32;
        b[14 + 1025 + 8..14 + 1025 + 12].copy_from_slice(&end.to_le_bytes());
        let err = parse_rain_iface(&b).unwrap_err();
        assert!(err.contains("whole number of readings"), "{err}");
    }

    /// An identifier longer than the field is truncated to fit rather
    /// than running into the interval that follows it.
    #[test]
    fn a_long_station_name_stays_inside_its_field() {
        let long = "S".repeat(4000);
        let gages = [record(&long, &[(43_831.0, 0.1)])];
        let mut b = Vec::new();
        write_rain_iface(&gages, &mut b).expect("write");
        assert_eq!(14 + 1037 + 12, b.len(), "the field is a fixed width");
        let f = parse_rain_iface(&b).expect("parse");
        assert_eq!(1024, f.gages[0].station.len(), "truncated, and terminated");
        assert_eq!(900.0, f.gages[0].interval, "the interval is still readable");
    }
}
