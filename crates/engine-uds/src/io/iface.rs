//! Routing interface files (§14.8): text files carrying dated flow and
//! concentration series at outlet vertices — inflow files are read-only
//! boundary inflows, outflow files are written from outlet vertices, one
//! file never serving both roles in a run. Values interpolate between
//! bracketing periods, unmatched pollutants read as zero, and flows
//! convert from the *file's* declared units.

use std::io::{self, Write};

use crate::model::Network;
use crate::simulation::engine::Snapshot;

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
        // Bracketing records; ends hold nothing (§14.8: boundary series).
        let i = match self.records.iter().position(|(t, _)| *t > epoch) {
            Some(0) => return out,
            Some(i) => i,
            None => return out,
        };
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
            let q = ((1.0 - f) * a[0] + f * b[0]) * self.flow_cv;
            let mut conc = vec![0.0; np];
            for (fc, m) in self.constituents.iter().enumerate() {
                if let Some(p) = m {
                    conc[*p] = (1.0 - f) * a[fc + 1] + f * b[fc + 1];
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
        let secs: f64 = t[4].parse::<f64>().map_err(|_| "bad hour")? * 3600.0
            + t[5].parse::<f64>().map_err(|_| "bad minute")? * 60.0
            + t[6].parse::<f64>().map_err(|_| "bad second")?;
        let epoch = crate::simulation::time::days_from_civil(date) as f64 * 86_400.0 + secs;
        let values: Vec<f64> = t[7..]
            .iter()
            .map(|s| s.parse::<f64>().unwrap_or(0.0))
            .collect();
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
