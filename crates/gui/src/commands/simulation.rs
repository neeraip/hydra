//! Simulation execution: the stepped hydraulics/quality run loop with results
//! streaming, progress emission, the per-target run lock, and the direct
//! `run_simulation` command.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::meta::bundle;

use super::network_dto::{format_inp_parse_error, NetworkState};
use super::projects::{results_path_for, validate_id};
use super::results::{collect_pump_energy, network_has_energy_price, PumpEnergyDto};

pub(crate) const SIMULATION_PROGRESS_EVENT: &str = "simulation_progress";

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(125);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SimulationProgressDto {
    /// The run-queue item UUID for queue-sourced runs; `None` for direct
    /// `run_simulation` runs (the frontend contract types this as `null`
    /// for direct runs).
    pub(crate) run_id: Option<String>,
    pub(crate) phase: &'static str,
    pub(crate) simulated_seconds: f64,
    pub(crate) duration_seconds: f64,
    pub(crate) percent: f64,
    pub(crate) done: bool,
    pub(crate) failed: bool,
    pub(crate) message: Option<String>,
    /// Whether water-quality is enabled for this simulation.
    pub(crate) run_quality: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum RunLoopError {
    Failed(String),
    Cancelled,
}

/// Emit `event` to all windows, logging a warning instead of silently
/// swallowing a failed emit (delivery is best-effort; the frontend recovers
/// via refetch, but the failure should not be invisible).
pub(crate) fn emit_or_warn<S: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: S) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!(event, error = %e, "failed to emit event");
    }
}

/// Best-effort removal of a temporary results stream, warning on failure —
/// a leftover `.tmp` is harmless (outside every reader's naming) but should
/// not disappear silently.
fn remove_tmp_or_warn(tmp: &std::path::Path) {
    if let Err(e) = std::fs::remove_file(tmp) {
        tracing::warn!(
            path = %tmp.display(),
            error = %e,
            "failed to remove temporary results file"
        );
    }
}

pub(crate) fn progress_percent(simulated_seconds: f64, duration_seconds: f64) -> f64 {
    if duration_seconds > 0.0 {
        (100.0 * simulated_seconds / duration_seconds).clamp(0.0, 100.0)
    } else {
        100.0
    }
}

