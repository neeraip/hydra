//! Result-reading commands over `results.out`: metadata + ranges, per-period
//! arrays, element series, cross-period analytics, pump energy, and CSV export.

use serde::{Deserialize, Serialize};

use crate::meta::{self, bundle};

use super::binary_codec::encode_period_results;
use super::network_dto::{
    format_inp_parse_error, NetworkState, NetworkStateInner, FT3_TO_M3, FT_TO_MM,
};
use super::projects::{app_data_dir, model_path_for, results_path_for, validate_target_ids};

// ── Simulation helpers ───────────────────────────────────────────────────────

/// `true` when the network carries any energy-price information the engine's
/// accounting could have used: a positive global `[ENERGY]` price or a
/// positive per-pump price. Price patterns are only multipliers, so a pattern
/// without a base price still yields zero cost — pattern presence alone does
/// not count as price information.
pub(crate) fn network_has_energy_price(network: &hydra::Network) -> bool {
    network.options.energy_price > 0.0
        || network.links.iter().any(|l| match &l.kind {
            hydra::LinkKind::Pump(p) => p.energy_price.is_some_and(|v| v > 0.0),
            _ => false,
        })
}

/// Recover a pump's total kWh and total cost from a `.out` energy record.
///
/// The `.out` file stores per-day / per-hour normalisations (see the engine's
/// `out_writer::write_energy`), so the totals are re-derived by inverting the
/// writer's formulas:
/// - `time_online = pct_online / 100 × duration` (with EPANET's synthetic
///   1-hour horizon when `duration == 0`, i.e. a steady-state run);
/// - `total_kwh = avg_kw × time_online / 3600`;
/// - `total_cost = avg_cost_per_day × duration / 86400` (or `/ 24` of the
///   ×24 steady-state figure).
///
/// The cost stored in the file was accumulated by the engine period-by-period
/// with the effective price (per-pump/global price × price-pattern
/// multiplier), so patterns are already respected.
fn energy_totals_from_record(
    avg_kw: f64,
    pct_online: f64,
    avg_cost_per_day: f64,
    duration_secs: f64,
) -> (f64, f64) {
    let horizon = if duration_secs > 0.0 {
        duration_secs
    } else {
        3600.0
    };
    let time_online_secs = pct_online / 100.0 * horizon;
    let total_kwh = avg_kw * time_online_secs / 3600.0;
    let total_cost = if duration_secs > 0.0 {
        avg_cost_per_day * duration_secs / 86_400.0
    } else {
        avg_cost_per_day / 24.0
    };
    (total_kwh, total_cost)
}

/// Read the total simulation duration (seconds) from a `.out` prolog header.
///
/// `OutMetadata` does not expose the prolog's duration field, so read the
/// INT4 at byte offset 56 directly (the header layout is fixed; see
/// `OutProlog`). Callers should have validated the file via
/// `read_metadata_checked` first.
fn out_duration_secs(out_path: &std::path::Path) -> Result<f64, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(out_path).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(56)).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 4];
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(i32::from_le_bytes(buf) as f64)
}

/// Collect per-pump energy from a completed simulation session.
///
/// `has_price_info` must be computed from the network *before* it is moved
/// into the simulation (see [`network_has_energy_price`]); when `false`,
/// `total_cost` is `None` for every pump.
pub(crate) fn collect_pump_energy(
    sim: &hydra::Simulation,
    duration_seconds: f64,
    has_price_info: bool,
) -> Vec<PumpEnergyDto> {
    sim.pump_ids()
        .into_iter()
        .filter_map(|id| {
            let pe = sim.get_pump_energy(id).ok()?;
            let pct_online = if duration_seconds > 0.0 {
                (pe.time_online / duration_seconds * 100.0).min(100.0)
            } else {
                0.0
            };
            let avg_kw = if pe.time_online > 0.0 {
                pe.kwh / (pe.time_online / 3600.0)
            } else {
                0.0
            };
            Some(PumpEnergyDto {
                id: id.to_string(),
                pct_online,
                avg_efficiency: pe.avg_efficiency() * 100.0,
                avg_kwh_per_flow: pe.kwh_per_flow,
                avg_kw,
                peak_kw: pe.max_kw,
                total_kwh: pe.kwh,
                total_cost: has_price_info.then_some(pe.total_cost),
            })
        })
        .collect()
}

/// Build `PumpEnergyDto` entries from the energy section of a `.out` file.
/// Returns an empty vec on any read error (energy data is non-critical).
fn pump_energy_from_out(
    out_path: &std::path::Path,
    network: &hydra::Network,
    meta: &hydra::io::out_reader::OutMetadata,
) -> Vec<PumpEnergyDto> {
    let energy = match hydra::io::out_reader::read_energy(out_path, meta) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let duration_secs = out_duration_secs(out_path).unwrap_or(0.0);
    let has_price_info = network_has_energy_price(network);
    energy
        .pumps
        .iter()
        .filter_map(|rec| {
            // `link_index` is 1-based.
            let idx = (rec.link_index as usize).checked_sub(1)?;
            let link = network.links.get(idx)?;
            let (total_kwh, total_cost) = energy_totals_from_record(
                rec.avg_kw as f64,
                rec.pct_online as f64,
                rec.avg_cost_per_day as f64,
                duration_secs,
            );
            Some(PumpEnergyDto {
                id: link.base.id.clone(),
                pct_online: rec.pct_online as f64,
                avg_efficiency: rec.avg_efficiency as f64,
                avg_kwh_per_flow: rec.avg_kwh_per_flow as f64,
                avg_kw: rec.avg_kw as f64,
                peak_kw: rec.peak_kw as f64,
                total_kwh,
                total_cost: has_price_info.then_some(total_cost),
            })
        })
        .collect()
}

/// Per-pump energy accounting returned with every simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PumpEnergyDto {
    pub id: String,
    /// Percentage of simulation duration the pump was online (0–100).
    pub pct_online: f64,
    /// Time-weighted average efficiency (%).
    pub avg_efficiency: f64,
    /// Average energy intensity (kWh per unit of flow).
    pub avg_kwh_per_flow: f64,
    /// Average electrical power while running (kW).
    pub avg_kw: f64,
    /// Peak electrical power observed (kW).
    pub peak_kw: f64,
    /// Total electrical energy consumed over the simulation horizon (kWh).
    #[serde(default)]
    pub total_kwh: f64,
    /// Total energy cost over the simulation horizon, in the model's currency
    /// units. The engine's accounting derives the effective price per period
    /// as `(pump price | global [ENERGY] price) × price-pattern multiplier`
    /// (see the engine's `effective_price`), so price patterns are respected.
    /// `None` when the model carries no price information at all (no global
    /// `[ENERGY]` price and no per-pump price).
    #[serde(default)]
    pub total_cost: Option<f64>,
}

