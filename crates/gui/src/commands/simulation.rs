//! Simulation execution: the stepped hydraulics/quality run loop with results
//! streaming, progress emission, and the per-target run lock.
//!
//! Runs are always queued (see `run_queue`) — there is no direct-run command.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::meta::bundle;

use super::projects::{results_path_for, validate_id};

pub(crate) const SIMULATION_PROGRESS_EVENT: &str = "simulation_progress";

const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(125);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SimulationProgressDto {
    /// The run-queue item UUID. Always `Some` — every run is queued.
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

// ── Run warnings ──────────────────────────────────────────────────────────────

/// One non-fatal simulation warning, persisted to `warnings.json` beside
/// `results.out` and served by [`get_run_warnings`]. Wire shape (camelCase):
/// `{ "code": string, "message": string, "elementId": string|null }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunWarningDto {
    /// Stable kebab-case code derived from the engine's `WarningKind`:
    /// `"unbalanced-hydraulics"` | `"negative-pressure"` | `"pump-x-head"`.
    pub code: String,
    /// Human-readable description, including the simulation time.
    pub message: String,
    /// ID of the affected node/link, or `null` for network-wide warnings.
    pub element_id: Option<String>,
}

/// Format a simulation time (seconds) as `H:MM:SS` for warning messages.
fn format_sim_time(t: f64) -> String {
    let total = t.max(0.0).round() as u64;
    format!(
        "{}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

/// Map one engine warning to its wire DTO. `node_ids` / `link_ids` are the
/// load-order ID arrays (`Simulation::node_ids` / `link_ids`) used to resolve
/// the zero-based indices carried by `WarningKind`.
pub(crate) fn warning_to_dto(
    w: &hydra::SimWarning,
    node_ids: &[&str],
    link_ids: &[&str],
) -> RunWarningDto {
    use hydra::WarningKind;
    let at = format_sim_time(w.t);
    match &w.kind {
        WarningKind::UnbalancedHydraulics => RunWarningDto {
            code: "unbalanced-hydraulics".into(),
            message: format!("Hydraulic equations were not fully balanced at {at}"),
            element_id: None,
        },
        WarningKind::NegativePressure { node_index } => {
            let id = node_ids.get(*node_index).map(|s| s.to_string());
            let name = id.clone().unwrap_or_else(|| format!("#{}", node_index + 1));
            RunWarningDto {
                code: "negative-pressure".into(),
                message: format!("Negative pressure at junction {name} at {at}"),
                element_id: id,
            }
        }
        WarningKind::TankLevelAccuracy { node_index } => {
            let id = node_ids.get(*node_index).map(|s| s.to_string());
            let name = id.clone().unwrap_or_else(|| format!("#{}", node_index + 1));
            RunWarningDto {
                code: "tank-level-accuracy".into(),
                message: format!("Tank {name} level computed with degraded accuracy at {at}"),
                element_id: id,
            }
        }
        WarningKind::LinkStatusPinned { link_index } => {
            let id = link_ids.get(*link_index).map(|s| s.to_string());
            let name = id.clone().unwrap_or_else(|| format!("#{}", link_index + 1));
            RunWarningDto {
                code: "link-status-pinned".into(),
                message: format!(
                    "{name} kept opening and closing at {at}, so it was held fixed \
                     for the rest of that step"
                ),
                element_id: id,
            }
        }
        WarningKind::PumpXHead { link_index } => {
            let id = link_ids.get(*link_index).map(|s| s.to_string());
            let name = id.clone().unwrap_or_else(|| format!("#{}", link_index + 1));
            RunWarningDto {
                code: "pump-x-head".into(),
                message: format!("Pump {name} operating outside its head curve at {at}"),
                element_id: id,
            }
        }
        WarningKind::PumpSpeedPatternSupersedesSetting { link_index } => {
            let id = link_ids.get(*link_index).map(|s| s.to_string());
            let name = id.clone().unwrap_or_else(|| format!("#{}", link_index + 1));
            RunWarningDto {
                code: "pump-speed-pattern".into(),
                message: format!(
                    "Pump {name}: its speed pattern supersedes the initial speed setting"
                ),
                element_id: id,
            }
        }
    }
}

/// Collect a finished run's warnings as wire DTOs. The wds arm keeps its
/// established codes and element resolution; other engines' warnings pass
/// through the session's neutral shape verbatim.
pub(crate) fn collect_run_warnings(es: &hydra::engines::EngineSession) -> Vec<RunWarningDto> {
    if let Some(sim) = es.as_wds() {
        let node_ids = sim.node_ids();
        let link_ids = sim.link_ids();
        return sim
            .warnings()
            .iter()
            .map(|w| warning_to_dto(w, &node_ids, &link_ids))
            .collect();
    }
    es.warnings()
        .into_iter()
        .map(|w| RunWarningDto {
            code: w.code,
            message: w.message,
            element_id: w.element,
        })
        .collect()
}

/// `run.json` path for the run whose results live at `results_path`.
pub(crate) fn run_meta_path(results_path: &std::path::Path) -> std::path::PathBuf {
    results_path.with_file_name("run.json")
}

/// §14.16 surface-results sidecar beside a results file: `results.out`
/// becomes `results.2d.out`, the CLI's stem convention. Only a mesh run
/// writes one, but the name is fixed by the results path alone so every
/// lifecycle step (publish, clear, size) can address it without asking
/// the engine.
pub(crate) fn surface_results_path(results_path: &std::path::Path) -> std::path::PathBuf {
    let name = results_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match name.strip_suffix(".out") {
        Some(stem) => format!("{stem}.2d.out"),
        None => format!("{name}.2d.out"),
    };
    results_path.with_file_name(name)
}

/// Hydra's own metadata about a run, kept beside the results rather than
/// inside them.
///
/// `results.out` is EPANET's format, and Hydra writing its own fields into it
/// cost interchange for nothing — a reader taking the classic 12-byte tail got
/// a corrupt period count (model spec §4.4.1). This file is where those fields
/// belong: it is ours, it is versionless, and it can grow without touching a
/// format someone else defined.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunMeta {
    /// Topology digest of the network that produced these results, as 16
    /// lowercase hex chars. Lets a consumer detect that the model has been
    /// edited since — including edits that leave the file correctly paired
    /// with its project, which is the case no amount of trusting the pairing
    /// can catch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_digest: Option<String>,
    /// Wall-clock instant the run started, milliseconds since the Unix
    /// epoch. Absent for results written before the field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    /// Wall-clock instant the run finished and its results were published,
    /// milliseconds since the Unix epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
}

/// Read the `run.json` beside `results_path`, or `None` when absent or
/// unreadable — results written before this file existed simply have no
/// metadata, which callers treat as "unknown" rather than as an error.
pub(crate) fn read_run_meta(results_path: &std::path::Path) -> Option<RunMeta> {
    let bytes = std::fs::read(run_meta_path(results_path)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Persist or clear the `run.json` sibling, on the same terms as
/// `sync_run_warnings_file`: metadata must never outlive the results it
/// describes, or it would describe the next run instead.
pub(crate) fn sync_run_meta_file(results_path: &std::path::Path, meta: Option<&RunMeta>) {
    let path = run_meta_path(results_path);
    match meta {
        Some(meta) => match serde_json::to_vec(meta) {
            Ok(bytes) => {
                if let Err(e) = bundle::atomic_write(&path, &bytes) {
                    tracing::warn!(error = %e, "could not write run metadata");
                }
            }
            Err(e) => tracing::warn!(error = %e, "could not serialise run metadata"),
        },
        None => {
            if path.is_file() {
                if let Err(e) = std::fs::remove_file(&path) {
                    tracing::warn!(error = %e, "could not remove stale run metadata");
                }
            }
        }
    }
}

/// `warnings.json` path for the run whose results live at `results_path`.
pub(crate) fn run_warnings_path(results_path: &std::path::Path) -> std::path::PathBuf {
    results_path.with_file_name("warnings.json")
}

/// Persist or clear the `warnings.json` sibling of `results_path`:
/// `Some(warnings)` (successful published run) writes the JSON array
/// atomically; `None` (failed run with no surviving `results.out`) removes
/// any stale file so warnings can never exist without results. Callers keep
/// the file when a failed run leaves a previous run's `results.out` in place
/// — those warnings still describe the results being served. Both directions
/// are best-effort — warnings are diagnostics and must never fail a finished
/// run.
pub(crate) fn sync_run_warnings_file(
    results_path: &std::path::Path,
    warnings: Option<&[RunWarningDto]>,
) {
    let path = run_warnings_path(results_path);
    match warnings {
        Some(w) => {
            let bytes = match serde_json::to_vec(w) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "could not serialise run warnings");
                    return;
                }
            };
            if let Err(e) = bundle::atomic_write(&path, &bytes) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not write run warnings file"
                );
            }
        }
        None => {
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "could not remove stale run warnings file"
                    );
                }
            }
        }
    }
}

