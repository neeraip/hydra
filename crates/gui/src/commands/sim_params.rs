//! Simulation-parameter (\[TIMES\]/\[OPTIONS\]) commands: DTO conversion and
//! the get/update commands that treat the base INP as canonical.

use serde::{Deserialize, Serialize};

use crate::meta::bundle;

use super::network_dto::{format_read_error, NetworkState, NetworkStateInner};
use super::projects::{app_data_dir, list_scenario_ids, validate_id};

// ── Simulation parameters (TIMES + OPTIONS, INP-canonical) ────────────────────
//
// The base/model.inp file is the single source of truth for [TIMES] and
// [OPTIONS]. `get_sim_params` parses the base INP and exposes the editable
// subset to the frontend. `update_sim_params` parses, mutates, and rewrites
// the INP — and propagates the same params to every scenario INP so they stay
// in lockstep with the base.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimParamsDto {
    // ── [TIMES] ──
    /// Total simulation duration in seconds.
    pub duration: f64,
    /// Hydraulic timestep in seconds.
    pub hyd_step: f64,
    /// Quality timestep in seconds.
    pub qual_step: f64,
    /// Pattern timestep in seconds.
    pub pattern_step: f64,
    /// Report timestep in seconds.
    pub report_step: f64,
    /// Wall-clock time of t=0 (seconds since midnight).
    pub start_clocktime: f64,
    /// `"series" | "average" | "minimum" | "maximum" | "range"`.
    pub statistic: String,

    // ── [OPTIONS] core ──
    /// `"H-W" | "D-W" | "C-M"`.
    pub head_loss_formula: String,
    /// `"DDA" | "PDA"`.
    pub demand_model: String,
    pub demand_multiplier: f64,
    /// PDA min pressure in metres (SI — converted from internal feet).
    pub pda_min_pressure: f64,
    /// PDA required pressure in metres (SI — converted from internal feet).
    pub pda_required_pressure: f64,
    pub pda_pressure_exponent: f64,

    // ── [OPTIONS] quality ──
    /// `"none" | "chemical" | "age" | "trace"`.
    pub quality_mode: String,
    pub trace_node: Option<String>,
    pub chem_name: String,
    pub chem_units: String,

    // ── [ENERGY] global ──
    /// Global default pump efficiency, as a percentage (INP-native units).
    pub energy_efficiency: f64,
    /// Global unit energy price ($/kWh).
    pub energy_price: f64,
    /// Pattern ID modulating the energy price over time (empty → none).
    pub energy_price_pattern: Option<String>,
    /// Peak demand charge ($/kW).
    pub peak_demand_charge: f64,

    // ── Advanced (numerical) ──
    pub max_iter: u32,
    /// Relative flow accuracy.
    pub flow_tol: f64,
    pub head_tol: f64,
    pub damp_limit: f64,
    pub check_freq: u32,
    pub max_check: u32,
    pub viscosity: f64,
    pub specific_gravity: f64,
}

fn options_to_dto(o: &hydra::SimulationOptions) -> SimParamsDto {
    use hydra::{DemandModel, HeadLossFormula, QualityMode, StatisticType};
    let head_loss_formula = match o.head_loss_formula {
        HeadLossFormula::HazenWilliams => "H-W",
        HeadLossFormula::DarcyWeisbach => "D-W",
        HeadLossFormula::ChezyManning => "C-M",
    }
    .to_string();
    let demand_model = match o.demand_model {
        DemandModel::DemandDriven => "DDA",
        DemandModel::PressureDriven => "PDA",
    }
    .to_string();
    let quality_mode = match o.quality_mode {
        QualityMode::None => "none",
        QualityMode::Chemical => "chemical",
        QualityMode::Age => "age",
        QualityMode::Trace => "trace",
    }
    .to_string();
    let statistic = match o.statistic {
        StatisticType::Series => "series",
        StatisticType::Average => "average",
        StatisticType::Minimum => "minimum",
        StatisticType::Maximum => "maximum",
        StatisticType::Range => "range",
    }
    .to_string();
    SimParamsDto {
        duration: o.duration,
        hyd_step: o.hyd_step,
        qual_step: o.qual_step,
        pattern_step: o.pattern_step,
        report_step: o.report_step,
        start_clocktime: o.start_clocktime,
        statistic,
        head_loss_formula,
        demand_model,
        demand_multiplier: o.demand_multiplier,
        // Pressures are metres internally and metres on the wire.
        pda_min_pressure: o.pda_min_pressure,
        pda_required_pressure: o.pda_required_pressure,
        pda_pressure_exponent: o.pda_pressure_exponent,
        quality_mode,
        trace_node: o.trace_node.clone(),
        chem_name: o.chem_name.clone(),
        chem_units: o.chem_units.clone(),
        // Engine stores efficiency as a fraction; INP/UI use percent.
        energy_efficiency: o.energy_efficiency * 100.0,
        energy_price: o.energy_price,
        energy_price_pattern: o.energy_price_pattern.clone(),
        peak_demand_charge: o.peak_demand_charge,
        max_iter: o.max_iter,
        flow_tol: o.flow_tol,
        head_tol: o.head_tol,
        damp_limit: o.damp_limit,
        check_freq: o.check_freq,
        max_check: o.max_check,
        viscosity: o.viscosity,
        specific_gravity: o.specific_gravity,
    }
}

