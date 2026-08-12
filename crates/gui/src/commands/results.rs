//! Result-reading commands over `results.out`: metadata + ranges, per-period
//! arrays, element series, cross-period analytics, pump energy, and CSV export.

use serde::{Deserialize, Serialize};

use crate::meta::{self, bundle};

use super::binary_codec::encode_period_results;
use super::network_dto::{format_read_error, NetworkState, NetworkStateInner};
use super::projects::{
    app_data_dir, model_path_for, read_model_bytes, results_path_for, validate_target_ids,
};

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
    let duration_secs = meta.duration;
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

/// Global min/max ranges for the common result variables across all periods.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
/// Serialize-only: the frontend is the sole consumer, and the embedded §5
/// quantity descriptors are `&'static str`-backed engine constants that
/// cannot be deserialized.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultMetaDto {
    pub times: Vec<f64>,
    /// Whether `get_period_results` can serve per-period arrays for this
    /// target. False for engines whose period serving has not landed —
    /// the timeline steps, but the canvas stays uncoloured.
    pub has_period_data: bool,
    pub ranges: ResultRangesDto,
    /// Quality mode used in the simulation: `"none"`, `"chemical"`, `"age"`, or `"trace"`.
    pub quality_mode: String,
    /// Topology digest of the network the results were produced from, as
    /// 16 lowercase hex chars (see the engine's `compute_network_digest`).
    /// `None` for pre-digest `.out` files — the frontend must then treat the
    /// topology match as unknown and apply no staleness gating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_digest: Option<String>,
    /// Engine-described variable catalog with per-run ranges. Every engine
    /// publishes one (§6), and both serve it here — it is what lets a
    /// single legend render either engine's results.
    ///
    /// wds additionally fills the fixed `ranges` above, which predate the
    /// catalog contract and still feed its canvas colouring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic: Option<super::generic_results::GenericResultMetaDto>,
    /// Whether this target's per-period values are served in the generic
    /// variable-major payload (`get_generic_period_values`) rather than the
    /// fixed wds arrays (`get_period_results`).
    ///
    /// Deliberately independent of `generic`: publishing a catalog is a
    /// statement about what the results *contain*, and this is a statement
    /// about how they are *encoded*. Conflating them routed wds onto a
    /// payload nothing serves for it, and its canvas silently fell back to
    /// the network-at-rest palette — results present, everything grey, no
    /// error anywhere.
    pub generic_periods: bool,
}

/// The §5 quantity descriptor for a wds catalog quantity key.
pub(crate) fn wds_quantity(key: &str) -> Option<hydra::common::QuantityDescriptor> {
    hydra::descriptors::QUANTITIES
        .iter()
        .find(|q| q.key == key)
        .copied()
}

/// The per-run range for one declared wds variable, or `None` when this run
/// carries no values for it.
///
/// `results.out` is always written with `FlowUnits::Lps`, and the wds
/// quantity catalog declares L/s as flow's own SI label — so the scanned
/// ranges are already in each variable's declared SI unit and reach the
/// frontend unscaled, which converts them for display like any other
/// catalog value.
fn wds_range(id: &str, ranges: &hydra::io::out_reader::ResultRanges) -> Option<(f64, f64)> {
    Some(match id {
        "pressure" => (ranges.pressure_min, ranges.pressure_max),
        "head" => (ranges.head_min, ranges.head_max),
        "demand" => (ranges.demand_min, ranges.demand_max),
        "flow" => (ranges.flow_min, ranges.flow_max),
        "velocity" => (ranges.velocity_min, ranges.velocity_max),
        "headloss" => (ranges.headloss_min, ranges.headloss_max),
        // Present only when the run simulated quality; absent leaves the
        // variable out of the catalog entirely rather than showing an
        // empty ramp the user cannot fill.
        "quality" => (ranges.quality_min?, ranges.quality_max?),
        // A categorical variable's states come from the hint, not a range.
        "status" => (0.0, 0.0),
        _ => return None,
    })
}

