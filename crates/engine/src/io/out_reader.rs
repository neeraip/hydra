//! out_reader — EPANET binary output file reader (`crates/cli/spec.md` §4.1).
//!
//! Parses an EPANET-compatible `.out` binary file (produced by either EPANET
//! 2.3 or Hydra) and returns a fully structured `OutFile`.  The parsed form
//! is used by integration tests and is the foundation for `hydra-analysis`.

// ── Public types ──────────────────────────────────────────────────────────────

/// The 15 INT4 integer header fields from the prolog, plus the per-object
/// static arrays (tank areas, elevations, lengths, diameters).
#[derive(Debug, Clone)]
pub struct OutProlog {
    /// EPANET magic number (must be 516114521).
    pub magic: i32,
    /// EPANET file format version (must be 200).
    pub version: i32,
    /// Number of nodes (junctions + reservoirs + tanks).
    pub n_nodes: usize,
    /// Number of tank and reservoir nodes.
    pub n_tanks: usize,
    /// Number of links (pipes + pumps + valves).
    pub n_links: usize,
    /// Number of pumps.
    pub n_pumps: usize,
    /// Number of valves.
    pub n_valves: usize,
    /// Quality mode flag: 0=None, 1=Chemical, 2=Age, 3=Trace.
    pub quality_flag: i32,
    /// 1-based node index used as trace source (meaningful only when `quality_flag == 3`).
    pub trace_node: i32,
    /// Flow unit code (see EPANET spec table §4.1).
    pub flow_units: i32,
    /// Pressure unit code (0=psi, 1=m, 2=kPa).
    pub pressure_units: i32,
    /// Report-statistic code (0=Series, 1=Average, 2=Minimum, 3=Maximum, 4=Range).
    pub report_statistic: i32,
    /// Reporting start time (s).
    pub report_start: i32,
    /// Reporting step duration (s).
    pub report_step: i32,
    /// Total simulation duration (s).
    pub duration: i32,
    /// Cross-section areas for tanks/reservoirs in the file's internal length units
    /// (ft² for US-customary files, m² for SI files).  Length = `n_tanks`.
    pub tank_areas: Vec<f32>,
    /// Node elevations in output length units.  Length = `n_nodes`.
    pub elevations: Vec<f32>,
    /// Link lengths in output length units (0 for pumps/valves).  Length = `n_links`.
    pub lengths: Vec<f32>,
    /// Link diameters in output diameter units (0 for pumps).  Length = `n_links`.
    pub diameters: Vec<f32>,
}

/// One pump-energy record from the energy section (28 bytes).
#[derive(Debug, Clone)]
pub struct PumpEnergyRecord {
    /// 1-based link index of the pump.
    pub link_index: i32,
    /// Percentage of simulation time the pump was online (0–100).
    pub pct_online: f32,
    /// Average efficiency (%).
    pub avg_efficiency: f32,
    /// Average kWh per unit of flow.
    pub avg_kwh_per_flow: f32,
    /// Average power (kW).
    pub avg_kw: f32,
    /// Peak power (kW).
    pub peak_kw: f32,
    /// Average daily cost.
    pub avg_cost_per_day: f32,
}

/// The energy section: one record per pump plus the trailing demand charge.
#[derive(Debug, Clone)]
pub struct OutEnergy {
    /// Per-pump energy records.  Length = `n_pumps`.
    pub pumps: Vec<PumpEnergyRecord>,
    /// Demand charge (trailing REAL4 after all pump records).
    pub demand_charge: f32,
}

/// All node and link variable values for one reporting period.
#[derive(Debug, Clone)]
pub struct PeriodResult {
    // Node variables (each Vec has length `n_nodes`)
    /// Actual delivered demand at each node (flow units from prolog header).
    pub node_demand: Vec<f32>,
    /// Hydraulic head at each node (length units from prolog header).
    pub node_head: Vec<f32>,
    /// Gauge pressure at each node (pressure units from prolog header).
    pub node_pressure: Vec<f32>,
    /// Water quality value at each node (mg/L, h, or % depending on mode).
    pub node_quality: Vec<f32>,
    // Link variables (each Vec has length `n_links`)
    /// Volumetric flow rate through each link (flow units; positive = from→to).
    pub link_flow: Vec<f32>,
    /// Mean velocity through each link (velocity units).
    pub link_velocity: Vec<f32>,
    /// Head loss across each link (length units; positive = from head > to head).
    pub link_headloss: Vec<f32>,
    /// Water quality value in each link.
    pub link_quality: Vec<f32>,
    /// Link status flag (0 = closed/inactive, 1 = open/active).
    pub link_status: Vec<f32>,
    /// Link setting (pump speed ratio or valve setpoint).
    pub link_setting: Vec<f32>,
    /// Bulk reaction rate in each link (mass/time).
    pub link_reaction_rate: Vec<f32>,
    /// Darcy-Weisbach friction factor for each link (dimensionless).
    pub link_friction_factor: Vec<f32>,
}

/// The network-reactions section: four aggregate rates (mass/hr).
#[derive(Debug, Clone, Copy)]
pub struct OutReactions {
    /// Bulk reaction rate summed across all pipes (mass/hr).
    pub bulk_rate: f32,
    /// Wall reaction rate summed across all pipes (mass/hr).
    pub wall_rate: f32,
    /// Tank reaction rate summed across all tanks (mass/hr).
    pub tank_rate: f32,
    /// Mass injected by all quality sources (mass/hr).
    pub source_rate: f32,
}

/// The epilog section: three INT4 values.
#[derive(Debug, Clone, Copy)]
pub struct OutEpilog {
    /// Number of reporting periods actually written.
    pub n_periods: i32,
    /// Non-zero if the solver issued warnings during the run.
    pub warning_flag: i32,
    /// Magic number used to validate file integrity (must equal the prolog magic).
    pub magic: i32,
}

/// A fully parsed EPANET binary output file.
#[derive(Debug)]
pub struct OutFile {
    /// Prolog header: counts, options, and static per-object arrays.
    pub prolog: OutProlog,
    /// Energy section: one record per pump plus the trailing demand charge.
    pub energy: OutEnergy,
    /// One entry per reporting period.
    pub periods: Vec<PeriodResult>,
    /// Network-level aggregate reaction rates.
    pub reactions: OutReactions,
    /// Epilog: period count, warning flag, and integrity magic number.
    pub epilog: OutEpilog,
}

