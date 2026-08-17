//! out_reader — EPANET binary output file reader.
//!
//! The binary file layout is documented in `out_writer.rs` (this module's
//! writer counterpart).
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
    /// File format version: always 20012, the EPANET 2.3 layout (model spec
    /// §4.4.1). Any other value is rejected before a prolog is built.
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
    /// Flow unit code (model spec §3.1 table order: CFS = 0 … CMS = 10).
    pub flow_units: i32,
    /// Pressure unit code (0=psi, 1=kPa, 2=m) — EPANET `PressUnitsType` order,
    /// as written by `out_writer` (2 for SI files, 0 for US-customary files).
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

/// The epilog section (model spec §4.4.6): period count, warning flag and
/// closing magic number — the classic 12 bytes a consumer reading the tail
/// depends on.
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

/// Lightweight metadata extracted from the `.out` prolog header (first 60
/// bytes) and epilog (last 12 or 20 bytes, by format version).  Total I/O is
/// at most 80 bytes regardless of file size.
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
    /// Total simulation duration (seconds) from the prolog header
    /// (model spec §4.4.2). `0` for a steady-state run.
    pub duration: f64,
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
#[deprecated(note = "use the _checked variant (read_metadata_checked)")]
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

    // Model spec §4.4.1: 20012 is the only version, and the epilog is
    // EPANET's 12 bytes.
    let version = i32_at(4);
    if version != 20_012 {
        return Err(OutValidityError {
            kind: OutValidityKind::Unsupported,
            // 20013 gets its own sentence: Hydra wrote it from v2.0.0 to
            // v5.1.0, so anyone meeting this is looking at their own old
            // results and needs to know they are re-runnable, not corrupt.
            detail: if version == 20_013 {
                "results were written by Hydra 5.1 or earlier (.out version \
                 20013), which this build cannot read. Re-run the simulation \
                 to regenerate them"
                    .to_string()
            } else {
                format!("unsupported .out version: {version}")
            },
        });
    }
    let epilog_len: u64 = 12;

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
    let duration = i32_at(56) as f64;

    if file_len < 60 + epilog_len {
        return Err(OutValidityError {
            kind: OutValidityKind::Incomplete,
            detail: format!(
                "file too short: {file_len} bytes (minimum {} for header+epilog)",
                60 + epilog_len
            ),
        });
    }
    if let Err(e) = f.seek(SeekFrom::End(-(epilog_len as i64))) {
        return Err(OutValidityError {
            kind: OutValidityKind::Io,
            detail: format!("failed to seek epilog: {e}"),
        });
    }
    let mut epi = [0u8; 20];
    let epi = &mut epi[..epilog_len as usize];
    if let Err(e) = f.read_exact(epi) {
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

    let magic_off = epilog_len as usize - 4;
    let magic_end = i32::from_le_bytes(epi[magic_off..magic_off + 4].try_into().unwrap());
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
    // 16 bytes of network reactions + the version-dependent epilog.
    let expected_total = checked_add(
        checked_add(checked_add(prolog_bytes, energy_bytes)?, dynamic_bytes)?,
        16 + epilog_len,
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
        duration,
        n_periods,
    })
}

/// Read the 1-based tank/reservoir node indices from the `.out` prolog.
///
/// Seeks directly to the tank-index array (model spec §4.4.2) and reads
/// `n_tanks` INT4 values; total I/O is `4 × n_tanks` bytes regardless of file
/// size. Junction nodes are exactly those whose 1-based index does **not**
/// appear in the returned list.
pub fn read_tank_node_indices(
    path: &std::path::Path,
    meta: &OutMetadata,
) -> Result<Vec<usize>, String> {
    use std::io::{Read, Seek, SeekFrom};

    // Prolog layout before the tank-index array: 60-byte header, 824 bytes of
    // string fields, node IDs (32 × n_nodes), link IDs (32 × n_links), and
    // three INT4 link arrays (from, to, type: 12 × n_links).
    let offset = (884 + 32 * meta.n_nodes + 44 * meta.n_links) as u64;

    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("Invalid .out (io): failed to open file: {e}"))?;
    f.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Invalid .out (io): failed to seek tank indices: {e}"))?;

    let mut buf = vec![0u8; 4 * meta.n_tanks];
    f.read_exact(&mut buf).map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
            "incomplete"
        } else {
            "io"
        };
        format!("Invalid .out ({kind}): failed to read tank indices: {e}")
    })?;

    let mut indices = Vec::with_capacity(meta.n_tanks);
    for i in 0..meta.n_tanks {
        let v = i32::from_le_bytes(buf[4 * i..4 * i + 4].try_into().unwrap());
        if v < 1 || v as usize > meta.n_nodes {
            return Err(format!(
                "Invalid .out (corrupt): tank node index {v} out of range 1..={}",
                meta.n_nodes
            ));
        }
        indices.push(v as usize);
    }
    Ok(indices)
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
            link_index: i32::from_le_bytes(buf[0..4].try_into().unwrap()),
            pct_online: f32::from_le_bytes(buf[4..8].try_into().unwrap()),
            avg_efficiency: f32::from_le_bytes(buf[8..12].try_into().unwrap()),
            avg_kwh_per_flow: f32::from_le_bytes(buf[12..16].try_into().unwrap()),
            avg_kw: f32::from_le_bytes(buf[16..20].try_into().unwrap()),
            peak_kw: f32::from_le_bytes(buf[20..24].try_into().unwrap()),
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