/// Build the wds result catalog for one run: every variable the engine
/// declares (§6) that this run actually carries, with its scanned range.
///
/// The engine has always published this catalog; serving it here is what
/// lets one legend component render either engine, instead of the frontend
/// re-declaring wds's variable list by hand and drifting from it.
fn wds_generic_meta(
    ranges: &hydra::io::out_reader::ResultRanges,
) -> super::generic_results::GenericResultMetaDto {
    use super::generic_results::{GenericResultMetaDto, GenericVariableDto};
    use hydra::common::ElementClass;

    let class_vars = |class| {
        hydra::descriptors::result_variables(class)
            .into_iter()
            .filter_map(|v| {
                let (min, max) = wds_range(v.id, ranges)?;
                Some(GenericVariableDto::from_descriptor(
                    &v,
                    min,
                    max,
                    wds_quantity,
                ))
            })
            .collect()
    };
    GenericResultMetaDto {
        point_vars: class_vars(ElementClass::Point),
        polyline_vars: class_vars(ElementClass::Polyline),
        region_vars: Vec::new(),
    }
}

/// Format a topology digest as the wire representation shared with the
/// frontend: 16 lowercase hex characters, zero-padded.
pub(crate) fn digest_hex(digest: u64) -> String {
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
    // Engine-dispatched: each engine's reader serves its own results file.
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "wds" => {}
        "uds" => {
            let meta = hydra::uds::io::out_reader::read_metadata(&out_path)?;
            // Sim-relative period instants. The stored clock carries absolute
            // record times; the standard case reports one step after start,
            // which (i+1)·step reproduces.
            let step = meta.report_step_s as f64;
            let times: Vec<f64> = (0..meta.n_periods)
                .map(|i| (i as f64 + 1.0) * step)
                .collect();
            let generic = super::uds_results::generic_meta(&out_path, &meta)?;
            return Ok(Some(ResultMetaDto {
                times,
                has_period_data: true,
                quality_mode: "none".to_string(),
                network_digest: None,
                // The wds-shaped fixed ranges stay empty; the canvas reads
                // the per-variable ranges from `generic` instead.
                ranges: ResultRangesDto::default(),
                generic: Some(generic),
                generic_periods: true,
            }));
        }
        _ => return Ok(None),
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
        has_period_data: true,
        quality_mode: quality_mode.to_string(),
        // From `run.json` beside the results: `results.out` is EPANET's
        // format and carries none of Hydra's fields (model spec §4.4.1).
        // Absent is "unknown", which the frontend treats as no staleness
        // gating rather than as stale.
        network_digest: crate::commands::simulation::read_run_meta(&out_path)
            .and_then(|run| run.network_digest),
        generic: Some(wds_generic_meta(&ranges)),
        // wds serves the fixed arrays; only its catalog is generic.
        generic_periods: false,
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
/// written with `FlowUnits::Lps`. Returns an **empty** payload when
/// `results.out` does not exist (the target has not been simulated) — the
/// frontend reads a zero-length buffer as "no results". Returns an error only
/// when a present `.out` is unreadable or `period` is out of range.
#[tauri::command(async)]
/// Return flat arrays for a single reporting period (nodes + links).
pub fn get_period_results(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    period: usize,
    scenario_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !out_path.exists() {
        // Not simulated yet — return an empty payload rather than a hard error
        // (mirrors `load_result_meta`'s `Ok(None)`). A view can legitimately
        // fetch during the window where the active scenario switched but its
        // result metadata has not yet been reloaded; that must not raise a
        // scary "missing .out" backend-error toast.
        return Ok(tauri::ipc::Response::new(Vec::new()));
    }
    // Engine-dispatched: each engine's reader serves its own results file,
    // in its own payload layout (the frontend picks the decoder from
    // `load_result_meta`'s answer).
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "wds" => {}
        "uds" => {
            let meta = hydra::uds::io::out_reader::read_metadata(&out_path)?;
            let rec = hydra::uds::io::out_reader::read_period(&out_path, &meta, period)?;
            // Snapshot order comes from the same view build the canvas
            // rendered, so values line up positionally with v4 indices.
            let network =
                uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            let view = super::uds_view::build_view(&network);
            return Ok(tauri::ipc::Response::new(
                super::uds_results::encode_generic_period(&view, &meta, &rec),
            ));
        }
        // Engines without a period provider serve the empty "no results"
        // payload rather than handing their file to the wrong reader.
        _ => return Ok(tauri::ipc::Response::new(Vec::new())),
    }
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
/// The uds counterpart of [`network_for_target`]: the cached parse when the
/// state holds exactly this target, otherwise a fresh parse from disk.
pub(crate) fn uds_network_for_target(
    app_data: &std::path::Path,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<std::sync::Arc<hydra::uds::model::Network>, String> {
    {
        let guard = state.0.lock();
        if let NetworkStateInner::LoadedUds {
            network,
            owner_project_id: Some(owner),
            owner_scenario_id,
            ..
        } = &*guard
        {
            if owner == project_id && owner_scenario_id.as_deref() == scenario_id {
                return Ok(network.clone());
            }
        }
    }
    let model_path = model_path_for(app_data, project_id, scenario_id);
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    let text = String::from_utf8_lossy(&raw);
    let (network, diags) = hydra::uds::io::objects::parse_network(&text);
    if let Some(first) = diags.iter().find(|d| d.kind.is_error()) {
        return Err(format!("Cannot read model: {first}"));
    }
    Ok(std::sync::Arc::new(network))
}

pub(crate) fn network_for_target(
    app_data: &std::path::Path,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<std::sync::Arc<hydra::Network>, String> {
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
    // Tolerant (model spec §4.1.2): a network under construction is not
    // simulable, and this is a read-only inspection — failing here surfaced a
    // "Backend error" toast for a model the editor is perfectly able to show.
    // Runs still use the strict parse, so nothing unsimulable reaches the solver.
    let (network, _validation_errors) =
        hydra::io::parse_tolerant(&raw).map_err(format_read_error)?;
    Ok(std::sync::Arc::new(network))
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
///
/// Returns `Ok(None)` when the target has no model on disk yet (a project
/// created without importing one): there is no topology to fingerprint, and
/// no results for it to disagree with either.
fn live_network_digest(
    app_data: &std::path::Path,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<Option<u64>, String> {
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
                return Ok(Some(hydra::compute_network_digest(network)));
            }
        }
    }
    let model_path = model_path_for(app_data, project_id, scenario_id);
    let Some(raw) = read_model_bytes(&model_path)? else {
        return Ok(None);
    };
    // Tolerant (model spec §4.1.2): a network under construction is not
    // simulable, and this is a read-only inspection — failing here surfaced a
    // "Backend error" toast for a model the editor is perfectly able to show.
    // Runs still use the strict parse, so nothing unsimulable reaches the solver.
    let (network, _validation_errors) =
        hydra::io::parse_tolerant(&raw).map_err(format_read_error)?;
    Ok(Some(hydra::compute_network_digest(&network)))
}

/// Return the topology digest of the current model for a project or scenario
/// as 16 lowercase hex chars — including unsaved in-memory edits when the
/// managed network cache holds that target (see [`live_network_digest`]).
/// The frontend compares this against `ResultMetaDto::network_digest` to
/// detect results that predate the live topology.
///
/// `null` when the target has no model yet — the frontend already treats an
/// absent digest as "unknown" and simply skips the staleness comparison.
#[tauri::command(async)]
/// Return the current model's topology digest (hex) for a project/scenario.
pub fn get_network_digest(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<String>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    // Only the wds model has a digest today; "no digest" is an ordinary
    // answer (results freshness reads as unknown), not an error.
    if super::projects::project_engine_key(&app_data, &project_id) != "wds" {
        return Ok(None);
    }
    Ok(
        live_network_digest(&app_data, &state, &project_id, scenario_id.as_deref())?
            .map(digest_hex),
    )
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
    // wds-shaped results reading; other engines' results arrive with their
    // own provider (registry pattern). "No results" is the honest interim
    // answer — never a foreign-dialect or corrupt-file error.
    if super::projects::project_engine_key(&app_data, &project_id) != "wds" {
        return Ok(Vec::new());
    }
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

/// Field order presented to the frontend, per element kind. `quality` is
/// dropped when the run carried no quality data.
const NODE_FIELDS: &[&str] = &["pressure", "head", "demand", "quality"];
const LINK_FIELDS: &[&str] = &["flow", "velocity", "headloss", "status", "quality"];

/// Build the per-element time series by addressing each value directly in the
/// `.out` file (`read_element_series`; model spec §4.4.8).
///
/// `kind` is `"node"` or `"link"`; `index` is the element's network-order
/// index (0-based), bounds-checked against the result file's counts. Values
/// are returned exactly as stored in `results.out` — the same SI display
/// units (m, L/s, m/s) the `get_period_results` path returns, because the
/// file is always written with `FlowUnits::Lps` (no conversion needed).
///
/// The previous implementation called `read_period` per period, which
/// materialises every node and link array to keep one column: selecting an
/// element on a 46k-element network over a week-long run read hundreds of
/// megabytes to produce a few kilobytes of series.
fn element_series_from_out(
    out_path: &std::path::Path,
    kind: &str,
    index: u32,
) -> Result<Option<SeriesDto>, String> {
    use hydra::io::out_reader::ElementKind;

    let meta = hydra::io::out_reader::read_metadata_checked(out_path).map_err(|e| e.to_string())?;
    let idx = index as usize;
    let has_quality = meta.quality_flag != 0;

    let (element_kind, count, wire_fields) = match kind {
        "node" => (ElementKind::Node, meta.n_nodes, NODE_FIELDS),
        "link" => (ElementKind::Link, meta.n_links, LINK_FIELDS),
        other => {
            return Err(format!(
                "unknown element kind {other:?}: expected \"node\" or \"link\""
            ))
        }
    };
    // An element the results do not reach is the same answer as a run
    // that has not happened: these results hold nothing for it. It is a
    // normal state, not a failure — results describe the model as it was
    // when it ran, and any element added since is beyond them. Reported
    // as an error, it surfaced as a toast when a newly added element was
    // selected, and again when it was deleted and something asked once
    // more before noticing.
    if idx >= count {
        return Ok(None);
    }

    let series = hydra::io::out_reader::read_element_series(out_path, &meta, element_kind, idx)?;
    let column = |name: &str| -> Option<Vec<f64>> {
        series
            .series
            .iter()
            .find(|s| s.variable == name)
            .map(|s| s.values.iter().map(|&v| v as f64).collect())
    };

    // Present the frontend's field order, dropping quality when absent. The
    // engine returns every variable the file holds, in file order.
    let fields = wire_fields
        .iter()
        .filter(|name| has_quality || **name != "quality")
        .filter_map(|&name| {
            Some(SeriesFieldDto {
                name: name.to_string(),
                values: column(name)?,
            })
        })
        .collect();

    Ok(Some(SeriesDto {
        times: series.times.iter().map(|&t| t as u32).collect(),
        fields,
    }))
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
    state: tauri::State<'_, NetworkState>,
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
    // Engine-dispatched: each engine's reader serves its own series. Field
    // names are variable ids — wds's fixed set or the uds §6 catalog's.
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "wds" => element_series_from_out(&out_path, &kind, index),
        "uds" => {
            let network =
                uds_network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            super::uds_results::element_series(&out_path, &network, &kind, index as usize)
        }
        _ => Ok(None),
    }
}

/// Sibling CSV paths for `export_results_csv`: `<base>-nodes.csv` and
/// `<base>-links.csv` next to the user-chosen path (its extension, if any,
/// is replaced).
fn csv_sibling_paths(
    base: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("results");
    (
        base.with_file_name(format!("{stem}-nodes.csv")),
        base.with_file_name(format!("{stem}-links.csv")),
        // Written only by engines with areal elements (uds subcatchments).
        base.with_file_name(format!("{stem}-subcatchments.csv")),
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
    // Engine-dispatched: prepare a streaming job for this engine's reader,
    // then share the dialog + blocking-write plumbing below.
    type CsvJob = Box<
        dyn FnOnce(&std::path::Path, &std::path::Path, &std::path::Path) -> Result<(), String>
            + Send,
    >;
    let job: CsvJob = match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "wds" => {
            let out_meta = hydra::io::out_reader::read_metadata_checked(&out_path)
                .map_err(|e| e.to_string())?;
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
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
            let out_path = out_path.clone();
            Box::new(move |nodes_csv, links_csv, _subs_csv| {
                stream_results_csv(
                    &out_path, &out_meta, &node_ids, &link_ids, nodes_csv, links_csv,
                )
            })
        }
        "uds" => {
            let meta = hydra::uds::io::out_reader::read_metadata(&out_path)?;
            let out_path = out_path.clone();
            Box::new(move |nodes_csv, links_csv, subs_csv| {
                super::uds_results::stream_uds_results_csv(
                    &out_path, &meta, nodes_csv, links_csv, subs_csv,
                )
            })
        }
        _ => {
            return Err("Results export is not available for this project's engine yet".to_string())
        }
    };

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
    let (nodes_csv, links_csv, subs_csv) = csv_sibling_paths(&base_path);

    // Streaming a large result set is heavy IO — keep it off the async pool.
    tauri::async_runtime::spawn_blocking(move || job(&nodes_csv, &links_csv, &subs_csv))
        .await
        .map_err(|e| format!("CSV export task panicked: {e}"))??;

    Ok(Some(base_path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::simulation::run_sim_loops;
    use crate::commands::test_fixtures::{loaded_sim, loaded_state, TEST_INP};

    // ── wds result catalog (§6) ───────────────────────────────────────────

    fn ranges_with_quality(quality: Option<(f64, f64)>) -> hydra::io::out_reader::ResultRanges {
        hydra::io::out_reader::ResultRanges {
            pressure_min: 1.0,
            pressure_max: 2.0,
            head_min: 3.0,
            head_max: 4.0,
            demand_min: 5.0,
            demand_max: 6.0,
            flow_min: 7.0,
            flow_max: 8.0,
            velocity_min: 9.0,
            velocity_max: 10.0,
            headloss_min: 11.0,
            headloss_max: 12.0,
            quality_min: quality.map(|q| q.0),
            quality_max: quality.map(|q| q.1),
        }
    }

    /// Every variable the engine declares must reach the catalog with the
    /// range that was actually scanned for it — a mapping that pairs a
    /// variable with a neighbouring range is invisible until someone reads
    /// a legend.
    #[test]
    fn wds_catalog_pairs_each_variable_with_its_own_range() {
        let meta = wds_generic_meta(&ranges_with_quality(Some((0.5, 1.5))));
        let by_id = |vars: &[super::super::generic_results::GenericVariableDto], id: &str| {
            vars.iter()
                .find(|v| v.id == id)
                .map(|v| (v.min, v.max))
                .unwrap_or_else(|| panic!("{id} missing from catalog"))
        };
        assert_eq!(by_id(&meta.point_vars, "pressure"), (1.0, 2.0));
        assert_eq!(by_id(&meta.point_vars, "head"), (3.0, 4.0));
        assert_eq!(by_id(&meta.point_vars, "demand"), (5.0, 6.0));
        assert_eq!(by_id(&meta.point_vars, "quality"), (0.5, 1.5));
        assert_eq!(by_id(&meta.polyline_vars, "flow"), (7.0, 8.0));
        assert_eq!(by_id(&meta.polyline_vars, "velocity"), (9.0, 10.0));
        assert_eq!(by_id(&meta.polyline_vars, "headloss"), (11.0, 12.0));
    }

    /// A run without quality simulation must omit the quality variables
    /// rather than offer a ramp over a range that does not exist.
    #[test]
    fn wds_catalog_omits_quality_when_the_run_had_none() {
        let meta = wds_generic_meta(&ranges_with_quality(None));
        assert!(!meta.point_vars.iter().any(|v| v.id == "quality"));
        assert!(!meta.polyline_vars.iter().any(|v| v.id == "quality"));
        // The rest of the catalog is unaffected.
        assert!(meta.point_vars.iter().any(|v| v.id == "pressure"));
        assert!(meta.polyline_vars.iter().any(|v| v.id == "status"));
    }

    /// Publishing a catalog and serving the generic period payload are two
    /// different claims, and wds makes only the first. Conflating them sent
    /// wds to a payload nothing serves for it: results loaded, the canvas
    /// painted every element in the network-at-rest palette, and no error
    /// surfaced anywhere because the empty fetch was indistinguishable from
    /// "not simulated yet".
    #[test]
    fn wds_publishes_a_catalog_without_claiming_generic_periods() {
        let ranges = ranges_with_quality(None);
        let meta = ResultMetaDto {
            times: vec![],
            has_period_data: true,
            quality_mode: "none".into(),
            network_digest: None,
            generic: Some(wds_generic_meta(&ranges)),
            generic_periods: false,
            ranges: ResultRangesDto::default(),
        };
        let json = serde_json::to_value(&meta).expect("serialisable");
        assert!(
            json["generic"]["polylineVars"]
                .as_array()
                .is_some_and(|v| !v.is_empty()),
            "wds must publish a catalog for the legend"
        );
        assert_eq!(
            json["genericPeriods"], false,
            "wds serves the fixed period arrays, not the generic payload"
        );
    }

    /// wds has no areal elements, so the region class must stay empty —
    /// the legend hides a class with no variables, and a stray entry would
    /// give a water-distribution map a region picker.
    #[test]
    fn wds_catalog_declares_no_region_variables() {
        assert!(wds_generic_meta(&ranges_with_quality(None))
            .region_vars
            .is_empty());
    }

    /// Link status is the one categorical variable; its engine-authored
    /// state labels must survive into the catalog, because a legend cannot
    /// draw discrete swatches without them.
    #[test]
    fn wds_catalog_carries_link_status_states() {
        let meta = wds_generic_meta(&ranges_with_quality(None));
        let status = meta
            .polyline_vars
            .iter()
            .find(|v| v.id == "status")
            .expect("status declared");
        match &status.ramp {
            hydra::common::RampHint::Categorical { items } => {
                assert!(
                    items.iter().any(|i| i.label == "Open"),
                    "expected an Open state, got {items:?}"
                );
            }
            other => panic!("status should be categorical, got {other:?}"),
        }
    }

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
            .expect("matching cache must be served without disk IO")
            .expect("a cached network always has a digest");
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
            let network = std::sync::Arc::make_mut(network);
            // Add a junction the way create_node does (id + topology change).
            let mut node = network.nodes[0].clone();
            node.base.id = "J-NEW".into();
            node.base.index = network.nodes.len() + 1;
            network.nodes.push(node);
            *dirty = true;
        }
        let state = NetworkState(parking_lot::Mutex::new(inner));

        let digest = live_network_digest(dir.path(), &state, "test-project", None)
            .expect("dirty matching cache must be served without disk IO")
            .expect("a cached network always has a digest");
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
        let digest = live_network_digest(dir.path(), &state, "test-project", Some("s1"))
            .unwrap()
            .expect("the on-disk model has a digest");
        let expected =
            hydra::compute_network_digest(&hydra::io::parse(DISK_INP.as_bytes()).unwrap());
        assert_eq!(digest, expected);
    }

    #[test]
    fn live_network_digest_is_absent_for_a_project_with_no_model() {
        // A project created without importing a source model has no
        // model.inp at all. That is its normal resting state, so the digest
        // is "none", not an error — otherwise opening a blank project greets
        // the user with a backend-error toast.
        let dir = tempfile::tempdir().unwrap();
        let state = NetworkState(parking_lot::Mutex::new(NetworkStateInner::Empty));
        let digest = live_network_digest(dir.path(), &state, "blank-project", None)
            .expect("a missing model is not a failure");
        assert!(digest.is_none());
    }

    #[test]
    fn read_model_bytes_still_fails_loudly_on_a_non_notfound_error() {
        // Only NotFound is folded into "no model". A model that exists but
        // cannot be read is a real fault the user needs told about — here a
        // directory standing where the model file should be.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.inp");
        std::fs::create_dir(&path).unwrap();
        assert!(read_model_bytes(&path).is_err());
    }

    #[test]
    fn result_meta_dto_network_digest_wire_contract() {
        // None (pre-digest .out file) must serialise with the field absent —
        // the frontend then treats the topology match as unknown (no gating).
        let dto = |d: Option<String>| ResultMetaDto {
            times: vec![],
            has_period_data: true,
            quality_mode: "none".into(),
            network_digest: d,
            generic: None,
            generic_periods: false,
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
    fn a_run_records_its_digest_beside_the_results_not_inside_them() {
        // End-to-end through the streaming run path: `results.out` stays
        // EPANET's format with nothing of Hydra's in it (model spec §4.4.1),
        // and the digest that detects a since-edited model lands in the
        // `run.json` sibling instead.
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let expected =
            hydra::compute_network_digest(&hydra::io::parse(TEST_INP.as_bytes()).unwrap());

        // The results file parses as plain EPANET 20012 — it has no field
        // for a digest to be in any more.
        let meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();
        assert_eq!(meta.n_periods, 1);

        let run = crate::commands::simulation::read_run_meta(&out)
            .expect("a completed run writes run.json");
        assert_eq!(run.network_digest, Some(digest_hex(expected)));
    }

    // ── get_element_series / element_series_from_out ──────────────────────

    /// Generate a real `results.out` from `TEST_INP` via the same streaming
    /// path production uses.
    fn generated_results_out(dir: &std::path::Path) -> std::path::PathBuf {
        let out = dir.join("results.out");
        let digest = hydra::compute_network_digest(&hydra::io::parse(TEST_INP.as_bytes()).unwrap());
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out.clone()),
            0.0,
            false,
            Some(digest),
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
        let series = element_series_from_out(&out, "node", 0)
            .unwrap()
            .expect("a series");
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
        let series = element_series_from_out(&out, "link", 1)
            .unwrap()
            .expect("a series");
        let names: Vec<&str> = series.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["flow", "velocity", "headloss", "status"]);
        assert_eq!(series.fields[0].values[0], pr0.link_flow[1] as f64);
        assert_eq!(series.fields[3].values[0], pr0.link_status[1] as f64);
    }

    /// An element the results do not reach has no series, and that is an
    /// answer rather than a failure.
    ///
    /// Results describe the model as it was when it ran, so any element
    /// added since is beyond them — which is the ordinary state between
    /// an edit and the next run. Reported as an error it reached the
    /// user as a toast: adding a junction to a simulated network and
    /// then deleting it produced "node index 407 out of range" from
    /// whatever asked about it once more on the way out.
    #[test]
    fn an_element_beyond_the_results_has_no_series() {
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
        let out_meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();

        assert!(
            element_series_from_out(&out, "node", out_meta.n_nodes as u32)
                .expect("not an error")
                .is_none()
        );
        assert!(
            element_series_from_out(&out, "link", out_meta.n_links as u32)
                .expect("not an error")
                .is_none()
        );
        // Far beyond, not merely one past: a model can grow by more than
        // one element between runs.
        assert!(
            element_series_from_out(&out, "node", out_meta.n_nodes as u32 + 500)
                .expect("not an error")
                .is_none()
        );
        // The last element the results *do* hold still answers, so this
        // is a bound rather than a blanket refusal.
        assert!(
            element_series_from_out(&out, "node", out_meta.n_nodes as u32 - 1)
                .expect("not an error")
                .is_some()
        );
    }

    /// A kind this reader does not know is still an error: nobody asks
    /// for a "pipe" series by accident, and answering nothing would hide
    /// a caller using the wrong vocabulary.
    #[test]
    fn an_unknown_element_kind_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = generated_results_out(dir.path());
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

        let (nodes_csv, links_csv, _subs_csv) = csv_sibling_paths(&dir.path().join("export.csv"));
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

    fn model(demand_gpm: f64) -> String {
        format!(
            "[JUNCTIONS]\nJ1  100  {demand_gpm}\n\n\
             [RESERVOIRS]\nR1  200\n\n\
             [PIPES]\nP1  R1  J1  1000  12  100  0  Open\n\n\
             [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n\
             [TIMES]\nDuration  0\n"
        )
    }

    /// The columns of one decoded period, in wire order.
    struct Period {
        demand: Vec<f32>,
        head: Vec<f32>,
        pressure: Vec<f32>,
        flow: Vec<f32>,
        velocity: Vec<f32>,
    }

    /// Run through the same streaming path production uses and decode the
    /// payload `get_period_results` hands the frontend.
    fn period_arrays(inp: &str) -> Period {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let network = hydra::io::parse(inp.as_bytes()).unwrap();
        let mut sim = hydra::Simulation::create();
        sim.load(network).unwrap();
        let (_s, err, _w, _st) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(sim, hydra::FlowUnits::Lps),
            Some(out.clone()),
            0.0,
            false,
            None,
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "fixture run must succeed: {err:?}");

        let meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();
        let pr = hydra::io::out_reader::read_period(&out, &meta, 0).unwrap();
        let buf = encode_period_results(&pr, meta.quality_flag != 0);

        let n_nodes = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        let n_links = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
        let f32s = |start: usize, n: usize| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let o = start + 4 * i;
                    f32::from_le_bytes(buf[o..o + 4].try_into().unwrap())
                })
                .collect()
        };
        Period {
            demand: f32s(12, n_nodes),
            head: f32s(12 + 4 * n_nodes, n_nodes),
            pressure: f32s(12 + 8 * n_nodes, n_nodes),
            flow: f32s(12 + 12 * n_nodes, n_links),
            velocity: f32s(12 + 12 * n_nodes + 4 * n_links, n_links),
        }
    }

    /// Relative comparison — the engine converts through EPANET's rounded
    /// factors, so a hand-computed metre lands within about five significant
    /// figures, never exactly. Far tighter than any unit error.
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-4 * b.abs().max(1.0)
    }

    /// With no demand there is no flow, so no head loss, so the junction sits
    /// at exactly the reservoir's head. Every number here is determined by
    /// the model file alone — the solver cannot move any of them.
    #[test]
    fn a_static_network_reports_head_and_pressure_in_metres() {
        let p = period_arrays(&model(0.0));
        let (head, pressure) = (p.head, p.pressure);
        // Node order is the file's: J1 then R1.
        // Head at J1 = the reservoir's 200 ft = 60.96 m.
        assert!(
            close(head[0] as f64, 60.96),
            "J1 sits at the reservoir's head, 200 ft = 60.96 m, got {}",
            head[0]
        );
        // Pressure = head − elevation = (200 − 100) ft = 30.48 m. Serving
        // 9.29 would mean metres scaled as though they were feet; serving
        // 100 would mean no conversion happened at all.
        assert!(
            close(pressure[0] as f64, 30.48),
            "J1 is 100 ft below the reservoir = 30.48 m of head, got {}",
            pressure[0]
        );
    }

    /// One junction, one pipe, one source: conservation of mass fixes the
    /// pipe's flow at the junction's demand, and the pipe's velocity at that
    /// flow over its area — regardless of head loss or the solver's path to
    /// convergence.
    #[test]
    fn flow_velocity_and_demand_are_reported_in_si() {
        let p = period_arrays(&model(50.0));
        let (demand, flow, velocity) = (p.demand, p.flow, p.velocity);
        // 50 gpm = 3.1545 L/s.
        assert!(
            close(demand[0] as f64, 3.1545),
            "J1 demands 50 gpm = 3.1545 L/s, got {}",
            demand[0]
        );
        // The pipe is the only path to J1, so it carries exactly that.
        assert!(
            close(flow[0] as f64, 3.1545),
            "P1 is the only supply to J1, so it carries 3.1545 L/s, got {}",
            flow[0]
        );
        // v = Q/A, A = π/4 · (0.3048 m)² = 0.0729656 m²
        //       → 0.0031545 / 0.0729656 = 0.043233 m/s.
        let area = std::f64::consts::PI / 4.0 * 0.3048_f64.powi(2);
        let expected_v = 0.0031545 / area;
        assert!(
            close(velocity[0] as f64, expected_v),
            "P1 carries 3.1545 L/s through 0.0729656 m², so {expected_v:.6} m/s, got {}",
            velocity[0]
        );
    }

    /// The analytics scan reads the same file by a different route (a
    /// streaming pass rather than one period), so it gets its own check
    /// against the same hand-computed pressure.
    #[test]
    fn the_analytics_scan_agrees_with_the_period_reader() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let network = hydra::io::parse(model(0.0).as_bytes()).unwrap();
        let mut sim = hydra::Simulation::create();
        sim.load(network).unwrap();
        let (_s, err, _w, _st) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(sim, hydra::FlowUnits::Lps),
            Some(out.clone()),
            0.0,
            false,
            None,
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "fixture run must succeed: {err:?}");

        let meta = hydra::io::out_reader::read_metadata_checked(&out).unwrap();
        let scan = hydra::io::out_reader::scan_analytics(&out, &meta).unwrap();
        assert!(
            close(scan.node_min_pressure[0] as f64, 30.48),
            "the scan's minimum pressure for J1 is 30.48 m, got {}",
            scan.node_min_pressure[0]
        );
    }
}