/// Apply a [`SimParamsDto`] onto a parsed `SimulationOptions` in place.
/// Unknown enum strings return `Err` so the frontend can surface a useful
/// validation message rather than silently picking a default.
fn apply_dto_to_options(
    o: &mut hydra::SimulationOptions,
    dto: &SimParamsDto,
) -> Result<(), String> {
    use hydra::{DemandModel, HeadLossFormula, QualityMode, StatisticType};

    o.duration = dto.duration;
    o.hyd_step = dto.hyd_step;
    o.qual_step = dto.qual_step;
    o.pattern_step = dto.pattern_step;
    o.report_step = dto.report_step;
    o.start_clocktime = dto.start_clocktime;
    o.statistic = match dto.statistic.as_str() {
        "series" => StatisticType::Series,
        "average" => StatisticType::Average,
        "minimum" => StatisticType::Minimum,
        "maximum" => StatisticType::Maximum,
        "range" => StatisticType::Range,
        s => return Err(format!("unknown statistic '{s}'")),
    };
    o.head_loss_formula = match dto.head_loss_formula.as_str() {
        "H-W" => HeadLossFormula::HazenWilliams,
        "D-W" => HeadLossFormula::DarcyWeisbach,
        "C-M" => HeadLossFormula::ChezyManning,
        s => return Err(format!("unknown headloss formula '{s}'")),
    };
    o.demand_model = match dto.demand_model.as_str() {
        "DDA" => DemandModel::DemandDriven,
        "PDA" => DemandModel::PressureDriven,
        s => return Err(format!("unknown demand model '{s}'")),
    };
    o.demand_multiplier = dto.demand_multiplier;
    o.pda_min_pressure = dto.pda_min_pressure;
    o.pda_required_pressure = dto.pda_required_pressure;
    o.pda_pressure_exponent = dto.pda_pressure_exponent;
    o.quality_mode = match dto.quality_mode.as_str() {
        "none" => QualityMode::None,
        "chemical" => QualityMode::Chemical,
        "age" => QualityMode::Age,
        "trace" => QualityMode::Trace,
        s => return Err(format!("unknown quality mode '{s}'")),
    };
    o.trace_node = dto.trace_node.clone().filter(|s| !s.is_empty());
    o.chem_name = dto.chem_name.clone();
    o.chem_units = dto.chem_units.clone();
    // UI/INP use percent; engine stores a fraction.
    o.energy_efficiency = dto.energy_efficiency / 100.0;
    o.energy_price = dto.energy_price;
    o.energy_price_pattern = dto.energy_price_pattern.clone().filter(|s| !s.is_empty());
    o.peak_demand_charge = dto.peak_demand_charge;
    o.max_iter = dto.max_iter;
    o.flow_tol = dto.flow_tol;
    o.head_tol = dto.head_tol;
    o.damp_limit = dto.damp_limit;
    o.check_freq = dto.check_freq;
    o.max_check = dto.max_check;
    o.viscosity = dto.viscosity;
    o.specific_gravity = dto.specific_gravity;
    Ok(())
}

/// Parse the base `model.inp` for `project_id` and return its \[TIMES\]/\[OPTIONS\]
/// values. Returns `None` when the project has no base INP yet (draft).
#[tauri::command(async)]
/// Return a project's simulation parameters (\[TIMES\]/\[OPTIONS\]).
///
/// Served from the cached parsed network in `NetworkState` when it holds this
/// project's base model — avoids re-reading and re-parsing a multi-MB INP on
/// every call. Falls back to the on-disk base INP otherwise.
pub fn get_sim_params(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
) -> Result<Option<SimParamsDto>, String> {
    validate_id(&project_id)?;
    // wds-shaped [TIMES]/[OPTIONS]/[ENERGY] only; other engines get their
    // own settings surface. None = "nothing to show", the same answer as a
    // draft project with no model yet.
    {
        let app_data = app_data_dir(&app)?;
        if super::projects::project_engine_key(&app_data, &project_id) != "wds" {
            return Ok(None);
        }
    }
    {
        let guard = state.0.lock();
        if let NetworkStateInner::Loaded {
            network,
            owner_project_id: Some(owner),
            owner_scenario_id: None,
            ..
        } = &*guard
        {
            if *owner == project_id {
                return Ok(Some(options_to_dto(&network.options)));
            }
        }
    }
    let app_data = app_data_dir(&app)?;
    let path = bundle::base_model_path(&app_data, &project_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    // Tolerant: simulation options are readable regardless of whether the
    // network is finished, and refusing here blocked the settings dialog on a
    // model the editor can open.
    let (network, _validation_errors) =
        hydra::io::parse_tolerant(&bytes).map_err(format_read_error)?;
    Ok(Some(options_to_dto(&network.options)))
}

/// One display pair of a read-only simulation-settings summary.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimSummaryPairDto {
    pub label: String,
    pub value: String,
}

