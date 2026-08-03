//! Binary results reader (§14.9): the writer's other half, and the one
//! filesystem carve-out inside this engine.
//!
//! Results files can dwarf the model that produced them, so the reader
//! operates on an explicitly supplied path and seeks — metadata, one
//! period, or one element's series — rather than requiring the whole file
//! in memory. Opening validates before serving: both magic numbers, the
//! version, the epilog's section positions against the actual file length,
//! and the stored error code. Values are served as stored, in the file's
//! declared unit system; the metadata carries everything a consumer needs
//! to interpret them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::options::FlowUnits;

const MAGIC: i32 = 516_114_522;
const VERSION: i32 = 52_004;
/// Days between the predecessor's epoch (1899-12-30) and the civil epoch
/// (1970-01-01).
const EPOCH_OFFSET_DAYS: f64 = 25_569.0;
/// Identifier-length sanity bound: no real model carries kilobyte ids, and
/// the cap turns a corrupt length prefix into a refusal instead of an
/// attempted gigabyte allocation.
const MAX_ID_LEN: i32 = 1024;

/// Which record family an element's values live in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Subcatchment,
    Node,
    Link,
}

/// Everything needed to locate and interpret the file's records, read once
/// at open (§14.9).
#[derive(Debug, Clone)]
pub struct OutMetadata {
    /// The file's declared flow-unit system; all stored values are in the
    /// user units this implies.
    pub flow_units: FlowUnits,
    /// Reported subcatchment identifiers, in record order.
    pub subcatchment_ids: Vec<String>,
    /// Reported node identifiers, in record order.
    pub node_ids: Vec<String>,
    /// Reported link identifiers, in record order.
    pub link_ids: Vec<String>,
    /// Pollutant identifiers, in stored order.
    pub pollutant_ids: Vec<String>,
    /// Per-pollutant concentration-unit codes (0 mg/L, 1 µg/L, 2 counts/L).
    pub pollutant_units: Vec<i32>,
    /// True instant of the first period record (Unix seconds) — the
    /// header's backdated start undone (§14.9).
    pub start_epoch_s: f64,
    /// Reporting interval (s).
    pub report_step_s: i32,
    /// Number of period records.
    pub n_periods: usize,
    /// Result variables per subcatchment (8 + pollutants).
    pub n_subcatch_vars: usize,
    /// Result variables per node (6 + pollutants).
    pub n_node_vars: usize,
    /// Result variables per link (5 + pollutants).
    pub n_link_vars: usize,
    /// Byte offset of the first period record.
    output_start: u64,
    /// Bytes per period record.
    record_bytes: u64,
}

impl OutMetadata {
    /// True instant of period `p` (Unix seconds).
    pub fn period_epoch_s(&self, p: usize) -> f64 {
        self.start_epoch_s + p as f64 * self.report_step_s as f64
    }

    /// System-series variables per period (fixed by the format).
    pub const N_SYSTEM_VARS: usize = 15;
}

/// One period's stored values, exactly as written (§14.9): element-major —
/// each element's variables are consecutive.
#[derive(Debug, Clone)]
pub struct PeriodRecord {
    /// The record's own timestamp (Unix seconds).
    pub epoch_s: f64,
    /// `n_subcatchments × n_subcatch_vars`, element-major.
    pub subcatchments: Vec<f32>,
    /// `n_nodes × n_node_vars`, element-major.
    pub nodes: Vec<f32>,
    /// `n_links × n_link_vars`, element-major.
    pub links: Vec<f32>,
    /// The fifteen system series.
    pub system: [f32; OutMetadata::N_SYSTEM_VARS],
}

/// One element's full time series: `vars[v][p]` is variable `v` at period
/// `p`, in the element kind's variable order.
#[derive(Debug, Clone)]
pub struct ElementSeries {
    /// Record timestamps (Unix seconds), one per period.
    pub epochs_s: Vec<f64>,
    /// Variable-major series values.
    pub vars: Vec<Vec<f32>>,
}

fn read_i32(f: &mut File) -> Result<i32, String> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(i32::from_le_bytes(b))
}

fn read_f64(f: &mut File) -> Result<f64, String> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(f64::from_le_bytes(b))
}