// ── Streaming API ─────────────────────────────────────────────────────────────
//
// Lightweight accessors that read only the bytes they need, enabling the GUI
// (and any other consumer) to work with `.out` files without loading the entire
// file into memory.

/// Lightweight metadata extracted from the `.out` prolog header (first 60 bytes)
/// and epilog (last 12 bytes).  Total I/O: 72 bytes regardless of file size.
#[derive(Debug, Clone)]
pub struct OutMetadata {
    /// Number of nodes (junctions + reservoirs + tanks) in the network.
    pub n_nodes: usize,
    /// Number of tank/reservoir nodes in the network.
    pub n_tanks: usize,
    /// Number of links (pipes + pumps + valves) in the network.
    pub n_links: usize,
    /// Number of pumps in the network.
    pub n_pumps: usize,
    /// EPANET-compatible quality mode flag from the prolog header.
    /// 0=None, 1=Chemical, 2=Age, 3=Trace.
    pub quality_flag: i32,
    /// Simulation time at which reporting starts (seconds).
    pub report_start: f64,
    /// Duration of each reporting period (seconds).
    pub report_step: f64,
    /// Number of reporting periods written to the file.
    pub n_periods: usize,
}

/// Category for invalid or unreadable `.out` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutValidityKind {
    /// The file is missing.
    Missing,
    /// The file exists but could not be read due to an I/O error.
    Io,
    /// The file is truncated and does not contain all required bytes.
    Incomplete,
    /// The file bytes are malformed or internally inconsistent.
    Corrupt,
    /// The file appears structurally valid but uses unsupported values/version.
    Unsupported,
}

/// Structured validation error for `.out` reads.
#[derive(Debug, Clone)]
pub struct OutValidityError {
    /// Category of the validity failure.
    pub kind: OutValidityKind,
    /// Human-readable description of the specific problem.
    pub detail: String,
}

impl std::fmt::Display for OutValidityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = match self.kind {
            OutValidityKind::Missing => "missing",
            OutValidityKind::Io => "io",
            OutValidityKind::Incomplete => "incomplete",
            OutValidityKind::Corrupt => "corrupt",
            OutValidityKind::Unsupported => "unsupported",
        };
        write!(f, "Invalid .out ({tag}): {}", self.detail)
    }
}

impl std::error::Error for OutValidityError {}

impl OutMetadata {
    /// Byte size of the prolog section.
    pub fn prolog_bytes(&self) -> u64 {
        (884 + 36 * self.n_nodes + 52 * self.n_links + 8 * self.n_tanks) as u64
    }
    /// Byte size of the energy section.
    pub fn energy_bytes(&self) -> u64 {
        (28 * self.n_pumps + 4) as u64
    }
    /// Byte offset where dynamic (per-period) data begins.
    pub fn dynamic_offset(&self) -> u64 {
        self.prolog_bytes() + self.energy_bytes()
    }
    /// Byte size of one period's data block.
    pub fn period_bytes(&self) -> u64 {
        (4 * (4 * self.n_nodes + 8 * self.n_links)) as u64
    }
    /// Build the snapshot-time vector from prolog header fields.
    pub fn snapshot_times(&self) -> Vec<f64> {
        (0..self.n_periods)
            .map(|i| self.report_start + (i as f64) * self.report_step)
            .collect()
    }
}

/// Read only the 60-byte prolog header and 12-byte epilog from a `.out` file.
///
/// Total I/O is 72 bytes — this never touches the dynamic data section.
pub fn read_metadata(path: &std::path::Path) -> Result<OutMetadata, String> {
    read_metadata_checked(path).map_err(|e| e.to_string())
}