/// A read-only settings summary for engines without the wds-shaped
/// editable params surface — engine-authored display pairs the run modal
/// shows as-is. Empty for engines that use `get_sim_params` instead, and
/// for draft projects with no model.
#[tauri::command(async)]
pub fn get_sim_summary_pairs(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
) -> Result<Vec<SimSummaryPairDto>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Ok(Vec::new());
    }
    let network = match super::results::uds_network_for_target(&app_data, &state, &project_id, None)
    {
        Ok(n) => n,
        // No model yet is the draft-project case, not an error.
        Err(_) => return Ok(Vec::new()),
    };
    let o = &network.options;

    let pair = |label: &str, value: String| SimSummaryPairDto {
        label: label.to_string(),
        value,
    };
    let date = |d: &hydra::uds::io::options::Date, t: f64| {
        let (h, rem) = ((t / 3600.0) as u32, t % 3600.0);
        format!(
            "{:02}/{:02}/{} {:02}:{:02}",
            d.month,
            d.day,
            d.year,
            h,
            (rem / 60.0) as u32
        )
    };
    let step = |s: f64| {
        if s >= 60.0 && s % 60.0 == 0.0 {
            format!("{} min", (s / 60.0) as u32)
        } else {
            format!("{s} s")
        }
    };
    let duration_hr = {
        use chrono::NaiveDate;
        let d0 = NaiveDate::from_ymd_opt(o.start_date.year, o.start_date.month, o.start_date.day);
        let d1 = NaiveDate::from_ymd_opt(o.end_date.year, o.end_date.month, o.end_date.day);
        match (d0, d1) {
            (Some(d0), Some(d1)) => {
                let days = (d1 - d0).num_days() as f64;
                Some((days * 86_400.0 + o.end_time - o.start_time) / 3600.0)
            }
            _ => None,
        }
    };

    let mut pairs = vec![
        pair("Flow units", format!("{:?}", o.flow_units).to_uppercase()),
        pair(
            "Routing",
            match o.routing_request {
                hydra::uds::io::options::RoutingRequest::Steady => "Steady flow".to_string(),
                hydra::uds::io::options::RoutingRequest::KinematicWave => {
                    "Kinematic wave".to_string()
                }
                hydra::uds::io::options::RoutingRequest::DynamicWave => "Dynamic wave".to_string(),
            },
        ),
        pair(
            "Infiltration",
            match o.infiltration {
                hydra::uds::io::options::InfiltrationModel::Horton => "Horton".to_string(),
                hydra::uds::io::options::InfiltrationModel::ModifiedHorton => {
                    "Modified Horton".to_string()
                }
                hydra::uds::io::options::InfiltrationModel::GreenAmpt => "Green-Ampt".to_string(),
                hydra::uds::io::options::InfiltrationModel::ModifiedGreenAmpt => {
                    "Modified Green-Ampt".to_string()
                }
                hydra::uds::io::options::InfiltrationModel::CurveNumber => {
                    "Curve number".to_string()
                }
            },
        ),
        pair("Start", date(&o.start_date, o.start_time)),
    ];
    if let Some(hr) = duration_hr {
        pairs.push(pair("Duration", format!("{hr:.2} hr")));
    }
    pairs.push(pair("Routing step", step(o.routing_step)));
    pairs.push(pair("Report step", step(o.report_step)));
    Ok(pairs)
}

/// Fast-path sim-params update: when the cached parse holds `project_id`'s
/// base model with no pending unsaved edits, apply `params` directly to the
/// cache and return freshly serialised INP bytes for the caller to write to
/// disk. Returns `Ok(None)` when the cache does not match (slow path applies).
///
/// # Why this sets `dirty = true` even though `raw_bytes` is refreshed here
///
/// The returned bytes are written to disk *after* the state lock is released,
/// which races with `save_project`: save also clones bytes under the lock
/// (clearing `dirty` in `up_to_date_raw_bytes`) and writes after dropping it.
/// An in-flight save that snapshotted the *old* bytes can land its write
/// after ours, leaving stale options on disk. Performing our file write while
/// still holding the lock would NOT close that race — the conflicting save
/// write happens outside any lock, so it could still land last against a
/// `dirty == false` state that no longer records the divergence. Setting
/// `dirty = true` does close it: whatever write order occurs, the state
/// records that disk may not match the cache, so the next consumer
/// (save/export/run) re-serialises from the updated cache and repairs disk.
/// When no save is racing, the only cost is one redundant re-serialisation at
/// the next consumption point.
fn apply_sim_params_to_cached_base(
    state: &mut NetworkStateInner,
    project_id: &str,
    params: &SimParamsDto,
) -> Result<Option<Vec<u8>>, String> {
    if let NetworkStateInner::Loaded {
        raw_bytes,
        dirty,
        network,
        owner_project_id: Some(owner),
        owner_scenario_id: None,
        ..
    } = state
    {
        if !*dirty && owner == project_id {
            // Apply to a scratch copy first so a validation error cannot
            // leave the cached network half-updated.
            let mut new_options = network.options.clone();
            apply_dto_to_options(&mut new_options, params)?;
            std::sync::Arc::make_mut(network).options = new_options;
            *raw_bytes = hydra::write_inp(network);
            // See doc comment: guards against a racing `save_project`.
            *dirty = true;
            return Ok(Some(raw_bytes.clone()));
        }
    }
    Ok(None)
}