/// Run the hydraulics and (optionally) quality loops on a pre-loaded simulation.
///
/// Streams incremental results to a sibling `<out_path>.tmp` (when `Some`) and
/// calls `emit` with progress updates after each significant step. On success
/// the temp file is atomically renamed onto `out_path`; on failure or
/// cancellation it is deleted. `out_path` therefore always holds the last
/// *complete* run: readers that key off its existence (`sim_state_from_results`,
/// `load_result_meta`, the period/analytics commands) can never observe a
/// truncated in-progress or failed file, and a previous successful
/// `results.out` survives a failed or cancelled re-run.
/// Returns `(sim, Some(error))` on failure and `(sim, None)` on success.
///
/// Designed to be called inside `tauri::async_runtime::spawn_blocking`.
pub(crate) fn run_sim_loops<F, C>(
    mut sim: hydra::Simulation,
    out_path: Option<std::path::PathBuf>,
    duration_seconds: f64,
    run_quality: bool,
    emit: F,
    should_cancel: C,
) -> (hydra::Simulation, Option<RunLoopError>, u64, u32)
where
    F: Fn(&'static str, f64, bool, bool, Option<String>),
    C: Fn() -> bool,
{
    let wall_start = std::time::Instant::now();
    let mut hyd_steps: u32 = 0;
    // Never write `out_path` directly: stream to `<name>.tmp` and promote it
    // only on success so a failed/cancelled run can never leave a truncated
    // results file behind (see the doc comment above). The `.tmp` suffix is
    // outside the `results.out` naming every reader uses, so metadata and
    // result commands never see the in-progress file.
    let tmp_path = out_path.as_ref().map(|p| {
        let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".tmp");
        p.with_file_name(name)
    });
    let mut out_writer = tmp_path.as_ref().and_then(|p| {
        if let Some(parent) = p.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "could not create results directory; run will not be persisted"
                );
            }
        }
        let file = match std::fs::File::create(p) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "could not create results stream; run will not be persisted"
                );
                return None;
            }
        };
        match hydra::io::out_writer::OutStreamWriter::begin(
            file,
            &sim,
            "",
            "",
            hydra::FlowUnits::Lps,
        ) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "could not start results stream; run will not be persisted"
                );
                None
            }
        }
    });

    let mut simulated_seconds = 0.0_f64;
    let mut last_emit_at = Instant::now();
    let mut last_percent_bucket = -1_i64;
    let mut run_err: Option<RunLoopError> = None;

    // A failed write to the results stream aborts the run as Failed: silently
    // continuing would report success for a run whose results.out is missing
    // periods (the tmp-file flow below then discards the partial stream).
    if let Some(w) = out_writer.as_mut() {
        if let Err(e) = w.append_available(&sim) {
            let msg = format!("simulation results could not be written: {e}");
            emit("hydraulics", 0.0, false, true, Some(msg.clone()));
            run_err = Some(RunLoopError::Failed(msg));
        }
    }

    if run_err.is_none() {
        emit("hydraulics", 0.0, false, false, None);
    }

    while run_err.is_none() {
        if should_cancel() {
            let msg = "Cancelled by user".to_string();
            emit("hydraulics", simulated_seconds, false, true, Some(msg));
            run_err = Some(RunLoopError::Cancelled);
            break;
        }
        match sim.step_hydraulics() {
            Ok(dt) => {
                if dt == 0.0 {
                    break;
                }
                simulated_seconds += dt;
                hyd_steps += 1;
                if let Some(w) = out_writer.as_mut() {
                    if let Err(e) = w.append_available(&sim) {
                        let msg = format!("simulation results could not be written: {e}");
                        emit(
                            "hydraulics",
                            simulated_seconds,
                            false,
                            true,
                            Some(msg.clone()),
                        );
                        run_err = Some(RunLoopError::Failed(msg));
                        break;
                    }
                }
                let pct = progress_percent(simulated_seconds, duration_seconds);
                let bucket = pct.floor() as i64;
                if bucket != last_percent_bucket || last_emit_at.elapsed() >= PROGRESS_EMIT_INTERVAL
                {
                    emit("hydraulics", simulated_seconds, false, false, None);
                    last_percent_bucket = bucket;
                    last_emit_at = Instant::now();
                }
            }
            Err(e) => {
                let msg = e.to_string();
                emit(
                    "hydraulics",
                    simulated_seconds,
                    false,
                    true,
                    Some(msg.clone()),
                );
                run_err = Some(RunLoopError::Failed(msg));
                break;
            }
        }
    }

    // Flush the final hydraulic snapshot (dt == 0.0 break path).
    if run_err.is_none() {
        if let Some(w) = out_writer.as_mut() {
            if let Err(e) = w.append_available(&sim) {
                let msg = format!("simulation results could not be written: {e}");
                emit(
                    "hydraulics",
                    simulated_seconds,
                    false,
                    true,
                    Some(msg.clone()),
                );
                run_err = Some(RunLoopError::Failed(msg));
            }
        }
    }
    if run_err.is_none() {
        emit(
            "hydraulics",
            duration_seconds.max(simulated_seconds),
            !run_quality,
            false,
            None,
        );
    }

    if run_err.is_none() && run_quality {
        let mut quality_simulated_seconds = 0.0_f64;
        let mut quality_started = false;
        last_emit_at = Instant::now();
        last_percent_bucket = -1;

        loop {
            if should_cancel() {
                let msg = "Cancelled by user".to_string();
                emit("quality", quality_simulated_seconds, false, true, Some(msg));
                run_err = Some(RunLoopError::Cancelled);
                break;
            }
            match sim.step_quality() {
                Ok(dt) => {
                    if dt == 0.0 {
                        break;
                    }
                    if !quality_started {
                        emit("quality", 0.0, false, false, None);
                        quality_started = true;
                    }
                    quality_simulated_seconds += dt;
                    let pct = progress_percent(quality_simulated_seconds, duration_seconds);
                    let bucket = pct.floor() as i64;
                    if bucket != last_percent_bucket
                        || last_emit_at.elapsed() >= PROGRESS_EMIT_INTERVAL
                    {
                        emit("quality", quality_simulated_seconds, false, false, None);
                        last_percent_bucket = bucket;
                        last_emit_at = Instant::now();
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    emit(
                        "quality",
                        quality_simulated_seconds,
                        false,
                        true,
                        Some(msg.clone()),
                    );
                    run_err = Some(RunLoopError::Failed(msg));
                    break;
                }
            }
        }

        if run_err.is_none() {
            emit(
                "quality",
                duration_seconds.max(quality_simulated_seconds),
                true,
                false,
                None,
            );
        }
    }

    let streamed = out_writer.is_some();
    if let Some(w) = out_writer {
        if let Err(e) = w.finish(&sim) {
            if run_err.is_none() {
                // Promoting a stream missing its epilogue would publish a
                // corrupt results.out — abort as Failed instead.
                let msg = format!("simulation finished but results could not be written: {e}");
                emit(
                    "hydraulics",
                    simulated_seconds,
                    false,
                    true,
                    Some(msg.clone()),
                );
                run_err = Some(RunLoopError::Failed(msg));
            } else {
                tracing::warn!(error = %e, "could not finalise discarded results stream");
            }
        }
    }

    // Promote the finished stream on success; discard it on failure/cancel.
    if let (true, Some(tmp), Some(final_path)) = (streamed, tmp_path.as_ref(), out_path.as_ref()) {
        if run_err.is_none() {
            if let Err(e) = std::fs::rename(tmp, final_path) {
                remove_tmp_or_warn(tmp);
                let msg = format!("simulation finished but results could not be written: {e}");
                emit(
                    "hydraulics",
                    simulated_seconds,
                    false,
                    true,
                    Some(msg.clone()),
                );
                run_err = Some(RunLoopError::Failed(msg));
            }
        } else {
            remove_tmp_or_warn(tmp);
        }
    }

    (
        sim,
        run_err,
        wall_start.elapsed().as_millis() as u64,
        hyd_steps,
    )
}