/// Read and validate `.out` metadata with explicit validity classification.
pub fn read_metadata_checked(path: &std::path::Path) -> Result<OutMetadata, OutValidityError> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::NotFound {
            OutValidityKind::Missing
        } else {
            OutValidityKind::Io
        };
        OutValidityError {
            kind,
            detail: format!("failed to open file: {e}"),
        }
    })?;

    let file_len = f
        .metadata()
        .map(|m| m.len())
        .map_err(|e| OutValidityError {
            kind: OutValidityKind::Io,
            detail: format!("failed to read file metadata: {e}"),
        })?;

    if file_len < 72 {
        return Err(OutValidityError {
            kind: OutValidityKind::Incomplete,
            detail: format!("file too short: {file_len} bytes (minimum 72 for header+epilog)"),
        });
    }

    let mut hdr = [0u8; 60];
    if let Err(e) = f.read_exact(&mut hdr) {
        return Err(OutValidityError {
            kind: if e.kind() == std::io::ErrorKind::UnexpectedEof {
                OutValidityKind::Incomplete
            } else {
                OutValidityKind::Io
            },
            detail: format!("failed to read header: {e}"),
        });
    }

    let i32_at = |off: usize| i32::from_le_bytes(hdr[off..off + 4].try_into().unwrap());
    let magic = i32_at(0);
    if magic != 516_114_521 {
        return Err(OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: format!("unexpected start magic: {magic}"),
        });
    }

    let version = i32_at(4);
    if version != 20_012 {
        return Err(OutValidityError {
            kind: OutValidityKind::Unsupported,
            detail: format!("unsupported .out version: {version}"),
        });
    }

    let n_nodes_i = i32_at(8);
    let n_tanks_i = i32_at(12);
    let n_links_i = i32_at(16);
    let n_pumps_i = i32_at(20);
    let n_valves_i = i32_at(24);

    if n_nodes_i < 0 || n_tanks_i < 0 || n_links_i < 0 || n_pumps_i < 0 || n_valves_i < 0 {
        return Err(OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: "negative object counts in header".to_string(),
        });
    }

    let n_nodes = n_nodes_i as usize;
    let n_tanks = n_tanks_i as usize;
    let n_links = n_links_i as usize;
    let n_pumps = n_pumps_i as usize;

    if n_tanks > n_nodes {
        return Err(OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: format!("invalid counts: n_tanks ({n_tanks}) > n_nodes ({n_nodes})"),
        });
    }
    if n_pumps > n_links {
        return Err(OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: format!("invalid counts: n_pumps ({n_pumps}) > n_links ({n_links})"),
        });
    }

    let quality_flag = i32_at(28);
    if !(0..=3).contains(&quality_flag) {
        return Err(OutValidityError {
            kind: OutValidityKind::Unsupported,
            detail: format!("unsupported quality flag: {quality_flag}"),
        });
    }

    let report_start = i32_at(48) as f64;
    let report_step = i32_at(52) as f64;

    if let Err(e) = f.seek(SeekFrom::End(-12)) {
        return Err(OutValidityError {
            kind: OutValidityKind::Io,
            detail: format!("failed to seek epilog: {e}"),
        });
    }
    let mut epi = [0u8; 12];
    if let Err(e) = f.read_exact(&mut epi) {
        return Err(OutValidityError {
            kind: if e.kind() == std::io::ErrorKind::UnexpectedEof {
                OutValidityKind::Incomplete
            } else {
                OutValidityKind::Io
            },
            detail: format!("failed to read epilog: {e}"),
        });
    }

    let n_periods_i = i32::from_le_bytes(epi[0..4].try_into().unwrap());
    if n_periods_i < 0 {
        return Err(OutValidityError {
            kind: OutValidityKind::Incomplete,
            detail: format!("negative period count in epilog: {n_periods_i}"),
        });
    }
    let n_periods = n_periods_i as usize;

    let magic_end = i32::from_le_bytes(epi[8..12].try_into().unwrap());
    if magic_end != 516_114_521 {
        return Err(OutValidityError {
            kind: OutValidityKind::Incomplete,
            detail: format!("unexpected end magic: {magic_end}"),
        });
    }

    let checked_mul = |a: u64, b: u64| {
        a.checked_mul(b).ok_or_else(|| OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: "layout size overflow".to_string(),
        })
    };
    let checked_add = |a: u64, b: u64| {
        a.checked_add(b).ok_or_else(|| OutValidityError {
            kind: OutValidityKind::Corrupt,
            detail: "layout size overflow".to_string(),
        })
    };

    let prolog_bytes = checked_add(
        checked_add(884, checked_mul(36, n_nodes as u64)?)?,
        checked_add(
            checked_mul(52, n_links as u64)?,
            checked_mul(8, n_tanks as u64)?,
        )?,
    )?;
    let energy_bytes = checked_add(checked_mul(28, n_pumps as u64)?, 4)?;
    let period_bytes = checked_mul(
        4,
        checked_add(
            checked_mul(4, n_nodes as u64)?,
            checked_mul(8, n_links as u64)?,
        )?,
    )?;
    let dynamic_bytes = checked_mul(period_bytes, n_periods as u64)?;
    let expected_total = checked_add(
        checked_add(checked_add(prolog_bytes, energy_bytes)?, dynamic_bytes)?,
        28,
    )?;

    if file_len < expected_total {
        return Err(OutValidityError {
            kind: OutValidityKind::Incomplete,
            detail: format!(
                "file truncated: {file_len} bytes, expected at least {expected_total} bytes"
            ),
        });
    }

    Ok(OutMetadata {
        n_nodes,
        n_tanks,
        n_links,
        n_pumps,
        quality_flag,
        report_start,
        report_step,
        n_periods,
    })
}

/// Read the energy section from a `.out` file without loading any period data.
///
/// Seeks directly to `meta.prolog_bytes()` and reads `n_pumps` × 28-byte
/// records plus the 4-byte trailing demand charge.  Total I/O is at most
/// `28 × n_pumps + 4` bytes regardless of file size.
pub fn read_energy(path: &std::path::Path, meta: &OutMetadata) -> Result<OutEnergy, String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(meta.prolog_bytes()))
        .map_err(|e| e.to_string())?;

    let mut pump_records = Vec::with_capacity(meta.n_pumps);
    for _ in 0..meta.n_pumps {
        let mut buf = [0u8; 28];
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
        pump_records.push(PumpEnergyRecord {
            link_index:       i32::from_le_bytes(buf[0..4].try_into().unwrap()),
            pct_online:       f32::from_le_bytes(buf[4..8].try_into().unwrap()),
            avg_efficiency:   f32::from_le_bytes(buf[8..12].try_into().unwrap()),
            avg_kwh_per_flow: f32::from_le_bytes(buf[12..16].try_into().unwrap()),
            avg_kw:           f32::from_le_bytes(buf[16..20].try_into().unwrap()),
            peak_kw:          f32::from_le_bytes(buf[20..24].try_into().unwrap()),
            avg_cost_per_day: f32::from_le_bytes(buf[24..28].try_into().unwrap()),
        });
    }
    let mut charge_buf = [0u8; 4];
    f.read_exact(&mut charge_buf).map_err(|e| e.to_string())?;
    Ok(OutEnergy {
        pumps: pump_records,
        demand_charge: f32::from_le_bytes(charge_buf),
    })
}

/// Read a single period's results from a `.out` file by seeking to the correct
/// offset.  Returns the same [`PeriodResult`] that [`parse`] produces for each
/// period, but without loading the rest of the file.
pub fn read_period(
    path: &std::path::Path,
    meta: &OutMetadata,
    period: usize,
) -> Result<PeriodResult, String> {
    use std::io::{Read, Seek, SeekFrom};

    if period >= meta.n_periods {
        return Err(format!(
            "Period {period} out of range (0..{})",
            meta.n_periods
        ));
    }

    let nn = meta.n_nodes;
    let nl = meta.n_links;
    let pbytes = meta.period_bytes() as usize;
    let mut buf = vec![0u8; pbytes];

    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("Invalid .out (io): failed to open file: {e}"))?;
    let offset = meta.dynamic_offset() + (period as u64) * meta.period_bytes();
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Invalid .out (io): failed to seek to period {period}: {e}"))?;
    f.read_exact(&mut buf).map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
            "incomplete"
        } else {
            "io"
        };
        format!("Invalid .out ({kind}): failed to read period {period}: {e}")
    })?;

    let f32_slice = |start: usize, count: usize| -> Vec<f32> {
        (0..count)
            .map(|i| {
                let off = (start + i) * 4;
                f32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
            })
            .collect()
    };

    // Node block: demand[nn] | head[nn] | pressure[nn] | quality[nn]
    let node_demand = f32_slice(0, nn);
    let node_head = f32_slice(nn, nn);
    let node_pressure = f32_slice(2 * nn, nn);
    let node_quality = f32_slice(3 * nn, nn);

    // Link block: flow[nl] | velocity[nl] | headloss[nl] | quality[nl] |
    //             status[nl] | setting[nl] | reaction_rate[nl] | friction_factor[nl]
    let lb = 4 * nn;
    let link_flow = f32_slice(lb, nl);
    let link_velocity = f32_slice(lb + nl, nl);
    let link_headloss = f32_slice(lb + 2 * nl, nl);
    let link_quality = f32_slice(lb + 3 * nl, nl);
    let link_status = f32_slice(lb + 4 * nl, nl);
    let link_setting = f32_slice(lb + 5 * nl, nl);
    let link_reaction_rate = f32_slice(lb + 6 * nl, nl);
    let link_friction_factor = f32_slice(lb + 7 * nl, nl);

    Ok(PeriodResult {
        node_demand,
        node_head,
        node_pressure,
        node_quality,
        link_flow,
        link_velocity,
        link_headloss,
        link_quality,
        link_status,
        link_setting,
        link_reaction_rate,
        link_friction_factor,
    })
}