/// What to do with `warnings.json` after a run finishes (pure decision seam,
/// unit-tested below):
///
///   - a successful streamed run publishes fresh results → **write** its
///     warnings beside them;
///   - a failed run discards its own stream, so an existing `results.out`
///     still belongs to the previous successful run — as do its warnings,
///     which must be **kept** (clearing them made the Issues panel show
///     "no warnings" for results that were still being served);
///   - a failed run with no surviving results file **clears** any orphaned
///     `warnings.json`, keeping the invariant "warnings never exist without
///     results";
///   - cancellation and never-streamed runs **keep** both files untouched.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WarningsSync {
    Write,
    Clear,
    Keep,
}

pub(crate) fn warnings_sync_after_run(
    run_err: Option<&RunLoopError>,
    streamed: bool,
    results_file_exists: bool,
) -> WarningsSync {
    match run_err {
        None if streamed => WarningsSync::Write,
        Some(RunLoopError::Failed(_)) if !results_file_exists => WarningsSync::Clear,
        _ => WarningsSync::Keep,
    }
}

/// Read a `warnings.json` written by [`sync_run_warnings_file`]. An absent
/// file is an empty warning list (target never run, last run predates warning
/// persistence, or last run failed).
pub(crate) fn read_run_warnings_file(path: &std::path::Path) -> Result<Vec<RunWarningDto>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Cannot read run warnings: {e}")),
    };
    serde_json::from_slice(&bytes).map_err(|e| format!("Malformed warnings file: {e}"))
}