// ── Single-element series (strided access) ───────────────────────────────────

/// Which side of the model an element series addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    /// A node — variables in file order: demand, head, pressure, quality.
    Node,
    /// A link — variables in file order: flow, velocity, headloss, quality,
    /// status, setting, reaction rate, friction factor.
    Link,
}

impl ElementKind {
    /// Variable names in the file's column order (model spec §4.4.4).
    pub fn variables(self) -> &'static [&'static str] {
        match self {
            ElementKind::Node => &["demand", "head", "pressure", "quality"],
            ElementKind::Link => &[
                "flow",
                "velocity",
                "headloss",
                "quality",
                "status",
                "setting",
                "reaction_rate",
                "friction_factor",
            ],
        }
    }
}

/// One variable's full-simulation series for a single element.
#[derive(Debug, Clone)]
pub struct ElementVariableSeries {
    /// Variable name, from [`ElementKind::variables`].
    pub variable: &'static str,
    /// One value per reporting period, in period order.
    pub values: Vec<f32>,
}

/// Every result variable of one element, across every reporting period.
#[derive(Debug, Clone)]
pub struct ElementSeries {
    /// Snapshot times (s), one per reporting period — parallel to each
    /// series' `values`.
    pub times: Vec<f64>,
    /// One entry per variable, in the file's column order.
    pub series: Vec<ElementVariableSeries>,
}