/// Global min/max ranges across sampled periods for common result variables.
///
/// All values are in the units stored in the `.out` file (which match the
/// user-declared unit system in the INP `[OPTIONS]` section).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResultRanges {
    /// Minimum nodal gauge pressure across all sampled periods.
    pub pressure_min: f64,
    /// Maximum nodal gauge pressure across all sampled periods.
    pub pressure_max: f64,
    /// Minimum nodal hydraulic head across all sampled periods.
    pub head_min: f64,
    /// Maximum nodal hydraulic head across all sampled periods.
    pub head_max: f64,
    /// Minimum nodal demand across all sampled periods.
    pub demand_min: f64,
    /// Maximum nodal demand across all sampled periods.
    pub demand_max: f64,
    /// Minimum link flow rate across all sampled periods.
    pub flow_min: f64,
    /// Maximum link flow rate across all sampled periods.
    pub flow_max: f64,
    /// Minimum link velocity across all sampled periods.
    pub velocity_min: f64,
    /// Maximum link velocity across all sampled periods.
    pub velocity_max: f64,
    /// Global min/max quality value across all periods and nodes.
    /// `None` when the file was written with `quality_flag == 0` (no quality run).
    pub quality_min: Option<f64>,
    /// Global maximum quality value across all periods and nodes.
    /// `None` when `quality_flag == 0`.
    pub quality_max: Option<f64>,
}

impl Default for ResultRanges {
    fn default() -> Self {
        Self {
            pressure_min: f64::INFINITY,
            pressure_max: f64::NEG_INFINITY,
            head_min: f64::INFINITY,
            head_max: f64::NEG_INFINITY,
            demand_min: f64::INFINITY,
            demand_max: f64::NEG_INFINITY,
            flow_min: f64::INFINITY,
            flow_max: f64::NEG_INFINITY,
            velocity_min: f64::INFINITY,
            velocity_max: f64::NEG_INFINITY,
            quality_min: None,
            quality_max: None,
        }
    }
}

impl ResultRanges {
    /// Replace infinities with sensible defaults and ensure max > min.
    pub fn sanitise(&mut self) {
        fn fix(min: &mut f64, max: &mut f64, default_min: f64, default_max: f64) {
            if !min.is_finite() {
                *min = default_min;
            }
            if !max.is_finite() {
                *max = default_max;
            }
            if (*max - *min).abs() < 1e-9 {
                *max = *min + 1.0;
            }
        }
        fix(&mut self.pressure_min, &mut self.pressure_max, 0.0, 80.0);
        fix(&mut self.head_min, &mut self.head_max, 0.0, 100.0);
        fix(&mut self.demand_min, &mut self.demand_max, 0.0, 10.0);
        fix(&mut self.flow_min, &mut self.flow_max, 0.0, 100.0);
        fix(&mut self.velocity_min, &mut self.velocity_max, 0.0, 5.0);
        if let (Some(qmin), Some(qmax)) = (&mut self.quality_min, &mut self.quality_max) {
            fix(qmin, qmax, 0.0, 1.0);
        }
    }

    /// Update ranges from a single [`PeriodResult`].
    pub fn update_from_period(&mut self, pr: &PeriodResult) {
        for &v in &pr.node_pressure {
            let v = v as f64;
            if v < self.pressure_min {
                self.pressure_min = v;
            }
            if v > self.pressure_max {
                self.pressure_max = v;
            }
        }
        for &v in &pr.node_head {
            let v = v as f64;
            if v < self.head_min {
                self.head_min = v;
            }
            if v > self.head_max {
                self.head_max = v;
            }
        }
        for &v in &pr.node_demand {
            let v = v as f64;
            if v < self.demand_min {
                self.demand_min = v;
            }
            if v > self.demand_max {
                self.demand_max = v;
            }
        }
        for &v in &pr.link_flow {
            let v = v as f64;
            if v < self.flow_min {
                self.flow_min = v;
            }
            if v > self.flow_max {
                self.flow_max = v;
            }
        }
        for &v in &pr.link_velocity {
            let v = v as f64;
            if v < self.velocity_min {
                self.velocity_min = v;
            }
            if v > self.velocity_max {
                self.velocity_max = v;
            }
        }
        // Quality arrays are populated only when quality_flag != 0.  When they
        // are non-empty, fold them into the running quality min/max.
        for &v in pr.node_quality.iter().chain(pr.link_quality.iter()) {
            let v = v as f64;
            match &mut self.quality_min {
                Some(m) => { if v < *m { *m = v; } }
                None    => { self.quality_min = Some(v); }
            }
            match &mut self.quality_max {
                Some(m) => { if v > *m { *m = v; } }
                None    => { self.quality_max = Some(v); }
            }
        }
    }
}