fn read_f32_vec(f: &mut File, n: usize) -> Result<Vec<f32>, String> {
    let mut bytes = vec![0u8; n * 4];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

fn read_id(f: &mut File) -> Result<String, String> {
    let len = read_i32(f)?;
    if !(0..=MAX_ID_LEN).contains(&len) {
        return Err(format!("implausible identifier length {len}"));
    }
    let mut bytes = vec![0u8; len as usize];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "identifier is not valid UTF-8".to_string())
}

/// Open and validate a results file, returning its metadata (§14.9).
///
/// Refuses — with a message naming what failed — a file whose magic
/// numbers, version, or error code are wrong, or whose epilog geometry
/// does not tile the actual file length.
pub fn read_metadata(path: &Path) -> Result<OutMetadata, String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    if len < 24 + 28 {
        return Err(format!(
            "file is too short ({len} bytes) to be a results file"
        ));
    }

    // ── Epilog: six ints, located from the end ──────────────────────────
    f.seek(SeekFrom::End(-24)).map_err(|e| e.to_string())?;
    let id_start = read_i32(&mut f)?;
    let input_start = read_i32(&mut f)?;
    let output_start = read_i32(&mut f)?;
    let n_periods = read_i32(&mut f)?;
    let error_code = read_i32(&mut f)?;
    let tail_magic = read_i32(&mut f)?;
    if tail_magic != MAGIC {
        return Err(format!(
            "trailing magic number is {tail_magic}, not {MAGIC}"
        ));
    }
    if error_code != 0 {
        return Err(format!(
            "the run that wrote this file recorded error code {error_code}"
        ));
    }
    if n_periods < 0
        || id_start < 28
        || input_start < id_start
        || output_start < input_start
        || output_start as u64 > len
    {
        return Err("epilog section positions do not fit the file".to_string());
    }

    // ── Header ──────────────────────────────────────────────────────────
    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let magic = read_i32(&mut f)?;
    if magic != MAGIC {
        return Err(format!("leading magic number is {magic}, not {MAGIC}"));
    }
    let version = read_i32(&mut f)?;
    if version != VERSION {
        return Err(format!(
            "results version {version} is not the supported {VERSION}"
        ));
    }
    let flow_units_code = read_i32(&mut f)?;
    let flow_units = FlowUnits::from_code(flow_units_code)
        .ok_or_else(|| format!("unknown flow-units code {flow_units_code}"))?;
    let n_subcatch = read_count(&mut f, "subcatchment")?;
    let n_nodes = read_count(&mut f, "node")?;
    let n_links = read_count(&mut f, "link")?;
    let n_pollut = read_count(&mut f, "pollutant")?;

    // ── Identifier tables ───────────────────────────────────────────────
    let subcatchment_ids = read_ids(&mut f, n_subcatch)?;
    let node_ids = read_ids(&mut f, n_nodes)?;
    let link_ids = read_ids(&mut f, n_links)?;
    let pollutant_ids = read_ids(&mut f, n_pollut)?;
    let mut pollutant_units = Vec::with_capacity(n_pollut);
    for _ in 0..n_pollut {
        pollutant_units.push(read_i32(&mut f)?);
    }

    // ── Reporting clock: fixed distance back from the records ───────────
    let n_subcatch_vars = 8 + n_pollut;
    let n_node_vars = 6 + n_pollut;
    let n_link_vars = 5 + n_pollut;
    f.seek(SeekFrom::Start(output_start as u64 - 12))
        .map_err(|e| e.to_string())?;
    let stored_start_days = read_f64(&mut f)?;
    let report_step_s = read_i32(&mut f)?;
    if report_step_s <= 0 {
        return Err(format!("implausible report step {report_step_s} s"));
    }

    // ── Geometry: records must tile the file exactly ────────────────────
    let record_bytes = 8 + 4
        * (n_subcatch * n_subcatch_vars
            + n_nodes * n_node_vars
            + n_links * n_link_vars
            + OutMetadata::N_SYSTEM_VARS) as u64;
    let expected = output_start as u64 + n_periods as u64 * record_bytes + 24;
    if expected != len {
        return Err(format!(
            "file length {len} does not match {n_periods} period records of \
             {record_bytes} bytes (expected {expected})"
        ));
    }

    // The header start is backdated one period (§14.9): undo it so served
    // times are true record instants.
    let start_epoch_s = (stored_start_days - EPOCH_OFFSET_DAYS) * 86_400.0 + report_step_s as f64;

    Ok(OutMetadata {
        flow_units,
        subcatchment_ids,
        node_ids,
        link_ids,
        pollutant_ids,
        pollutant_units,
        start_epoch_s,
        report_step_s,
        n_periods: n_periods as usize,
        n_subcatch_vars,
        n_node_vars,
        n_link_vars,
        output_start: output_start as u64,
        record_bytes,
    })
}