/// Read every result variable of a single element across all reporting
/// periods, addressing each value directly (model spec §4.4.8).
///
/// Reads `4 × variables × n_periods` bytes total — independent of network
/// size. The whole-period alternative (`read_period` in a loop) costs
/// `n_periods × (16·N_n + 32·N_l)` bytes to extract the same values, which on
/// a 46k-node network is four orders of magnitude more I/O.
///
/// `index` is the element's 0-based network-order index, bounds-checked
/// against the file's counts. Values are returned exactly as stored (the
/// units declared in the prolog header).
pub fn read_element_series(
    path: &std::path::Path,
    meta: &OutMetadata,
    kind: ElementKind,
    index: usize,
) -> Result<ElementSeries, String> {
    use std::io::{Read, Seek, SeekFrom};

    let count = match kind {
        ElementKind::Node => meta.n_nodes,
        ElementKind::Link => meta.n_links,
    };
    if index >= count {
        return Err(format!(
            "Invalid .out (corrupt): {} index {index} out of range (0..{count})",
            match kind {
                ElementKind::Node => "node",
                ElementKind::Link => "link",
            }
        ));
    }

    let variables = kind.variables();
    let n_periods = meta.n_periods;
    let mut values: Vec<Vec<f32>> = variables
        .iter()
        .map(|_| Vec::with_capacity(n_periods))
        .collect();

    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("Invalid .out (io): failed to open file: {e}"))?;

    // Value offset within a period block (spec §4.5.8): node variable `v` of
    // node `i` sits at 4(v·N_n + i); link variable `v` of link `j` sits at
    // 4(4·N_n + v·N_l + j).
    let value_offset = |var: usize| -> u64 {
        let words = match kind {
            ElementKind::Node => var * meta.n_nodes + index,
            ElementKind::Link => 4 * meta.n_nodes + var * meta.n_links + index,
        };
        4 * words as u64
    };

    let mut buf = [0u8; 4];
    for period in 0..n_periods {
        let period_offset = meta.dynamic_offset() + (period as u64) * meta.period_bytes();
        for (var, column) in values.iter_mut().enumerate() {
            f.seek(SeekFrom::Start(period_offset + value_offset(var)))
                .map_err(|e| format!("Invalid .out (io): failed to seek period {period}: {e}"))?;
            f.read_exact(&mut buf).map_err(|e| {
                let kind = if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    "incomplete"
                } else {
                    "io"
                };
                format!("Invalid .out ({kind}): failed to read period {period}: {e}")
            })?;
            column.push(f32::from_le_bytes(buf));
        }
    }

    Ok(ElementSeries {
        times: meta.snapshot_times(),
        series: variables
            .iter()
            .zip(values)
            .map(|(&variable, values)| ElementVariableSeries { variable, values })
            .collect(),
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
    /// Minimum link unit headloss across all sampled periods.
    pub headloss_min: f64,
    /// Maximum link unit headloss across all sampled periods.
    pub headloss_max: f64,
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
            headloss_min: f64::INFINITY,
            headloss_max: f64::NEG_INFINITY,
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
        fix(&mut self.headloss_min, &mut self.headloss_max, 0.0, 10.0);
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
        for &v in &pr.link_headloss {
            let v = v as f64;
            if v < self.headloss_min {
                self.headloss_min = v;
            }
            if v > self.headloss_max {
                self.headloss_max = v;
            }
        }
        // Quality arrays are populated only when quality_flag != 0.  When they
        // are non-empty, fold them into the running quality min/max.
        for &v in pr.node_quality.iter().chain(pr.link_quality.iter()) {
            let v = v as f64;
            match &mut self.quality_min {
                Some(m) => {
                    if v < *m {
                        *m = v;
                    }
                }
                None => {
                    self.quality_min = Some(v);
                }
            }
            match &mut self.quality_max {
                Some(m) => {
                    if v > *m {
                        *m = v;
                    }
                }
                None => {
                    self.quality_max = Some(v);
                }
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
                    Some(m) => {
                        if q < *m {
                            *m = q;
                        }
                    }
                    None => {
                        ranges.quality_min = Some(q);
                    }
                }
                match &mut ranges.quality_max {
                    Some(m) => {
                        if q > *m {
                            *m = q;
                        }
                    }
                    None => {
                        ranges.quality_max = Some(q);
                    }
                }
            }
        }

        // Link block starts after 4*nn node floats.
        // Layout: flow[nl] | velocity[nl] | headloss[nl] | quality[nl] | status[nl]
        let link_base = 4 * nn;
        for i in 0..nl {
            let fv = f32_at(&buf, link_base + i) as f64;
            let vv = f32_at(&buf, link_base + nl + i) as f64;
            let hv = f32_at(&buf, link_base + 2 * nl + i) as f64;
            if hv < ranges.headloss_min {
                ranges.headloss_min = hv;
            }
            if hv > ranges.headloss_max {
                ranges.headloss_max = hv;
            }
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
                    Some(m) => {
                        if q < *m {
                            *m = q;
                        }
                    }
                    None => {
                        ranges.quality_min = Some(q);
                    }
                }
                match &mut ranges.quality_max {
                    Some(m) => {
                        if q > *m {
                            *m = q;
                        }
                    }
                    None => {
                        ranges.quality_max = Some(q);
                    }
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
    /// Per-link maximum unit headloss across all periods, in the file's
    /// stored ratio (m/km ≡ ft/kft for pipes; head gain/loss for pumps
    /// and valves, which consumers filter out).
    pub link_max_unit_headloss: Vec<f64>,
    /// Per-node minimum quality across all periods (mode's unit).
    /// `f64::INFINITY` when no data.
    pub node_min_quality: Vec<f64>,
    /// Per-node maximum quality across all periods (mode's unit).
    pub node_max_quality: Vec<f64>,
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
pub fn scan_analytics(path: &std::path::Path, meta: &OutMetadata) -> Result<AnalyticsScan, String> {
    let n_nodes = meta.n_nodes;
    let n_tanks = meta.n_tanks;
    let n_links = meta.n_links;
    let n_periods = meta.n_periods;
    let tank_start = n_nodes.saturating_sub(n_tanks);

    let mut node_min_pressure: Vec<f64> = vec![f64::INFINITY; n_nodes];
    let mut link_max_velocity: Vec<f64> = vec![0.0_f64; n_links];
    let mut link_max_unit_headloss: Vec<f64> = vec![0.0_f64; n_links];
    let mut node_min_quality: Vec<f64> = vec![f64::INFINITY; n_nodes];
    let mut node_max_quality: Vec<f64> = vec![0.0_f64; n_nodes];
    let mut mb_series: Vec<f64> = vec![0.0_f64; n_periods];
    let mut total_inflow: f64 = 0.0;
    let mut total_outflow: f64 = 0.0;
    let mut tank_head: Vec<Vec<f64>> = vec![vec![0.0_f64; n_periods]; n_tanks];

    for p in 0..n_periods {
        let pr = read_period(path, meta, p)?;

        let mut period_inflow = 0.0_f64;
        let mut period_outflow = 0.0_f64;
        for &d in &pr.node_demand {
            let d = d as f64;
            if d > 0.0 {
                period_inflow += d;
            } else {
                period_outflow -= d;
            }
        }
        mb_series[p] = if period_inflow > 0.0 {
            (period_outflow / period_inflow * 100.0).min(100.0)
        } else {
            100.0
        };
        total_inflow += period_inflow;
        total_outflow += period_outflow;

        for (i, &p_val) in pr.node_pressure.iter().enumerate() {
            let v = p_val as f64;
            if v < node_min_pressure[i] {
                node_min_pressure[i] = v;
            }
        }
        for (ti, h_val) in pr.node_head[tank_start..].iter().enumerate() {
            if ti < n_tanks {
                tank_head[ti][p] = *h_val as f64;
            }
        }
        for (i, &v_val) in pr.link_velocity.iter().enumerate() {
            let v = (v_val as f64).abs();
            if v > link_max_velocity[i] {
                link_max_velocity[i] = v;
            }
        }
        for (i, &h_val) in pr.link_headloss.iter().enumerate() {
            let h = (h_val as f64).abs();
            if h > link_max_unit_headloss[i] {
                link_max_unit_headloss[i] = h;
            }
        }
        for (i, &q_val) in pr.node_quality.iter().enumerate() {
            let q = q_val as f64;
            if q < node_min_quality[i] {
                node_min_quality[i] = q;
            }
            if q > node_max_quality[i] {
                node_max_quality[i] = q;
            }
        }
    }

    Ok(AnalyticsScan {
        node_min_pressure,
        link_max_velocity,
        link_max_unit_headloss,
        node_min_quality,
        node_max_quality,
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
/// magic numbers are wrong, header counts are negative or inconsistent, the
/// epilog period count does not fit the buffer, or any read extends beyond
/// the buffer.  All size computations are validated against the actual
/// buffer length before any allocation, so hostile inputs cannot trigger
/// huge allocations or arithmetic overflow.
pub fn parse(data: &[u8]) -> Result<OutFile, String> {
    if data.len() < 12 {
        return Err(format!("too short: {} bytes (minimum 12)", data.len()));
    }

    // One format, one epilog length (model spec §4.4.6). The version is still
    // read here so the length error can name it.
    let version_peek = i32::from_le_bytes(data[4..8].try_into().unwrap());
    let epilog_len = 12;
    if data.len() < epilog_len {
        return Err(format!(
            "too short: {} bytes (minimum {epilog_len} for a version {version_peek} epilog)",
            data.len()
        ));
    }

    // Read n_periods from the epilog before parsing the dynamic section, so
    // the number of periods is known without a seek.
    let epi_off = data.len() - epilog_len;
    let n_periods_i = i32::from_le_bytes(data[epi_off..epi_off + 4].try_into().unwrap());
    if n_periods_i < 0 {
        return Err(format!("negative period count in epilog: {n_periods_i}"));
    }
    let n_periods = n_periods_i as usize;

    let mut cur = Cursor::new(data);

    // ── Prolog (model spec §4.4.2) ───────────────────────────────────────────────────────

    let magic_start = cur.read_i32()?;
    if magic_start != 516_114_521 {
        return Err(format!("unexpected magic at start: {magic_start}"));
    }
    let version = cur.read_i32()?;
    let n_nodes_i = cur.read_i32()?;
    let n_tanks_i = cur.read_i32()?;
    let n_links_i = cur.read_i32()?;
    let n_pumps_i = cur.read_i32()?;
    let n_valves_i = cur.read_i32()?;
    if n_nodes_i < 0 || n_tanks_i < 0 || n_links_i < 0 || n_pumps_i < 0 || n_valves_i < 0 {
        return Err(format!(
            "negative object counts in header: nodes={n_nodes_i} tanks={n_tanks_i} \
             links={n_links_i} pumps={n_pumps_i} valves={n_valves_i}"
        ));
    }
    let n_nodes = n_nodes_i as usize;
    let n_tanks = n_tanks_i as usize;
    let n_links = n_links_i as usize;
    let n_pumps = n_pumps_i as usize;
    let n_valves = n_valves_i as usize;
    if n_tanks > n_nodes {
        return Err(format!(
            "invalid counts: n_tanks ({n_tanks}) > n_nodes ({n_nodes})"
        ));
    }
    if n_pumps > n_links {
        return Err(format!(
            "invalid counts: n_pumps ({n_pumps}) > n_links ({n_links})"
        ));
    }
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
    // Checked arithmetic: header counts up to i32::MAX would overflow the
    // byte total on 32-bit targets.
    let per_object_bytes = 32usize
        .checked_mul(n_nodes)
        .and_then(|a| 44usize.checked_mul(n_links).and_then(|b| a.checked_add(b)))
        .and_then(|a| 4usize.checked_mul(n_tanks).and_then(|b| a.checked_add(b)))
        .ok_or_else(|| "prolog size overflow".to_string())?;
    cur.skip(per_object_bytes)?;

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

    // ── Energy (model spec §4.4.3) ───────────────────────────────────────────────────────

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

    // ── Dynamic results (model spec §4.4.4) ──────────────────────────────────────────────

    // Bound the epilog's period count against the bytes actually remaining in
    // the buffer before allocating: a crafted epilog on a tiny file could
    // otherwise request a multi-GB `Vec` and abort the process.
    let period_bytes = 4 * (4 * n_nodes + 8 * n_links); // counts already buffer-bounded
    let remaining = data.len().saturating_sub(cur.pos);
    if period_bytes == 0 {
        if n_periods > 0 {
            return Err(format!(
                "epilog claims {n_periods} periods but the network has no nodes or links"
            ));
        }
    } else {
        let dynamic_bytes = period_bytes
            .checked_mul(n_periods)
            .ok_or_else(|| "dynamic section size overflow".to_string())?;
        if dynamic_bytes > remaining {
            return Err(format!(
                "epilog claims {n_periods} periods ({dynamic_bytes} bytes) \
                 but only {remaining} bytes remain in the buffer"
            ));
        }
    }

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

    // ── Network reactions (model spec §4.4.5) ────────────────────────────────────────────

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

    // ── Epilog (model spec §4.4.6) ───────────────────────────────────────────────────────

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

    /// Checked `pos + n` that also verifies the result stays inside the
    /// buffer.  Returns the new end position without advancing.
    fn checked_end(&self, n: usize, what: &str) -> Result<usize, String> {
        self.pos
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| format!("unexpected EOF {} at offset {}", what, self.pos))
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        let end = self.checked_end(4, "reading i32")?;
        let v = i32::from_le_bytes(self.data[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }

    fn read_f32(&mut self) -> Result<f32, String> {
        let end = self.checked_end(4, "reading f32")?;
        let v = f32::from_le_bytes(self.data[self.pos..end].try_into().unwrap());
        self.pos = end;
        Ok(v)
    }

    fn read_f32s(&mut self, n: usize) -> Result<Vec<f32>, String> {
        let bytes = n
            .checked_mul(4)
            .ok_or_else(|| format!("f32 array size overflow ({n} values)"))?;
        let end = self.checked_end(bytes, "reading f32 values")?;
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
        self.pos = self.checked_end(n, "skipping bytes")?;
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
    use std::time::UNIX_EPOCH;

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

    // ── Hostile-input hardening ──────────────────────────────────────────────

    #[test]
    fn parse_rejects_negative_node_count() {
        // A negative n_nodes cast to usize would become a huge value and
        // trigger an enormous skip/allocation.
        let mut data = make_minimal_out(2, 1, 1, 0);
        data[8..12].copy_from_slice(&(-1_i32).to_le_bytes());
        let err = parse(&data).expect_err("negative n_nodes must be rejected");
        assert!(err.contains("negative object counts"), "got: {err}");
    }

    #[test]
    fn parse_rejects_negative_period_count() {
        let mut data = make_minimal_out(2, 1, 1, 0);
        let epi = data.len() - 12;
        data[epi..epi + 4].copy_from_slice(&(-5_i32).to_le_bytes());
        let err = parse(&data).expect_err("negative n_periods must be rejected");
        assert!(err.contains("negative period count"), "got: {err}");
    }

    #[test]
    fn parse_rejects_huge_period_count_on_small_file() {
        // A tiny file whose epilog claims i32::MAX periods: must error out
        // instead of attempting a multi-GB Vec allocation.
        let mut data = make_minimal_out(2, 1, 1, 0);
        let epi = data.len() - 12;
        data[epi..epi + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        let err = parse(&data).expect_err("oversized n_periods must be rejected");
        assert!(
            err.contains("periods"),
            "expected a period-bound error, got: {err}"
        );
    }

    #[test]
    fn parse_rejects_huge_object_counts_on_small_file() {
        // Header claims i32::MAX nodes/links in a small buffer: the prolog
        // skip must fail cleanly (no overflow panic, no huge allocation).
        let mut data = make_minimal_out(2, 1, 1, 0);
        data[8..12].copy_from_slice(&i32::MAX.to_le_bytes()); // n_nodes
        data[16..20].copy_from_slice(&i32::MAX.to_le_bytes()); // n_links
        assert!(parse(&data).is_err());
    }

    #[test]
    fn parse_rejects_inconsistent_counts() {
        // n_tanks > n_nodes and n_pumps > n_links are structurally impossible.
        let mut data = make_minimal_out(2, 1, 1, 0);
        data[12..16].copy_from_slice(&3_i32.to_le_bytes()); // n_tanks > n_nodes
        let err = parse(&data).expect_err("n_tanks > n_nodes must be rejected");
        assert!(err.contains("n_tanks"), "got: {err}");

        let mut data = make_minimal_out(2, 1, 1, 0);
        data[20..24].copy_from_slice(&2_i32.to_le_bytes()); // n_pumps > n_links
        let err = parse(&data).expect_err("n_pumps > n_links must be rejected");
        assert!(err.contains("n_pumps"), "got: {err}");
    }

    #[test]
    fn parse_rejects_periods_with_empty_network() {
        // Zero nodes and links makes each period zero bytes; a huge period
        // count would then pass any byte-budget check but still allocate
        // unbounded empty PeriodResults.
        let mut data = make_minimal_out(0, 0, 0, 0);
        let epi = data.len() - 12;
        data[epi..epi + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        let err = parse(&data).expect_err("periods with empty network must be rejected");
        assert!(err.contains("no nodes or links"), "got: {err}");
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
            .join("tests/fixtures/wds/single_pipe_hw.inp");
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
        // Through `wall_clock` like everywhere else. This is test code and
        // could read the clock directly without breaking anything, but an
        // exception here is an exception someone copies into code that
        // ships — and the crate's `clippy.toml` says so out loud.
        let nanos = crate::wall_clock::now()
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

    /// Model spec §4.4.1: `20013` is refused, and refused by name.
    ///
    /// Hydra wrote that version from v2.0.0 to v5.1.0, so anyone meeting this
    /// is looking at their own old results. The message has to say they are
    /// re-runnable rather than corrupt — the alternative is a bare
    /// "unsupported version" against a file Hydra itself produced.
    #[test]
    fn a_20013_file_is_refused_with_a_message_naming_it() {
        // Build one: bump the version, insert the digest ahead of the magic.
        let data = make_minimal_out(3, 1, 2, 0);
        let magic_off = data.len() - 4;
        let mut legacy = data[..magic_off].to_vec();
        legacy[4..8].copy_from_slice(&20013_i32.to_le_bytes());
        legacy.extend_from_slice(&0xDEAD_BEEF_0123_4567_u64.to_le_bytes());
        legacy.extend_from_slice(&data[magic_off..]);

        let path = write_temp_bytes(&legacy);
        let err = read_metadata_checked(&path).expect_err("20013 is no longer readable");
        let _ = std::fs::remove_file(&path);

        assert_eq!(err.kind, OutValidityKind::Unsupported);
        assert!(err.detail.contains("20013"), "{}", err.detail);
        assert!(
            err.detail.contains("Re-run"),
            "the message must say the results are recoverable: {}",
            err.detail
        );
    }

    /// The tank/reservoir node index list is readable directly from the
    /// prolog via `read_tank_node_indices`.
    #[test]
    fn read_tank_node_indices_returns_prolog_list() {
        let n_nodes = 4;
        let n_tanks = 2;
        let n_links = 3;
        let mut data = make_minimal_out(n_nodes, n_tanks, n_links, 0);
        // Tank index array offset (spec §4.5.2).
        let off = 884 + 32 * n_nodes + 44 * n_links;
        data[off..off + 4].copy_from_slice(&2_i32.to_le_bytes());
        data[off + 4..off + 8].copy_from_slice(&4_i32.to_le_bytes());
        let path = write_temp_bytes(&data);
        let meta = read_metadata_checked(&path).expect("valid file");
        let indices = read_tank_node_indices(&path, &meta).expect("tank indices");
        let _ = std::fs::remove_file(&path);
        assert_eq!(indices, vec![2, 4]);
    }

    /// Out-of-range tank indices are rejected as corrupt.
    #[test]
    fn read_tank_node_indices_rejects_out_of_range() {
        let n_nodes = 4;
        let n_tanks = 1;
        let n_links = 3;
        let mut data = make_minimal_out(n_nodes, n_tanks, n_links, 0);
        let off = 884 + 32 * n_nodes + 44 * n_links;
        data[off..off + 4].copy_from_slice(&9_i32.to_le_bytes()); // > n_nodes
        let path = write_temp_bytes(&data);
        let meta = read_metadata_checked(&path).expect("valid file");
        let err = read_tank_node_indices(&path, &meta).expect_err("out-of-range index");
        let _ = std::fs::remove_file(&path);
        assert!(err.contains("out of range"), "got: {err}");
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
            duration: 18_000.0,
            n_periods: 5,
        };
        assert_eq!(meta.prolog_bytes(), (884 + 36 * 4 + 52 * 3 + 8 * 2) as u64);
        assert_eq!(meta.energy_bytes(), (28 + 4) as u64);
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
            duration: 7200.0,
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
        let mut r = ResultRanges {
            pressure_min: 5.0,
            pressure_max: 5.0, // equal → should be expanded
            head_min: f64::INFINITY,
            head_max: f64::NEG_INFINITY,
            demand_min: f64::INFINITY,
            demand_max: f64::NEG_INFINITY,
            flow_min: f64::INFINITY,
            flow_max: f64::NEG_INFINITY,
            velocity_min: f64::INFINITY,
            velocity_max: f64::NEG_INFINITY,
            ..Default::default()
        };
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
            link_headloss: vec![1.5, 4.0],
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
        assert_eq!(ranges.headloss_min, 1.5);
        assert_eq!(ranges.headloss_max, 4.0);
    }

    // ── duration + strided element series (spec §4.5.8) ───────────────────

    /// A well-formed multi-period `.out` whose every dynamic REAL4 carries a
    /// distinct value derived from its own byte offset, so a strided read
    /// landing one word off is detected rather than coincidentally matching.
    fn make_out_with_periods(
        n_nodes: usize,
        n_tanks: usize,
        n_links: usize,
        n_periods: usize,
        duration: i32,
    ) -> Vec<u8> {
        let prolog = 884 + 36 * n_nodes + 52 * n_links + 8 * n_tanks;
        let energy = 4; // no pumps
        let period = 4 * (4 * n_nodes + 8 * n_links);
        let size = prolog + energy + n_periods * period + 16 + 12;
        let mut data = vec![0u8; size];

        data[0..4].copy_from_slice(&516_114_521_i32.to_le_bytes());
        data[4..8].copy_from_slice(&20012_i32.to_le_bytes());
        data[8..12].copy_from_slice(&(n_nodes as i32).to_le_bytes());
        data[12..16].copy_from_slice(&(n_tanks as i32).to_le_bytes());
        data[16..20].copy_from_slice(&(n_links as i32).to_le_bytes());
        data[20..24].copy_from_slice(&0_i32.to_le_bytes()); // n_pumps
        data[48..52].copy_from_slice(&0_i32.to_le_bytes()); // report start
        data[52..56].copy_from_slice(&3600_i32.to_le_bytes()); // report step
        data[56..60].copy_from_slice(&duration.to_le_bytes());

        let dynamic = prolog + energy;
        for word in 0..(n_periods * period / 4) {
            let off = dynamic + 4 * word;
            let value = (word as f32) * 0.25 + 1.0;
            data[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }

        let epi = size - 12;
        data[epi..epi + 4].copy_from_slice(&(n_periods as i32).to_le_bytes());
        data[epi + 8..epi + 12].copy_from_slice(&516_114_521_i32.to_le_bytes());
        data
    }

    #[test]
    fn read_metadata_exposes_prolog_duration() {
        let path = write_temp_bytes(&make_out_with_periods(3, 1, 2, 2, 86_400));
        let meta = read_metadata_checked(&path).expect("valid file");
        let _ = std::fs::remove_file(&path);
        assert_eq!(meta.duration, 86_400.0);
    }

    #[test]
    fn read_metadata_duration_is_zero_for_steady_state() {
        let path = write_temp_bytes(&make_out_with_periods(2, 1, 1, 1, 0));
        let meta = read_metadata_checked(&path).expect("valid file");
        let _ = std::fs::remove_file(&path);
        assert_eq!(meta.duration, 0.0);
    }

    /// `scan_ranges` reads the dynamic block by hand-computed word offsets
    /// rather than through `read_period`'s decoder, so a stride that is one
    /// variable off yields a plausible-looking range taken from the wrong
    /// column. Every range it reports must therefore agree with the same
    /// range folded out of the decoded periods.
    #[test]
    fn scan_ranges_agrees_with_decoded_periods() {
        let (n_nodes, n_tanks, n_links, n_periods) = (4, 1, 3, 5);
        let path = write_temp_bytes(&make_out_with_periods(
            n_nodes, n_tanks, n_links, n_periods, 14_400,
        ));
        let meta = read_metadata_checked(&path).expect("valid file");

        let mut expected = ResultRanges::default();
        for p in 0..n_periods {
            expected.update_from_period(&read_period(&path, &meta, p).expect("period"));
        }
        let scanned = scan_ranges(&path, &meta, 2048).expect("scan");
        let _ = std::fs::remove_file(&path);

        // The fixture derives every word from its own byte offset, so each
        // variable's range is distinct — reading a neighbouring column
        // cannot coincidentally match.
        assert_eq!(scanned.headloss_min, expected.headloss_min, "headloss_min");
        assert_eq!(scanned.headloss_max, expected.headloss_max, "headloss_max");
        assert_eq!(scanned.velocity_min, expected.velocity_min, "velocity_min");
        assert_eq!(scanned.velocity_max, expected.velocity_max, "velocity_max");
        assert_eq!(scanned.flow_min, expected.flow_min, "flow_min");
        assert_eq!(scanned.flow_max, expected.flow_max, "flow_max");
        assert_eq!(scanned.pressure_min, expected.pressure_min, "pressure_min");
        assert_eq!(scanned.pressure_max, expected.pressure_max, "pressure_max");
        assert_eq!(scanned.head_min, expected.head_min, "head_min");
        assert_eq!(scanned.head_max, expected.head_max, "head_max");
        assert_eq!(scanned.demand_min, expected.demand_min, "demand_min");
        assert_eq!(scanned.demand_max, expected.demand_max, "demand_max");
    }

    /// The strided reader must agree with the whole-block reader for every
    /// element, variable, and period — it is a pure I/O optimisation.
    #[test]
    fn element_series_matches_read_period_for_every_variable() {
        let (n_nodes, n_tanks, n_links, n_periods) = (4, 1, 3, 5);
        let path = write_temp_bytes(&make_out_with_periods(
            n_nodes, n_tanks, n_links, n_periods, 14_400,
        ));
        let meta = read_metadata_checked(&path).expect("valid file");
        let periods: Vec<PeriodResult> = (0..n_periods)
            .map(|p| read_period(&path, &meta, p).expect("period"))
            .collect();

        for i in 0..n_nodes {
            let series = read_element_series(&path, &meta, ElementKind::Node, i).expect("node");
            assert_eq!(series.times, meta.snapshot_times());
            let by_name = |name: &str| {
                &series
                    .series
                    .iter()
                    .find(|s| s.variable == name)
                    .unwrap_or_else(|| panic!("node variable {name}"))
                    .values
            };
            for (p, pr) in periods.iter().enumerate() {
                assert_eq!(by_name("demand")[p], pr.node_demand[i], "node {i} demand");
                assert_eq!(by_name("head")[p], pr.node_head[i], "node {i} head");
                assert_eq!(
                    by_name("pressure")[p],
                    pr.node_pressure[i],
                    "node {i} pressure"
                );
                assert_eq!(
                    by_name("quality")[p],
                    pr.node_quality[i],
                    "node {i} quality"
                );
            }
        }

        for j in 0..n_links {
            let series = read_element_series(&path, &meta, ElementKind::Link, j).expect("link");
            let by_name = |name: &str| {
                &series
                    .series
                    .iter()
                    .find(|s| s.variable == name)
                    .unwrap_or_else(|| panic!("link variable {name}"))
                    .values
            };
            for (p, pr) in periods.iter().enumerate() {
                assert_eq!(by_name("flow")[p], pr.link_flow[j], "link {j} flow");
                assert_eq!(
                    by_name("velocity")[p],
                    pr.link_velocity[j],
                    "link {j} velocity"
                );
                assert_eq!(
                    by_name("headloss")[p],
                    pr.link_headloss[j],
                    "link {j} headloss"
                );
                assert_eq!(by_name("quality")[p], pr.link_quality[j], "link {j} qual");
                assert_eq!(by_name("status")[p], pr.link_status[j], "link {j} status");
                assert_eq!(
                    by_name("setting")[p],
                    pr.link_setting[j],
                    "link {j} setting"
                );
                assert_eq!(
                    by_name("reaction_rate")[p],
                    pr.link_reaction_rate[j],
                    "link {j} reaction"
                );
                assert_eq!(
                    by_name("friction_factor")[p],
                    pr.link_friction_factor[j],
                    "link {j} friction"
                );
            }
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn element_series_rejects_out_of_range_index() {
        let path = write_temp_bytes(&make_out_with_periods(2, 1, 1, 2, 3600));
        let meta = read_metadata_checked(&path).expect("valid file");
        let node_err = read_element_series(&path, &meta, ElementKind::Node, 2)
            .expect_err("node index past the end");
        let link_err = read_element_series(&path, &meta, ElementKind::Link, 1)
            .expect_err("link index past the end");
        let _ = std::fs::remove_file(&path);
        assert!(node_err.contains("out of range"), "got: {node_err}");
        assert!(link_err.contains("out of range"), "got: {link_err}");
    }

    #[test]
    fn element_series_variable_order_matches_file_layout() {
        assert_eq!(
            ElementKind::Node.variables(),
            ["demand", "head", "pressure", "quality"]
        );
        assert_eq!(
            ElementKind::Link.variables(),
            [
                "flow",
                "velocity",
                "headloss",
                "quality",
                "status",
                "setting",
                "reaction_rate",
                "friction_factor"
            ]
        );
    }
}