/// Persist new sim params for `project_id` by parsing the base INP, applying
/// the DTO, rewriting the base INP, and propagating to every scenario INP so
/// they stay in lockstep.
#[tauri::command(async)]
/// Persist a project's simulation parameters to its base and scenario INPs.
pub fn update_sim_params(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    params: SimParamsDto,
) -> Result<(), String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    // Writing settings serialises the model with the wds writer; other
    // engines' settings are read-only in the GUI (mirrors `get_sim_params`
    // serving only wds). Guarded here, not just in the frontend, so no new
    // caller can rewrite a foreign model's bytes with EPANET output.
    if super::projects::project_engine_key(&app_data, &project_id) != "wds" {
        return Err("This project's engine's settings are read-only in the GUI.".into());
    }

    // 1) Base model.
    let base_path = bundle::base_model_path(&app_data, &project_id);
    if !base_path.exists() {
        return Err("project has no base model".into());
    }

    // 1a) Fast path: the cached parse already holds this project's base model
    // and has no pending unsaved edits (`!dirty`, i.e. memory == disk), so the
    // new bytes can be serialised straight from the cache without re-reading
    // and re-parsing the base INP. Marks the state dirty to close the write
    // race with `save_project` — see `apply_sim_params_to_cached_base`.
    let cached_bytes: Option<Vec<u8>> = {
        let mut guard = state.0.lock();
        apply_sim_params_to_cached_base(&mut guard, &project_id, &params)?
    };
    match cached_bytes {
        Some(bytes) => {
            bundle::atomic_write(&base_path, &bytes).map_err(|e| e.to_string())?;
        }
        None => {
            let bytes = std::fs::read(&base_path).map_err(|e| e.to_string())?;
            // Tolerant, matching `get_sim_params`: editing options must not
            // require a simulable network.
            let (mut network, _validation_errors) =
                hydra::io::parse_tolerant(&bytes).map_err(format_read_error)?;
            apply_dto_to_options(&mut network.options, &params)?;
            let new_bytes = hydra::write_inp(&network);
            bundle::atomic_write(&base_path, &new_bytes).map_err(|e| e.to_string())?;

            // Keep the cached parse (base with unsaved edits, or a loaded
            // scenario of this project) in lockstep so `get_sim_params` served
            // from the cache reflects the new options; `dirty` makes the next
            // raw-bytes consumer re-serialise.
            let mut guard = state.0.lock();
            if let NetworkStateInner::Loaded {
                dirty,
                network,
                owner_project_id: Some(owner),
                ..
            } = &mut *guard
            {
                if *owner == project_id {
                    let mut new_options = network.options.clone();
                    if apply_dto_to_options(&mut new_options, &params).is_ok() {
                        std::sync::Arc::make_mut(network).options = new_options;
                        *dirty = true;
                    }
                }
            }
        }
    }

    // 2) Every scenario's INP — best-effort. Scenarios whose INP fails to
    //    read, parse, or rewrite are skipped (with a warning) so a single bad
    //    scenario doesn't block the user from updating the base.
    let scenario_ids = list_scenario_ids(&app_data, &project_id);
    for sc_id in scenario_ids {
        let path = bundle::scenario_model_path(&app_data, &project_id, &sc_id);
        if !path.exists() {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    scenario_id = %sc_id,
                    error = %e,
                    "sim-params propagation skipped scenario: cannot read model"
                );
                continue;
            }
        };
        let mut network = match hydra::io::parse_tolerant(&bytes) {
            Ok((n, _validation_errors)) => n,
            Err(e) => {
                tracing::warn!(
                    scenario_id = %sc_id,
                    error = %format_read_error(e),
                    "sim-params propagation skipped scenario: cannot parse model"
                );
                continue;
            }
        };
        if let Err(e) = apply_dto_to_options(&mut network.options, &params) {
            tracing::warn!(
                scenario_id = %sc_id,
                error = %e,
                "sim-params propagation skipped scenario: params rejected"
            );
            continue;
        }
        let new_bytes = hydra::write_inp(&network);
        if let Err(e) = bundle::atomic_write(&path, &new_bytes) {
            tracing::warn!(
                scenario_id = %sc_id,
                error = %e,
                "sim-params propagation skipped scenario: cannot write model"
            );
        }
    }

    Ok(())
}

// ── Drainage simulation parameters (uds [OPTIONS], INP-canonical) ─────────────
//
// The same shape as the wds surface above, for the drainage engine: the
// base model.inp is canonical, the editable subset travels as a DTO, and
// an update rewrites the base and every scenario INP in lockstep. The
// subset is the run's timing plus the three global choices — flow units,
// routing form, infiltration relation. Flow units is a lossless
// re-serialisation here (the model holds SI; the writer converts on the
// way out), routing is a request the solver already substitutes for, and
// infiltration is editable within a parameter family: a subcatchment's
// parameters are typed to their relation, so Horton ↔ modified Horton is
// an option flip while Horton → Green-Ampt would need every subcatchment
// re-described — that one refuses, saying so.