/// Return the non-fatal warnings recorded by the last successful simulation
/// run for `(project_id, scenario_id)` — the contents of the target's
/// `warnings.json`. Empty when the file is absent.
#[tauri::command(async)]
pub fn get_run_warnings(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<RunWarningDto>, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    read_run_warnings_file(&run_warnings_path(&out_path))
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
/// What a run is, apart from the session that performs it and the place
/// its results go.
///
/// These arrived as four more positional parameters and were one too many
/// for a reader to keep straight at a call site, three of them being
/// `f64`, `bool` and `Option<u64>` in a row.
pub(crate) struct RunContext {
    /// Simulated seconds the run covers, for progress reporting.
    pub duration_seconds: f64,
    /// Topology digest of the network being run, recorded beside the
    /// results so a consumer can tell later that the model has been
    /// edited since. `None` for engines whose model has no digest yet.
    pub network_digest: Option<u64>,
    /// Warnings raised before the session existed, opening the model and
    /// reading the files it names. This function owns `warnings.json`, so
    /// it owns the whole set: a notice from the opener would otherwise
    /// have no way to reach the reader, the session's own collection
    /// being the only channel and the session not yet built when the
    /// opener speaks.
    pub pre_run_warnings: Vec<RunWarningDto>,
}

pub(crate) fn run_sim_loops<F, C>(
    mut es: hydra::engines::EngineSession,
    out_path: Option<std::path::PathBuf>,
    run: RunContext,
    emit: F,
    should_cancel: C,
) -> (
    hydra::engines::EngineSession,
    Option<RunLoopError>,
    u64,
    u32,
)
where
    F: Fn(&'static str, f64, bool, bool, Option<String>),
    C: Fn() -> bool,
{
    let RunContext {
        duration_seconds,
        network_digest,
        pre_run_warnings,
    } = run;
    let wall_start = std::time::Instant::now();
    // The wall-clock twin of `wall_start`: `Instant` measures the run,
    // `now_ms` timestamps it for the metadata written beside the results.
    let started_at_ms = crate::meta::now_ms();
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
    let mut streamed = false;
    if let Some(p) = tmp_path.as_ref() {
        if let Some(parent) = p.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "could not create results directory; run will not be persisted"
                );
            }
        }
        match std::fs::File::create(p) {
            // The queue checkpoints a run it may pause, so the engine
            // keeps what one carries (§12.3).
            //
            // Buffered, as every CLI sink is: the results stream writes
            // one scalar at a time, so an unbuffered file costs a
            // syscall per value. Both streams are flushed by
            // `finish_results` before the file is promoted.
            Ok(file) => {
                match es.begin_results(
                    Box::new(std::io::BufWriter::new(file)),
                    hydra::engines::MayCheckpoint::Yes,
                    "",
                    "",
                ) {
                    Ok(()) => streamed = true,
                    Err(e) => {
                        tracing::warn!(
                            path = %p.display(),
                            error = %e,
                            "could not start results stream; run will not be persisted"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "could not create results stream; run will not be persisted"
                );
            }
        }
    }

    // §14.16: a mesh model's surface results stream to a sidecar beside
    // results.out, through the same tmp-then-promote lifecycle. Attached
    // only when the results stream itself started: the sidecar describes
    // the results.out it sits beside, never a run that was not persisted.
    let surface_path = out_path.as_ref().map(|p| surface_results_path(p));
    let surface_tmp = surface_path.as_ref().map(|p| {
        let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".tmp");
        p.with_file_name(name)
    });
    let mut surface_streamed = false;
    if streamed {
        if let (Some(tmp), Some(sim)) = (surface_tmp.as_ref(), es.as_uds_mut()) {
            if sim.has_overland() {
                match std::fs::File::create(tmp) {
                    Ok(file) => {
                        // Buffered for the same reason, and more sharply:
                        // an instant of a 7,500-cell mesh is 30,000
                        // values, which unbuffered took 33 ms of syscalls
                        // to write and buffered takes well under one.
                        match hydra::swmm::session::begin_overland_results(
                            sim,
                            Box::new(std::io::BufWriter::new(file)),
                        ) {
                            Ok(()) => surface_streamed = true,
                            Err(e) => tracing::warn!(
                                path = %tmp.display(),
                                error = %e,
                                "could not start surface results stream"
                            ),
                        }
                    }
                    Err(e) => tracing::warn!(
                        path = %tmp.display(),
                        error = %e,
                        "could not create surface results stream"
                    ),
                }
            }
        }
    }

    let mut simulated_seconds = 0.0_f64;
    let mut last_emit_at = Instant::now();
    let mut last_percent_bucket = -1_i64;
    let mut run_err: Option<RunLoopError> = None;

    // Wire phase codes are the session's phases mapped to the frontend's
    // vocabulary. Both engines now report a single phase: water distribution
    // advances quality alongside its hydraulics rather than in a pass of its
    // own, so there is no longer a separate quality stage to report. The
    // frontend already handles "simulation", which is what drainage runs have
    // always emitted.
    let to_code = |phase: &str| -> &'static str {
        match phase {
            "Hydraulics" => "hydraulics",
            "Water quality" => "quality",
            _ => "simulation",
        }
    };

    let mut phase = to_code(es.phase());
    emit(phase, 0.0, false, false, None);

    while run_err.is_none() {
        if should_cancel() {
            let msg = "Cancelled by user".to_string();
            emit(phase, simulated_seconds, false, true, Some(msg));
            run_err = Some(RunLoopError::Cancelled);
            break;
        }
        match es.advance() {
            Ok(p) => {
                let code = to_code(p.phase);
                if code != phase {
                    // Close the finished phase for the UI, then open the new
                    // one at zero so its progress bar starts fresh.
                    emit(
                        phase,
                        duration_seconds.max(simulated_seconds),
                        false,
                        false,
                        None,
                    );
                    phase = code;
                    last_percent_bucket = -1;
                    last_emit_at = Instant::now();
                    emit(phase, 0.0, false, false, None);
                }
                simulated_seconds = p.t;
                hyd_steps += 1;
                if p.done {
                    emit(
                        phase,
                        duration_seconds.max(simulated_seconds),
                        true,
                        false,
                        None,
                    );
                    break;
                }
                let pct = progress_percent(simulated_seconds, duration_seconds);
                let bucket = pct.floor() as i64;
                if bucket != last_percent_bucket || last_emit_at.elapsed() >= PROGRESS_EMIT_INTERVAL
                {
                    emit(phase, simulated_seconds, false, false, None);
                    last_percent_bucket = bucket;
                    last_emit_at = Instant::now();
                }
            }
            Err(e) => {
                let msg = match e {
                    hydra::engines::AdvanceError::Io(err) => {
                        format!("simulation results could not be written: {err}")
                    }
                    other => other.to_string(),
                };
                emit(phase, simulated_seconds, false, true, Some(msg.clone()));
                run_err = Some(RunLoopError::Failed(msg));
                break;
            }
        }
    }

    if streamed {
        if let Err(e) = es.finish_results() {
            if run_err.is_none() {
                // Promoting a stream missing its epilogue would publish a
                // corrupt results.out — abort as Failed instead.
                let msg = format!("simulation finished but results could not be written: {e}");
                emit(phase, simulated_seconds, false, true, Some(msg.clone()));
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

    // The sidecar follows the results file it describes: promoted with a
    // published results.out, discarded on failure or cancel (the previous
    // pair survives together), and *cleared* when a published run wrote
    // none — a stale surface beside fresh results must never be served.
    if let (Some(tmp), Some(final_path)) = (surface_tmp.as_ref(), surface_path.as_ref()) {
        if streamed && run_err.is_none() {
            if surface_streamed {
                if let Err(e) = std::fs::rename(tmp, final_path) {
                    // The primary artifact is published and stands; the
                    // surface alone is lost, loudly, with no stale file
                    // left masquerading as this run's.
                    remove_tmp_or_warn(tmp);
                    let _ = std::fs::remove_file(final_path);
                    tracing::warn!(
                        path = %final_path.display(),
                        error = %e,
                        "surface results could not be published"
                    );
                }
            } else {
                let _ = std::fs::remove_file(final_path);
            }
        } else if surface_streamed {
            remove_tmp_or_warn(tmp);
        }
    }

    // Persist the run's non-fatal warnings beside results.out (see
    // `warnings_sync_after_run` for the full decision rationale).
    if let Some(final_path) = out_path.as_ref() {
        match warnings_sync_after_run(run_err.as_ref(), streamed, final_path.is_file()) {
            WarningsSync::Write => {
                let mut warnings = pre_run_warnings;
                warnings.extend(collect_run_warnings(&es));
                sync_run_warnings_file(final_path, Some(&warnings));
                // Same lifecycle as the warnings: this describes the results
                // just published, so it is written and cleared with them.
                sync_run_meta_file(
                    final_path,
                    Some(&RunMeta {
                        network_digest: network_digest.map(crate::commands::results::digest_hex),
                        started_at_ms: Some(started_at_ms),
                        finished_at_ms: Some(crate::meta::now_ms()),
                    }),
                );
            }
            WarningsSync::Clear => {
                sync_run_warnings_file(final_path, None);
                sync_run_meta_file(final_path, None);
            }
            WarningsSync::Keep => {}
        }
    }

    (
        es,
        run_err,
        wall_start.elapsed().as_millis() as u64,
        hyd_steps,
    )
}

/// Simulation targets (project/scenario pairs) whose `results.out` is
/// currently being written by the queue processor. The processor drains items
/// one at a time, so this is belt-and-braces against a future second writer
/// rather than a live race — but a corrupted results file is expensive enough
/// to keep the guard.
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
/// simulation is already writing to the same target.
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

    // ── run metadata ──────────────────────────────────────────────────────

    #[test]
    fn a_pre_timestamp_run_json_still_parses_with_unknown_instants() {
        // run.json is versionless and grows by addition: a file written
        // before the wall-clock fields existed must read as "unknown", not
        // fail, or old results would lose their digest too.
        let meta: RunMeta =
            serde_json::from_slice(br#"{"networkDigest":"0000000000000abc"}"#).unwrap();
        assert_eq!(meta.network_digest.as_deref(), Some("0000000000000abc"));
        assert_eq!(meta.started_at_ms, None);
        assert_eq!(meta.finished_at_ms, None);
    }

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

    /// A uds session runs through the same loop the queue uses: results.out
    /// is published in the SWMM binary layout, phase events use the
    /// "simulation" code, and run.json carries no digest.
    #[test]
    fn run_sim_loops_runs_a_uds_session_end_to_end() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\nFLOW_ROUTING DYNWAVE\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 01:00:00\nREPORT_STEP 00:05:00\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [REPORT]\nNODES ALL\nLINKS ALL\n";
        let (sim, _diags, _findings) = hydra::swmm::session::open(model).expect("open uds model");

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let phases = std::sync::Mutex::new(Vec::new());
        let (_es, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_uds(sim),
            Some(out.clone()),
            RunContext {
                duration_seconds: 3600.0,
                network_digest: None,
                pre_run_warnings: Vec::new(),
            },
            |phase, _, _, _, _| phases.lock().unwrap().push(phase),
            || false,
        );
        assert!(err.is_none(), "uds run must succeed: {err:?}");
        assert!(out.exists(), "successful run must publish results.out");

        // SWMM layout: leading magic number 516114522.
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(
            i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            516_114_522
        );
        // And the engine's own reader accepts what the loop published.
        let meta = hydra::swmm::out_reader::read_metadata(&out).expect("readable");
        assert!(meta.n_periods > 0);

        assert!(
            phases.lock().unwrap().iter().all(|p| *p == "simulation"),
            "uds runs report the single 'simulation' phase"
        );
        let run_meta = read_run_meta(&out).expect("run.json written");
        assert!(
            run_meta.network_digest.is_none(),
            "uds runs carry no digest"
        );
    }

    // ── §14.16 surface sidecar lifecycle ──────────────────────────────────

    #[test]
    fn surface_results_path_is_the_cli_stem_convention() {
        let p = surface_results_path(std::path::Path::new("/a/b/results.out"));
        assert_eq!(p, std::path::Path::new("/a/b/results.2d.out"));
        // A name without .out still gets a distinct sidecar name.
        let p = surface_results_path(std::path::Path::new("/a/b/results"));
        assert_eq!(p, std::path::Path::new("/a/b/results.2d.out"));
    }

    /// A mesh model run through the queue's loop publishes the §14.16
    /// sidecar beside results.out, complete and readable, with no tmp
    /// left behind.
    #[test]
    fn run_sim_loops_publishes_the_surface_sidecar_for_a_mesh_model() {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\nFLOW_ROUTING DYNWAVE\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 00:10:00\nREPORT_STEP 00:05:00\n\
                     [2D_VERTICES]\n0 0 10.0\n1 0 10.2\n1 1 10.4\n0 1 10.6\n\
                     [2D_TRIANGLES]\n0 1 2 0.02 0.05\n0 2 3 0.03 0.05\n\
                     [2D_VERTEX_NODE_MAP]\n0 J1\n\
                     [JUNCTIONS]\nJ1 9 4 0 0 0\n[OUTFALLS]\nO1 8 FREE\n\
                     [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n\
                     [REPORT]\nNODES ALL\nLINKS ALL\n";
        let (sim, _diags, _findings) = hydra::swmm::session::open(model).expect("open mesh model");
        assert!(sim.has_overland(), "the model must carry a mesh");

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_es, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_uds(sim),
            Some(out.clone()),
            RunContext {
                duration_seconds: 600.0,
                network_digest: None,
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "mesh run must succeed: {err:?}");
        assert!(out.exists(), "results.out published");
        let sidecar = surface_results_path(&out);
        assert!(sidecar.exists(), "surface sidecar published beside it");
        assert!(
            !surface_results_path(&out)
                .with_file_name("results.2d.out.tmp")
                .exists(),
            "no surface tmp left behind"
        );
        // Complete and readable: the engine's own reader accepts it and
        // the record count matches the reporting clock.
        let r = hydra::swmm::out_reader::OverlandResults::open(&sidecar).expect("readable");
        assert_eq!(r.cells.len(), 2);
        assert_eq!(r.verts.len(), 4);
        assert!(r.periods > 0, "records written");
    }

    /// A published run that wrote no surface clears a stale sidecar: a
    /// previous mesh run's surface must never be served beside results
    /// it does not describe (the model may have lost its mesh).
    #[test]
    fn run_sim_loops_clears_a_stale_sidecar_on_a_meshless_success() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let stale = surface_results_path(&out);
        std::fs::write(&stale, b"previous mesh run's surface").unwrap();
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out.clone()),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "run must succeed: {err:?}");
        assert!(out.exists(), "results.out published");
        assert!(!stale.exists(), "stale surface sidecar cleared");
    }

    /// A cancelled run keeps the previous pair together: results.out and
    /// its sidecar both survive untouched.
    #[test]
    fn run_sim_loops_cancel_keeps_the_previous_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        std::fs::write(&out, b"previous successful run").unwrap();
        let sidecar = surface_results_path(&out);
        std::fs::write(&sidecar, b"previous run's surface").unwrap();
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out.clone()),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || true, // cancel immediately
        );
        assert!(matches!(err, Some(RunLoopError::Cancelled)));
        assert!(out.exists(), "previous results survive a cancel");
        assert!(sidecar.exists(), "previous surface survives with them");
    }

    // ── run_sim_loops results.out tmp/rename flow ─────────────────────────
    #[test]
    fn run_sim_loops_promotes_results_only_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out.clone()),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
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
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out.clone()),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
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

    // ── run warnings ──────────────────────────────────────────────────────

    #[test]
    fn warning_kind_maps_to_stable_codes_and_wire_shape() {
        use hydra::{SimWarning, WarningKind};
        let node_ids = ["J1", "J2"];
        let link_ids = ["P1", "PU1"];

        let w = warning_to_dto(
            &SimWarning {
                t: 3661.0,
                kind: WarningKind::UnbalancedHydraulics,
            },
            &node_ids,
            &link_ids,
        );
        assert_eq!(w.code, "unbalanced-hydraulics");
        assert_eq!(w.element_id, None);
        assert!(
            w.message.contains("1:01:01"),
            "time in message: {}",
            w.message
        );
        // Pinned wire shape: camelCase keys, explicit null for elementId.
        let json = serde_json::to_string(&w).unwrap();
        assert!(
            json.contains("\"code\":\"unbalanced-hydraulics\""),
            "{json}"
        );
        assert!(json.contains("\"message\":"), "{json}");
        assert!(json.contains("\"elementId\":null"), "{json}");

        let w = warning_to_dto(
            &SimWarning {
                t: 0.0,
                kind: WarningKind::NegativePressure { node_index: 1 },
            },
            &node_ids,
            &link_ids,
        );
        assert_eq!(w.code, "negative-pressure");
        assert_eq!(w.element_id.as_deref(), Some("J2"));
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("\"elementId\":\"J2\""), "{json}");

        let w = warning_to_dto(
            &SimWarning {
                t: 0.0,
                kind: WarningKind::PumpXHead { link_index: 1 },
            },
            &node_ids,
            &link_ids,
        );
        assert_eq!(w.code, "pump-x-head");
        assert_eq!(w.element_id.as_deref(), Some("PU1"));
    }

    #[test]
    fn read_run_warnings_file_absent_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("warnings.json");
        assert_eq!(
            read_run_warnings_file(&missing).unwrap(),
            Vec::<RunWarningDto>::new()
        );
    }

    #[test]
    fn run_sim_loops_writes_warnings_json_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        // Stale warnings from an earlier run must be overwritten, not merged.
        std::fs::write(dir.path().join("warnings.json"), b"[{\"bogus\":1}]").unwrap();
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "steady-state run must succeed: {err:?}");
        let warnings = read_run_warnings_file(&dir.path().join("warnings.json")).unwrap();
        assert!(
            warnings.is_empty(),
            "steady-state fixture yields no warnings: {warnings:?}"
        );
    }

    /// A warning raised before the session existed reaches the reader.
    ///
    /// Opening a model reads the files it names, and that can have
    /// something to say: an accumulation period spread evenly across the
    /// hours it covers, for one. The session is not built yet when the
    /// opener speaks, and the session's own collection is the only other
    /// channel, so `warnings.json` has to carry both or the opener's are
    /// lost.
    #[test]
    fn run_sim_loops_carries_warnings_raised_before_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let opening = vec![RunWarningDto {
            code: "rain-record".to_string(),
            message: "rain record \"acc.dat\": an accumulated total was divided evenly".to_string(),
            element_id: None,
        }];
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: opening.clone(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "steady-state run must succeed: {err:?}");
        let written = read_run_warnings_file(&dir.path().join("warnings.json")).unwrap();
        assert_eq!(
            vec![opening[0].code.clone()],
            written.iter().map(|w| w.code.clone()).collect::<Vec<_>>(),
            "the opener's warning should stand beside the session's: {written:?}"
        );
    }

    #[test]
    fn run_sim_loops_records_negative_pressure_warning_end_to_end() {
        // Junction 100 ft above the reservoir head with positive demand →
        // DDA negative-pressure warning attributed to J1.
        const NEG_PRESSURE_INP: &str = "\
[JUNCTIONS]
J1  200  5

[RESERVOIRS]
R1  100

[PIPES]
P1  R1  J1  1000  12  100  0  Open

[COORDINATES]
J1  1.0  2.0
R1  0.0  0.0

[OPTIONS]
Units  GPM

[TIMES]
Duration  0

[END]
";
        let network = hydra::io::parse(NEG_PRESSURE_INP.as_bytes()).unwrap();
        let mut sim = hydra::Simulation::create();
        sim.load(network).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(sim, hydra::FlowUnits::Lps),
            Some(out),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(err.is_none(), "run must succeed with a warning: {err:?}");
        let warnings = read_run_warnings_file(&dir.path().join("warnings.json")).unwrap();
        assert!(
            warnings
                .iter()
                .any(|w| w.code == "negative-pressure" && w.element_id.as_deref() == Some("J1")),
            "expected a negative-pressure warning for J1, got: {warnings:?}"
        );
    }

    #[test]
    fn run_sim_loops_failed_run_discards_stale_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        // Occupying the rename target with a directory makes the promote step
        // fail deterministically, driving the run to Failed after streaming.
        std::fs::create_dir(&out).unwrap();
        std::fs::write(dir.path().join("warnings.json"), b"[]").unwrap();
        let (_sim, err, _wall, _steps) = run_sim_loops(
            hydra::engines::EngineSession::from_wds(loaded_sim(), hydra::FlowUnits::Lps),
            Some(out),
            RunContext {
                duration_seconds: 0.0,
                network_digest: Some(0),
                pre_run_warnings: Vec::new(),
            },
            |_, _, _, _, _| {},
            || false,
        );
        assert!(matches!(err, Some(RunLoopError::Failed(_))), "{err:?}");
        // No results *file* survives (the path is a directory), so the
        // orphaned warnings.json must go — warnings never without results.
        assert!(
            !dir.path().join("warnings.json").exists(),
            "stale warnings.json must be removed on a failed run without results"
        );
    }

    /// Full decision matrix for [`warnings_sync_after_run`] — most
    /// importantly: a failed run whose previous `results.out` survives must
    /// KEEP that file's warnings paired with it, not clear them.
    #[test]
    fn warnings_sync_after_run_decision_matrix() {
        let failed = RunLoopError::Failed("boom".into());
        // Success + streamed publishes fresh warnings, with or without a
        // pre-existing results file (rename already replaced it).
        assert_eq!(
            warnings_sync_after_run(None, true, true),
            WarningsSync::Write
        );
        assert_eq!(
            warnings_sync_after_run(None, true, false),
            WarningsSync::Write
        );
        // Success without a stream (results dir unavailable): nothing was
        // published, so the previous pairing is left alone.
        assert_eq!(
            warnings_sync_after_run(None, false, true),
            WarningsSync::Keep
        );
        // Failure with surviving previous results: keep their warnings.
        assert_eq!(
            warnings_sync_after_run(Some(&failed), true, true),
            WarningsSync::Keep
        );
        // Failure with no surviving results file: clear orphaned warnings.
        assert_eq!(
            warnings_sync_after_run(Some(&failed), true, false),
            WarningsSync::Clear
        );
        assert_eq!(
            warnings_sync_after_run(Some(&failed), false, false),
            WarningsSync::Clear
        );
        // Cancellation never touches the files.
        assert_eq!(
            warnings_sync_after_run(Some(&RunLoopError::Cancelled), true, true),
            WarningsSync::Keep
        );
        assert_eq!(
            warnings_sync_after_run(Some(&RunLoopError::Cancelled), true, false),
            WarningsSync::Keep
        );
    }

    #[test]
    fn sync_run_warnings_file_round_trips_and_tolerates_absent_removal() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("results.out");
        let warnings = vec![RunWarningDto {
            code: "pump-x-head".into(),
            message: "Pump PU1 operating outside its head curve at 0:00:00".into(),
            element_id: Some("PU1".into()),
        }];
        sync_run_warnings_file(&out, Some(&warnings));
        assert_eq!(
            read_run_warnings_file(&dir.path().join("warnings.json")).unwrap(),
            warnings
        );
        // Failed-run direction removes the file; a second removal is a no-op.
        sync_run_warnings_file(&out, None);
        assert!(!dir.path().join("warnings.json").exists());
        sync_run_warnings_file(&out, None);
    }
}
