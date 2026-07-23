//! Simulation-parameter (\[TIMES\]/\[OPTIONS\]) commands: DTO conversion and
//! the get/update commands that treat the base INP as canonical.

use serde::{Deserialize, Serialize};

use crate::meta::bundle;

use super::network_dto::{format_inp_parse_error, NetworkState, NetworkStateInner, FT_TO_M};
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
        pda_min_pressure: o.pda_min_pressure * FT_TO_M,
        pda_required_pressure: o.pda_required_pressure * FT_TO_M,
        pda_pressure_exponent: o.pda_pressure_exponent,
        quality_mode,
        trace_node: o.trace_node.clone(),
        chem_name: o.chem_name.clone(),
        chem_units: o.chem_units.clone(),
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
    o.pda_min_pressure = dto.pda_min_pressure / FT_TO_M;
    o.pda_required_pressure = dto.pda_required_pressure / FT_TO_M;
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
    let network = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;
    Ok(Some(options_to_dto(&network.options)))
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
            network.options = new_options;
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
            let mut network = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;
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
                        network.options = new_options;
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
        let mut network = match hydra::io::parse(&bytes) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    scenario_id = %sc_id,
                    error = %format_inp_parse_error(e),
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