/// A calendar date on the wire, as the drainage options hold it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UdsDateDto {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl From<hydra::uds::io::options::Date> for UdsDateDto {
    fn from(d: hydra::uds::io::options::Date) -> Self {
        Self {
            year: d.year,
            month: d.month,
            day: d.day,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UdsSimParamsDto {
    // ── Editable: the run's timing ──
    pub start_date: UdsDateDto,
    /// Start time of day (seconds since midnight).
    pub start_time: f64,
    pub end_date: UdsDateDto,
    /// End time of day (seconds since midnight).
    pub end_time: f64,
    /// Reporting step (s).
    pub report_step: f64,
    /// Routing step cap (s).
    pub routing_step: f64,
    /// Wet hydrology step (s).
    pub wet_step: f64,
    /// Dry hydrology step (s).
    pub dry_step: f64,

    /// File keyword: `CFS | GPM | MGD | CMS | LPS | MLD`.
    pub flow_units: String,
    /// File keyword: `STEADY | KINWAVE | DYNWAVE`.
    pub routing: String,
    /// File keyword: `HORTON | MODIFIED_HORTON | GREEN_AMPT |
    /// MODIFIED_GREEN_AMPT | CURVE_NUMBER`.
    pub infiltration: String,
}

fn uds_options_to_dto(o: &hydra::uds::io::options::AnalysisOptions) -> UdsSimParamsDto {
    use hydra::uds::io::options::{InfiltrationModel, RoutingRequest};
    UdsSimParamsDto {
        start_date: o.start_date.into(),
        start_time: o.start_time,
        end_date: o.end_date.into(),
        end_time: o.end_time,
        report_step: o.report_step,
        routing_step: o.routing_step,
        wet_step: o.wet_step,
        dry_step: o.dry_step,
        flow_units: format!("{:?}", o.flow_units).to_uppercase(),
        routing: match o.routing_request {
            RoutingRequest::Steady => "STEADY",
            RoutingRequest::KinematicWave => "KINWAVE",
            RoutingRequest::DynamicWave => "DYNWAVE",
        }
        .to_string(),
        infiltration: match o.infiltration {
            InfiltrationModel::Horton => "HORTON",
            InfiltrationModel::ModifiedHorton => "MODIFIED_HORTON",
            InfiltrationModel::GreenAmpt => "GREEN_AMPT",
            InfiltrationModel::ModifiedGreenAmpt => "MODIFIED_GREEN_AMPT",
            InfiltrationModel::CurveNumber => "CURVE_NUMBER",
        }
        .to_string(),
    }
}

/// Which parameter set an infiltration relation reads (§3.3): Horton's
/// family, Green-Ampt's, or the curve number's. A subcatchment's stored
/// parameters are typed to the family, so a within-family flip is an
/// option change while a cross-family one would orphan every parameter
/// set in the model.
fn infiltration_family(m: hydra::uds::io::options::InfiltrationModel) -> u8 {
    use hydra::uds::io::options::InfiltrationModel::*;
    match m {
        Horton | ModifiedHorton => 0,
        GreenAmpt | ModifiedGreenAmpt => 1,
        CurveNumber => 2,
    }
}

/// The moment a date-and-time names, as an epoch the two ends can be
/// compared on. Refuses an impossible calendar date rather than guessing.
fn uds_epoch(date: &UdsDateDto, time: f64, which: &str) -> Result<f64, String> {
    use chrono::NaiveDate;
    let d = NaiveDate::from_ymd_opt(date.year, date.month, date.day).ok_or_else(|| {
        format!(
            "the {which} date {:02}/{:02}/{} is not a calendar date",
            date.month, date.day, date.year
        )
    })?;
    Ok(d.and_hms_opt(0, 0, 0)
        .map(|dt| dt.and_utc().timestamp() as f64)
        .unwrap_or(0.0)
        + time)
}

/// Apply the editable subset onto a parsed drainage network in place.
///
/// Two rules with stories behind them. **The run has to end after it
/// starts:** the engine happily runs for zero seconds — it completes at
/// once and writes a results file with no reporting periods, which
/// surfaces as an empty canvas and a RESULTS-EMPTY warning. This door is
/// where that state stops being reachable through the GUI. And **the
/// infiltration relation moves only within its parameter family** while
/// subcatchments carry typed parameters for the old one — a cross-family
/// flip is a model edit this dialog does not make, and it says so.
fn apply_uds_dto(
    net: &mut hydra::uds::model::Network,
    dto: &UdsSimParamsDto,
) -> Result<(), String> {
    use hydra::uds::io::options::{FlowUnits, InfiltrationModel, RoutingRequest};

    let start = uds_epoch(&dto.start_date, dto.start_time, "start")?;
    let end = uds_epoch(&dto.end_date, dto.end_time, "end")?;
    if end <= start {
        return Err("the run has to end after it starts".into());
    }
    for (step, name) in [
        (dto.report_step, "report step"),
        (dto.routing_step, "routing step"),
        (dto.wet_step, "wet step"),
        (dto.dry_step, "dry step"),
    ] {
        if !step.is_finite() || step <= 0.0 {
            return Err(format!("the {name} has to be a positive number of seconds"));
        }
    }

    let flow_units = match dto.flow_units.as_str() {
        "CFS" => FlowUnits::Cfs,
        "GPM" => FlowUnits::Gpm,
        "MGD" => FlowUnits::Mgd,
        "CMS" => FlowUnits::Cms,
        "LPS" => FlowUnits::Lps,
        "MLD" => FlowUnits::Mld,
        s => return Err(format!("unknown flow units '{s}'")),
    };
    let routing = match dto.routing.as_str() {
        "STEADY" => RoutingRequest::Steady,
        "KINWAVE" => RoutingRequest::KinematicWave,
        "DYNWAVE" => RoutingRequest::DynamicWave,
        s => return Err(format!("unknown routing form '{s}'")),
    };
    let infiltration = match dto.infiltration.as_str() {
        "HORTON" => InfiltrationModel::Horton,
        "MODIFIED_HORTON" => InfiltrationModel::ModifiedHorton,
        "GREEN_AMPT" => InfiltrationModel::GreenAmpt,
        "MODIFIED_GREEN_AMPT" => InfiltrationModel::ModifiedGreenAmpt,
        "CURVE_NUMBER" => InfiltrationModel::CurveNumber,
        s => return Err(format!("unknown infiltration relation '{s}'")),
    };
    if infiltration_family(infiltration) != infiltration_family(net.options.infiltration)
        && net.parcels.iter().any(|p| p.infiltration.is_some())
    {
        return Err(format!(
            "switching to {} would orphan every subcatchment's infiltration \
             parameters, which are entered for {}. Re-describe the \
             subcatchments first",
            dto.infiltration,
            uds_options_to_dto(&net.options).infiltration
        ));
    }

    let o = &mut net.options;
    o.flow_units = flow_units;
    o.routing_request = routing;
    o.infiltration = infiltration;
    o.start_date = hydra::uds::io::options::Date {
        year: dto.start_date.year,
        month: dto.start_date.month,
        day: dto.start_date.day,
    };
    o.start_time = dto.start_time;
    o.end_date = hydra::uds::io::options::Date {
        year: dto.end_date.year,
        month: dto.end_date.month,
        day: dto.end_date.day,
    };
    o.end_time = dto.end_time;
    o.report_step = dto.report_step;
    o.routing_step = dto.routing_step;
    o.wet_step = dto.wet_step;
    o.dry_step = dto.dry_step;
    Ok(())
}

/// A drainage project's simulation parameters, or `None` for a project of
/// another engine or one with no model yet.
#[tauri::command(async)]
pub fn get_uds_sim_params(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
) -> Result<Option<UdsSimParamsDto>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Ok(None);
    }
    match super::results::uds_network_for_target(&app_data, &state, &project_id, None) {
        Ok(network) => Ok(Some(uds_options_to_dto(&network.options))),
        // No model yet is the draft-project case, not an error.
        Err(_) => Ok(None),
    }
}