/// Mass-balance summary returned by `get_result_analytics`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MassBalanceDto {
    /// Cumulative network inflow over the simulation horizon (m³).
    pub inflow_m3: f64,
    /// Cumulative network outflow (demand consumed) over the horizon (m³).
    pub outflow_m3: f64,
    /// Overall mass-balance percentage: `outflow / inflow × 100` (capped at 100).
    pub balance_pct: f64,
    /// Per-period balance percentage (one value per reporting period).
    pub series: Vec<f64>,
}

/// One bin of a histogram returned by `get_result_analytics`.
/// `hi` is `f64::MAX` for the open upper bucket.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistogramBucketDto {
    pub lo: f64,
    pub hi: f64,
    pub count: u32,
}

/// One entry in the top-pipes list returned by `get_result_analytics`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopPipeDto {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    /// Nominal diameter in millimetres; 0 for pumps and valves.
    pub diameter_mm: f64,
    /// Peak velocity across all reporting periods (m/s).
    pub max_velocity_ms: f64,
}

/// Head time series for one tank returned by `get_result_analytics`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TankHeadSeriesDto {
    pub node_id: String,
    /// Hydraulic head (m) at each reporting period.
    pub head: Vec<f64>,
}

/// Full cross-period analytics computed by `get_result_analytics`.
/// All values are computed by streaming the `.out` file one period at a time —
/// no full-file load, safe for multi-gigabyte result sets.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultAnalyticsDto {
    pub period_count: u32,
    pub node_count: u32,
    pub link_count: u32,
    pub mass_balance: MassBalanceDto,
    /// Node ID with the lowest minimum pressure across all periods.
    /// Absent (omitted from the JSON) when no node has a finite pressure
    /// value — the frontend renders "—" for missing fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_pressure_node_id: Option<String>,
    /// Lowest minimum-pressure value (m) across all nodes and periods.
    /// Absent together with `min_pressure_node_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_pressure_m: Option<f64>,
    /// Number of nodes whose worst-case pressure is below 14 m.
    pub low_pressure_count: u32,
    /// Link ID with the highest peak velocity across all periods.
    /// Absent when every link's peak velocity is zero or NaN (the scan's
    /// "no data" default), i.e. there is no meaningful maximum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_velocity_link_id: Option<String>,
    /// Highest peak velocity (m/s) across all links and periods.
    /// Absent together with `max_velocity_link_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_velocity_ms: Option<f64>,
    /// Histogram of per-node minimum pressure (7 fixed bins, m).
    pub pressure_histogram: Vec<HistogramBucketDto>,
    /// Histogram of per-link maximum velocity (5 fixed bins, m/s).
    pub velocity_histogram: Vec<HistogramBucketDto>,
    /// Top 5 links ordered by peak velocity descending.
    pub top_pipes: Vec<TopPipeDto>,
    /// Head-over-time series for every tank node.
    pub tank_series: Vec<TankHeadSeriesDto>,
}

/// Global min/max ranges for the common result variables across all periods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRangesDto {
    pub pressure_min: f64,
    pub pressure_max: f64,
    pub head_min: f64,
    pub head_max: f64,
    pub demand_min: f64,
    pub demand_max: f64,
    pub flow_min: f64,
    pub flow_max: f64,
    pub velocity_min: f64,
    pub velocity_max: f64,
    /// Global quality min/max.  `None` when the results file contains no quality data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_max: Option<f64>,
}

/// Metadata returned by `load_result_meta`: snapshot times and global ranges.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultMetaDto {
    pub times: Vec<f64>,
    pub ranges: ResultRangesDto,
    /// Quality mode used in the simulation: `"none"`, `"chemical"`, `"age"`, or `"trace"`.
    pub quality_mode: String,
    /// Topology digest of the network the results were produced from, as
    /// 16 lowercase hex chars (see the engine's `compute_network_digest`).
    /// `None` for pre-digest `.out` files — the frontend must then treat the
    /// topology match as unknown and apply no staleness gating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_digest: Option<String>,
}

/// Format a topology digest as the wire representation shared with the
/// frontend: 16 lowercase hex characters, zero-padded.
fn digest_hex(digest: u64) -> String {
    format!("{digest:016x}")
}

// ── Result metadata + period commands ────────────────────────────────────────

/// Maximum number of evenly-spaced reporting periods `scan_ranges` samples
/// when computing global result ranges — keeps the scan fast (~50 ms) even
/// for very long simulations.
const RANGE_SCAN_MAX_SAMPLES: usize = 2048;

/// Return snapshot times and global result ranges for a project or scenario.
///
/// Reads the binary `results.out` on disk without loading the full file.
/// Returns `Ok(None)` when no simulation has been run yet for the target —
/// an expected state (e.g. a freshly imported project), not an error.
#[tauri::command(async)]
/// Parse result metadata (timestep count, reporting period) from `results.out`.
pub fn load_result_meta(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<ResultMetaDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !out_path.exists() {
        return Ok(None);
    }
    let meta =
        hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
    let times = meta.snapshot_times();
    let ranges = hydra::io::out_reader::scan_ranges(&out_path, &meta, RANGE_SCAN_MAX_SAMPLES)?;
    let quality_mode = match meta.quality_flag {
        1 => "chemical",
        2 => "age",
        3 => "trace",
        _ => "none",
    };
    Ok(Some(ResultMetaDto {
        times,
        quality_mode: quality_mode.to_string(),
        network_digest: meta.network_digest.map(digest_hex),
        ranges: ResultRangesDto {
            pressure_min: ranges.pressure_min,
            pressure_max: ranges.pressure_max,
            head_min: ranges.head_min,
            head_max: ranges.head_max,
            demand_min: ranges.demand_min,
            demand_max: ranges.demand_max,
            flow_min: ranges.flow_min,
            flow_max: ranges.flow_max,
            velocity_min: ranges.velocity_min,
            velocity_max: ranges.velocity_max,
            quality_min: ranges.quality_min,
            quality_max: ranges.quality_max,
        },
    }))
}