/// Scan up to `max_samples` evenly-spaced periods (always including first and
/// last) from a `.out` file and compute global min/max ranges.
///
/// This reads only the sampled periods via seeking — it never loads the entire
/// file.  With `max_samples = 2048` the scan stays under ~50 ms even for very
/// long simulations.
pub fn scan_ranges(
    path: &std::path::Path,
    meta: &OutMetadata,
    max_samples: usize,
) -> Result<ResultRanges, String> {
    use std::io::{Read, Seek, SeekFrom};

    let nn = meta.n_nodes;
    let nl = meta.n_links;
    let np = meta.n_periods;
    let pbytes = meta.period_bytes() as usize;

    let sample_indices: Vec<usize> = if np <= max_samples {
        (0..np).collect()
    } else {
        (0..max_samples)
            .map(|i| i * (np - 1) / (max_samples - 1))
            .collect()
    };

    let mut ranges = ResultRanges::default();
    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("Invalid .out (io): failed to open file: {e}"))?;
    let mut buf = vec![0u8; pbytes];

    let f32_at = |buf: &[u8], idx: usize| -> f32 {
        let off = idx * 4;
        f32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
    };

    for &p in &sample_indices {
        let offset = meta.dynamic_offset() + (p as u64) * meta.period_bytes();
        f.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Invalid .out (io): failed to seek to period {p}: {e}"))?;
        f.read_exact(&mut buf).map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                "incomplete"
            } else {
                "io"
            };
            format!("Invalid .out ({kind}): failed to read period {p}: {e}")
        })?;

        // Node block: demand[nn] | head[nn] | pressure[nn] | quality[nn]
        for i in 0..nn {
            let d = f32_at(&buf, i) as f64;
            let h = f32_at(&buf, nn + i) as f64;
            let pr = f32_at(&buf, 2 * nn + i) as f64;
            if d < ranges.demand_min {
                ranges.demand_min = d;
            }
            if d > ranges.demand_max {
                ranges.demand_max = d;
            }
            if h < ranges.head_min {
                ranges.head_min = h;
            }
            if h > ranges.head_max {
                ranges.head_max = h;
            }
            if pr < ranges.pressure_min {
                ranges.pressure_min = pr;
            }
            if pr > ranges.pressure_max {
                ranges.pressure_max = pr;
            }
            if meta.quality_flag != 0 {
                let q = f32_at(&buf, 3 * nn + i) as f64;
                match &mut ranges.quality_min {
                    Some(m) => { if q < *m { *m = q; } }
                    None    => { ranges.quality_min = Some(q); }
                }
                match &mut ranges.quality_max {
                    Some(m) => { if q > *m { *m = q; } }
                    None    => { ranges.quality_max = Some(q); }
                }
            }
        }

        // Link block starts after 4*nn node floats.
        // Layout: flow[nl] | velocity[nl] | headloss[nl] | quality[nl] | status[nl]
        let link_base = 4 * nn;
        for i in 0..nl {
            let fv = f32_at(&buf, link_base + i) as f64;
            let vv = f32_at(&buf, link_base + nl + i) as f64;
            if fv < ranges.flow_min {
                ranges.flow_min = fv;
            }
            if fv > ranges.flow_max {
                ranges.flow_max = fv;
            }
            if vv < ranges.velocity_min {
                ranges.velocity_min = vv;
            }
            if vv > ranges.velocity_max {
                ranges.velocity_max = vv;
            }
            if meta.quality_flag != 0 {
                let q = f32_at(&buf, link_base + 3 * nl + i) as f64;
                match &mut ranges.quality_min {
                    Some(m) => { if q < *m { *m = q; } }
                    None    => { ranges.quality_min = Some(q); }
                }
                match &mut ranges.quality_max {
                    Some(m) => { if q > *m { *m = q; } }
                    None    => { ranges.quality_max = Some(q); }
                }
            }
        }
    }

    ranges.sanitise();
    Ok(ranges)
}

// ── Analytics scan ────────────────────────────────────────────────────────────

/// Cross-period statistics accumulated by streaming every period of a `.out` file.
#[derive(Debug)]
pub struct AnalyticsScan {
    /// Per-node minimum pressure across all periods. `f64::INFINITY` when no data.
    pub node_min_pressure: Vec<f64>,
    /// Per-link maximum absolute velocity across all periods.
    pub link_max_velocity: Vec<f64>,
    /// Per-period mass-balance percentage (outflow / inflow × 100, capped at 100).
    pub mb_series: Vec<f64>,
    /// Cumulative demand summed over all nodes and periods where demand is positive
    /// (network inflow), in raw `.out` units.
    pub total_inflow: f64,
    /// Cumulative demand summed over all nodes and periods where demand is negative
    /// (network outflow), stored as a positive value, in raw `.out` units.
    pub total_outflow: f64,
    /// Per-tank head series: `tank_head[ti][p]` = head of tank `ti` at period `p`.
    /// Tank relative index `ti = node_idx − (n_nodes − n_tanks)`.
    pub tank_head: Vec<Vec<f64>>,
}

/// Stream every reporting period and accumulate cross-period node/link statistics.
///
/// Reads one period at a time — never loads more than a single period's data
/// into memory, so it is safe for arbitrarily large result files.
pub fn scan_analytics(
    path: &std::path::Path,
    meta: &OutMetadata,
) -> Result<AnalyticsScan, String> {
    let n_nodes    = meta.n_nodes;
    let n_tanks    = meta.n_tanks;
    let n_links    = meta.n_links;
    let n_periods  = meta.n_periods;
    let tank_start = n_nodes.saturating_sub(n_tanks);

    let mut node_min_pressure: Vec<f64> = vec![f64::INFINITY; n_nodes];
    let mut link_max_velocity: Vec<f64> = vec![0.0_f64; n_links];
    let mut mb_series: Vec<f64>         = vec![0.0_f64; n_periods];
    let mut total_inflow:  f64          = 0.0;
    let mut total_outflow: f64          = 0.0;
    let mut tank_head: Vec<Vec<f64>>    = vec![vec![0.0_f64; n_periods]; n_tanks];

    for p in 0..n_periods {
        let pr = read_period(path, meta, p)?;

        let mut period_inflow  = 0.0_f64;
        let mut period_outflow = 0.0_f64;
        for &d in &pr.node_demand {
            let d = d as f64;
            if d > 0.0 { period_inflow  += d; }
            else        { period_outflow -= d; }
        }
        mb_series[p] = if period_inflow > 0.0 {
            (period_outflow / period_inflow * 100.0).min(100.0)
        } else {
            100.0
        };
        total_inflow  += period_inflow;
        total_outflow += period_outflow;

        for (i, &p_val) in pr.node_pressure.iter().enumerate() {
            let v = p_val as f64;
            if v < node_min_pressure[i] { node_min_pressure[i] = v; }
        }
        for (ti, h_val) in pr.node_head[tank_start..].iter().enumerate() {
            if ti < n_tanks { tank_head[ti][p] = *h_val as f64; }
        }
        for (i, &v_val) in pr.link_velocity.iter().enumerate() {
            let v = (v_val as f64).abs();
            if v > link_max_velocity[i] { link_max_velocity[i] = v; }
        }
    }

    Ok(AnalyticsScan {
        node_min_pressure,
        link_max_velocity,
        mb_series,
        total_inflow,
        total_outflow,
        tank_head,
    })
}