/// Persist new drainage sim params to the base and every scenario INP —
/// the same contract as [`update_sim_params`], through the drainage
/// engine's own reader and writer.
#[tauri::command(async)]
pub fn update_uds_sim_params(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    params: UdsSimParamsDto,
) -> Result<(), String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    // Guarded here, not just in the frontend: this path serialises with
    // the drainage writer, and rewriting another engine's model with it
    // would destroy the file.
    if super::projects::project_engine_key(&app_data, &project_id) != "uds" {
        return Err("this command edits drainage models only".into());
    }

    let rewrite = |path: &std::path::Path| -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        // Tolerant by construction — the drainage reader always yields a
        // network plus diagnostics, so options are editable regardless of
        // whether the model is finished.
        let (mut network, _diagnostics) = hydra::uds::io::objects::parse_network(&text);
        apply_uds_dto(&mut network, &params)?;
        let new_text =
            hydra::uds::io::inp_writer::write_inp(&network).map_err(|e| e.to_string())?;
        bundle::atomic_write(path, new_text.as_bytes()).map_err(|e| e.to_string())
    };

    // 1) Base model — a failure here is the caller's answer.
    let base_path = bundle::base_model_path(&app_data, &project_id);
    if !base_path.exists() {
        return Err("project has no base model".into());
    }
    rewrite(&base_path)?;

    // Keep a loaded copy of this project in lockstep, and mark it dirty so
    // the next raw-bytes consumer re-serialises from the cache — the same
    // race-closing rule the wds path documents.
    {
        let mut guard = state.0.lock();
        if let super::network_dto::NetworkStateInner::LoadedUds {
            dirty,
            network,
            owner_project_id: Some(owner),
            ..
        } = &mut *guard
        {
            if *owner == project_id {
                // Validation is all-or-nothing, so a refusal leaves the
                // cached network untouched.
                if apply_uds_dto(std::sync::Arc::make_mut(network), &params).is_ok() {
                    *dirty = true;
                }
            }
        }
    }

    // 2) Every scenario's INP — best-effort, as the wds path is: one bad
    //    scenario must not block updating the base.
    for sc_id in list_scenario_ids(&app_data, &project_id) {
        let path = bundle::scenario_model_path(&app_data, &project_id, &sc_id);
        if !path.exists() {
            continue;
        }
        if let Err(e) = rewrite(&path) {
            tracing::warn!(
                scenario_id = %sc_id,
                error = %e,
                "uds sim-params propagation skipped scenario"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::loaded_state;

    // ── update_sim_params fast path vs save_project race ──────────────────

    #[test]
    fn sim_params_fast_path_marks_dirty_so_a_racing_save_cannot_strand_disk() {
        let mut state = loaded_state();
        // A concurrent save_project snapshots the raw bytes and clears
        // `dirty` before writing them to disk...
        let stale_snapshot = state.up_to_date_raw_bytes().unwrap().clone();

        // ...then the update_sim_params fast path applies new options.
        let params = {
            let NetworkStateInner::Loaded { network, .. } = &state else {
                panic!("state must be loaded");
            };
            SimParamsDto {
                duration: 7200.0,
                ..options_to_dto(&network.options)
            }
        };
        let written = apply_sim_params_to_cached_base(&mut state, "test-project", &params)
            .unwrap()
            .expect("matching non-dirty cache must take the fast path");
        let reparsed = hydra::io::parse(&written).unwrap();
        assert!((reparsed.options.duration - 7200.0).abs() < 1e-9);

        // Even though raw_bytes was refreshed in place, the state must be
        // flagged dirty: if the racing save's (stale) write lands last, the
        // next consumer re-serialises from the updated cache and repairs disk.
        let NetworkStateInner::Loaded { dirty, .. } = &state else {
            panic!("state must be loaded");
        };
        assert!(*dirty, "fast path must mark the state dirty");
        let next_save = state.up_to_date_raw_bytes().unwrap().clone();
        assert_ne!(next_save, stale_snapshot);
        let reparsed = hydra::io::parse(&next_save).unwrap();
        assert!((reparsed.options.duration - 7200.0).abs() < 1e-9);
    }

    #[test]
    fn energy_options_round_trip_through_dto_with_percent_conversion() {
        let opts = hydra::SimulationOptions {
            energy_efficiency: 0.82, // fraction internally
            energy_price: 0.14,
            energy_price_pattern: Some("EPRICE".to_string()),
            peak_demand_charge: 12.5,
            ..Default::default()
        };

        let dto = options_to_dto(&opts);
        // DTO surfaces efficiency as a percent for the UI.
        assert!((dto.energy_efficiency - 82.0).abs() < 1e-9);
        assert!((dto.energy_price - 0.14).abs() < 1e-9);
        assert_eq!(dto.energy_price_pattern.as_deref(), Some("EPRICE"));
        assert!((dto.peak_demand_charge - 12.5).abs() < 1e-9);

        let mut back = hydra::SimulationOptions::default();
        apply_dto_to_options(&mut back, &dto).unwrap();
        assert!((back.energy_efficiency - 0.82).abs() < 1e-9);
        assert!((back.energy_price - 0.14).abs() < 1e-9);
        assert_eq!(back.energy_price_pattern.as_deref(), Some("EPRICE"));
        assert!((back.peak_demand_charge - 12.5).abs() < 1e-9);

        // An empty price-pattern string is normalised to None.
        let mut dto_empty = dto.clone();
        dto_empty.energy_price_pattern = Some(String::new());
        let mut back_empty = hydra::SimulationOptions::default();
        apply_dto_to_options(&mut back_empty, &dto_empty).unwrap();
        assert_eq!(back_empty.energy_price_pattern, None);
    }

    // ── Drainage params ───────────────────────────────────────────────────

    fn uds_net() -> hydra::uds::model::Network {
        hydra::uds::io::objects::parse_network(
            "[OPTIONS]\nFLOW_UNITS CFS\nSTART_DATE 01/01/2004\nSTART_TIME 00:00:00\n\
             END_DATE 01/01/2004\nEND_TIME 00:00:00\nREPORT_STEP 00:15:00\nROUTING_STEP 20\n\
             [JUNCTIONS]\nJ1 100 4 0 0 0\n\
             [OUTFALLS]\nO1 97 FREE NO\n\
             [CONDUITS]\nC1 J1 O1 300 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n",
        )
        .0
    }

    /// A model whose subcatchment carries Horton parameters, for the
    /// infiltration family rule.
    fn uds_net_with_parcel() -> hydra::uds::model::Network {
        hydra::uds::io::objects::parse_network(
            "[OPTIONS]\nFLOW_UNITS CFS\nINFILTRATION HORTON\nSTART_DATE 01/01/2004\n\
             START_TIME 00:00:00\nEND_DATE 01/01/2004\nEND_TIME 06:00:00\n\
             [RAINGAGES]\nRG1 INTENSITY 1:00 1.0 TIMESERIES TS1\n\
             [TIMESERIES]\nTS1 0:00 1.0\nTS1 1:00 0.0\n\
             [JUNCTIONS]\nJ1 100 4 0 0 0\n\
             [OUTFALLS]\nO1 97 FREE NO\n\
             [CONDUITS]\nC1 J1 O1 300 0.013 0 0\n\
             [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n\
             [SUBCATCHMENTS]\nS1 RG1 J1 5 25 500 0.5 0\n\
             [SUBAREAS]\nS1 0.01 0.1 0.05 0.05 25 OUTLET\n\
             [INFILTRATION]\nS1 3.0 0.5 4 7 0\n",
        )
        .0
    }

    /// The infiltration relation moves freely within its parameter
    /// family, and refuses to cross families while subcatchments carry
    /// typed parameters for the old one.
    #[test]
    fn uds_infiltration_moves_within_its_family_and_refuses_across() {
        let mut net = uds_net_with_parcel();
        assert!(
            net.parcels[0].infiltration.is_some(),
            "fixture has Horton data"
        );
        let dto = uds_options_to_dto(&net.options);

        // Horton → modified Horton: same parameter set, an option flip.
        let mut within = dto.clone();
        within.infiltration = "MODIFIED_HORTON".into();
        apply_uds_dto(&mut net, &within).expect("within the family");
        assert_eq!(
            net.options.infiltration,
            hydra::uds::io::options::InfiltrationModel::ModifiedHorton
        );

        // Horton → Green-Ampt: every subcatchment's parameters would be
        // orphaned, so it refuses and names the problem.
        let mut across = dto.clone();
        across.infiltration = "GREEN_AMPT".into();
        let err = apply_uds_dto(&mut net, &across).expect_err("across families");
        assert!(err.contains("subcatchment"), "{err}");

        // With no subcatchment parameters in the model, the same flip is
        // just an option change.
        let mut bare = uds_net();
        let mut dto = uds_options_to_dto(&bare.options);
        dto.end_time = 21_600.0;
        dto.infiltration = "GREEN_AMPT".into();
        apply_uds_dto(&mut bare, &dto).expect("nothing to orphan");
    }

    /// Flow units is a re-serialisation, not a reinterpretation: the
    /// model holds SI, so the same physics comes back out of a file
    /// written in the other system.
    #[test]
    fn uds_flow_units_flip_preserves_the_model() {
        let mut net = uds_net_with_parcel();
        let invert_si = net
            .vertices
            .iter()
            .find(|v| v.id == "J1")
            .expect("J1")
            .invert;

        let mut dto = uds_options_to_dto(&net.options);
        dto.flow_units = "CMS".into();
        dto.routing = "KINWAVE".into();
        apply_uds_dto(&mut net, &dto).expect("apply");

        let text = hydra::uds::io::inp_writer::write_inp(&net).expect("write");
        let (reparsed, _) = hydra::uds::io::objects::parse_network(&text);
        assert_eq!(
            reparsed.options.flow_units,
            hydra::uds::io::options::FlowUnits::Cms
        );
        assert_eq!(
            reparsed.options.routing_request,
            hydra::uds::io::options::RoutingRequest::KinematicWave
        );
        let back = reparsed
            .vertices
            .iter()
            .find(|v| v.id == "J1")
            .expect("J1")
            .invert;
        assert!(
            (back - invert_si).abs() < 1e-6,
            "the invert moved: {invert_si} became {back}"
        );
    }

    /// The editable subset reaches the options and comes back, and the
    /// write survives the engine's own writer — which is the whole route
    /// an update takes.
    #[test]
    fn uds_params_round_trip_through_dto_and_writer() {
        let mut net = uds_net();
        let mut dto = uds_options_to_dto(&net.options);
        assert_eq!(dto.flow_units, "CFS");

        dto.end_date = UdsDateDto {
            year: 2004,
            month: 1,
            day: 1,
        };
        dto.end_time = 6.0 * 3600.0;
        apply_uds_dto(&mut net, &dto).expect("apply");
        assert!((net.options.end_time - 21_600.0).abs() < 1e-9);

        let text = hydra::uds::io::inp_writer::write_inp(&net).expect("write");
        let (reparsed, _) = hydra::uds::io::objects::parse_network(&text);
        assert!((reparsed.options.end_time - 21_600.0).abs() < 1e-9);
        assert_eq!(reparsed.options.end_date.day, 1);
    }

    /// The rule this surface exists to enforce: a run spanning no time
    /// completes at once and writes a results file with no periods, so
    /// the write refuses it — changing nothing.
    #[test]
    fn a_uds_run_that_ends_before_it_starts_is_refused() {
        let mut net = uds_net();
        let dto = uds_options_to_dto(&net.options);

        // The fixture's own state: end equals start.
        let err = apply_uds_dto(&mut net, &dto).expect_err("zero duration");
        assert!(err.contains("end after it starts"), "{err}");

        // Ends the day before it starts.
        let mut backwards = dto.clone();
        backwards.end_date = UdsDateDto {
            year: 2003,
            month: 12,
            day: 31,
        };
        assert!(apply_uds_dto(&mut net, &backwards).is_err());

        // A calendar date that does not exist is named, not guessed at.
        let mut impossible = dto.clone();
        impossible.end_date = UdsDateDto {
            year: 2004,
            month: 2,
            day: 30,
        };
        let err = apply_uds_dto(&mut net, &impossible).expect_err("Feb 30");
        assert!(err.contains("not a calendar date"), "{err}");

        // And crossing midnight is a longer run, not a backwards one:
        // end 01:00 next day beats start 23:00.
        let mut overnight = dto.clone();
        overnight.start_time = 23.0 * 3600.0;
        overnight.end_date = UdsDateDto {
            year: 2004,
            month: 1,
            day: 2,
        };
        overnight.end_time = 3600.0;
        apply_uds_dto(&mut net, &overnight).expect("overnight run");
    }

    #[test]
    fn a_uds_step_that_is_not_positive_is_refused() {
        let mut net = uds_net();
        let mut dto = uds_options_to_dto(&net.options);
        dto.end_time = 21_600.0;
        dto.report_step = 0.0;
        let err = apply_uds_dto(&mut net, &dto).expect_err("zero step");
        assert!(err.contains("report step"), "{err}");
        assert!(
            (net.options.report_step - 900.0).abs() < 1e-9,
            "a refusal changed the model"
        );
    }

    #[test]
    fn sim_params_fast_path_skips_mismatched_or_dirty_cache() {
        // Different owner: slow path.
        let mut state = loaded_state();
        let params = {
            let NetworkStateInner::Loaded { network, .. } = &state else {
                panic!("state must be loaded");
            };
            options_to_dto(&network.options)
        };
        assert!(
            apply_sim_params_to_cached_base(&mut state, "other-project", &params)
                .unwrap()
                .is_none()
        );

        // Pending unsaved edits (dirty): slow path, cache untouched.
        if let NetworkStateInner::Loaded { dirty, .. } = &mut state {
            *dirty = true;
        }
        assert!(
            apply_sim_params_to_cached_base(&mut state, "test-project", &params)
                .unwrap()
                .is_none()
        );
    }
}