/// Return flat result arrays for a single reporting period as a compact
/// binary payload (see [`encode_period_results`] for the byte layout).
///
/// Values are in SI units (L/s, m, m/s) because `results.out` is always
/// written with `FlowUnits::Lps`. Returns an error when `period` is out of
/// range or `results.out` does not exist.
#[tauri::command(async)]
/// Return flat arrays for a single reporting period (nodes + links).
pub fn get_period_results(
    app: tauri::AppHandle,
    project_id: String,
    period: usize,
    scenario_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    let meta =
        hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
    let pr = hydra::io::out_reader::read_period(&out_path, &meta, period)?;
    let has_quality = meta.quality_flag != 0;
    Ok(tauri::ipc::Response::new(encode_period_results(
        &pr,
        has_quality,
    )))
}

/// Parsed network for `(project_id, scenario_id)`: cloned from the in-memory
/// cache when `NetworkState` holds exactly that target **and has no unsaved
/// edits**, otherwise read and parsed from the on-disk model — avoids a
/// multi-MB INP re-parse per call in the common case where the requested
/// target is the loaded one.
///
/// The `dirty` check matters for correctness, not just freshness: callers
/// index `results.out` arrays positionally against the returned network, and
/// the `.out` file was produced from the on-disk model. A dirty cache may
/// contain structural edits (added/deleted elements) the results know nothing
/// about, which would silently attach results to the wrong elements — so a
/// dirty cache is treated exactly like a non-matching target.
fn network_for_target(
    app_data: &std::path::Path,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<hydra::Network, String> {
    // Decide (and clone) under the lock; all file IO happens after release.
    {
        let guard = state.0.lock();
        if let NetworkStateInner::Loaded {
            network,
            dirty,
            owner_project_id: Some(owner),
            owner_scenario_id,
            ..
        } = &*guard
        {
            if !*dirty && owner == project_id && owner_scenario_id.as_deref() == scenario_id {
                return Ok(network.clone());
            }
        }
    }
    let model_path = model_path_for(app_data, project_id, scenario_id);
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    hydra::io::parse(&raw).map_err(format_inp_parse_error)
}

/// Topology digest of the CURRENT model for `(project_id, scenario_id)`.
///
/// Deliberately the opposite cache policy of [`network_for_target`]: a
/// **dirty** cache owning the target is *preferred*, because the whole point
/// is to fingerprint the live in-memory topology (including unsaved edits) so
/// the frontend can detect that loaded results no longer match it. The digest
/// is computed under the lock without cloning — FNV-1a over element IDs is
/// cheap even at 46k nodes. Falls back to parsing the on-disk model when the
/// cache holds a different target.
fn live_network_digest(
    app_data: &std::path::Path,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<u64, String> {
    {
        let guard = state.0.lock();
        if let NetworkStateInner::Loaded {
            network,
            owner_project_id: Some(owner),
            owner_scenario_id,
            ..
        } = &*guard
        {
            if owner == project_id && owner_scenario_id.as_deref() == scenario_id {
                return Ok(hydra::compute_network_digest(network));
            }
        }
    }
    let model_path = model_path_for(app_data, project_id, scenario_id);
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    let network = hydra::io::parse(&raw).map_err(format_inp_parse_error)?;
    Ok(hydra::compute_network_digest(&network))
}

/// Return the topology digest of the current model for a project or scenario
/// as 16 lowercase hex chars — including unsaved in-memory edits when the
/// managed network cache holds that target (see [`live_network_digest`]).
/// The frontend compares this against `ResultMetaDto::network_digest` to
/// detect results that predate the live topology.
#[tauri::command(async)]
/// Return the current model's topology digest (hex) for a project/scenario.
pub fn get_network_digest(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<String, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    live_network_digest(&app_data, &state, &project_id, scenario_id.as_deref()).map(digest_hex)
}

/// Return the pump energy summary for a project or scenario.
///
/// Reads only the energy section of `results.out` (a few dozen bytes per pump)
/// without touching the period data.  Safe for any network size.
#[tauri::command(async)]
/// Return pump energy statistics from the binary output file.
pub fn get_pump_energy(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<PumpEnergyDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    // No simulation run yet — expected for a fresh project, not an error.
    if !out_path.exists() {
        return Ok(Vec::new());
    }
    let network = network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    let meta =
        hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
    Ok(pump_energy_from_out(&out_path, &network, &meta))
}

/// One named value series within a [`SeriesDto`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesFieldDto {
    pub name: String,
    pub values: Vec<f64>,
}

/// Full time series for a single element returned by `get_element_series`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDto {
    /// Snapshot times in seconds since simulation start, one per period.
    pub times: Vec<u32>,
    /// Value series, one entry per field; every `values` vec is parallel to
    /// `times`.
    pub fields: Vec<SeriesFieldDto>,
}

/// Build the per-element time series by streaming the `.out` file one period
/// at a time (`read_period` seeks; the file is never loaded whole).
///
/// `kind` is `"node"` or `"link"`; `index` is the element's network-order
/// index (0-based), bounds-checked against the result file's counts. Values
/// are returned exactly as stored in `results.out` — the same SI display
/// units (m, L/s, m/s) the `get_period_results` path returns, because the
/// file is always written with `FlowUnits::Lps` (no conversion needed).
fn element_series_from_out(
    out_path: &std::path::Path,
    kind: &str,
    index: u32,
) -> Result<SeriesDto, String> {
    let meta = hydra::io::out_reader::read_metadata_checked(out_path).map_err(|e| e.to_string())?;
    let idx = index as usize;
    let has_quality = meta.quality_flag != 0;

    // Field names in wire order, checked against the result's counts.
    let field_names: Vec<&str> = match kind {
        "node" => {
            if idx >= meta.n_nodes {
                return Err(format!(
                    "node index {idx} out of range: results hold {} nodes",
                    meta.n_nodes
                ));
            }
            let mut f = vec!["pressure", "head", "demand"];
            if has_quality {
                f.push("quality");
            }
            f
        }
        "link" => {
            if idx >= meta.n_links {
                return Err(format!(
                    "link index {idx} out of range: results hold {} links",
                    meta.n_links
                ));
            }
            let mut f = vec!["flow", "velocity", "headloss", "status"];
            if has_quality {
                f.push("quality");
            }
            f
        }
        other => {
            return Err(format!(
                "unknown element kind {other:?}: expected \"node\" or \"link\""
            ))
        }
    };

    let mut columns: Vec<Vec<f64>> = field_names
        .iter()
        .map(|_| Vec::with_capacity(meta.n_periods))
        .collect();
    for period in 0..meta.n_periods {
        let pr = hydra::io::out_reader::read_period(out_path, &meta, period)?;
        let values: Vec<f64> = if kind == "node" {
            let mut v = vec![
                pr.node_pressure[idx] as f64,
                pr.node_head[idx] as f64,
                pr.node_demand[idx] as f64,
            ];
            if has_quality {
                v.push(pr.node_quality[idx] as f64);
            }
            v
        } else {
            let mut v = vec![
                pr.link_flow[idx] as f64,
                pr.link_velocity[idx] as f64,
                pr.link_headloss[idx] as f64,
                pr.link_status[idx] as f64,
            ];
            if has_quality {
                v.push(pr.link_quality[idx] as f64);
            }
            v
        };
        for (col, v) in columns.iter_mut().zip(values) {
            col.push(v);
        }
    }

    Ok(SeriesDto {
        times: meta.snapshot_times().iter().map(|&t| t as u32).collect(),
        fields: field_names
            .into_iter()
            .zip(columns)
            .map(|(name, values)| SeriesFieldDto {
                name: name.to_string(),
                values,
            })
            .collect(),
    })
}