// ── Full-file parser ──────────────────────────────────────────────────────────

/// Parse an EPANET binary output file from a byte slice.
///
/// Returns an error string if the data is too short, the opening or closing
/// magic numbers are wrong, or any read extends beyond the buffer.
pub fn parse(data: &[u8]) -> Result<OutFile, String> {
    if data.len() < 12 {
        return Err(format!("too short: {} bytes (minimum 12)", data.len()));
    }

    // Read n_periods from the epilog (last 12 bytes) before parsing the dynamic
    // section, so the number of periods is known without a seek.
    let epi_off = data.len() - 12;
    let n_periods = i32::from_le_bytes(data[epi_off..epi_off + 4].try_into().unwrap()) as usize;

    let mut cur = Cursor::new(data);

    // ── Prolog (§4.1.1) ───────────────────────────────────────────────────────

    let magic_start = cur.read_i32()?;
    if magic_start != 516_114_521 {
        return Err(format!("unexpected magic at start: {magic_start}"));
    }
    let version = cur.read_i32()?;
    let n_nodes = cur.read_i32()? as usize;
    let n_tanks = cur.read_i32()? as usize;
    let n_links = cur.read_i32()? as usize;
    let n_pumps = cur.read_i32()? as usize;
    let n_valves = cur.read_i32()? as usize;
    let quality_flag = cur.read_i32()?;
    let trace_node = cur.read_i32()?;
    let flow_units = cur.read_i32()?;
    let pressure_units = cur.read_i32()?;
    let report_statistic = cur.read_i32()?;
    let report_start = cur.read_i32()?;
    let report_step = cur.read_i32()?;
    let duration = cur.read_i32()?;

    // String fields: 3×80 title lines + 2×260 filenames + 2×32 chem strings = 824 bytes.
    cur.skip(824)?;

    // Per-object arrays: node IDs (n_nodes×32), link IDs (n_links×32),
    // link from/to/type (3×n_links×INT4), tank node indices (n_tanks×INT4).
    cur.skip(32 * n_nodes + 32 * n_links + 12 * n_links + 4 * n_tanks)?;

    // Tank areas, node elevations, link lengths, link diameters.
    let tank_areas = cur.read_f32s(n_tanks)?;
    let elevations = cur.read_f32s(n_nodes)?;
    let lengths = cur.read_f32s(n_links)?;
    let diameters = cur.read_f32s(n_links)?;

    let prolog = OutProlog {
        magic: magic_start,
        version,
        n_nodes,
        n_tanks,
        n_links,
        n_pumps,
        n_valves,
        quality_flag,
        trace_node,
        flow_units,
        pressure_units,
        report_statistic,
        report_start,
        report_step,
        duration,
        tank_areas,
        elevations,
        lengths,
        diameters,
    };

    // ── Energy (§4.1.2) ───────────────────────────────────────────────────────

    let mut pump_records = Vec::with_capacity(n_pumps);
    for _ in 0..n_pumps {
        let link_index = cur.read_i32()?;
        let pct_online = cur.read_f32()?;
        let avg_efficiency = cur.read_f32()?;
        let avg_kwh_per_flow = cur.read_f32()?;
        let avg_kw = cur.read_f32()?;
        let peak_kw = cur.read_f32()?;
        let avg_cost_per_day = cur.read_f32()?;
        pump_records.push(PumpEnergyRecord {
            link_index,
            pct_online,
            avg_efficiency,
            avg_kwh_per_flow,
            avg_kw,
            peak_kw,
            avg_cost_per_day,
        });
    }
    let demand_charge = cur.read_f32()?;
    let energy = OutEnergy {
        pumps: pump_records,
        demand_charge,
    };

    // ── Dynamic results (§4.1.3) ──────────────────────────────────────────────

    let mut periods = Vec::with_capacity(n_periods);
    for _ in 0..n_periods {
        // Node variables: demand, head, pressure, quality (column-major).
        let node_demand = cur.read_f32s(n_nodes)?;
        let node_head = cur.read_f32s(n_nodes)?;
        let node_pressure = cur.read_f32s(n_nodes)?;
        let node_quality = cur.read_f32s(n_nodes)?;
        // Link variables: flow, velocity, headloss, quality, status, setting,
        // reaction_rate, friction_factor (column-major).
        let link_flow = cur.read_f32s(n_links)?;
        let link_velocity = cur.read_f32s(n_links)?;
        let link_headloss = cur.read_f32s(n_links)?;
        let link_quality = cur.read_f32s(n_links)?;
        let link_status = cur.read_f32s(n_links)?;
        let link_setting = cur.read_f32s(n_links)?;
        let link_reaction_rate = cur.read_f32s(n_links)?;
        let link_friction_factor = cur.read_f32s(n_links)?;
        periods.push(PeriodResult {
            node_demand,
            node_head,
            node_pressure,
            node_quality,
            link_flow,
            link_velocity,
            link_headloss,
            link_quality,
            link_status,
            link_setting,
            link_reaction_rate,
            link_friction_factor,
        });
    }

    // ── Network reactions (§4.1.4) ────────────────────────────────────────────

    let bulk_rate = cur.read_f32()?;
    let wall_rate = cur.read_f32()?;
    let tank_rate = cur.read_f32()?;
    let source_rate = cur.read_f32()?;
    let reactions = OutReactions {
        bulk_rate,
        wall_rate,
        tank_rate,
        source_rate,
    };

    // ── Epilog (§4.1.5) ───────────────────────────────────────────────────────

    let n_periods_check = cur.read_i32()?;
    let warning_flag = cur.read_i32()?;
    let magic_end = cur.read_i32()?;
    if magic_end != 516_114_521 {
        return Err(format!("unexpected magic at end: {magic_end}"));
    }
    let epilog = OutEpilog {
        n_periods: n_periods_check,
        warning_flag,
        magic: magic_end,
    };

    Ok(OutFile {
        prolog,
        energy,
        periods,
        reactions,
        epilog,
    })
}