/// Result returned by `run_simulation`.
/// Does **not** contain per-period timestep arrays — those can be gigabytes for
/// large networks and are always accessed on demand via `get_period_results`.
/// Cross-period analytics are available via `get_result_analytics`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationResultDto {
    /// Per-pump energy accounting for the full simulation.  Empty when no pumps
    /// exist or when energy accounting was not available.
    #[serde(default)]
    pub pump_energy: Vec<PumpEnergyDto>,
}

/// Simulation targets (project/scenario pairs) whose `results.out` is
/// currently being written, by direct `run_simulation` calls or by the queue
/// processor. Guards against two runs corrupting the same output file.
static ACTIVE_RUN_TARGETS: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());

/// RAII lock on a single simulation target. Released on drop.
pub(crate) struct RunTargetGuard(String);

impl Drop for RunTargetGuard {
    fn drop(&mut self) {
        ACTIVE_RUN_TARGETS.lock().retain(|k| k != &self.0);
    }
}

/// Claim exclusive write access to the `results.out` of
/// `(project_id, scenario_id)`. Fails fast with a clear error when another
/// simulation (direct or queued) is already writing to the same target.
pub(crate) fn try_acquire_run_target(
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<RunTargetGuard, String> {
    // Scenario ids are UUIDs, so "base" can never collide with one.
    let key = format!("{}/{}", project_id, scenario_id.unwrap_or("base"));
    let mut active = ACTIVE_RUN_TARGETS.lock();
    if active.contains(&key) {
        return Err(
            "A simulation is already running for this target; wait for it to finish \
             or cancel it before starting another run"
                .into(),
        );
    }
    active.push(key.clone());
    Ok(RunTargetGuard(key))
}

/// Run hydraulics (and optionally water quality) on the currently loaded
/// network and return EPS results.
///
/// Returns `null` if no network has been loaded yet.
/// Returns an error string if the simulation fails.
#[tauri::command]
/// Run hydraulics + optional quality directly (not queued); streams progress via `simulation_progress`.
pub async fn run_simulation(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: Option<String>,
    scenario_id: Option<String>,
    quality_mode: Option<String>,
    trace_node: Option<String>,
) -> Result<Option<SimulationResultDto>, String> {
    use hydra::{QualityMode, Simulation};
    if let Some(pid) = &project_id {
        validate_id(pid)?;
    }
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    // A scenario can only be resolved within its project — silently falling
    // back to the in-memory network would run the wrong model.
    if scenario_id.is_some() && project_id.is_none() {
        return Err(
            "scenario_id requires project_id: a scenario model can only be \
             located inside its project bundle"
                .into(),
        );
    }

    // Load model bytes.  When IDs are supplied we read directly from the
    // bundle on disk so the correct model.inp is always used, regardless of
    // which file (if any) is currently open in the editor.
    let raw_bytes: Vec<u8> = if let (Some(pid), Some(sid)) = (&project_id, &scenario_id) {
        let app_data = app.path().app_data_dir().map_err(|e| format!("{e:?}"))?;
        let path = bundle::scenario_model_path(&app_data, pid, sid);
        std::fs::read(&path).map_err(|e| format!("Cannot read scenario model '{}': {}", sid, e))?
    } else if let Some(pid) = &project_id {
        let app_data = app.path().app_data_dir().map_err(|e| format!("{e:?}"))?;
        let path = bundle::base_model_path(&app_data, pid);
        std::fs::read(&path).map_err(|e| format!("Cannot read base model '{}': {}", pid, e))?
    } else {
        // Fall back to the in-memory network (opened via file picker).
        let mut guard = state.0.lock();
        match guard.up_to_date_raw_bytes() {
            Some(bytes) => bytes.clone(),
            None => return Ok(None),
        }
    };

    let mut network = hydra::io::parse(&raw_bytes).map_err(format_inp_parse_error)?;

    // Apply quality mode override from the caller. When `None` is passed,
    // honour whatever the INP `[OPTIONS]` declares — the INP is the canonical
    // source for sim params now that the Overview page edits it directly.
    if let Some(q) = quality_mode.as_deref() {
        let resolved = match q {
            "chemical" => QualityMode::Chemical,
            "age" => QualityMode::Age,
            "trace" => QualityMode::Trace,
            _ => QualityMode::None,
        };
        network.options.quality_mode = resolved;
        if resolved == QualityMode::Trace {
            network.options.trace_node = trace_node.clone();
        }
    }
    let resolved_quality = network.options.quality_mode;
    let run_quality = resolved_quality != QualityMode::None;
    let duration_seconds = network.options.duration;
    // Captured before `sim.load(network)` consumes the network — decides
    // whether pump `total_cost` is meaningful (`None` when no price info).
    let has_price_info = network_has_energy_price(&network);

    // Resolve the .out path before moving into spawn_blocking. In-memory runs
    // (no project id) write no results file.
    let out_path: Option<std::path::PathBuf> = match (app.path().app_data_dir(), &project_id) {
        (Ok(app_data), Some(pid)) => Some(results_path_for(&app_data, pid, scenario_id.as_deref())),
        _ => None,
    };

    // Claim exclusive write access to this target's results.out so a direct
    // run and a queued run can never write the same file concurrently. Held
    // (via RAII) until this function returns. In-memory runs (no project id)
    // write no .out file and need no lock.
    let _run_guard = match &project_id {
        Some(pid) => Some(try_acquire_run_target(pid, scenario_id.as_deref())?),
        None => None,
    };

    let mut sim = Simulation::create();
    sim.load(network).map_err(|e| format!("{e:?}"))?;

    // ── Phase 2: stepped loops on a blocking thread ─────────────────────────
    let app2 = app.clone();
    let (sim, run_err, wall_ms, hyd_steps) = tauri::async_runtime::spawn_blocking(move || {
        run_sim_loops(
            sim,
            out_path,
            duration_seconds,
            run_quality,
            |phase, ss, done, failed, msg| {
                emit_or_warn(
                    &app2,
                    SIMULATION_PROGRESS_EVENT,
                    &SimulationProgressDto {
                        // Direct runs are not queue items: `None` per the
                        // frontend contract (simulation.ts types run_id as
                        // "null for direct runs").
                        run_id: None,
                        phase,
                        simulated_seconds: ss,
                        duration_seconds,
                        percent: if done {
                            100.0
                        } else {
                            progress_percent(ss, duration_seconds)
                        },
                        done,
                        failed,
                        message: msg,
                        run_quality,
                    },
                );
            },
            || false,
        )
    })
    .await
    .map_err(|e| format!("Simulation task panicked: {e:?}"))?;

    tracing::info!(
        project_id = project_id.as_deref().unwrap_or("-"),
        scenario_id = scenario_id.as_deref().unwrap_or("-"),
        wall_ms,
        hyd_steps,
        outcome = run_loop_outcome(&run_err),
        "direct simulation run finished"
    );

    if let Some(err) = run_err {
        return Err(match err {
            RunLoopError::Failed(msg) => msg,
            RunLoopError::Cancelled => "Simulation cancelled".into(),
        });
    }

    let result = SimulationResultDto {
        pump_energy: collect_pump_energy(&sim, duration_seconds, has_price_info),
    };

    Ok(Some(result))
}

/// Terse outcome label for run-summary log lines.
pub(crate) fn run_loop_outcome(run_err: &Option<RunLoopError>) -> &'static str {
    match run_err {
        None => "done",
        Some(RunLoopError::Failed(_)) => "failed",
        Some(RunLoopError::Cancelled) => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::loaded_sim;

    // ── run-target lock ───────────────────────────────────────────────────

    #[test]
    fn run_target_lock_is_exclusive_per_target() {
        let base = try_acquire_run_target("proj-lock-test", None).unwrap();
        // Same target: rejected while held.
        assert!(try_acquire_run_target("proj-lock-test", None).is_err());
        // Different scenario of the same project: independent target.
        let scenario = try_acquire_run_target("proj-lock-test", Some("sc-1")).unwrap();
        assert!(try_acquire_run_target("proj-lock-test", Some("sc-1")).is_err());
        // Dropping the guard releases the target.
        drop(base);
        assert!(try_acquire_run_target("proj-lock-test", None).is_ok());
        drop(scenario);
        assert!(try_acquire_run_target("proj-lock-test", Some("sc-1")).is_ok());
    }

    // ── run_sim_loops results.out tmp/rename flow ─────────────────────────
    #[test]
    fn run_sim_loops_promotes_results_only_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_sim, err, _wall, _steps) = run_sim_loops(
            loaded_sim(),
            Some(out.clone()),
            0.0,
            false,
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "steady-state run must succeed: {err:?}");
        assert!(out.exists(), "successful run must publish results.out");
        assert!(
            !dir.path().join("results.out.tmp").exists(),
            "tmp stream must be renamed away on success"
        );
        // The published file is a complete, readable .out file.
        hydra::io::out_reader::read_metadata_checked(&out)
            .expect("results.out must be well-formed");
    }

    #[test]
    fn run_sim_loops_cancel_discards_tmp_and_keeps_previous_results() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        std::fs::write(&out, b"previous successful run").unwrap();
        let (_sim, err, _wall, _steps) = run_sim_loops(
            loaded_sim(),
            Some(out.clone()),
            0.0,
            false,
            |_, _, _, _, _| {},
            || true, // cancel immediately
        );
        assert!(matches!(err, Some(RunLoopError::Cancelled)));
        // The previous results survive untouched and no tmp is left behind,
        // so `sim_state_from_results` never reports a truncated file as done.
        assert_eq!(std::fs::read(&out).unwrap(), b"previous successful run");
        assert!(!dir.path().join("results.out.tmp").exists());
    }

    // ── progress_percent ──────────────────────────────────────────────────

    #[test]
    fn progress_percent_clamps_and_handles_zero_duration() {
        assert_eq!(progress_percent(0.0, 0.0), 100.0);
        assert_eq!(progress_percent(50.0, 200.0), 25.0);
        assert_eq!(progress_percent(300.0, 200.0), 100.0);
        assert_eq!(progress_percent(-10.0, 200.0), 0.0);
    }
}