/// Return the full time series of every result field for one element.
///
/// `kind` is `"node"` or `"link"`; `index` is the element's network-order
/// index (the same positional index the binary snapshot / period-results
/// arrays use). Returns `Ok(None)` when no `results.out` exists for the
/// target (no simulation run yet). See [`element_series_from_out`] for the
/// payload shape and units.
#[tauri::command(async)]
/// Return per-period result series for a single node or link.
pub fn get_element_series(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
    kind: String,
    index: u32,
) -> Result<Option<SeriesDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    // No simulation run yet — expected for a fresh project, not an error.
    if !out_path.exists() {
        return Ok(None);
    }
    element_series_from_out(&out_path, &kind, index).map(Some)
}

/// Index and value of the smallest **finite** entry in `values`; `None` when
/// no entry is finite (the analytics scan initialises per-node minimum
/// pressure to `f64::INFINITY`, so an untouched array means "no data").
fn min_finite_with_index(values: &[f64]) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &v) in values.iter().enumerate() {
        if v.is_finite() && best.is_none_or(|(_, bv)| v < bv) {
            best = Some((i, v));
        }
    }
    best
}

/// Index and value of the largest **strictly positive** entry in `values`;
/// `None` when every entry is zero, negative, or NaN (the analytics scan
/// initialises per-link maximum velocity to `0.0`, so an all-zero array means
/// "no data").
fn max_positive_with_index(values: &[f64]) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, &v) in values.iter().enumerate() {
        if v > 0.0 && best.is_none_or(|(_, bv)| v > bv) {
            best = Some((i, v));
        }
    }
    best
}