fn read_count(f: &mut File, what: &str) -> Result<usize, String> {
    let n = read_i32(f)?;
    if !(0..=10_000_000).contains(&n) {
        return Err(format!("implausible {what} count {n}"));
    }
    Ok(n as usize)
}

fn read_ids(f: &mut File, n: usize) -> Result<Vec<String>, String> {
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        ids.push(read_id(f)?);
    }
    Ok(ids)
}

/// Read one period record (§14.9). `period` is 0-based.
pub fn read_period(path: &Path, meta: &OutMetadata, period: usize) -> Result<PeriodRecord, String> {
    if period >= meta.n_periods {
        return Err(format!(
            "period {period} is out of range (file has {})",
            meta.n_periods
        ));
    }
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.seek(SeekFrom::Start(
        meta.output_start + period as u64 * meta.record_bytes,
    ))
    .map_err(|e| e.to_string())?;
    let date_days = read_f64(&mut f)?;
    let subcatchments = read_f32_vec(&mut f, meta.subcatchment_ids.len() * meta.n_subcatch_vars)?;
    let nodes = read_f32_vec(&mut f, meta.node_ids.len() * meta.n_node_vars)?;
    let links = read_f32_vec(&mut f, meta.link_ids.len() * meta.n_link_vars)?;
    let sys = read_f32_vec(&mut f, OutMetadata::N_SYSTEM_VARS)?;
    let mut system = [0f32; OutMetadata::N_SYSTEM_VARS];
    system.copy_from_slice(&sys);
    Ok(PeriodRecord {
        epoch_s: (date_days - EPOCH_OFFSET_DAYS) * 86_400.0,
        subcatchments,
        nodes,
        links,
        system,
    })
}

/// Read one element's full time series with one seek per period (§14.9).
/// `index` addresses the metadata's id list for `kind`.
pub fn read_element_series(
    path: &Path,
    meta: &OutMetadata,
    kind: ElementKind,
    index: usize,
) -> Result<ElementSeries, String> {
    let n_sub = meta.subcatchment_ids.len();
    let n_nodes = meta.node_ids.len();
    let n_links = meta.link_ids.len();
    let (count, n_vars, base) = match kind {
        ElementKind::Subcatchment => (n_sub, meta.n_subcatch_vars, 0),
        ElementKind::Node => (n_nodes, meta.n_node_vars, n_sub * meta.n_subcatch_vars),
        ElementKind::Link => (
            n_links,
            meta.n_link_vars,
            n_sub * meta.n_subcatch_vars + n_nodes * meta.n_node_vars,
        ),
    };
    if index >= count {
        return Err(format!("element index {index} is out of range ({count})"));
    }
    let offset_in_record = 8 + 4 * (base + index * n_vars) as u64;

    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut epochs_s = Vec::with_capacity(meta.n_periods);
    let mut vars: Vec<Vec<f32>> = vec![Vec::with_capacity(meta.n_periods); n_vars];
    for p in 0..meta.n_periods {
        let record = meta.output_start + p as u64 * meta.record_bytes;
        f.seek(SeekFrom::Start(record)).map_err(|e| e.to_string())?;
        let date_days = read_f64(&mut f)?;
        epochs_s.push((date_days - EPOCH_OFFSET_DAYS) * 86_400.0);
        f.seek(SeekFrom::Start(record + offset_in_record))
            .map_err(|e| e.to_string())?;
        let values = read_f32_vec(&mut f, n_vars)?;
        for (v, value) in values.into_iter().enumerate() {
            vars[v].push(value);
        }
    }
    Ok(ElementSeries { epochs_s, vars })
}
