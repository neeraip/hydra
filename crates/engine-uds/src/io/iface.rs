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
}