// ── Internal byte cursor ──────────────────────────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        let end = self.pos + 4;
        if end > self.data.len() {
            return Err(format!("unexpected EOF reading i32 at offset {}", self.pos));
        }
        let v = i32::from_le_bytes(self.data[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        let end = self.pos + 4;
        if end > self.data.len() {
            return Err(format!("unexpected EOF reading f32 at offset {}", self.pos));
        }
        let v = f32::from_le_bytes(self.data[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }

    fn read_f32s(&mut self, n: usize) -> Result<Vec<f32>, String> {
        let end = self.pos + n * 4;
        if end > self.data.len() {
            return Err(format!(
                "unexpected EOF reading {} f32 values at offset {}",
                n, self.pos
            ));
        }
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let off = self.pos + i * 4;
            v.push(f32::from_le_bytes(
                self.data[off..off + 4].try_into().unwrap(),
            ));
        }
        self.pos = end;
        Ok(v)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        let end = self.pos + n;
        if end > self.data.len() {
            return Err(format!(
                "unexpected EOF skipping {} bytes at offset {}",
                n, self.pos
            ));
        }
        self.pos = end;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::out_writer;
    use crate::io::WritableSimulation;
    use std::io::Cursor as StdCursor;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn make_minimal_out(n_nodes: usize, n_tanks: usize, n_links: usize, n_pumps: usize) -> Vec<u8> {
        // prolog_size = 884 + 36*nn + 52*nl + 8*nt
        let prolog = 884 + 36 * n_nodes + 52 * n_links + 8 * n_tanks;
        let energy = 28 * n_pumps + 4;
        let n_periods: usize = 1;
        let period = 4 * (4 * n_nodes + 8 * n_links);
        let reactions: usize = 16;
        let epilog: usize = 12;
        let size = prolog + energy + n_periods * period + reactions + epilog;

        let mut data = vec![0u8; size];

        // Write opening magic
        data[0..4].copy_from_slice(&516_114_521_i32.to_le_bytes());
        // version
        data[4..8].copy_from_slice(&20012_i32.to_le_bytes());
        // n_nodes
        data[8..12].copy_from_slice(&(n_nodes as i32).to_le_bytes());
        // n_tanks
        data[12..16].copy_from_slice(&(n_tanks as i32).to_le_bytes());
        // n_links
        data[16..20].copy_from_slice(&(n_links as i32).to_le_bytes());
        // n_pumps
        data[20..24].copy_from_slice(&(n_pumps as i32).to_le_bytes());

        // epilog: n_periods=1, warning=0, magic
        let epi = size - 12;
        data[epi..epi + 4].copy_from_slice(&(n_periods as i32).to_le_bytes());
        data[epi + 8..epi + 12].copy_from_slice(&516_114_521_i32.to_le_bytes());

        data
    }

    #[test]
    fn parse_rejects_too_short_input() {
        assert!(parse(&[0u8; 4]).is_err());
    }

    #[test]
    fn parse_rejects_wrong_magic() {
        let data = make_minimal_out(2, 1, 1, 0);
        let mut bad = data.clone();
        bad[0..4].copy_from_slice(&0_i32.to_le_bytes());
        assert!(parse(&bad).is_err());
    }

    #[test]
    fn parse_rejects_wrong_end_magic() {
        let mut data = make_minimal_out(2, 1, 1, 0);
        let len = data.len();
        data[len - 4..len].copy_from_slice(&0_i32.to_le_bytes());
        assert!(parse(&data).is_err());
    }

    #[test]
    fn parse_dimensions_are_correct() {
        let data = make_minimal_out(4, 2, 3, 1);
        let out = parse(&data).expect("parse");
        assert_eq!(out.prolog.n_nodes, 4);
        assert_eq!(out.prolog.n_tanks, 2);
        assert_eq!(out.prolog.n_links, 3);
        assert_eq!(out.prolog.n_pumps, 1);
        assert_eq!(out.prolog.elevations.len(), 4);
        assert_eq!(out.prolog.tank_areas.len(), 2);
        assert_eq!(out.prolog.lengths.len(), 3);
        assert_eq!(out.prolog.diameters.len(), 3);
        assert_eq!(out.energy.pumps.len(), 1);
        assert_eq!(out.periods.len(), 1);
        assert_eq!(out.periods[0].node_demand.len(), 4);
        assert_eq!(out.periods[0].link_flow.len(), 3);
    }

    #[test]
    fn parse_roundtrip_vs_writer() {
        use std::path::Path;

        struct MockSession {
            network: crate::Network,
            snapshots: Vec<crate::io::HydSnapshot>,
        }
        impl WritableSimulation for MockSession {
            fn net(&self) -> &crate::Network {
                &self.network
            }
            fn snapshots(&self) -> &[crate::io::HydSnapshot] {
                &self.snapshots
            }
            fn pump_energy_at(&self, _: usize) -> Option<&crate::io::PumpEnergy> {
                None
            }
            fn peak_demand_kw(&self) -> f64 {
                0.0
            }
            fn mass_balance(&self) -> Option<&crate::io::MassBalance> {
                None
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

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/single_pipe_hw.inp");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let network = crate::io::parse(&bytes).expect("parse network");
        let n_nodes = network.nodes.len();
        let n_links = network.links.len();
        let node_states = network
            .nodes
            .iter()
            .map(|n| crate::NodeState {
                head: n.base.elevation,
                ..Default::default()
            })
            .collect();
        let link_states = network
            .links
            .iter()
            .map(|_| crate::LinkState::default())
            .collect();
        let session = MockSession {
            network,
            snapshots: vec![crate::io::HydSnapshot {
                t: 0.0,
                node_states,
                link_states,
            }],
        };

        let mut buf = StdCursor::new(Vec::new());
        out_writer::write_binary_output(&mut buf, &session, "test.inp", "", crate::FlowUnits::Gpm)
            .expect("write");
        let raw = buf.into_inner();
        let out = parse(&raw).expect("parse writer output");

        assert_eq!(out.prolog.n_nodes, n_nodes);
        assert_eq!(out.prolog.n_links, n_links);
        assert_eq!(out.periods.len(), 1);
        assert_eq!(out.periods[0].node_demand.len(), n_nodes);
        assert_eq!(out.periods[0].link_flow.len(), n_links);
    }

    fn write_temp_bytes(data: &[u8]) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let pid = std::process::id();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        path.push(format!("hydra-out-reader-test-{pid}-{nanos}-{seq}.out"));
        std::fs::write(&path, data).expect("write temp .out");
        path
    }

    #[test]
    fn read_metadata_checked_classifies_corrupt_magic() {
        let mut data = make_minimal_out(2, 1, 1, 0);
        data[0..4].copy_from_slice(&0_i32.to_le_bytes());
        let path = write_temp_bytes(&data);
        let err = read_metadata_checked(&path).expect_err("expected corrupt classification");
        let _ = std::fs::remove_file(&path);
        assert_eq!(err.kind, OutValidityKind::Corrupt);
    }

    #[test]
    fn read_metadata_checked_classifies_incomplete_truncation() {
        let data = make_minimal_out(3, 1, 2, 0);
        let truncated_len = data.len().saturating_sub(64);
        let path = write_temp_bytes(&data[..truncated_len]);
        let err = read_metadata_checked(&path).expect_err("expected incomplete classification");
        let _ = std::fs::remove_file(&path);
        assert_eq!(err.kind, OutValidityKind::Incomplete);
    }

    #[test]
    fn read_metadata_checked_classifies_unsupported_version() {
        let mut data = make_minimal_out(2, 1, 1, 0);
        data[4..8].copy_from_slice(&12345_i32.to_le_bytes());
        let path = write_temp_bytes(&data);
        let err = read_metadata_checked(&path).expect_err("expected unsupported classification");
        let _ = std::fs::remove_file(&path);
        assert_eq!(err.kind, OutValidityKind::Unsupported);
    }

    #[test]
    fn read_metadata_missing_file_classified_as_missing() {
        let path = std::path::PathBuf::from("/tmp/hydra_test_this_file_does_not_exist_ever.out");
        let err = read_metadata_checked(&path).expect_err("expected missing classification");
        assert_eq!(err.kind, OutValidityKind::Missing);
    }

    #[test]
    fn read_metadata_checked_succeeds_on_valid_file() {
        let data = make_minimal_out(3, 1, 2, 0);
        let path = write_temp_bytes(&data);
        let meta = read_metadata_checked(&path).expect("valid file should parse");
        let _ = std::fs::remove_file(&path);
        assert_eq!(meta.n_nodes, 3);
        assert_eq!(meta.n_tanks, 1);
        assert_eq!(meta.n_links, 2);
        assert_eq!(meta.n_pumps, 0);
        assert_eq!(meta.n_periods, 1);
    }

    #[test]
    fn out_metadata_byte_size_calculations() {
        let meta = OutMetadata {
            n_nodes: 4,
            n_tanks: 2,
            n_links: 3,
            n_pumps: 1,
            quality_flag: 0,
            report_start: 0.0,
            report_step: 3600.0,
            n_periods: 5,
        };
        assert_eq!(meta.prolog_bytes(), (884 + 36 * 4 + 52 * 3 + 8 * 2) as u64);
        assert_eq!(meta.energy_bytes(), (28 * 1 + 4) as u64);
        assert_eq!(meta.period_bytes(), (4 * (4 * 4 + 8 * 3)) as u64);
        assert_eq!(
            meta.dynamic_offset(),
            meta.prolog_bytes() + meta.energy_bytes()
        );
    }

    #[test]
    fn out_metadata_snapshot_times() {
        let meta = OutMetadata {
            n_nodes: 2,
            n_tanks: 1,
            n_links: 1,
            n_pumps: 0,
            quality_flag: 0,
            report_start: 0.0,
            report_step: 3600.0,
            n_periods: 3,
        };
        assert_eq!(meta.snapshot_times(), vec![0.0, 3600.0, 7200.0]);
    }

    #[test]
    fn result_ranges_sanitise_replaces_infinities() {
        let mut r = ResultRanges::default();
        r.sanitise();
        assert!(r.pressure_min.is_finite(), "pressure_min should be finite");
        assert!(r.pressure_max.is_finite(), "pressure_max should be finite");
        assert!(r.head_min.is_finite(), "head_min should be finite");
        assert!(r.head_max.is_finite(), "head_max should be finite");
        assert!(r.demand_min.is_finite(), "demand_min should be finite");
        assert!(r.demand_max.is_finite(), "demand_max should be finite");
        assert!(r.flow_min.is_finite(), "flow_min should be finite");
        assert!(r.flow_max.is_finite(), "flow_max should be finite");
        assert!(r.velocity_min.is_finite(), "velocity_min should be finite");
        assert!(r.velocity_max.is_finite(), "velocity_max should be finite");
    }

    #[test]
    fn result_ranges_sanitise_expands_equal_min_max() {
        let mut r = ResultRanges::default();
        r.pressure_min = 5.0;
        r.pressure_max = 5.0; // equal → should be expanded
        r.head_min = f64::INFINITY;
        r.head_max = f64::NEG_INFINITY;
        r.demand_min = f64::INFINITY;
        r.demand_max = f64::NEG_INFINITY;
        r.flow_min = f64::INFINITY;
        r.flow_max = f64::NEG_INFINITY;
        r.velocity_min = f64::INFINITY;
        r.velocity_max = f64::NEG_INFINITY;
        r.sanitise();
        assert!(
            r.pressure_max > r.pressure_min,
            "equal min/max should be expanded: min={}, max={}",
            r.pressure_min,
            r.pressure_max
        );
    }

    #[test]
    fn result_ranges_update_from_period_tracks_min_max() {
        let pr = PeriodResult {
            node_demand: vec![1.0, 3.0],
            node_head: vec![10.0, 20.0],
            node_pressure: vec![5.0, 15.0],
            node_quality: vec![],
            link_flow: vec![2.0],
            link_velocity: vec![0.5],
            link_headloss: vec![0.0],
            link_quality: vec![],
            link_status: vec![1.0],
            link_setting: vec![1.0],
            link_reaction_rate: vec![0.0],
            link_friction_factor: vec![0.0],
        };
        let mut ranges = ResultRanges::default();
        ranges.update_from_period(&pr);
        assert_eq!(ranges.pressure_min, 5.0);
        assert_eq!(ranges.pressure_max, 15.0);
        assert_eq!(ranges.demand_min, 1.0);
        assert_eq!(ranges.demand_max, 3.0);
        assert_eq!(ranges.flow_min, 2.0);
        assert_eq!(ranges.velocity_min, 0.5);
    }
}