/// Compute cross-period analytics by streaming the `.out` file one period at a
/// time.  Never loads more than a single period's data into memory, so it is
/// safe for arbitrarily large result files.
///
/// Returns histograms, summary statistics, a tank head time series, and a
/// mass-balance series — everything the Analysis view needs without holding a
/// full `SimulationResult` in memory.
#[tauri::command(async)]
/// Return post-simulation analytics for a completed run.
pub fn get_result_analytics(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<ResultAnalyticsDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    // No simulation run yet — expected for a fresh project, not an error.
    if !out_path.exists() {
        return Ok(None);
    }
    let network = network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    let meta =
        hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;

    let n_nodes = meta.n_nodes;
    let n_tanks = meta.n_tanks;
    let n_links = meta.n_links;
    let n_periods = meta.n_periods;
    let tank_start = n_nodes.saturating_sub(n_tanks);

    let scan = hydra::io::out_reader::scan_analytics(&out_path, &meta)?;
    let node_min_pressure = scan.node_min_pressure;
    let link_max_velocity = scan.link_max_velocity;
    let mb_series = scan.mb_series;
    let total_inflow = scan.total_inflow;
    let total_outflow = scan.total_outflow;
    let tank_head = scan.tank_head;

    // ── Pressure histogram (same 7 bins as the frontend) ─────────────────────
    const PRESSURE_BINS: &[(f64, f64)] = &[
        (0.0, 10.0),
        (10.0, 20.0),
        (20.0, 30.0),
        (30.0, 40.0),
        (40.0, 50.0),
        (50.0, 60.0),
        (60.0, f64::MAX),
    ];
    let mut pressure_histogram: Vec<HistogramBucketDto> = PRESSURE_BINS
        .iter()
        .map(|&(lo, hi)| HistogramBucketDto { lo, hi, count: 0 })
        .collect();

    const LOW_PRESSURE_THRESHOLD: f64 = 14.0; // m
    let mut low_pressure_count = 0u32;

    for &p in node_min_pressure.iter() {
        if p.is_finite() {
            if p < LOW_PRESSURE_THRESHOLD {
                low_pressure_count += 1;
            }
            for bin in &mut pressure_histogram {
                if p >= bin.lo && p < bin.hi {
                    bin.count += 1;
                    break;
                }
            }
        }
    }
    let min_pressure = min_finite_with_index(&node_min_pressure);

    // ── Velocity histogram (same 5 bins as the frontend) ─────────────────────
    const VELOCITY_BINS: &[(f64, f64)] = &[
        (0.0, 0.1),
        (0.1, 0.3),
        (0.3, 0.6),
        (0.6, 1.0),
        (1.0, f64::MAX),
    ];
    let mut velocity_histogram: Vec<HistogramBucketDto> = VELOCITY_BINS
        .iter()
        .map(|&(lo, hi)| HistogramBucketDto { lo, hi, count: 0 })
        .collect();

    for &v in link_max_velocity.iter() {
        for bin in &mut velocity_histogram {
            if v >= bin.lo && v < bin.hi {
                bin.count += 1;
                break;
            }
        }
    }
    let max_velocity = max_positive_with_index(&link_max_velocity);

    // ── Top 5 links by peak velocity ─────────────────────────────────────────
    let mut sorted_link_idxs: Vec<usize> = (0..n_links).collect();
    sorted_link_idxs.sort_unstable_by(|&a, &b| {
        link_max_velocity[b]
            .partial_cmp(&link_max_velocity[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_pipes: Vec<TopPipeDto> = sorted_link_idxs
        .iter()
        .take(5)
        .filter(|&&idx| link_max_velocity[idx] > 0.0)
        .filter_map(|&idx| {
            let link = network.links.get(idx)?;
            let from_id = network
                .nodes
                .get(link.base.from_idx())
                .map(|n| n.base.id.clone())
                .unwrap_or_default();
            let to_id = network
                .nodes
                .get(link.base.to_idx())
                .map(|n| n.base.id.clone())
                .unwrap_or_default();
            let diameter_mm = match &link.kind {
                hydra::LinkKind::Pipe(p) => p.diameter * FT_TO_MM,
                _ => 0.0,
            };
            Some(TopPipeDto {
                id: link.base.id.clone(),
                from_id,
                to_id,
                diameter_mm,
                max_velocity_ms: link_max_velocity[idx],
            })
        })
        .collect();

    // ── Tank head series ──────────────────────────────────────────────────────
    let tank_series: Vec<TankHeadSeriesDto> = network
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.kind, hydra::NodeKind::Tank(_)))
        .filter_map(|(node_idx, n)| {
            // The relative index within the tank block at the end of the node array.
            let ti = node_idx.checked_sub(tank_start)?;
            if ti >= n_tanks {
                return None;
            }
            Some(TankHeadSeriesDto {
                node_id: n.base.id.clone(),
                head: tank_head[ti].clone(),
            })
        })
        .collect();

    // ── Summary values — absent (`None`) when no valid data exists ───────────
    let min_pressure_node_id = min_pressure
        .and_then(|(idx, _)| network.nodes.get(idx))
        .map(|n| n.base.id.clone());
    let min_pressure_m = min_pressure.map(|(_, v)| v);
    let max_velocity_link_id = max_velocity
        .and_then(|(idx, _)| network.links.get(idx))
        .map(|l| l.base.id.clone());
    let max_velocity_ms = max_velocity.map(|(_, v)| v);

    // Convert demand accumulations from ft³/s·period to m³ (multiply by
    // period duration in seconds then by the module-level ft³→m³ factor).
    let report_step_s = meta.report_step;
    let inflow_m3 = total_inflow * report_step_s * FT3_TO_M3;
    let outflow_m3 = total_outflow * report_step_s * FT3_TO_M3;
    let balance_pct = if inflow_m3 > 0.0 {
        (outflow_m3 / inflow_m3 * 100.0).min(100.0)
    } else {
        100.0
    };

    Ok(Some(ResultAnalyticsDto {
        period_count: n_periods as u32,
        node_count: n_nodes as u32,
        link_count: n_links as u32,
        mass_balance: MassBalanceDto {
            inflow_m3,
            outflow_m3,
            balance_pct,
            series: mb_series,
        },
        min_pressure_node_id,
        min_pressure_m,
        low_pressure_count,
        max_velocity_link_id,
        max_velocity_ms,
        pressure_histogram,
        velocity_histogram,
        top_pipes,
        tank_series,
    }))
}

/// Sibling CSV paths for `export_results_csv`: `<base>-nodes.csv` and
/// `<base>-links.csv` next to the user-chosen path (its extension, if any,
/// is replaced).
fn csv_sibling_paths(base: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("results");
    (
        base.with_file_name(format!("{stem}-nodes.csv")),
        base.with_file_name(format!("{stem}-links.csv")),
    )
}

/// Stream every reporting period of `out_path` into two CSV files.
///
/// Node rows: `id,time_s,pressure,head,demand[,quality]`; link rows:
/// `id,time_s,flow,velocity,headloss,status[,quality]` — one row per
/// (element, period), ordered period-major. The quality column is present
/// exactly when the results carry quality data. Values are written in the
/// same SI display units the period-results path returns (the `.out` file is
/// always written with `FlowUnits::Lps`).
///
/// The `.out` file is read one period at a time via the seeking reader and
/// rows go through `BufWriter`s, so memory stays flat regardless of network
/// size or period count.
fn stream_results_csv(
    out_path: &std::path::Path,
    out_meta: &hydra::io::out_reader::OutMetadata,
    node_ids: &[String],
    link_ids: &[String],
    nodes_csv: &std::path::Path,
    links_csv: &std::path::Path,
) -> Result<(), String> {
    use std::io::Write;

    let has_quality = out_meta.quality_flag != 0;
    let open = |p: &std::path::Path| {
        std::fs::File::create(p)
            .map(std::io::BufWriter::new)
            .map_err(|e| format!("Cannot create {}: {e}", p.display()))
    };
    let werr = |e: std::io::Error| format!("Cannot write CSV: {e}");
    let mut nw = open(nodes_csv)?;
    let mut lw = open(links_csv)?;
    let quality_col = if has_quality { ",quality" } else { "" };
    writeln!(nw, "id,time_s,pressure,head,demand{quality_col}").map_err(werr)?;
    writeln!(lw, "id,time_s,flow,velocity,headloss,status{quality_col}").map_err(werr)?;

    let times = out_meta.snapshot_times();
    for (period, &time) in times.iter().enumerate() {
        let pr = hydra::io::out_reader::read_period(out_path, out_meta, period)?;
        let t = time as u64;
        for (i, id) in node_ids.iter().enumerate() {
            write!(
                nw,
                "{id},{t},{},{},{}",
                pr.node_pressure[i], pr.node_head[i], pr.node_demand[i]
            )
            .map_err(werr)?;
            if has_quality {
                write!(nw, ",{}", pr.node_quality[i]).map_err(werr)?;
            }
            writeln!(nw).map_err(werr)?;
        }
        for (i, id) in link_ids.iter().enumerate() {
            write!(
                lw,
                "{id},{t},{},{},{},{}",
                pr.link_flow[i], pr.link_velocity[i], pr.link_headloss[i], pr.link_status[i]
            )
            .map_err(werr)?;
            if has_quality {
                write!(lw, ",{}", pr.link_quality[i]).map_err(werr)?;
            }
            writeln!(lw).map_err(werr)?;
        }
    }
    nw.flush().map_err(werr)?;
    lw.flush().map_err(werr)?;
    Ok(())
}

/// Export the target's simulation results as CSV files via a native save
/// dialog. The chosen path is used as a base name: `<base>-nodes.csv` and
/// `<base>-links.csv` are written next to it (see [`stream_results_csv`] for
/// the row layout). Returns `Ok(Some(base-path))` on success, `Ok(None)` when
/// the user cancels, and an error when no results exist for the target.
#[tauri::command]
/// Export node and link result series to two CSV files via a save dialog.
pub async fn export_results_csv(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !out_path.exists() {
        return Err(
            "No simulation results exist for this target — run a simulation first".to_string(),
        );
    }
    let out_meta =
        hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
    let network = network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    if network.nodes.len() != out_meta.n_nodes || network.links.len() != out_meta.n_links {
        return Err(format!(
            "results.out does not match the current model ({} nodes / {} links in results, \
             {} / {} in the model) — re-run the simulation before exporting",
            out_meta.n_nodes,
            out_meta.n_links,
            network.nodes.len(),
            network.links.len(),
        ));
    }
    let node_ids: Vec<String> = network.nodes.iter().map(|n| n.base.id.clone()).collect();
    let link_ids: Vec<String> = network.links.iter().map(|l| l.base.id.clone()).collect();

    let default_name = meta::read_project_meta(&bundle::project_dir(&app_data, &project_id))
        .map(|m| format!("{}-results.csv", m.name))
        .unwrap_or_else(|_| "results.csv".to_string());

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("CSV", &["csv"])
            .set_file_name(default_name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;

    let file_path = match picked {
        Some(p) => p,
        None => return Ok(None), // user cancelled
    };
    let base_path = file_path.into_path().map_err(|e| e.to_string())?;
    let (nodes_csv, links_csv) = csv_sibling_paths(&base_path);

    // Streaming a large result set is heavy IO — keep it off the async pool.
    tauri::async_runtime::spawn_blocking(move || {
        stream_results_csv(
            &out_path, &out_meta, &node_ids, &link_ids, &nodes_csv, &links_csv,
        )
    })
    .await
    .map_err(|e| format!("CSV export task panicked: {e}"))??;

    Ok(Some(base_path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::simulation::run_sim_loops;
    use crate::commands::test_fixtures::{loaded_sim, loaded_state, TEST_INP};

    // ── network_for_target cache/dirty decision ───────────────────────────

    /// TEST_INP plus one extra junction (`J2`) — distinguishable from the
    /// cached parse (3 nodes) by node count, so tests can tell whether the
    /// returned network came from the cache or from disk.
    const DISK_INP: &str = "\
[JUNCTIONS]
J1  10  5
J2  12  3

[RESERVOIRS]
R1  100

[TANKS]
T1  50  10  5  20  40  0

[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  T1  800   10  100  0  Open
P3  J1  J2  500   8   100  0  Open

[COORDINATES]
J1  1.0  2.0
J2  3.0  2.0
R1  0.0  0.0
T1  2.0  2.0

[OPTIONS]
Units  GPM

[TIMES]
Duration  0

[END]
";

    #[test]
    fn network_for_target_uses_cache_when_clean_and_matching() {
        // No model.inp on disk at all: a cache hit is the only way this
        // call can succeed, so success proves the cache was used.
        let dir = tempfile::tempdir().unwrap();
        let state = NetworkState(parking_lot::Mutex::new(loaded_state()));
        let net = network_for_target(dir.path(), &state, "test-project", None)
            .expect("clean matching cache must be served without disk IO");
        assert_eq!(net.nodes.len(), 3);
    }

    #[test]
    fn network_for_target_refuses_dirty_cache_and_reparses_disk() {
        let dir = tempfile::tempdir().unwrap();
        bundle::atomic_write(
            &bundle::base_model_path(dir.path(), "test-project"),
            DISK_INP.as_bytes(),
        )
        .unwrap();

        let mut inner = loaded_state();
        if let NetworkStateInner::Loaded { dirty, .. } = &mut inner {
            *dirty = true;
        }
        let state = NetworkState(parking_lot::Mutex::new(inner));

        // Same (project, scenario) target as the cache — but the cache is
        // dirty, so the on-disk model (4 nodes) must be parsed instead of
        // returning the 3-node cached network.
        let net = network_for_target(dir.path(), &state, "test-project", None).unwrap();
        assert_eq!(net.nodes.len(), 4);
        assert!(net.nodes.iter().any(|n| n.base.id == "J2"));
    }

    #[test]
    fn network_for_target_ignores_cache_for_non_matching_target() {
        let dir = tempfile::tempdir().unwrap();
        bundle::atomic_write(
            &bundle::scenario_model_path(dir.path(), "test-project", "s1"),
            DISK_INP.as_bytes(),
        )
        .unwrap();

        // Cache owner is (test-project, base); requesting scenario "s1" must
        // hit the scenario model on disk even though the cache is clean.
        let state = NetworkState(parking_lot::Mutex::new(loaded_state()));
        let net = network_for_target(dir.path(), &state, "test-project", Some("s1")).unwrap();
        assert_eq!(net.nodes.len(), 4);
    }

    #[test]
    fn network_for_target_dirty_cache_with_missing_disk_model_errors() {
        // A dirty cache must never be served, even when the disk fallback
        // fails — otherwise results would be indexed against unsaved edits.
        let dir = tempfile::tempdir().unwrap();
        let mut inner = loaded_state();
        if let NetworkStateInner::Loaded { dirty, .. } = &mut inner {
            *dirty = true;
        }
        let state = NetworkState(parking_lot::Mutex::new(inner));
        let err = network_for_target(dir.path(), &state, "test-project", None).unwrap_err();
        assert!(err.contains("Cannot read model"));
    }

    // ── network digest (get_network_digest / load_result_meta wiring) ─────

    #[test]
    fn digest_hex_is_16_lowercase_zero_padded_chars() {
        assert_eq!(digest_hex(0), "0000000000000000");
        assert_eq!(digest_hex(0xABC), "0000000000000abc");
        assert_eq!(digest_hex(u64::MAX), "ffffffffffffffff");
        assert_eq!(digest_hex(0x451f_672d_2d21_a3c4).len(), 16);
    }

    #[test]
    fn live_network_digest_uses_clean_matching_cache() {
        // No model.inp on disk: success proves the cache was served.
        let dir = tempfile::tempdir().unwrap();
        let state = NetworkState(parking_lot::Mutex::new(loaded_state()));
        let digest = live_network_digest(dir.path(), &state, "test-project", None)
            .expect("matching cache must be served without disk IO");
        let expected =
            hydra::compute_network_digest(&hydra::io::parse(TEST_INP.as_bytes()).unwrap());
        assert_eq!(digest, expected);
    }

    #[test]
    fn live_network_digest_prefers_dirty_cache_and_reflects_added_node() {
        // Opposite policy to network_for_target: the dirty in-memory network
        // IS the digest source (unsaved topology edits must be detectable).
        // No model.inp on disk, so any disk fallback would error.
        let dir = tempfile::tempdir().unwrap();
        let baseline =
            hydra::compute_network_digest(&hydra::io::parse(TEST_INP.as_bytes()).unwrap());

        let mut inner = loaded_state();
        if let NetworkStateInner::Loaded { network, dirty, .. } = &mut inner {
            // Add a junction the way create_node does (id + topology change).
            let mut node = network.nodes[0].clone();
            node.base.id = "J-NEW".into();
            node.base.index = network.nodes.len() + 1;
            network.nodes.push(node);
            *dirty = true;
        }
        let state = NetworkState(parking_lot::Mutex::new(inner));

        let digest = live_network_digest(dir.path(), &state, "test-project", None)
            .expect("dirty matching cache must be served without disk IO");
        assert_ne!(
            digest, baseline,
            "digest must reflect the unsaved added node"
        );
    }

    #[test]
    fn live_network_digest_falls_back_to_disk_for_other_target() {
        let dir = tempfile::tempdir().unwrap();
        bundle::atomic_write(
            &bundle::scenario_model_path(dir.path(), "test-project", "s1"),
            DISK_INP.as_bytes(),
        )
        .unwrap();
        let state = NetworkState(parking_lot::Mutex::new(loaded_state()));
        let digest = live_network_digest(dir.path(), &state, "test-project", Some("s1")).unwrap();
        let expected =
            hydra::compute_network_digest(&hydra::io::parse(DISK_INP.as_bytes()).unwrap());
        assert_eq!(digest, expected);
    }

    #[test]
    fn result_meta_dto_network_digest_wire_contract() {
        // None (pre-digest .out file) must serialise with the field absent —
        // the frontend then treats the topology match as unknown (no gating).
        let dto = |d: Option<String>| ResultMetaDto {
            times: vec![],
            quality_mode: "none".into(),
            network_digest: d,
            ranges: ResultRangesDto {
                pressure_min: 0.0,
                pressure_max: 0.0,
                head_min: 0.0,
                head_max: 0.0,
                demand_min: 0.0,
                demand_max: 0.0,
                flow_min: 0.0,
                flow_max: 0.0,
                velocity_min: 0.0,
                velocity_max: 0.0,
                quality_min: None,
                quality_max: None,
            },
        };
        let json = serde_json::to_string(&dto(None)).unwrap();
        assert!(!json.contains("networkDigest"), "got: {json}");
        let json = serde_json::to_string(&dto(Some(digest_hex(0xABC)))).unwrap();
        assert!(
            json.contains("\"networkDigest\":\"0000000000000abc\""),
            "got: {json}"
        );
    }

    #[test]
    fn generated_results_carry_the_fixture_network_digest() {
        // End-to-end: a results.out produced by the streaming run path stores
        // the digest of the network it was run from, and load_result_meta's
        // mapping (meta.network_digest → hex) matches get_network_digest's
        // view of the same unedited model.
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();
        let expected =
            hydra::compute_network_digest(&hydra::io::parse(TEST_INP.as_bytes()).unwrap());
        assert_eq!(meta.network_digest, Some(expected));
        assert_eq!(
            meta.network_digest.map(digest_hex),
            Some(digest_hex(expected))
        );
    }

    // ── get_element_series / element_series_from_out ──────────────────────

    /// Generate a real `results.out` from `TEST_INP` via the same streaming
    /// path production uses.
    fn generated_results_out(dir: &std::path::Path) -> std::path::PathBuf {
        let out = dir.join("results.out");
        let (_sim, err, _wall, _steps) = run_sim_loops(
            loaded_sim(),
            Some(out.clone()),
            0.0,
            false,
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "fixture run must succeed: {err:?}");
        out
    }

    #[test]
    fn element_series_from_out_matches_period_reader() {
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let out_meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();
        assert!(out_meta.n_periods >= 1);

        // Node series: fields in wire order, one value per period, values
        // identical to what the period reader returns.
        let series = element_series_from_out(&out, "node", 0).unwrap();
        let names: Vec<&str> = series.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["pressure", "head", "demand"], "no quality run");
        assert_eq!(series.times.len(), out_meta.n_periods);
        for f in &series.fields {
            assert_eq!(f.values.len(), out_meta.n_periods);
        }
        let pr0 = hydra::io::out_reader::read_period(&out, &out_meta, 0).unwrap();
        assert_eq!(series.fields[0].values[0], pr0.node_pressure[0] as f64);
        assert_eq!(series.fields[1].values[0], pr0.node_head[0] as f64);
        assert_eq!(series.fields[2].values[0], pr0.node_demand[0] as f64);
        assert_eq!(series.times[0], out_meta.report_start as u32);

        // Link series.
        let series = element_series_from_out(&out, "link", 1).unwrap();
        let names: Vec<&str> = series.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["flow", "velocity", "headloss", "status"]);
        assert_eq!(series.fields[0].values[0], pr0.link_flow[1] as f64);
        assert_eq!(series.fields[3].values[0], pr0.link_status[1] as f64);
    }

    #[test]
    fn element_series_from_out_bounds_and_kind_errors() {
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let out_meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();

        let err = element_series_from_out(&out, "node", out_meta.n_nodes as u32).unwrap_err();
        assert!(err.contains("out of range"), "unexpected error: {err}");
        let err = element_series_from_out(&out, "link", out_meta.n_links as u32).unwrap_err();
        assert!(err.contains("out of range"), "unexpected error: {err}");
        let err = element_series_from_out(&out, "pipe", 0).unwrap_err();
        assert!(err.contains("unknown element kind"), "unexpected: {err}");
    }

    // ── export_results_csv streaming ──────────────────────────────────────

    #[test]
    fn stream_results_csv_writes_wide_per_field_rows() {
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let out_meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();

        let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        let node_ids: Vec<String> = network.nodes.iter().map(|n| n.base.id.clone()).collect();
        let link_ids: Vec<String> = network.links.iter().map(|l| l.base.id.clone()).collect();
        assert_eq!(node_ids.len(), out_meta.n_nodes);
        assert_eq!(link_ids.len(), out_meta.n_links);

        let (nodes_csv, links_csv) = csv_sibling_paths(&dir.path().join("export.csv"));
        assert!(nodes_csv.ends_with("export-nodes.csv"));
        assert!(links_csv.ends_with("export-links.csv"));
        stream_results_csv(
            &out, &out_meta, &node_ids, &link_ids, &nodes_csv, &links_csv,
        )
        .unwrap();

        let nodes = std::fs::read_to_string(&nodes_csv).unwrap();
        let mut lines = nodes.lines();
        assert_eq!(lines.next().unwrap(), "id,time_s,pressure,head,demand");
        // One row per (node, period).
        assert_eq!(lines.count(), out_meta.n_nodes * out_meta.n_periods);
        let pr0 = hydra::io::out_reader::read_period(&out, &out_meta, 0).unwrap();
        let first = nodes.lines().nth(1).unwrap();
        assert_eq!(
            first,
            format!(
                "{},0,{},{},{}",
                node_ids[0], pr0.node_pressure[0], pr0.node_head[0], pr0.node_demand[0]
            )
        );

        let links = std::fs::read_to_string(&links_csv).unwrap();
        let mut lines = links.lines();
        assert_eq!(
            lines.next().unwrap(),
            "id,time_s,flow,velocity,headloss,status"
        );
        assert_eq!(lines.count(), out_meta.n_links * out_meta.n_periods);
        let first = links.lines().nth(1).unwrap();
        assert_eq!(
            first,
            format!(
                "{},0,{},{},{},{}",
                link_ids[0],
                pr0.link_flow[0],
                pr0.link_velocity[0],
                pr0.link_headloss[0],
                pr0.link_status[0]
            )
        );
    }

    // ── pump energy totals ────────────────────────────────────────────────

    #[test]
    fn energy_totals_invert_out_writer_normalisations() {
        // 24 h run: pump online 50% of the time at an average 10 kW.
        let (kwh, cost) = energy_totals_from_record(10.0, 50.0, 3.6, 86_400.0);
        assert!((kwh - 120.0).abs() < 1e-9, "10 kW × 12 h, got {kwh}");
        assert!((cost - 3.6).abs() < 1e-9, "one day at 3.6/day, got {cost}");

        // 12 h run: avg_cost_per_day is normalised per day, so half a day
        // of it is charged.
        let (kwh, cost) = energy_totals_from_record(10.0, 100.0, 4.8, 43_200.0);
        assert!((kwh - 120.0).abs() < 1e-9);
        assert!((cost - 2.4).abs() < 1e-9);

        // Steady state (duration 0): EPANET's synthetic 1-hour horizon.
        let (kwh, cost) = energy_totals_from_record(10.0, 100.0, 24.0, 0.0);
        assert!((kwh - 10.0).abs() < 1e-9, "1 h at 10 kW, got {kwh}");
        assert!((cost - 1.0).abs() < 1e-9, "avg_cost/24, got {cost}");
    }

    #[test]
    fn network_has_energy_price_checks_global_and_per_pump() {
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        assert!(!network_has_energy_price(&network));
        network.options.energy_price = 0.12;
        assert!(network_has_energy_price(&network));
    }

    // ── analytics absent markers ──────────────────────────────────────────

    #[test]
    fn min_finite_with_index_ignores_non_finite_and_keeps_first_tie() {
        assert_eq!(min_finite_with_index(&[]), None);
        assert_eq!(
            min_finite_with_index(&[f64::INFINITY, f64::NAN, f64::NEG_INFINITY]),
            None
        );
        assert_eq!(
            min_finite_with_index(&[f64::INFINITY, 3.0, -2.0, f64::NAN]),
            Some((2, -2.0))
        );
        // Equal minima: the first index wins (matches the previous loop).
        assert_eq!(min_finite_with_index(&[5.0, 1.0, 1.0]), Some((1, 1.0)));
    }

    #[test]
    fn max_positive_with_index_treats_all_zero_as_absent() {
        assert_eq!(max_positive_with_index(&[]), None);
        // 0.0 is the scan's "no data" default — an all-zero array means no
        // link ever had a valid velocity.
        assert_eq!(max_positive_with_index(&[0.0, 0.0]), None);
        assert_eq!(max_positive_with_index(&[f64::NAN, -1.0]), None);
        assert_eq!(max_positive_with_index(&[0.0, 2.5, 1.0]), Some((1, 2.5)));
        // Equal maxima: the first index wins.
        assert_eq!(max_positive_with_index(&[3.0, 3.0]), Some((0, 3.0)));
    }

    fn analytics_dto_with_summary(
        min_pressure: Option<(String, f64)>,
        max_velocity: Option<(String, f64)>,
    ) -> ResultAnalyticsDto {
        let (min_pressure_node_id, min_pressure_m) = match min_pressure {
            Some((id, v)) => (Some(id), Some(v)),
            None => (None, None),
        };
        let (max_velocity_link_id, max_velocity_ms) = match max_velocity {
            Some((id, v)) => (Some(id), Some(v)),
            None => (None, None),
        };
        ResultAnalyticsDto {
            period_count: 0,
            node_count: 0,
            link_count: 0,
            mass_balance: MassBalanceDto {
                inflow_m3: 0.0,
                outflow_m3: 0.0,
                balance_pct: 100.0,
                series: vec![],
            },
            min_pressure_node_id,
            min_pressure_m,
            low_pressure_count: 0,
            max_velocity_link_id,
            max_velocity_ms,
            pressure_histogram: vec![],
            velocity_histogram: vec![],
            top_pipes: vec![],
            tank_series: vec![],
        }
    }

    #[test]
    fn analytics_dto_omits_summary_fields_when_no_valid_data() {
        // Wire contract with the frontend: absent field = no data (render "—").
        let json = serde_json::to_string(&analytics_dto_with_summary(None, None)).unwrap();
        assert!(!json.contains("minPressureNodeId"), "got: {json}");
        assert!(!json.contains("minPressureM"), "got: {json}");
        assert!(!json.contains("maxVelocityLinkId"), "got: {json}");
        assert!(!json.contains("maxVelocityMs"), "got: {json}");
        assert!(!json.contains("null"), "no null placeholders: {json}");

        let json = serde_json::to_string(&analytics_dto_with_summary(
            Some(("J1".into(), 12.5)),
            Some(("P1".into(), 1.75)),
        ))
        .unwrap();
        assert!(json.contains("\"minPressureNodeId\":\"J1\""), "got: {json}");
        assert!(json.contains("\"minPressureM\":12.5"), "got: {json}");
        assert!(json.contains("\"maxVelocityLinkId\":\"P1\""), "got: {json}");
        assert!(json.contains("\"maxVelocityMs\":1.75"), "got: {json}");
    }
}
