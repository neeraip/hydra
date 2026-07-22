//! Tauri command surface for the Hydra GUI.
//!
//! # Storage layout
//!
//! The filesystem is the sole source of truth — there is no database.
//! `project_id` and `scenario_id` are directory names (UUIDs) and are never
//! stored inside `meta.json`; they are always derived from the directory name.
//!
//! ```text
//! <app_data>/
//!   projects/
//!     <project_id>/
//!       meta.json               ← project metadata (name, CRS, node/link counts)
//!       base/
//!         model.inp             ← INP text for the base model
//!         results.out           ← binary simulation output (present when run)
//!         reports/
//!       scenarios/
//!         <scenario_id>/
//!           meta.json           ← scenario metadata (name)
//!           model.inp
//!           results.out         ← present when run
//!           reports/
//! ```
//!
//! `modified_at` is the mtime of `base/model.inp` (falling back to the project
//! directory mtime). `last_run_at` is the mtime of `base/results.out` when it
//! exists. `scenario_count` is the count of subdirectories under `scenarios/`.
//!
//! # Backend events
//!
//! | Event | Payload | Throttle |
//! |---|---|---|
//! | `simulation_progress` | `SimulationProgressDto` | ≤1 emit per 125 ms + always on completion/failure |
//! | `run_queue_update` | `Vec<RunQueueItemDto>` | on every queue state change |
//! | `network-changed` | `null` | on every mutating command (`patch_element`, `patch_node_position`, `delete_element`) |
//!
//! # Run queue
//!
//! The queue processor is a single background task; at most one simulation runs
//! at a time. Items are processed FIFO. Each item transitions:
//! `queued` → `running` → `done` | `failed` | `cancelled`.
//! Cancellation of a running item is advisory: the current simulation step
//! completes before the item is marked cancelled. Closing the application
//! discards all pending items (queue state is not persisted).

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

use crate::meta::{self, bundle};

const SIMULATION_PROGRESS_EVENT: &str = "simulation_progress";
const RUN_QUEUE_UPDATE_EVENT: &str = "run_queue_update";
const NETWORK_CHANGED_EVENT: &str = "network-changed";
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(125);
const RUN_QUEUE_TERMINAL_TTL_SECS: i64 = 6 * 60 * 60;
const RUN_QUEUE_TERMINAL_MAX_PER_PROJECT: usize = 100;

fn is_terminal_queue_status(status: &str) -> bool {
    status == "done" || status == "failed" || status == "cancelled"
}

/// In-memory run queue. Replaces the `run_queue` DB table.
///
/// Holds the ordered list of run items for the current session and a flag
/// tracking whether the background processor is active. Queue state is
/// intentionally transient: closing the application clears all pending items.
#[derive(Default)]
pub struct RunQueue {
    inner: std::sync::Mutex<RunQueueInner>,
}

#[derive(Default)]
struct RunQueueInner {
    items: Vec<RunQueueItem>,
    processor_running: bool,
    cancel_requested: std::collections::HashSet<String>,
}

#[derive(Clone)]
struct RunQueueItem {
    id: String,
    project_id: String,
    /// `None` = base model; `Some(id)` = scenario UUID.
    target_id: Option<String>,
    /// Resolved scenario name, or `None` for the base model.
    target_name: Option<String>,
    /// "queued" | "running" | "done" | "failed" | "cancelled"
    status: String,
    queued_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    error: Option<String>,
}

impl RunQueue {
    fn enqueue(&self, item: RunQueueItem) {
        let mut g = self.inner.lock().unwrap();
        g.items.push(item);
        Self::prune_terminal_history_locked(&mut g, meta::now_secs());
    }

    /// Atomically claim the processor slot. Returns `true` if the caller
    /// should spawn the queue processor (i.e. it was not already running).
    pub fn try_claim_processor(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.processor_running {
            return false;
        }
        g.processor_running = true;
        true
    }

    pub fn release_processor(&self) {
        self.inner.lock().unwrap().processor_running = false;
    }

    fn get_for_project(&self, project_id: &str) -> Vec<RunQueueItem> {
        self.inner
            .lock()
            .unwrap()
            .items
            .iter()
            .filter(|i| i.project_id == project_id)
            .cloned()
            .collect()
    }

    /// Returns cloned data of the next "queued" item (global FIFO across all projects).
    fn next_queued(&self) -> Option<RunQueueItem> {
        self.inner
            .lock()
            .unwrap()
            .items
            .iter()
            .find(|i| i.status == "queued")
            .cloned()
    }

    fn mark_running(&self, id: &str, started_at: i64) {
        let mut g = self.inner.lock().unwrap();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "running".into();
            item.started_at = Some(started_at);
        }
    }

    fn mark_done(&self, id: &str, finished_at: i64) {
        let mut g = self.inner.lock().unwrap();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "done".into();
            item.finished_at = Some(finished_at);
        }
        g.cancel_requested.remove(id);
        Self::prune_terminal_history_locked(&mut g, finished_at);
    }

    fn mark_failed(&self, id: &str, finished_at: i64, error: &str) {
        let mut g = self.inner.lock().unwrap();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "failed".into();
            item.finished_at = Some(finished_at);
            item.error = Some(error.into());
        }
        g.cancel_requested.remove(id);
        Self::prune_terminal_history_locked(&mut g, finished_at);
    }

    fn mark_cancelled(&self, id: &str, finished_at: i64) {
        let mut g = self.inner.lock().unwrap();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "cancelled".into();
            item.finished_at = Some(finished_at);
        }
        g.cancel_requested.remove(id);
        Self::prune_terminal_history_locked(&mut g, finished_at);
    }

    /// Cancel all queued items and request cancellation for running items
    /// for `project_id`. Returns number of affected items.
    fn cancel_for_project(&self, project_id: &str) -> u32 {
        let mut count = 0u32;
        let mut g = self.inner.lock().unwrap();
        let mut running_ids: Vec<String> = Vec::new();
        for item in g.items.iter_mut() {
            if item.project_id == project_id && item.status == "queued" {
                item.status = "cancelled".into();
                count += 1;
            } else if item.project_id == project_id && item.status == "running" {
                running_ids.push(item.id.clone());
            }
        }
        for id in running_ids {
            if g.cancel_requested.insert(id) {
                count += 1;
            }
        }
        if count > 0 {
            Self::prune_terminal_history_locked(&mut g, meta::now_secs());
        }
        count
    }

    /// Cancel a single queued item, or request cancellation for a running item.
    /// Returns `(cancelled_or_requested, project_id)`.
    fn cancel_item(&self, id: &str) -> (bool, Option<String>) {
        let mut g = self.inner.lock().unwrap();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            let pid = item.project_id.clone();
            if item.status == "queued" {
                item.status = "cancelled".into();
                Self::prune_terminal_history_locked(&mut g, meta::now_secs());
                return (true, Some(pid));
            }
            if item.status == "running" {
                let run_id = item.id.clone();
                let accepted = g.cancel_requested.insert(run_id);
                return (accepted, Some(pid));
            }
            return (false, Some(pid));
        }
        (false, None)
    }

    fn is_cancel_requested(&self, id: &str) -> bool {
        self.inner.lock().unwrap().cancel_requested.contains(id)
    }

    /// Keep active queue items forever, but cap terminal history growth so
    /// long-lived sessions don't accumulate unbounded memory and response size.
    fn prune_terminal_history_locked(g: &mut RunQueueInner, now_secs: i64) {
        g.items.retain(|item| {
            if !is_terminal_queue_status(&item.status) {
                return true;
            }
            let finished = item.finished_at.unwrap_or(item.queued_at);
            now_secs.saturating_sub(finished) <= RUN_QUEUE_TERMINAL_TTL_SECS
        });

        let mut per_project_terminal_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for item in &g.items {
            if is_terminal_queue_status(&item.status) {
                *per_project_terminal_counts
                    .entry(item.project_id.clone())
                    .or_insert(0) += 1;
            }
        }

        if per_project_terminal_counts
            .values()
            .all(|count| *count <= RUN_QUEUE_TERMINAL_MAX_PER_PROJECT)
        {
            return;
        }

        // Drop oldest terminal items first (vector order is append order),
        // keeping only the newest N terminal entries per project.
        g.items.retain(|item| {
            if !is_terminal_queue_status(&item.status) {
                return true;
            }
            let Some(count) = per_project_terminal_counts.get_mut(&item.project_id) else {
                return false;
            };
            if *count > RUN_QUEUE_TERMINAL_MAX_PER_PROJECT {
                *count -= 1;
                return false;
            }
            true
        });
    }
}

/// Flat run-queue item returned to the frontend for the task tray.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunQueueItemDto {
    pub id: String,
    pub project_id: String,
    /// `None` = base model; `Some(id)` = scenario UUID.
    pub target_id: Option<String>,
    /// Human-readable scenario name, or `None` for the base model.
    pub target_name: Option<String>,
    /// "queued" | "running" | "done" | "failed" | "cancelled"
    pub status: String,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SimulationProgressDto {
    /// The run-queue item UUID, present only for queue-sourced runs.
    run_id: Option<String>,
    phase: &'static str,
    simulated_seconds: f64,
    duration_seconds: f64,
    percent: f64,
    done: bool,
    failed: bool,
    message: Option<String>,
    /// Whether water-quality is enabled for this simulation.
    run_quality: bool,
}

#[derive(Debug, Clone)]
enum RunLoopError {
    Failed(String),
    Cancelled,
}

fn progress_percent(simulated_seconds: f64, duration_seconds: f64) -> f64 {
    if duration_seconds > 0.0 {
        (100.0 * simulated_seconds / duration_seconds).clamp(0.0, 100.0)
    } else {
        100.0
    }
}

/// Run the hydraulics and (optionally) quality loops on a pre-loaded simulation.
///
/// Streams incremental results to `out_path` (when `Some`) and calls `emit` with
/// progress updates after each significant step. Returns `(sim, Some(error))`
/// on failure and `(sim, None)` on success.
///
/// Designed to be called inside `tauri::async_runtime::spawn_blocking`.
fn run_sim_loops<F, C>(
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
    let mut out_writer = out_path.as_ref().and_then(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::File::create(p).ok()?;
        hydra::io::out_writer::OutStreamWriter::begin(file, &sim, "", "", hydra::FlowUnits::Lps)
            .ok()
    });

    if let Some(w) = out_writer.as_mut() {
        let _ = w.append_available(&sim);
    }

    let mut simulated_seconds = 0.0_f64;
    let mut last_emit_at = Instant::now();
    let mut last_percent_bucket = -1_i64;
    let mut run_err: Option<RunLoopError> = None;

    emit("hydraulics", 0.0, false, false, None);

    loop {
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
                    let _ = w.append_available(&sim);
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
            let _ = w.append_available(&sim);
        }
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

    if let Some(w) = out_writer {
        let _ = w.finish(&sim);
    }

    (
        sim,
        run_err,
        wall_start.elapsed().as_millis() as u64,
        hyd_steps,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInsights {
    pub min_pressure: f64,
    pub min_pressure_node: String,
    pub max_velocity: f64,
    pub pump_energy: f64,
    pub warning_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub scenario_count: u32,
    pub state: String,
    pub modified_label: String,
    /// Relative label for the last completed simulation, e.g. "2h ago".
    /// `None` when the project has never been simulated.
    pub last_run_label: Option<String>,
    pub node_count: u32,
    pub link_count: u32,
    /// EPSG code for the coordinate reference system of the INP \[COORDINATES\].
    pub source_crs: String,
    pub insights: Option<ProjectInsights>,
    /// `true` when the DB row exists but the on-disk bundle directory is absent.
    /// The frontend renders these rows muted and offers "Remove from list"
    /// instead of "Open folder".
    pub folder_missing: bool,
}

#[tauri::command]
/// Scan the `projects/` directory and return all projects with their metadata.
pub fn list_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let app_data = app_data_dir(&app)?;
    let projects_root = bundle::projects_root(&app_data);
    if !projects_root.exists() {
        return Ok(vec![]);
    }
    let mut projects = Vec::new();
    let entries = std::fs::read_dir(&projects_root).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let meta = match meta::read_project_meta(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let scenario_count = count_scenario_dirs(&app_data, &id);
        let results_path = bundle::base_results_path(&app_data, &id);
        let sim_state = meta::sim_state_from_results(&results_path);
        let last_run_at = if results_path.exists() {
            meta::mtime_secs(&results_path)
        } else {
            None
        };
        let modified_at = meta::mtime_secs(&bundle::base_model_path(&app_data, &id))
            .or_else(|| meta::mtime_secs(&path))
            .unwrap_or_else(meta::now_secs);
        projects.push(project_to_dto(
            &id,
            &meta,
            scenario_count,
            last_run_at,
            sim_state,
            false,
            modified_at,
        ));
    }
    // Sort most-recently-modified first.
    projects.sort_by(|a, b| b.modified_label.cmp(&a.modified_label));
    Ok(projects)
}

/// Persist a new project. Called from the frontend's "New Project" wizard
/// once a network has been loaded into [`NetworkState`]. The INP bytes
/// currently held in managed state are copied into the bundle as the
/// project's canonical base model so the bundle is self-contained on disk
/// even if the original source file is later moved or deleted.
#[tauri::command]
/// Create a new project directory with `meta.json` and `base/` subdirectories.
pub fn create_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    name: String,
) -> Result<Project, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;

    // Snapshot the currently loaded network (if any).
    let (inp_bytes, node_count, link_count) = match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { raw_bytes, dto, .. } => (
            Some(raw_bytes.clone()),
            dto.nodes.len() as u32,
            dto.links.len() as u32,
        ),
        NetworkStateInner::Empty => (None, 0, 0),
    };

    let project_dir = bundle::project_dir(&app_data, &id);
    let base_dir = bundle::base_dir(&app_data, &id);
    let scenarios_dir = project_dir.join("scenarios");
    std::fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&scenarios_dir).map_err(|e| e.to_string())?;

    let meta = meta::ProjectMeta {
        name,
        description: None,
        source_crs: "EPSG:4326".into(),
        node_count,
        link_count,
        analysis_options: None,
    };
    meta::write_project_meta(&project_dir, &meta)?;

    if let Some(bytes) = inp_bytes {
        bundle::atomic_write(&bundle::base_model_path(&app_data, &id), &bytes)
            .map_err(|e| e.to_string())?;
    }

    let modified_at = meta::mtime_secs(&bundle::base_model_path(&app_data, &id))
        .or_else(|| meta::mtime_secs(&project_dir))
        .unwrap_or_else(meta::now_secs);
    Ok(project_to_dto(
        &id,
        &meta,
        0,
        None,
        "not-run",
        false,
        modified_at,
    ))
}

/// Result returned by `load_project`: the persisted row, plus the network it
/// carries (if any). The network is parsed during the load and stashed in
/// `NetworkState` so subsequent `get_nodes` / `get_links` / `run_simulation`
/// calls operate on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedProject {
    pub project: Project,
    /// `None` when the bundle has no `base/model.inp` (draft project).
    pub network: Option<NetworkDto>,
}

/// Open an existing project from disk. Reads the metadata, and if a base model
/// is present, parses it into [`NetworkState`] so the rest of the app can read
/// nodes/links/results from it.
#[tauri::command]
/// Read project `meta.json` and derive simulation state from `results.out` presence.
pub fn load_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<Option<LoadedProject>, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;
    let project_dir = bundle::project_dir(&app_data, &id);
    if !project_dir.exists() {
        return Ok(None);
    }
    let meta = match meta::read_project_meta(&project_dir) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let scenario_count = count_scenario_dirs(&app_data, &id);
    let results_path = bundle::base_results_path(&app_data, &id);
    let sim_state = meta::sim_state_from_results(&results_path);
    let last_run_at = if results_path.exists() {
        meta::mtime_secs(&results_path)
    } else {
        None
    };
    let modified_at = meta::mtime_secs(&bundle::base_model_path(&app_data, &id))
        .or_else(|| meta::mtime_secs(&project_dir))
        .unwrap_or_else(meta::now_secs);

    // If the bundle has a base model on disk, parse it and populate state.
    let model_path = bundle::base_model_path(&app_data, &id);
    let (network, _raw_net) = if model_path.exists() {
        let bytes = std::fs::read(&model_path).map_err(|e| e.to_string())?;
        let net = hydra::io::parse(&bytes).map_err(|e| format!("{e:?}"))?;
        let dto = network_to_dto(&net);
        *state.0.lock().unwrap() = NetworkStateInner::Loaded {
            raw_bytes: bytes,
            network: net.clone(),
            dto: dto.clone(),
        };
        (Some(dto), Some(net))
    } else {
        *state.0.lock().unwrap() = NetworkStateInner::Empty;
        (None, None)
    };

    Ok(Some(LoadedProject {
        project: project_to_dto(
            &id,
            &meta,
            scenario_count,
            last_run_at,
            sim_state,
            false,
            modified_at,
        ),
        network,
    }))
}

/// Permanently delete a project. Returns `true` when the directory was removed,
/// `false` when the id was not found on disk.
#[tauri::command]
/// Remove the project directory tree.
pub fn delete_project(app: tauri::AppHandle, id: String) -> Result<bool, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;
    let dir = bundle::project_dir(&app_data, &id);
    if !dir.exists() {
        return Ok(false);
    }
    bundle::delete_project_dir(&app_data, &id).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Rename a project. Returns the updated DTO, or `None` when the project is
/// not found on disk.
#[tauri::command]
/// Update the `name` field in project `meta.json`.
pub fn rename_project(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<Option<Project>, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;
    let project_dir = bundle::project_dir(&app_data, &id);
    if !project_dir.exists() {
        return Ok(None);
    }
    let mut project_meta = meta::read_project_meta(&project_dir)?;
    project_meta.name = name;
    meta::write_project_meta(&project_dir, &project_meta)?;
    let scenario_count = count_scenario_dirs(&app_data, &id);
    let results_path = bundle::base_results_path(&app_data, &id);
    let sim_state = meta::sim_state_from_results(&results_path);
    let last_run_at = if results_path.exists() {
        meta::mtime_secs(&results_path)
    } else {
        None
    };
    let modified_at = meta::mtime_secs(&bundle::base_model_path(&app_data, &id))
        .or_else(|| meta::mtime_secs(&project_dir))
        .unwrap_or_else(meta::now_secs);
    Ok(Some(project_to_dto(
        &id,
        &project_meta,
        scenario_count,
        last_run_at,
        sim_state,
        false,
        modified_at,
    )))
}

/// Update the source CRS for a project. Returns `true` when the metadata was
/// updated, `false` when the project is not found on disk.
#[tauri::command]
/// Update the `source_crs` field in project `meta.json`.
pub fn update_project_crs(app: tauri::AppHandle, id: String, crs: String) -> Result<bool, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;
    let project_dir = bundle::project_dir(&app_data, &id);
    if !project_dir.exists() {
        return Ok(false);
    }
    let mut project_meta = meta::read_project_meta(&project_dir)?;
    project_meta.source_crs = crs;
    meta::write_project_meta(&project_dir, &project_meta)?;
    Ok(true)
}

/// Persisted custom CRS definition shared across all projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCrsDef {
    pub label: String,
    pub epsg: String,
    pub proj4: String,
}

#[derive(Debug, Clone)]
struct CuratedCrsDef {
    label: String,
    epsg: String,
    proj4: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrsCatalogEntry {
    pub label: String,
    pub epsg: String,
    pub proj4: String,
    pub custom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrsCatalogPage {
    pub items: Vec<CrsCatalogEntry>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
    pub has_more: bool,
}

fn parse_wkt_label(wkt: &str, epsg: &str) -> String {
    if let Some(start) = wkt.find('"') {
        let rest = &wkt[(start + 1)..];
        if let Some(end) = rest.find('"') {
            let name = rest[..end].trim();
            if !name.is_empty() {
                return format!("{} ({})", name, epsg);
            }
        }
    }
    epsg.to_string()
}

fn curated_crs_defs() -> &'static Vec<CuratedCrsDef> {
    static CACHE: std::sync::OnceLock<Vec<CuratedCrsDef>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let raw = include_str!("../resources/crs-catalog.json");
        let parsed = serde_json::from_str::<std::collections::BTreeMap<String, String>>(raw);
        match parsed {
            Ok(entries) => entries
                .into_iter()
                .map(|(epsg, proj4)| {
                    let normalized = normalize_epsg(&epsg);
                    CuratedCrsDef {
                        label: parse_wkt_label(&proj4, &normalized),
                        epsg: normalized,
                        proj4,
                    }
                })
                .collect(),
            Err(_) => vec![],
        }
    })
}

fn custom_to_catalog_entry(def: CustomCrsDef) -> CrsCatalogEntry {
    let epsg = normalize_epsg(&def.epsg);
    let label = def.label.trim();
    let display = if label.is_empty() {
        epsg.clone()
    } else {
        format!("{} ({})", label, epsg)
    };
    CrsCatalogEntry {
        label: display,
        epsg,
        proj4: def.proj4,
        custom: true,
    }
}

fn custom_crs_path(app_data: &std::path::Path) -> std::path::PathBuf {
    app_data.join("custom_crs.json")
}

fn read_custom_crs_defs(app_data: &std::path::Path) -> Result<Vec<CustomCrsDef>, String> {
    let path = custom_crs_path(app_data);
    if !path.exists() {
        return Ok(vec![]);
    }
    let bytes =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    serde_json::from_slice::<Vec<CustomCrsDef>>(&bytes)
        .map_err(|e| format!("cannot parse {}: {}", path.display(), e))
}

fn write_custom_crs_defs(app_data: &std::path::Path, defs: &[CustomCrsDef]) -> Result<(), String> {
    let path = custom_crs_path(app_data);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create dir {}: {}", parent.display(), e))?;
    }
    let json = serde_json::to_string_pretty(defs)
        .map_err(|e| format!("cannot serialise custom CRS: {e}"))?;
    std::fs::write(&path, json.as_bytes())
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

fn normalize_epsg(raw: &str) -> String {
    let upper = raw.trim().to_uppercase();
    if upper.is_empty() {
        return String::new();
    }
    if upper.starts_with("EPSG:") {
        return upper;
    }
    if upper.chars().all(|c| c.is_ascii_digit()) {
        return format!("EPSG:{}", upper);
    }
    upper
}

#[tauri::command]
/// Return globally saved custom CRS definitions.
pub fn list_custom_crs(app: tauri::AppHandle) -> Result<Vec<CustomCrsDef>, String> {
    let app_data = app_data_dir(&app)?;
    let mut defs = read_custom_crs_defs(&app_data)?;
    defs.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(defs)
}

#[tauri::command]
/// Return a paginated CRS catalog for the picker, merging curated + custom
/// definitions and applying query filtering in the backend.
pub fn list_crs_catalog_page(
    app: tauri::AppHandle,
    query: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<CrsCatalogPage, String> {
    let app_data = app_data_dir(&app)?;
    let custom_defs = read_custom_crs_defs(&app_data)?;
    let mut custom_by_epsg: std::collections::HashMap<String, CustomCrsDef> =
        std::collections::HashMap::new();
    for def in custom_defs {
        custom_by_epsg.insert(normalize_epsg(&def.epsg), def);
    }

    let mut merged: Vec<CrsCatalogEntry> = Vec::with_capacity(curated_crs_defs().len());
    for curated in curated_crs_defs() {
        if let Some(custom) = custom_by_epsg.remove(&curated.epsg) {
            merged.push(custom_to_catalog_entry(custom));
        } else {
            merged.push(CrsCatalogEntry {
                label: curated.label.clone(),
                epsg: curated.epsg.clone(),
                proj4: curated.proj4.clone(),
                custom: false,
            });
        }
    }
    for (_, custom) in custom_by_epsg {
        merged.push(custom_to_catalog_entry(custom));
    }

    let q = query.unwrap_or_default().trim().to_lowercase();
    if !q.is_empty() {
        merged.retain(|entry| {
            let hay = format!("{} {}", entry.label, entry.epsg).to_lowercase();
            hay.contains(&q)
        });
    }
    merged.sort_by(|a, b| a.label.cmp(&b.label).then(a.epsg.cmp(&b.epsg)));

    let total = merged.len() as u32;
    let page_size = page_size.unwrap_or(100).clamp(1, 250);
    let page = page.unwrap_or(0);
    let start = (page as usize).saturating_mul(page_size as usize);
    let end = std::cmp::min(start.saturating_add(page_size as usize), merged.len());
    let items = if start < merged.len() {
        merged[start..end].to_vec()
    } else {
        vec![]
    };

    Ok(CrsCatalogPage {
        items,
        total,
        page,
        page_size,
        has_more: end < merged.len(),
    })
}

#[tauri::command]
/// Create or update a globally saved custom CRS definition.
pub fn upsert_custom_crs(
    app: tauri::AppHandle,
    label: String,
    epsg: String,
    proj4: String,
) -> Result<Vec<CustomCrsDef>, String> {
    let label = label.trim().to_string();
    let epsg = normalize_epsg(&epsg);
    let proj4 = proj4.trim().to_string();
    if label.is_empty() {
        return Err("custom CRS label is required".into());
    }
    if epsg.is_empty() {
        return Err("custom CRS code is required".into());
    }
    if proj4.is_empty() {
        return Err("custom CRS proj4 definition is required".into());
    }

    let app_data = app_data_dir(&app)?;
    let mut defs = read_custom_crs_defs(&app_data)?;
    defs.retain(|d| normalize_epsg(&d.epsg) != epsg);
    defs.push(CustomCrsDef { label, epsg, proj4 });
    defs.sort_by(|a, b| a.label.cmp(&b.label));
    write_custom_crs_defs(&app_data, &defs)?;
    Ok(defs)
}

#[tauri::command]
/// Delete a globally saved custom CRS definition.
pub fn delete_custom_crs(app: tauri::AppHandle, epsg: String) -> Result<Vec<CustomCrsDef>, String> {
    let app_data = app_data_dir(&app)?;
    let normalized = normalize_epsg(&epsg);
    let mut defs = read_custom_crs_defs(&app_data)?;
    defs.retain(|d| normalize_epsg(&d.epsg) != normalized);
    defs.sort_by(|a, b| a.label.cmp(&b.label));
    write_custom_crs_defs(&app_data, &defs)?;
    Ok(defs)
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Reject any string that is not a valid UUID v4, preventing path traversal via
/// `project_id` / `scenario_id` parameters supplied by the frontend.
fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| format!("invalid id: expected UUID, got {:?}", id))
}

/// Count the number of scenario subdirectories under `<app_data>/projects/<id>/scenarios/`.
fn count_scenario_dirs(app_data: &std::path::Path, project_id: &str) -> u32 {
    let scenarios_dir = bundle::project_dir(app_data, project_id).join("scenarios");
    if !scenarios_dir.exists() {
        return 0;
    }
    std::fs::read_dir(&scenarios_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count() as u32
        })
        .unwrap_or(0)
}

/// Return a list of scenario IDs (directory names) under `<app_data>/projects/<id>/scenarios/`.
fn list_scenario_ids(app_data: &std::path::Path, project_id: &str) -> Vec<String> {
    let scenarios_dir = bundle::project_dir(app_data, project_id).join("scenarios");
    if !scenarios_dir.exists() {
        return vec![];
    }
    std::fs::read_dir(&scenarios_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

// ── Scenario commands ─────────────────────────────────────────────────────────

/// Flat scenario row returned to the frontend. The frontend builds the tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    pub id: String,
    pub project_id: String,
    pub parent_scenario_id: Option<String>,
    pub name: String,
    /// "not-run" | "simulated" (extended later)
    pub state: String,
}

/// Return every scenario for `project_id` as a flat list. The frontend
/// assembles the tree from `parent_scenario_id`.
#[tauri::command]
/// Scan the project `scenarios/` directory and return all scenarios.
pub fn list_scenarios(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Vec<ScenarioDto>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let scenarios_dir = bundle::project_dir(&app_data, &project_id).join("scenarios");
    if !scenarios_dir.exists() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    let entries = std::fs::read_dir(&scenarios_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let sc_id = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let sc_meta = match meta::read_scenario_meta(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let results_path = bundle::scenario_results_path(&app_data, &project_id, &sc_id);
        let sim_state = meta::sim_state_from_results(&results_path);
        result.push(scenario_meta_to_dto(
            &sc_id,
            &project_id,
            &sc_meta,
            sim_state,
        ));
    }
    result.sort_by_key(|s| s.id.clone());
    Ok(result)
}

/// Create a new scenario under `project_id`. If `parent_scenario_id` is
/// `Some`, the parent's model.inp is copied into the new scenario directory
/// as a starting point; otherwise the base model is used. Returns the new
/// `ScenarioDto`.
#[tauri::command]
/// Create a new scenario directory with `meta.json`, copying `base/model.inp`.
pub fn create_scenario(
    app: tauri::AppHandle,
    project_id: String,
    name: String,
    parent_scenario_id: Option<String>,
) -> Result<ScenarioDto, String> {
    validate_id(&project_id)?;
    if let Some(pid) = &parent_scenario_id {
        validate_id(pid)?;
    }
    let app_data = app_data_dir(&app)?;
    let id = uuid::Uuid::new_v4().to_string();

    let sc_dir = bundle::scenario_dir(&app_data, &project_id, &id);
    std::fs::create_dir_all(&sc_dir).map_err(|e| e.to_string())?;

    let sc_meta = meta::ScenarioMeta {
        name,
        description: None,
        parent_scenario_id: parent_scenario_id.clone(),
    };
    meta::write_scenario_meta(&sc_dir, &sc_meta)?;

    // Copy the parent model (or base model) into the new scenario directory.
    let src = match &parent_scenario_id {
        Some(pid) => bundle::scenario_model_path(&app_data, &project_id, pid),
        None => bundle::base_model_path(&app_data, &project_id),
    };
    if src.exists() {
        let dest = bundle::scenario_model_path(&app_data, &project_id, &id);
        std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    }

    Ok(scenario_meta_to_dto(&id, &project_id, &sc_meta, "not-run"))
}

fn scenario_meta_to_dto(
    id: &str,
    project_id: &str,
    m: &meta::ScenarioMeta,
    sim_state: &str,
) -> ScenarioDto {
    let state = match sim_state {
        "done" => "simulated",
        _ => "not-run",
    };
    ScenarioDto {
        id: id.to_string(),
        project_id: project_id.to_string(),
        parent_scenario_id: m.parent_scenario_id.clone(),
        name: m.name.clone(),
        state: state.into(),
    }
}

/// Permanently delete a scenario and its on-disk bundle.
/// Returns `true` when the directory was removed, `false` when the id was not found.
#[tauri::command]
/// Remove the scenario directory tree.
pub fn delete_scenario(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: String,
) -> Result<bool, String> {
    validate_id(&project_id)?;
    validate_id(&scenario_id)?;
    let app_data = app_data_dir(&app)?;
    let dir = bundle::scenario_dir(&app_data, &project_id, &scenario_id);
    if !dir.exists() {
        return Ok(false);
    }
    bundle::delete_scenario_dir(&app_data, &project_id, &scenario_id).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Rename a scenario. Returns `true` on success, `false` if not found.
#[tauri::command]
/// Update the `name` field in scenario `meta.json`.
pub fn rename_scenario(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: String,
    name: String,
) -> Result<bool, String> {
    validate_id(&project_id)?;
    validate_id(&scenario_id)?;
    let app_data = app_data_dir(&app)?;
    let sc_dir = bundle::scenario_dir(&app_data, &project_id, &scenario_id);
    if !sc_dir.exists() {
        return Ok(false);
    }
    let mut sc_meta = meta::read_scenario_meta(&sc_dir)?;
    sc_meta.name = name;
    meta::write_scenario_meta(&sc_dir, &sc_meta)?;
    Ok(true)
}

// ── File manager commands ─────────────────────────────────────────────────────

/// Open the base model directory for `project_id` in the system file manager
/// (Finder on macOS, Explorer on Windows, default file manager on Linux).
#[tauri::command]
/// Open the project base bundle directory in the OS file manager.
pub fn open_base_folder(app: tauri::AppHandle, project_id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let dir = bundle::base_dir(&app_data, &project_id);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    app.opener()
        .reveal_item_in_dir(&dir)
        .map_err(|e| e.to_string())
}

/// Open the scenario directory for `scenario_id` in the system file manager.
#[tauri::command]
/// Open a scenario bundle directory in the OS file manager.
pub fn open_scenario_folder(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    validate_id(&project_id)?;
    validate_id(&scenario_id)?;
    let app_data = app_data_dir(&app)?;
    let dir = bundle::scenario_dir(&app_data, &project_id, &scenario_id);
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    app.opener()
        .reveal_item_in_dir(&dir)
        .map_err(|e| e.to_string())
}

fn project_to_dto(
    id: &str,
    meta: &meta::ProjectMeta,
    scenario_count: u32,
    last_run_at: Option<i64>,
    sim_state: &str,
    folder_missing: bool,
    modified_at: i64,
) -> Project {
    let last_run_label = last_run_at.map(format_modified);
    let state = match sim_state {
        "done" => "simulated",
        _ if meta.node_count > 0 || meta.link_count > 0 => "ready",
        _ => "draft",
    };
    Project {
        modified_label: format_modified(modified_at),
        last_run_label,
        id: id.to_string(),
        name: meta.name.clone(),
        scenario_count,
        state: state.into(),
        node_count: meta.node_count,
        link_count: meta.link_count,
        source_crs: meta.source_crs.clone(),
        insights: None,
        folder_missing,
    }
}

/// Summary returned by [`reconcile_projects`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileReport {
    /// Number of orphaned on-disk project folders that were re-imported.
    pub recovered: u32,
    /// Project IDs present in the DB whose on-disk folder is missing.
    pub folder_missing: Vec<String>,
}

/// No-op reconcile command. The filesystem is now the source of truth, so
/// there is no DB to sync against. Returns an empty report.
#[tauri::command]
/// No-op (returns empty report); the filesystem is always authoritative.
pub fn reconcile_projects(_app: tauri::AppHandle) -> Result<ReconcileReport, String> {
    Ok(ReconcileReport {
        recovered: 0,
        folder_missing: vec![],
    })
}

fn format_modified(modified_at: i64) -> String {
    let now = meta::now_secs();
    let delta = (now - modified_at).max(0);
    if delta < 60 {
        "just now".into()
    } else if delta < 3_600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3_600)
    } else if delta < 30 * 86_400 {
        format!("{}d ago", delta / 86_400)
    } else {
        format!("{}mo ago", delta / (30 * 86_400))
    }
}

// ── Network load commands ─────────────────────────────────────────────────────

/// Serialisable node sent to the frontend. Mirrors `MockNode` in the
/// frontend's `data/mock/data.ts` so existing consumers need no changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    pub id: String,
    /// "junction" | "tank" | "reservoir"
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    /// Elevation in metres (converted from internal feet).
    pub elevation: f64,
    /// Base demand in L/s (converted from internal ft³/s); 0 for non-junctions.
    pub base_demand: f64,
    /// `null` until a simulation result is available.
    pub pressure: Option<f64>,
    /// `null` until a simulation result is available.
    pub demand: Option<f64>,
    // ── Tank-only fields ─────────────────────────────────────────────────
    /// Minimum water level above bottom (m); `null` for non-tanks.
    pub tank_min_level: Option<f64>,
    /// Maximum water level above bottom (m); `null` for non-tanks.
    pub tank_max_level: Option<f64>,
    /// Initial water level above bottom (m); `null` for non-tanks.
    pub tank_initial_level: Option<f64>,
    /// Tank diameter (m); `null` for non-tanks or volume-curve tanks.
    pub tank_diameter: Option<f64>,
    /// Volume curve ID; `null` when the tank uses a simple cylindrical model.
    pub tank_volume_curve: Option<String>,
    // ── Reservoir-only fields ─────────────────────────────────────────────
    /// Pattern ID modulating head over time; `null` for reservoirs without a
    /// head pattern, and `null` for junctions / tanks.
    pub head_pattern: Option<String>,
}

/// Serialisable link sent to the frontend. Mirrors `MockLink`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDto {
    pub id: String,
    /// "pipe" | "pump" | "valve"
    #[serde(rename = "type")]
    pub kind: String,
    pub from_id: String,
    pub to_id: String,
    /// 0.0 until a simulation result is available.
    pub velocity: f64,
    /// Diameter in mm (converted from internal ft).
    pub diameter: f64,
    /// Length in metres (converted from internal ft); 0 for pumps/valves.
    pub length: f64,
    /// Hazen-Williams roughness coefficient (C); 0 for pumps/valves.
    pub roughness: f64,
    // ── Pump-only fields ──────────────────────────────────────────────────
    /// Head-flow curve ID; `null` for constant-power pumps and non-pumps.
    pub pump_curve: Option<String>,
    /// Rated power in kW; `null` for curve-based pumps and non-pumps.
    pub pump_power_kw: Option<f64>,
    /// Initial relative speed (1.0 = rated speed); `null` for non-pumps.
    pub pump_speed: Option<f64>,
    // ── Valve-only fields ────────────────────────────────────────────────
    /// Valve type: `"PRV"` | `"PSV"` | `"FCV"` | `"TCV"` | `"GPV"` | `"PBV"` | `"PCV"`;
    /// `null` for non-valves.
    pub valve_type: Option<String>,
    /// Valve setting in display units: head (m) for PRV/PSV/PBV, flow (L/s) for FCV,
    /// dimensionless loss coefficient for TCV.  `null` for GPV/PCV (curve-based) and
    /// non-valves.
    pub valve_setting: Option<f64>,
    /// Curve ID for GPV (`GpvHeadloss`) and PCV (`PcvLossRatio`) valve types;
    /// `null` for all other types.
    pub valve_curve: Option<String>,
}

/// Serialisable pattern sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDto {
    pub id: String,
    /// Dimensionless multipliers [F₀, F₁, …, F_{L−1}].
    pub multipliers: Vec<f64>,
}

/// Serialisable curve sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveDto {
    pub id: String,
    /// "pump-head" | "pump-efficiency" | "pump-volume" | "tank-volume" |
    /// "gpv-headloss" | "pcv-loss-ratio" | "generic"
    pub kind: String,
    /// x-axis values. Units depend on kind (flow L/s for pump curves).
    pub x: Vec<f64>,
    /// y-axis values. Units depend on kind (head m for pump-head curves).
    pub y: Vec<f64>,
}

/// The full network payload returned to the frontend after parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDto {
    pub nodes: Vec<NodeDto>,
    pub links: Vec<LinkDto>,
    pub patterns: Vec<PatternDto>,
    pub curves: Vec<CurveDto>,
    /// Stem of the source file name (no directory, no extension).
    /// Empty string when the DTO was constructed without a file context.
    #[serde(default)]
    pub file_stem: String,
}

/// Inner state for `NetworkState`.
#[allow(clippy::large_enum_variant)]
#[derive(Default)]
pub enum NetworkStateInner {
    #[default]
    Empty,
    Loaded {
        /// INP bytes kept for `save_project` / `create_project`.
        raw_bytes: Vec<u8>,
        /// Parsed network — cached to avoid re-parsing on every `patch_element` call.
        network: hydra::Network,
        dto: NetworkDto,
    },
}

/// Tauri managed state — holds the most recently loaded network (if any).
#[derive(Default)]
pub struct NetworkState(pub std::sync::Mutex<NetworkStateInner>);

/// Minimal descriptor returned when the user picks a field-data file (CSV,
/// Excel, etc.) via the native file-open dialog. The frontend adds this to the
/// import source list; actual CSV parsing is not yet implemented on the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickedCsvFile {
    pub id: String,
    pub filename: String,
}

/// Open a native file-open dialog filtered to common field-data formats and
/// return just the filename. Returns `null` when the dialog is cancelled.
#[tauri::command]
/// Open a file-picker filtered to CSV/Excel; returns the filename and a generated ID.
pub async fn pick_csv_file(app: tauri::AppHandle) -> Result<Option<PickedCsvFile>, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = app
        .dialog()
        .file()
        .add_filter("Field data (CSV, Excel)", &["csv", "xlsx", "xls"])
        .blocking_pick_file();

    let file_path = match path {
        Some(p) => p,
        None => return Ok(None),
    };

    let path_buf = file_path.into_path().map_err(|e| e.to_string())?;
    let filename = path_buf
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(Some(PickedCsvFile {
        id: format!(
            "src-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
        filename,
    }))
}

/// Open a native file-open dialog, parse the chosen `.inp` file, store the
/// result in managed state, and return the `NetworkDto` to the caller.
///
/// Returns `null` to the frontend when the dialog is cancelled.
#[tauri::command]
/// Open a native file-picker, parse the chosen INP, and hold it in `NetworkState`.
pub async fn open_and_load_network(
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
) -> Result<Option<NetworkDto>, String> {
    use tauri_plugin_dialog::DialogExt;

    // Show a synchronous file-open dialog (blocks the async task, not the UI).
    let path = app
        .dialog()
        .file()
        .add_filter("EPANET Input File", &["inp"])
        .blocking_pick_file();

    let file_path = match path {
        Some(p) => p,
        None => return Ok(None), // user cancelled
    };

    let path_buf = file_path.into_path().map_err(|e| e.to_string())?;
    let file_stem = path_buf
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let bytes = std::fs::read(&path_buf).map_err(|e| e.to_string())?;
    let network = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;

    let mut dto = network_to_dto(&network);
    dto.file_stem = file_stem;

    *state.0.lock().unwrap() = NetworkStateInner::Loaded {
        raw_bytes: bytes,
        network,
        dto: dto.clone(),
    };
    Ok(Some(dto))
}

fn format_inp_parse_error(err: hydra::io::ParseError) -> String {
    match err {
        hydra::io::ParseError::ValidationFailed(errors) => {
            if errors.is_empty() {
                return "validation failed".to_string();
            }

            if let Some(summary) = summarize_unknown_pattern_refs(&errors) {
                return summary;
            }

            const PREVIEW_LIMIT: usize = 2;
            let preview: Vec<String> = errors
                .iter()
                .take(PREVIEW_LIMIT)
                .map(ToString::to_string)
                .collect();

            if errors.len() > PREVIEW_LIMIT {
                format!(
                    "validation failed ({} errors): {}; and {} more",
                    errors.len(),
                    preview.join("; "),
                    errors.len() - PREVIEW_LIMIT,
                )
            } else {
                format!("validation failed: {}", preview.join("; "))
            }
        }
        other => other.to_string(),
    }
}

fn summarize_unknown_pattern_refs(errors: &[hydra::ValidationError]) -> Option<String> {
    let mut refs_by_pattern: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for err in errors {
        if let hydra::ValidationError::UnknownPatternRef {
            object_id,
            pattern_id,
        } = err
        {
            refs_by_pattern
                .entry(pattern_id.clone())
                .or_default()
                .push(object_id.clone());
        }
    }

    let (pattern_id, object_ids) = refs_by_pattern
        .iter()
        .max_by_key(|(_, object_ids)| object_ids.len())?;

    let group_count = object_ids.len();
    if group_count == 0 {
        return None;
    }

    let preview_limit = 2usize;
    let preview_list = object_ids
        .iter()
        .take(preview_limit)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let remaining_in_group = group_count.saturating_sub(preview_limit);

    fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
        if count == 1 {
            singular
        } else {
            plural
        }
    }

    let mut summary = if group_count == 1 {
        format!(
            "missing pattern '{}' referenced by {}",
            pattern_id, object_ids[0]
        )
    } else {
        let mut detail = format!(
            "missing pattern '{}' referenced by {} network {} ({})",
            pattern_id,
            group_count,
            pluralize(group_count, "element", "elements"),
            preview_list,
        );
        if remaining_in_group > 0 {
            let _ = detail.pop();
            detail.push_str(&format!(", +{} more)", remaining_in_group));
        }
        detail
    };

    let remaining_errors = errors.len().saturating_sub(group_count);
    if remaining_errors > 0 {
        summary.push_str(&format!(
            "; plus {} additional validation {}",
            remaining_errors,
            pluralize(remaining_errors, "issue", "issues")
        ));
    }

    Some(summary)
}

#[tauri::command]
/// Return the node list for the loaded network.
pub fn get_nodes(state: tauri::State<'_, NetworkState>) -> Vec<NodeDto> {
    match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { dto, .. } => dto.nodes.clone(),
        NetworkStateInner::Empty => vec![],
    }
}

/// Combined snapshot used by the frontend to fetch nodes and links in one IPC call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshotDto {
    pub nodes: Vec<NodeDto>,
    pub links: Vec<LinkDto>,
}

#[tauri::command]
/// Return nodes + links in one payload for the loaded network.
pub fn get_network_snapshot(state: tauri::State<'_, NetworkState>) -> NetworkSnapshotDto {
    match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { dto, .. } => NetworkSnapshotDto {
            nodes: dto.nodes.clone(),
            links: dto.links.clone(),
        },
        NetworkStateInner::Empty => NetworkSnapshotDto {
            nodes: vec![],
            links: vec![],
        },
    }
}

/// Return the links of the currently loaded network, or an empty list.
#[tauri::command]
/// Return the link list for the loaded network.
pub fn get_links(state: tauri::State<'_, NetworkState>) -> Vec<LinkDto> {
    match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { dto, .. } => dto.links.clone(),
        NetworkStateInner::Empty => vec![],
    }
}

/// Return the patterns of the currently loaded network, or an empty list.
#[tauri::command]
/// Return demand/head patterns for the loaded network.
pub fn get_patterns(state: tauri::State<'_, NetworkState>) -> Vec<PatternDto> {
    match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { dto, .. } => dto.patterns.clone(),
        NetworkStateInner::Empty => vec![],
    }
}

/// Return the curves of the currently loaded network, or an empty list.
#[tauri::command]
/// Return pump/GPV/volume curves for the loaded network.
pub fn get_curves(state: tauri::State<'_, NetworkState>) -> Vec<CurveDto> {
    match &*state.0.lock().unwrap() {
        NetworkStateInner::Loaded { dto, .. } => dto.curves.clone(),
        NetworkStateInner::Empty => vec![],
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn network_to_dto(network: &hydra::Network) -> NetworkDto {
    use hydra::{LinkKind, NodeKind};

    // Build a node-index → node-id map for resolving link endpoints.
    let node_id_by_index: std::collections::HashMap<usize, &str> = network
        .nodes
        .iter()
        .map(|n| (n.base.index, n.base.id.as_str()))
        .collect();

    const FT_TO_M: f64 = 0.3048;
    const CFS_TO_LPS: f64 = 28.3168;

    let nodes = network
        .nodes
        .iter()
        .map(|n| {
            let kind = match &n.kind {
                NodeKind::Junction(_) => "junction",
                NodeKind::Reservoir(_) => "reservoir",
                NodeKind::Tank(_) => "tank",
            };
            let (x, y) = network
                .coordinates
                .get(&n.base.id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let elevation = n.base.elevation * FT_TO_M;
            let base_demand = match &n.kind {
                NodeKind::Junction(j) => {
                    j.demands.iter().map(|d| d.base_demand).sum::<f64>() * CFS_TO_LPS
                }
                _ => 0.0,
            };
            let (
                tank_min_level,
                tank_max_level,
                tank_initial_level,
                tank_diameter,
                tank_volume_curve,
            ) = if let NodeKind::Tank(t) = &n.kind {
                (
                    Some(t.min_level * FT_TO_M),
                    Some(t.max_level * FT_TO_M),
                    Some(t.initial_level * FT_TO_M),
                    Some(t.diameter * FT_TO_M),
                    t.volume_curve.clone(),
                )
            } else {
                (None, None, None, None, None)
            };
            let head_pattern = if let NodeKind::Reservoir(r) = &n.kind {
                r.head_pattern.clone()
            } else {
                None
            };
            NodeDto {
                id: n.base.id.clone(),
                kind: kind.into(),
                x,
                y,
                elevation,
                base_demand,
                pressure: None,
                demand: None,
                tank_min_level,
                tank_max_level,
                tank_initial_level,
                tank_diameter,
                tank_volume_curve,
                head_pattern,
            }
        })
        .collect();

    let links = network
        .links
        .iter()
        .map(|l| {
            let (kind, diameter, length, roughness) = match &l.kind {
                LinkKind::Pipe(p) => ("pipe", p.diameter * 304.8, p.length * FT_TO_M, p.roughness),
                LinkKind::Pump(_) => ("pump", 0.0, 0.0, 0.0),
                LinkKind::Valve(v) => ("valve", v.diameter * 304.8, 0.0, 0.0),
            };
            let (pump_curve, pump_power_kw, pump_speed) = if let LinkKind::Pump(p) = &l.kind {
                // power is stored in Watts; convert to kW for the DTO
                let kw = p.power.map(|pw| pw / 1000.0);
                // initial_setting on the base is the initial relative speed (ω); default 1.0
                let speed = l.base.initial_setting.or(Some(1.0));
                (p.head_curve.clone(), kw, speed)
            } else {
                (None, None, None)
            };
            let (valve_type, valve_setting, valve_curve) = if let LinkKind::Valve(v) = &l.kind {
                use hydra::ValveType;
                let vt = match v.valve_type {
                    ValveType::Prv => "PRV",
                    ValveType::Psv => "PSV",
                    ValveType::Fcv => "FCV",
                    ValveType::Tcv => "TCV",
                    ValveType::Gpv => "GPV",
                    ValveType::Pcv => "PCV",
                    ValveType::Pbv => "PBV",
                };
                // Convert setting from internal ft/cfs/dimensionless to display units.
                let setting = match v.valve_type {
                    ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                        l.base.initial_setting.map(|s| s * FT_TO_M)
                    }
                    ValveType::Fcv => l.base.initial_setting.map(|s| s * CFS_TO_LPS),
                    ValveType::Tcv => l.base.initial_setting,
                    ValveType::Gpv | ValveType::Pcv => None,
                };
                (Some(vt.to_string()), setting, v.curve.clone())
            } else {
                (None, None, None)
            };
            let from_id = node_id_by_index
                .get(&l.base.from_node)
                .map(|s| s.to_string())
                .unwrap_or_default();
            let to_id = node_id_by_index
                .get(&l.base.to_node)
                .map(|s| s.to_string())
                .unwrap_or_default();
            LinkDto {
                id: l.base.id.clone(),
                kind: kind.into(),
                from_id,
                to_id,
                velocity: 0.0,
                diameter,
                length,
                roughness,
                pump_curve,
                pump_power_kw,
                pump_speed,
                valve_type,
                valve_setting,
                valve_curve,
            }
        })
        .collect();

    let patterns = network
        .patterns
        .iter()
        .map(|p| PatternDto {
            id: p.id.clone(),
            multipliers: p.factors.clone(),
        })
        .collect();

    let curves = network
        .curves
        .iter()
        .map(|c| {
            use hydra::CurveKind;
            let kind = match c.kind {
                CurveKind::PumpHead => "pump-head",
                CurveKind::PumpEfficiency => "pump-efficiency",
                CurveKind::PumpVolume => "pump-volume",
                CurveKind::TankVolume => "tank-volume",
                CurveKind::GpvHeadloss => "gpv-headloss",
                CurveKind::PcvLossRatio => "pcv-loss-ratio",
                CurveKind::Generic => "generic",
            };
            // Pump-head: x = flow (cfs → L/s), y = head (ft → m).
            // All others: pass raw values through (unit conversion is
            // context-dependent; the frontend labels accordingly).
            let (xs, ys): (Vec<f64>, Vec<f64>) = if c.kind == CurveKind::PumpHead {
                c.points
                    .iter()
                    .map(|p| (p.x * CFS_TO_LPS, p.y * FT_TO_M))
                    .unzip()
            } else {
                c.points.iter().map(|p| (p.x, p.y)).unzip()
            };
            CurveDto {
                id: c.id.clone(),
                kind: kind.into(),
                x: xs,
                y: ys,
            }
        })
        .collect();

    NetworkDto {
        nodes,
        links,
        patterns,
        curves,
        file_stem: String::new(),
    }
}

// ── Simulation helpers ───────────────────────────────────────────────────────

/// Collect per-pump energy from a completed simulation session.
fn collect_pump_energy(sim: &hydra::Simulation, duration_seconds: f64) -> Vec<PumpEnergyDto> {
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
    energy
        .pumps
        .iter()
        .filter_map(|rec| {
            // `link_index` is 1-based.
            let idx = (rec.link_index as usize).checked_sub(1)?;
            let link = network.links.get(idx)?;
            Some(PumpEnergyDto {
                id: link.base.id.clone(),
                pct_online: rec.pct_online as f64,
                avg_efficiency: rec.avg_efficiency as f64,
                avg_kwh_per_flow: rec.avg_kwh_per_flow as f64,
                avg_kw: rec.avg_kw as f64,
                peak_kw: rec.peak_kw as f64,
            })
        })
        .collect()
}

// ── Simulation ────────────────────────────────────────────────────────────────

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
    pub min_pressure_node_id: String,
    /// Lowest minimum-pressure value (m) across all nodes and periods.
    pub min_pressure_m: f64,
    /// Number of nodes whose worst-case pressure is below 14 m.
    pub low_pressure_count: u32,
    /// Link ID with the highest peak velocity across all periods.
    pub max_velocity_link_id: String,
    /// Highest peak velocity (m/s) across all links and periods.
    pub max_velocity_ms: f64,
    /// Histogram of per-node minimum pressure (7 fixed bins, m).
    pub pressure_histogram: Vec<HistogramBucketDto>,
    /// Histogram of per-link maximum velocity (5 fixed bins, m/s).
    pub velocity_histogram: Vec<HistogramBucketDto>,
    /// Top 5 links ordered by peak velocity descending.
    pub top_pipes: Vec<TopPipeDto>,
    /// Head-over-time series for every tank node.
    pub tank_series: Vec<TankHeadSeriesDto>,
}

/// A node whose minimum pressure (across all periods) is below the threshold.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeViolationDto {
    pub id: String,
    pub min_pressure_m: f64,
}

/// A link whose peak velocity (across all periods) is above the threshold.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkViolationDto {
    pub id: String,
    pub max_velocity_ms: f64,
}

/// Threshold violations returned by `get_violations`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViolationsDto {
    pub pressure_violations: Vec<NodeViolationDto>,
    pub velocity_violations: Vec<LinkViolationDto>,
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
}

/// Flat per-period result arrays returned by `get_period_results`.
/// Values are in SI units (L/s, m, m/s) — the same units the frontend expects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodResultsDto {
    pub node_demand: Vec<f32>,
    pub node_head: Vec<f32>,
    pub node_pressure: Vec<f32>,
    pub link_flow: Vec<f32>,
    pub link_velocity: Vec<f32>,
    pub link_headloss: Vec<f32>,
    pub link_status: Vec<f32>,
    /// Per-node quality values.  `None` when no quality simulation was run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_quality: Option<Vec<f32>>,
    /// Per-link quality values.  `None` when no quality simulation was run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_quality: Option<Vec<f32>>,
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
        let guard = state.0.lock().unwrap();
        match &*guard {
            NetworkStateInner::Loaded { raw_bytes, .. } => raw_bytes.clone(),
            NetworkStateInner::Empty => return Ok(None),
        }
    };

    let mut network = hydra::io::parse(&raw_bytes).map_err(|e| format!("{e:?}"))?;

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

    // Resolve the .out path before moving into spawn_blocking.
    let out_path: Option<std::path::PathBuf> = if let Ok(app_data) = app.path().app_data_dir() {
        match (&project_id, &scenario_id) {
            (Some(pid), Some(sid)) => Some(bundle::scenario_results_path(&app_data, pid, sid)),
            (Some(pid), None) => Some(bundle::base_results_path(&app_data, pid)),
            _ => None,
        }
    } else {
        None
    };

    let mut sim = Simulation::create();
    sim.load(network).map_err(|e| format!("{e:?}"))?;

    // ── Phase 2: stepped loops on a blocking thread ─────────────────────────
    let direct_run_id = uuid::Uuid::new_v4().to_string();
    let app2 = app.clone();
    let (sim, run_err, _wall_ms, _hyd_steps) = tauri::async_runtime::spawn_blocking(move || {
        run_sim_loops(
            sim,
            out_path,
            duration_seconds,
            run_quality,
            |phase, ss, done, failed, msg| {
                let _ = app2.emit(
                    SIMULATION_PROGRESS_EVENT,
                    &SimulationProgressDto {
                        run_id: Some(direct_run_id.clone()),
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

    if let Some(err) = run_err {
        return Err(match err {
            RunLoopError::Failed(msg) => msg,
            RunLoopError::Cancelled => "Simulation cancelled".into(),
        });
    }

    let result = SimulationResultDto {
        pump_energy: collect_pump_energy(&sim, duration_seconds),
    };

    Ok(Some(result))
}

/// Persist the currently loaded network (`NetworkState`) back into the named
/// project as `base/model.inp`.
///
/// Returns `true` when the file was written, `false` when no network is loaded
/// in managed state (i.e. the project is a draft with no INP attached yet).
///
/// When `scenario_id` is `Some`, writes to the scenario's INP file instead of
/// the base model file (and skips the base-model node/link count update).
#[tauri::command]
/// Flush in-memory patches to `base/model.inp`; update node/link counts in `meta.json`.
pub fn save_project(
    id: String,
    scenario_id: Option<String>,
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    validate_id(&id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let (raw, node_count, link_count) = {
        let guard = state.0.lock().unwrap();
        match &*guard {
            NetworkStateInner::Loaded { raw_bytes, dto, .. } => (
                raw_bytes.clone(),
                dto.nodes.len() as u32,
                dto.links.len() as u32,
            ),
            NetworkStateInner::Empty => return Ok(false),
        }
    };
    let app_data = app_data_dir(&app)?;
    match scenario_id {
        Some(ref sid) => {
            bundle::atomic_write(&bundle::scenario_model_path(&app_data, &id, sid), &raw)
                .map_err(|e| e.to_string())?;
        }
        None => {
            bundle::atomic_write(&bundle::base_model_path(&app_data, &id), &raw)
                .map_err(|e| e.to_string())?;
            // Update cached node/link counts in meta.json.
            let project_dir = bundle::project_dir(&app_data, &id);
            if let Ok(mut project_meta) = meta::read_project_meta(&project_dir) {
                project_meta.node_count = node_count;
                project_meta.link_count = link_count;
                let _ = meta::write_project_meta(&project_dir, &project_meta);
            }
        }
    }
    Ok(true)
}

// ── Queue commands ────────────────────────────────────────────────────────────

/// Return the current run queue for `project_id`, ordered by `queued_at`.
#[tauri::command]
/// Return the current run queue items.
pub fn get_run_queue(
    run_queue: tauri::State<'_, RunQueue>,
    project_id: String,
) -> Result<Vec<RunQueueItemDto>, String> {
    Ok(run_queue
        .get_for_project(&project_id)
        .into_iter()
        .map(run_queue_item_to_dto)
        .collect())
}

/// Enqueue one or more simulation runs for `project_id`.
///
/// `targets` is a list where `None` = base model and `Some(scenario_id)` =
/// a specific scenario. Each target is pushed to the in-memory queue. If the
/// queue processor is not already running it is spawned immediately.
#[tauri::command]
/// Add one or more runs to the queue and start the background processor.
pub async fn enqueue_runs(
    app: tauri::AppHandle,
    run_queue: tauri::State<'_, RunQueue>,
    project_id: String,
    targets: Vec<Option<String>>,
) -> Result<Vec<RunQueueItemDto>, String> {
    validate_id(&project_id)?;
    for sid in targets.iter().flatten() {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let now = meta::now_secs();
    for target_id in &targets {
        let target_name = target_id.as_deref().and_then(|sid| {
            let sc_dir = bundle::scenario_dir(&app_data, &project_id, sid);
            meta::read_scenario_meta(&sc_dir).ok().map(|m| m.name)
        });
        run_queue.enqueue(RunQueueItem {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.clone(),
            target_id: target_id.clone(),
            target_name,
            status: "queued".into(),
            queued_at: now,
            started_at: None,
            finished_at: None,
            error: None,
        });
    }

    // Notify the frontend immediately so newly-queued items appear in the
    // task tray before the queue processor picks them up.
    let _ = app.emit(RUN_QUEUE_UPDATE_EVENT, &project_id);

    // Kick the queue processor if it is not already running.
    if run_queue.try_claim_processor() {
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            process_queue(app2).await;
        });
    }

    Ok(run_queue
        .get_for_project(&project_id)
        .into_iter()
        .map(run_queue_item_to_dto)
        .collect())
}

/// Cancel all queued (not yet started) runs for `project_id`.
/// Returns the number of items cancelled.
#[tauri::command]
/// Cancel all pending queue items.
pub fn cancel_run_queue(
    app: tauri::AppHandle,
    run_queue: tauri::State<'_, RunQueue>,
    project_id: String,
) -> Result<u32, String> {
    let n = run_queue.cancel_for_project(&project_id);
    let _ = app.emit(RUN_QUEUE_UPDATE_EVENT, &project_id);
    Ok(n)
}

/// Cancel a single queued run item by its run ID. Returns `true` when the item
/// was actually in the `queued` state and has been moved to `cancelled`.
#[tauri::command]
/// Cancel a single queue item by ID.
pub fn cancel_run_item(
    app: tauri::AppHandle,
    run_queue: tauri::State<'_, RunQueue>,
    run_id: String,
) -> Result<bool, String> {
    let (cancelled, project_id) = run_queue.cancel_item(&run_id);
    if cancelled {
        if let Some(pid) = project_id {
            let _ = app.emit(RUN_QUEUE_UPDATE_EVENT, &pid);
        }
    }
    Ok(cancelled)
}

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
        pda_min_pressure: o.pda_min_pressure * 0.3048,
        pda_required_pressure: o.pda_required_pressure * 0.3048,
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
    o.pda_min_pressure = dto.pda_min_pressure / 0.3048;
    o.pda_required_pressure = dto.pda_required_pressure / 0.3048;
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
#[tauri::command]
/// Return simulation parameter overrides for a project.
pub fn get_sim_params(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Option<SimParamsDto>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let path = bundle::base_model_path(&app_data, &project_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let network = hydra::io::parse(&bytes).map_err(|e| format!("{e:?}"))?;
    Ok(Some(options_to_dto(&network.options)))
}

/// Persist new sim params for `project_id` by parsing the base INP, applying
/// the DTO, rewriting the base INP, and propagating to every scenario INP so
/// they stay in lockstep.
#[tauri::command]
/// Persist simulation parameter overrides for a project.
pub fn update_sim_params(
    app: tauri::AppHandle,
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
    {
        let bytes = std::fs::read(&base_path).map_err(|e| e.to_string())?;
        let mut network = hydra::io::parse(&bytes).map_err(|e| format!("{e:?}"))?;
        apply_dto_to_options(&mut network.options, &params)?;
        let new_bytes = hydra::write_inp(&network);
        bundle::atomic_write(&base_path, &new_bytes).map_err(|e| e.to_string())?;
    }

    // 2) Every scenario's INP — best-effort. We skip but log scenarios whose
    //    INP fails to parse so a single bad scenario doesn't block the user
    //    from updating the base.
    let scenario_ids = list_scenario_ids(&app_data, &project_id);
    for sc_id in scenario_ids {
        let path = bundle::scenario_model_path(&app_data, &project_id, &sc_id);
        if !path.exists() {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let mut network = match hydra::io::parse(&bytes) {
            Ok(n) => n,
            Err(_) => continue,
        };
        if apply_dto_to_options(&mut network.options, &params).is_err() {
            continue;
        }
        let new_bytes = hydra::write_inp(&network);
        let _ = bundle::atomic_write(&path, &new_bytes);
    }

    Ok(())
}

fn run_queue_item_to_dto(item: RunQueueItem) -> RunQueueItemDto {
    RunQueueItemDto {
        id: item.id,
        project_id: item.project_id,
        target_id: item.target_id,
        target_name: item.target_name,
        status: item.status,
        queued_at: item.queued_at,
        started_at: item.started_at,
        finished_at: item.finished_at,
        error: item.error,
    }
}

/// Background queue processor. Drains the in-memory run queue one item at a
/// time, emitting [`RUN_QUEUE_UPDATE_EVENT`] events after each state change.
///
/// Claims the processor slot on entry and releases it on exit so that at
/// most one processor is active at any time.
async fn process_queue(app: tauri::AppHandle) {
    let rq = app.state::<RunQueue>();
    loop {
        let item = rq.next_queued();
        let item = match item {
            Some(i) => i,
            None => break,
        };

        let now = meta::now_secs();
        rq.mark_running(&item.id, now);
        let _ = app.emit(RUN_QUEUE_UPDATE_EVENT, &item.project_id);

        let result =
            run_sim_for_queue(&app, &item.id, &item.project_id, item.target_id.as_deref()).await;

        let now = meta::now_secs();
        match result {
            Ok(QueueRunResult::Done) => {
                rq.mark_done(&item.id, now);
            }
            Ok(QueueRunResult::Cancelled) => {
                rq.mark_cancelled(&item.id, now);
            }
            Err(e) => {
                rq.mark_failed(&item.id, now, &e);
            }
        }
        let _ = app.emit(RUN_QUEUE_UPDATE_EVENT, &item.project_id);
    }

    // Release the slot so future `enqueue_runs` calls can respawn the processor.
    rq.release_processor();
}

/// Run a single simulation on behalf of the queue processor.
///
/// Unlike [`run_simulation`], this reads the model entirely from disk, does
/// not accept quality-mode overrides (the INP `[OPTIONS]` section is the sole
/// source of truth), and returns only success/failure — results are accessed
/// on demand from `results.out`.
async fn run_sim_for_queue(
    app: &tauri::AppHandle,
    run_id: &str,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<QueueRunResult, String> {
    use hydra::{QualityMode, Simulation};

    let app_data = app.path().app_data_dir().map_err(|e| format!("{e:?}"))?;
    let raw_bytes: Vec<u8> = match scenario_id {
        Some(sid) => {
            let path = bundle::scenario_model_path(&app_data, project_id, sid);
            std::fs::read(&path).map_err(|e| format!("Cannot read scenario '{}': {}", sid, e))?
        }
        None => {
            let path = bundle::base_model_path(&app_data, project_id);
            std::fs::read(&path)
                .map_err(|e| format!("Cannot read base model '{}': {}", project_id, e))?
        }
    };

    let network = hydra::io::parse(&raw_bytes).map_err(|e| format!("{e:?}"))?;
    let run_quality = network.options.quality_mode != QualityMode::None;
    let duration_seconds = network.options.duration;

    let out_path = match scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, project_id, sid),
        None => bundle::base_results_path(&app_data, project_id),
    };

    let mut sim = Simulation::create();
    sim.load(network).map_err(|e| format!("{e:?}"))?;

    let run_id_owned = run_id.to_string();
    let out_path_for_cleanup = out_path.clone();
    let app_emit = app.clone();
    let app_cancel = app.clone();
    let (_, run_err, _wall_ms, _hyd_steps) = tauri::async_runtime::spawn_blocking(move || {
        run_sim_loops(
            sim,
            Some(out_path),
            duration_seconds,
            run_quality,
            |phase, ss, done, failed, msg| {
                let _ = app_emit.emit(
                    SIMULATION_PROGRESS_EVENT,
                    &SimulationProgressDto {
                        run_id: Some(run_id_owned.clone()),
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
            || {
                app_cancel
                    .state::<RunQueue>()
                    .is_cancel_requested(&run_id_owned)
            },
        )
    })
    .await
    .map_err(|e| format!("Simulation task panicked: {e:?}"))?;

    if let Some(err) = run_err {
        return match err {
            RunLoopError::Failed(msg) => Err(msg),
            RunLoopError::Cancelled => {
                let _ = std::fs::remove_file(out_path_for_cleanup);
                Ok(QueueRunResult::Cancelled)
            }
        };
    }

    Ok(QueueRunResult::Done)
}

#[derive(Debug, Clone, Copy)]
enum QueueRunResult {
    Done,
    Cancelled,
}

// ── Result metadata + period commands ────────────────────────────────────────

/// Return snapshot times and global result ranges for a project or scenario.
///
/// Reads the binary `results.out` on disk without loading the full file.
/// Returns an error when no simulation has been run yet for the target.
#[tauri::command]
/// Parse result metadata (timestep count, reporting period) from `results.out`.
pub fn load_result_meta(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<ResultMetaDto, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let out_path = match &scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, &project_id, sid),
        None => bundle::base_results_path(&app_data, &project_id),
    };
    let meta = hydra::io::out_reader::read_metadata(&out_path)?;
    let times = meta.snapshot_times();
    let ranges = hydra::io::out_reader::scan_ranges(&out_path, &meta, 2048)?;
    let quality_mode = match meta.quality_flag {
        1 => "chemical",
        2 => "age",
        3 => "trace",
        _ => "none",
    };
    Ok(ResultMetaDto {
        times,
        quality_mode: quality_mode.to_string(),
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
    })
}

/// Return flat result arrays for a single reporting period.
///
/// Values are in SI units (L/s, m, m/s) because `results.out` is always
/// written with `FlowUnits::Lps`. Returns an error when `period` is out of
/// range or `results.out` does not exist.
#[tauri::command]
/// Return flat arrays for a single reporting period (nodes + links).
pub fn get_period_results(
    app: tauri::AppHandle,
    project_id: String,
    period: usize,
    scenario_id: Option<String>,
) -> Result<PeriodResultsDto, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let out_path = match &scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, &project_id, sid),
        None => bundle::base_results_path(&app_data, &project_id),
    };
    let meta = hydra::io::out_reader::read_metadata(&out_path)?;
    let pr = hydra::io::out_reader::read_period(&out_path, &meta, period)?;
    let has_quality = meta.quality_flag != 0;
    Ok(PeriodResultsDto {
        node_demand: pr.node_demand,
        node_head: pr.node_head,
        node_pressure: pr.node_pressure,
        link_flow: pr.link_flow,
        link_velocity: pr.link_velocity,
        link_headloss: pr.link_headloss,
        link_status: pr.link_status,
        node_quality: if has_quality {
            Some(pr.node_quality)
        } else {
            None
        },
        link_quality: if has_quality {
            Some(pr.link_quality)
        } else {
            None
        },
    })
}

/// Return the pump energy summary for a project or scenario.
///
/// Reads only the energy section of `results.out` (a few dozen bytes per pump)
/// without touching the period data.  Safe for any network size.
#[tauri::command]
/// Return pump energy statistics from the binary output file.
pub fn get_pump_energy(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<PumpEnergyDto>, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let out_path = match &scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, &project_id, sid),
        None => bundle::base_results_path(&app_data, &project_id),
    };
    let model_path = match &scenario_id {
        Some(sid) => bundle::scenario_model_path(&app_data, &project_id, sid),
        None => bundle::base_model_path(&app_data, &project_id),
    };
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    let network = hydra::io::parse(&raw).map_err(|e| format!("{e:?}"))?;
    let meta = hydra::io::out_reader::read_metadata(&out_path)?;
    Ok(pump_energy_from_out(&out_path, &network, &meta))
}

/// Compute cross-period analytics by streaming the `.out` file one period at a
/// time.  Never loads more than a single period's data into memory, so it is
/// safe for arbitrarily large result files.
///
/// Returns histograms, summary statistics, a tank head time series, and a
/// mass-balance series — everything the Analysis view needs without holding a
/// full `SimulationResult` in memory.
#[tauri::command]
/// Return post-simulation analytics for a completed run.
pub fn get_result_analytics(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<ResultAnalyticsDto, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let out_path = match &scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, &project_id, sid),
        None => bundle::base_results_path(&app_data, &project_id),
    };
    let model_path = match &scenario_id {
        Some(sid) => bundle::scenario_model_path(&app_data, &project_id, sid),
        None => bundle::base_model_path(&app_data, &project_id),
    };
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    let network = hydra::io::parse(&raw).map_err(|e| format!("{e:?}"))?;
    let meta = hydra::io::out_reader::read_metadata(&out_path)?;

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
    let mut min_pressure_val = f64::INFINITY;
    let mut min_pressure_idx = 0usize;

    for (i, &p) in node_min_pressure.iter().enumerate() {
        if p.is_finite() {
            if p < LOW_PRESSURE_THRESHOLD {
                low_pressure_count += 1;
            }
            if p < min_pressure_val {
                min_pressure_val = p;
                min_pressure_idx = i;
            }
            for bin in &mut pressure_histogram {
                if p >= bin.lo && p < bin.hi {
                    bin.count += 1;
                    break;
                }
            }
        }
    }

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

    let mut max_velocity_val = 0.0_f64;
    let mut max_velocity_idx = 0usize;

    for (i, &v) in link_max_velocity.iter().enumerate() {
        if v > max_velocity_val {
            max_velocity_val = v;
            max_velocity_idx = i;
        }
        for bin in &mut velocity_histogram {
            if v >= bin.lo && v < bin.hi {
                bin.count += 1;
                break;
            }
        }
    }

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
                hydra::LinkKind::Pipe(p) => p.diameter * 304.8,
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

    // ── Summary strings ───────────────────────────────────────────────────────
    let min_pressure_node_id = network
        .nodes
        .get(min_pressure_idx)
        .map(|n| n.base.id.clone())
        .unwrap_or_default();
    let min_pressure_m = if min_pressure_val.is_finite() {
        min_pressure_val
    } else {
        0.0
    };
    let max_velocity_link_id = network
        .links
        .get(max_velocity_idx)
        .map(|l| l.base.id.clone())
        .unwrap_or_default();

    // Convert demand accumulations from ft³/s·period to m³ (multiply by
    // period duration in seconds then by ft³→m³).
    const FT3_TO_M3: f64 = 0.028_316_847;
    let report_step_s = meta.report_step;
    let inflow_m3 = total_inflow * report_step_s * FT3_TO_M3;
    let outflow_m3 = total_outflow * report_step_s * FT3_TO_M3;
    let balance_pct = if inflow_m3 > 0.0 {
        (outflow_m3 / inflow_m3 * 100.0).min(100.0)
    } else {
        100.0
    };

    Ok(ResultAnalyticsDto {
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
        max_velocity_ms: max_velocity_val,
        pressure_histogram,
        velocity_histogram,
        top_pipes,
        tank_series,
    })
}

/// Return threshold violations by streaming the `.out` file one period at a time.
///
/// Only nodes/links that violate the supplied thresholds are included in the
/// response, so the payload stays small even for large networks.
#[tauri::command]
/// Return pressure/velocity/quality violations for a completed run.
pub fn get_violations(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
    pressure_min_m: f64,
    velocity_max_ms: f64,
) -> Result<ViolationsDto, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let out_path = match &scenario_id {
        Some(sid) => bundle::scenario_results_path(&app_data, &project_id, sid),
        None => bundle::base_results_path(&app_data, &project_id),
    };
    let model_path = match &scenario_id {
        Some(sid) => bundle::scenario_model_path(&app_data, &project_id, sid),
        None => bundle::base_model_path(&app_data, &project_id),
    };
    let raw = std::fs::read(&model_path).map_err(|e| format!("Cannot read model: {e}"))?;
    let network = hydra::io::parse(&raw).map_err(|e| format!("{e:?}"))?;
    let meta = hydra::io::out_reader::read_metadata(&out_path)?;

    let scan = hydra::io::out_reader::scan_analytics(&out_path, &meta)?;
    let node_min_pressure = scan.node_min_pressure;
    let link_max_velocity = scan.link_max_velocity;

    let pressure_violations: Vec<NodeViolationDto> = network
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let min_p = node_min_pressure[i];
            if min_p.is_finite() && min_p < pressure_min_m {
                Some(NodeViolationDto {
                    id: n.base.id.clone(),
                    min_pressure_m: min_p,
                })
            } else {
                None
            }
        })
        .collect();

    let velocity_violations: Vec<LinkViolationDto> = network
        .links
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let max_v = link_max_velocity[i];
            if max_v > velocity_max_ms {
                Some(LinkViolationDto {
                    id: l.base.id.clone(),
                    max_velocity_ms: max_v,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(ViolationsDto {
        pressure_violations,
        velocity_violations,
    })
}

/// Load the INP for a project's base model or a named scenario into
/// `NetworkState`, making it available to `get_nodes` / `get_links`.
///
/// Returns a lightweight nodes+links snapshot when loaded, or `null` when the
/// target INP does not exist on disk yet.
#[tauri::command]
/// Parse the project bundle's INP and load it into `NetworkState`.
pub fn load_project_network(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<NetworkSnapshotDto>, String> {
    validate_id(&project_id)?;
    if let Some(sid) = &scenario_id {
        validate_id(sid)?;
    }
    let app_data = app_data_dir(&app)?;
    let path = match &scenario_id {
        Some(sid) => bundle::scenario_model_path(&app_data, &project_id, sid),
        None => bundle::base_model_path(&app_data, &project_id),
    };
    if !path.exists() {
        *state.0.lock().unwrap() = NetworkStateInner::Empty;
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let network = hydra::io::parse(&bytes).map_err(|e| format!("{e:?}"))?;
    let dto = network_to_dto(&network);
    let snapshot = NetworkSnapshotDto {
        nodes: dto.nodes.clone(),
        links: dto.links.clone(),
    };
    *state.0.lock().unwrap() = NetworkStateInner::Loaded {
        raw_bytes: bytes,
        network,
        dto,
    };
    Ok(Some(snapshot))
}

/// Apply a single field change to the in-memory `Network`, re-serialise it to
/// INP bytes, and update `NetworkState`.
///
/// `kind`  — `"junction"` | `"reservoir"` | `"tank"` | `"pipe"` | `"pump"`
/// `id`    — element ID as it appears in the INP
/// `field` — camelCase field name matching the frontend's display label
/// `value` — new value **in the same display units the frontend uses**:
///   • distances / elevations : metres  (m)
///   • flows / demands        : litres per second  (L/s)
///   • pipe diameters         : millimetres  (mm)
///   • roughness / speed      : dimensionless number
///   • status                 : string `"Open"` | `"Closed"`
///   • curve / headPattern    : string ID
///
/// Returns the updated `NetworkDto` on success so the frontend can refresh.
#[tauri::command]
/// Apply a single field mutation to a `Network` in place.
///
/// All value conversions mirror the ones in `patch_element`; this helper is
/// shared between `patch_element` (commits to state) and `preview_patches`
/// (dry-run, never touches state).
fn apply_patch_to_network(
    network: &mut hydra::Network,
    kind: &str,
    id: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    const M_TO_FT: f64 = 1.0 / 0.3048;
    const LPS_TO_CFS: f64 = 1.0 / 28.3168;
    const MM_TO_FT: f64 = 1.0 / 304.8;

    let as_f64 = |v: &serde_json::Value| -> Result<f64, String> {
        v.as_f64()
            .ok_or_else(|| format!("expected number, got {v}"))
    };

    match kind {
        "junction" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Junction(_)))
                .ok_or_else(|| format!("junction '{id}' not found"))?;
            match field {
                "elevation" => {
                    node.base.elevation = as_f64(&value)? * M_TO_FT;
                }
                "baseDemand" => {
                    if let hydra::NodeKind::Junction(ref mut j) = node.kind {
                        let demand_cfs = as_f64(&value)? * LPS_TO_CFS;
                        if let Some(first) = j.demands.first_mut() {
                            first.base_demand = demand_cfs;
                        } else {
                            j.demands.push(hydra::DemandCategory {
                                base_demand: demand_cfs,
                                pattern: None,
                                name: None,
                            });
                        }
                    }
                }
                "x" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.0 = as_f64(&value)?;
                }
                "y" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.1 = as_f64(&value)?;
                }
                other => return Err(format!("unknown junction field '{other}'")),
            }
        }
        "reservoir" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Reservoir(_)))
                .ok_or_else(|| format!("reservoir '{id}' not found"))?;
            match field {
                "head" => {
                    node.base.elevation = as_f64(&value)? * M_TO_FT;
                }
                "headPattern" => {
                    if let hydra::NodeKind::Reservoir(ref mut r) = node.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        r.head_pattern = if s.is_empty() { None } else { Some(s) };
                    }
                }
                "x" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.0 = as_f64(&value)?;
                }
                "y" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.1 = as_f64(&value)?;
                }
                other => return Err(format!("unknown reservoir field '{other}'")),
            }
        }
        "tank" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Tank(_)))
                .ok_or_else(|| format!("tank '{id}' not found"))?;
            match field {
                "elevation" => {
                    let new_bottom_ft = as_f64(&value)? * M_TO_FT;
                    if let hydra::NodeKind::Tank(ref t) = node.kind {
                        node.base.elevation = new_bottom_ft + t.min_level;
                    }
                }
                "minLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        let old_min = t.min_level;
                        let new_min = as_f64(&value)? * M_TO_FT;
                        node.base.elevation = node.base.elevation - old_min + new_min;
                        t.min_level = new_min;
                    }
                }
                "maxLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.max_level = as_f64(&value)? * M_TO_FT;
                    }
                }
                "initialLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.initial_level = as_f64(&value)? * M_TO_FT;
                    }
                }
                "diameter" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.diameter = as_f64(&value)? * M_TO_FT;
                    }
                }
                "volumeCurve" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        t.volume_curve = if s.is_empty() { None } else { Some(s) };
                    }
                }
                "x" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.0 = as_f64(&value)?;
                }
                "y" => {
                    let entry = network
                        .coordinates
                        .entry(id.to_string())
                        .or_insert((0.0, 0.0));
                    entry.1 = as_f64(&value)?;
                }
                other => return Err(format!("unknown tank field '{other}'")),
            }
        }
        "pipe" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Pipe(_)))
                .ok_or_else(|| format!("pipe '{id}' not found"))?;
            if let hydra::LinkKind::Pipe(ref mut p) = link.kind {
                match field {
                    "length" => {
                        p.length = as_f64(&value)? * M_TO_FT;
                    }
                    "diameter" => {
                        let new_diam_ft = as_f64(&value)? * MM_TO_FT;
                        if p.minor_loss > 0.0 {
                            let old_d4 = p.diameter.powi(4);
                            let kv = p.minor_loss * old_d4 / 0.02517;
                            let new_d4 = new_diam_ft.powi(4);
                            p.minor_loss = 0.02517 * kv / new_d4;
                        }
                        p.diameter = new_diam_ft;
                    }
                    "roughness" => {
                        p.roughness = as_f64(&value)?;
                    }
                    "status" => {
                        let s = value.as_str().unwrap_or("Open");
                        link.base.initial_status = match s.to_ascii_lowercase().as_str() {
                            "closed" => hydra::LinkStatus::Closed,
                            _ => hydra::LinkStatus::Open,
                        };
                    }
                    other => return Err(format!("unknown pipe field '{other}'")),
                }
            }
        }
        "pump" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Pump(_)))
                .ok_or_else(|| format!("pump '{id}' not found"))?;
            match field {
                "speed" => {
                    link.base.initial_setting = Some(as_f64(&value)?);
                }
                "curve" => {
                    if let hydra::LinkKind::Pump(ref mut p) = link.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        p.head_curve = if s.is_empty() { None } else { Some(s) };
                        // Curve and constant power are mutually exclusive.
                        if p.head_curve.is_some() {
                            p.power = None;
                        }
                    }
                }
                "powerKw" => {
                    if let hydra::LinkKind::Pump(ref mut p) = link.kind {
                        // power is stored in Watts; input is kW
                        p.power = Some(as_f64(&value)? * 1000.0);
                        // Constant power replaces the head curve.
                        p.head_curve = None;
                    }
                }
                other => return Err(format!("unknown pump field '{other}'")),
            }
        }
        "valve" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Valve(_)))
                .ok_or_else(|| format!("valve '{id}' not found"))?;
            match field {
                "diameter" => {
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        v.diameter = as_f64(&value)? * MM_TO_FT;
                    }
                }
                "valveType" => {
                    let s = value.as_str().unwrap_or("").to_ascii_uppercase();
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        v.valve_type = match s.as_str() {
                            "PRV" => hydra::ValveType::Prv,
                            "PSV" => hydra::ValveType::Psv,
                            "FCV" => hydra::ValveType::Fcv,
                            "TCV" => hydra::ValveType::Tcv,
                            "GPV" => hydra::ValveType::Gpv,
                            "PCV" => hydra::ValveType::Pcv,
                            "PBV" => hydra::ValveType::Pbv,
                            other => return Err(format!("unknown valve type '{other}'")),
                        };
                    }
                }
                "valveSetting" => {
                    let raw = as_f64(&value)?;
                    // Read the valve type before taking a mutable borrow on link.kind.
                    let vt = if let hydra::LinkKind::Valve(ref v) = link.kind {
                        v.valve_type
                    } else {
                        unreachable!()
                    };
                    link.base.initial_setting = Some(match vt {
                        hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => {
                            raw * M_TO_FT
                        }
                        hydra::ValveType::Fcv => raw * LPS_TO_CFS,
                        _ => raw,
                    });
                }
                "valveCurve" => {
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        v.curve = if s.is_empty() { None } else { Some(s) };
                    }
                }
                other => return Err(format!("unknown valve field '{other}'")),
            }
        }
        other => return Err(format!("unknown element kind '{other}'")),
    }
    Ok(())
}

#[tauri::command]
/// Apply a single property edit to the in-memory network.
pub fn patch_element(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
    field: String,
    value: serde_json::Value,
) -> Result<NetworkDto, String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                raw_bytes,
                network,
                dto,
            } => {
                apply_patch_to_network(network, &kind, &id, &field, value)?;
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(dto.clone())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Move a node to a new coordinate position in a single write (avoids two
/// serial `patch_element` calls and two INP re-serialisations).
#[tauri::command]
pub fn patch_node_position(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                raw_bytes,
                network,
                dto,
            } => {
                let entry = network.coordinates.entry(id.clone()).or_insert((0.0, 0.0));
                entry.0 = x;
                entry.1 = y;
                if let Some(node) = dto.nodes.iter_mut().find(|n| n.id == id) {
                    node.x = x;
                    node.y = y;
                }
                *raw_bytes = hydra::write_inp(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Remove a node or link from the in-memory network.
///
/// `kind` must be one of `"junction"`, `"reservoir"`, `"tank"`, `"pipe"`,
/// `"pump"`, or `"valve"`.  The element is removed from the relevant vec and
/// all ancillary maps (`coordinates`, `vertices`, `node_tags`, `link_tags`).
/// Any links that reference a deleted node are also removed (dangling links).
/// All `base.index` values are rebuilt after deletion so the INP writer
/// produces a valid file.
#[tauri::command]
pub fn delete_element(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                raw_bytes,
                network,
                dto,
            } => {
                match kind.as_str() {
                    "junction" | "reservoir" | "tank" => {
                        let pos = network
                            .nodes
                            .iter()
                            .position(|n| n.base.id == id)
                            .ok_or_else(|| format!("node '{}' not found", id))?;
                        let node_1based = pos + 1;
                        // Collect + remove dangling links that reference this node.
                        let dangling: Vec<String> = network
                            .links
                            .iter()
                            .filter(|l| {
                                l.base.from_node == node_1based || l.base.to_node == node_1based
                            })
                            .map(|l| l.base.id.clone())
                            .collect();
                        for lid in &dangling {
                            network.vertices.remove(lid);
                            network.link_tags.remove(lid);
                        }
                        network.links.retain(|l| {
                            l.base.from_node != node_1based && l.base.to_node != node_1based
                        });
                        // Remove the node itself.
                        network.nodes.remove(pos);
                        network.coordinates.remove(&id);
                        network.node_tags.remove(&id);
                        // Rebuild node indices and fix up link from/to references.
                        for (i, n) in network.nodes.iter_mut().enumerate() {
                            n.base.index = i + 1;
                        }
                        for l in network.links.iter_mut() {
                            // from_node and to_node are 1-based; shift down if they
                            // referred to a node that was after the deleted one.
                            if l.base.from_node > node_1based {
                                l.base.from_node -= 1;
                            }
                            if l.base.to_node > node_1based {
                                l.base.to_node -= 1;
                            }
                        }
                    }
                    "pipe" | "pump" | "valve" => {
                        let pos = network
                            .links
                            .iter()
                            .position(|l| l.base.id == id)
                            .ok_or_else(|| format!("link '{}' not found", id))?;
                        network.links.remove(pos);
                        network.vertices.remove(&id);
                        network.link_tags.remove(&id);
                        // Rebuild link indices.
                        for (i, l) in network.links.iter_mut().enumerate() {
                            l.base.index = i + 1;
                        }
                    }
                    other => return Err(format!("unknown element kind '{}'", other)),
                }
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Add a new node (junction, tank, or reservoir) to the in-memory network.
///
/// `id` must be unique across all nodes and links.  `x` / `y` are geographic
/// coordinates (longitude / latitude in WGS-84) stored directly in
/// `[COORDINATES]`.  Sensible hydraulic defaults are used for all
/// type-specific fields so the resulting network is immediately parseable.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_node(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
    x: f64,
    y: f64,
    elevation: Option<f64>,
    min_level: Option<f64>,
    max_level: Option<f64>,
    initial_level: Option<f64>,
) -> Result<(), String> {
    const M_TO_FT: f64 = 1.0 / 0.3048;
    let elev_ft = elevation.unwrap_or(0.0) * M_TO_FT;
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                raw_bytes,
                network,
                dto,
            } => {
                if id.trim().is_empty() {
                    return Err("ID must not be empty".into());
                }
                if network.nodes.iter().any(|n| n.base.id == id)
                    || network.links.iter().any(|l| l.base.id == id)
                {
                    return Err(format!("ID '{}' is already in use", id));
                }
                let index = network.nodes.len() + 1;
                // Tank level defaults: ~3 m min gap, ~1.5 m initial (matching original 10 ft / 5 ft).
                let min_ft = min_level.unwrap_or(0.0) * M_TO_FT;
                let max_ft = max_level.map(|v| v * M_TO_FT).unwrap_or(10.0);
                let init_ft = initial_level.map(|v| v * M_TO_FT).unwrap_or(5.0);
                let node_kind = match kind.as_str() {
                    "junction" => hydra::NodeKind::Junction(hydra::Junction {
                        demands: vec![hydra::DemandCategory {
                            base_demand: 0.0,
                            pattern: None,
                            name: None,
                        }],
                        emitter_coeff: 0.0,
                        emitter_exp: 0.5,
                    }),
                    "reservoir" => {
                        hydra::NodeKind::Reservoir(hydra::Reservoir { head_pattern: None })
                    }
                    "tank" => hydra::NodeKind::Tank(hydra::Tank {
                        min_level: min_ft,
                        max_level: max_ft,
                        initial_level: init_ft,
                        diameter: 10.0,
                        min_volume: 0.0,
                        volume_curve: None,
                        mix_model: hydra::MixModel::Cstr,
                        mix_fraction: 1.0,
                        bulk_coeff: 0.0,
                        overflow: false,
                        head_pattern: None,
                    }),
                    other => return Err(format!("unknown node kind '{}'", other)),
                };
                // For tanks: EPANET stores base.elevation = bottom + min_level (the minimum
                // piezometric head).  For junctions / reservoirs: base.elevation = elevation.
                let base_elev = if matches!(node_kind, hydra::NodeKind::Tank(_)) {
                    elev_ft + min_ft
                } else {
                    elev_ft
                };
                network.nodes.push(hydra::Node {
                    base: hydra::NodeBase {
                        id: id.clone(),
                        index,
                        elevation: base_elev,
                        initial_quality: 0.0,
                    },
                    kind: node_kind,
                    source: None,
                });
                network.coordinates.insert(id, (x, y));
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Add a new link (pipe or pump) between two existing nodes.
///
/// `id` must be unique across all nodes and links.  `from_id` / `to_id` must
/// identify existing nodes.  Pipe defaults: length 100 m, diameter 300 mm,
/// roughness 100 (Hazen-Williams C).  Pump defaults: constant-power 10 kW.
#[tauri::command]
pub fn create_link(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
    from_id: String,
    to_id: String,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                raw_bytes,
                network,
                dto,
            } => {
                if id.trim().is_empty() {
                    return Err("ID must not be empty".into());
                }
                if network.nodes.iter().any(|n| n.base.id == id)
                    || network.links.iter().any(|l| l.base.id == id)
                {
                    return Err(format!("ID '{}' is already in use", id));
                }
                let from_node = network
                    .nodes
                    .iter()
                    .find(|n| n.base.id == from_id)
                    .map(|n| n.base.index)
                    .ok_or_else(|| format!("node '{}' not found", from_id))?;
                let to_node = network
                    .nodes
                    .iter()
                    .find(|n| n.base.id == to_id)
                    .map(|n| n.base.index)
                    .ok_or_else(|| format!("node '{}' not found", to_id))?;
                if from_node == to_node {
                    return Err("from and to nodes must be different".into());
                }
                let index = network.links.len() + 1;
                let link_kind = match kind.as_str() {
                    "pipe" => hydra::LinkKind::Pipe(hydra::Pipe {
                        length: 100.0,
                        diameter: 0.3,
                        roughness: 100.0,
                        minor_loss: 0.0,
                        check_valve: false,
                        bulk_coeff: None,
                        wall_coeff: None,
                        leak_coeff_1: 0.0,
                        leak_coeff_2: 0.0,
                    }),
                    "pump" => hydra::LinkKind::Pump(hydra::Pump {
                        curve_type: hydra::PumpCurveType::ConstHp,
                        head_curve: None,
                        power: Some(10_000.0), // 10 kW in Watts
                        efficiency_curve: None,
                        default_efficiency: 0.75,
                        speed_pattern: None,
                        energy_price: None,
                        price_pattern: None,
                    }),
                    "valve" => hydra::LinkKind::Valve(hydra::Valve {
                        valve_type: hydra::ValveType::Prv,
                        diameter: 0.3, // 300 mm in metres
                        minor_loss: 0.0,
                        curve: None,
                    }),
                    other => return Err(format!("unknown link kind '{}'", other)),
                };
                let initial_setting = match &link_kind {
                    hydra::LinkKind::Valve(_) => Some(0.0),
                    _ => None,
                };
                network.links.push(hydra::Link {
                    base: hydra::LinkBase {
                        id,
                        index,
                        from_node,
                        to_node,
                        initial_status: hydra::LinkStatus::Open,
                        initial_setting,
                    },
                    kind: link_kind,
                });
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Create a new pump-head curve with default single-point data.
///
/// `id` must be unique within the network. Creates a two-point pump-head curve
/// at [(0.0, head_m), (flow_ls, 0.0)] in SI units (L/s, m), converted to
/// internal US-customary (cfs, ft) for storage.
#[tauri::command]
pub fn create_curve(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    const FT_TO_M: f64 = 0.3048;
    const CFS_TO_LS: f64 = 28.316_846_6;
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                network,
                raw_bytes,
                dto,
            } => {
                if network.curves.iter().any(|c| c.id == id) {
                    return Err(format!("curve '{}' already exists", id));
                }
                // Default: two-point pump-head curve spanning ~(0 L/s, 50 m) to (5 L/s, 0 m)
                network.curves.push(hydra::Curve {
                    id: id.clone(),
                    kind: hydra::CurveKind::PumpHead,
                    points: vec![
                        hydra::CurvePoint {
                            x: 0.0,
                            y: 50.0 / FT_TO_M,
                        },
                        hydra::CurvePoint {
                            x: 5.0 / CFS_TO_LS,
                            y: 0.0,
                        },
                    ],
                });
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Delete a curve from the network.
///
/// Fails if any pump, valve, or tank still references the curve (by
/// head-curve, valve-curve, or volume-curve respectively) — the reference
/// must be cleared first so the network never ends up with a dangling curve
/// ID that would fail to parse on the next INP round-trip.
#[tauri::command]
pub fn delete_curve(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                network,
                raw_bytes,
                dto,
            } => {
                if !network.curves.iter().any(|c| c.id == id) {
                    return Err(format!("curve '{}' not found", id));
                }

                let mut referenced_by: Vec<String> = Vec::new();
                for l in &network.links {
                    if let hydra::LinkKind::Pump(p) = &l.kind {
                        if p.head_curve.as_deref() == Some(id.as_str()) {
                            referenced_by.push(l.base.id.clone());
                        }
                    }
                    if let hydra::LinkKind::Valve(v) = &l.kind {
                        if v.curve.as_deref() == Some(id.as_str()) {
                            referenced_by.push(l.base.id.clone());
                        }
                    }
                }
                for n in &network.nodes {
                    if let hydra::NodeKind::Tank(t) = &n.kind {
                        if t.volume_curve.as_deref() == Some(id.as_str()) {
                            referenced_by.push(n.base.id.clone());
                        }
                    }
                }
                if !referenced_by.is_empty() {
                    return Err(format!(
                        "curve '{}' is still attached to {}; detach it first",
                        id,
                        referenced_by.join(", ")
                    ));
                }

                network.curves.retain(|c| c.id != id);
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

///
/// `xs`/`ys` must be in the same display units returned by `get_curves`
/// (flow L/s and head m for pump-head curves; raw pass-through units for all
/// other curve kinds) and have equal length. Pump-head curves require at
/// least 2 points.
#[tauri::command]
pub fn update_curve_points(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    xs: Vec<f64>,
    ys: Vec<f64>,
) -> Result<(), String> {
    const FT_TO_M: f64 = 0.3048;
    const CFS_TO_LS: f64 = 28.316_846_6;
    if xs.len() != ys.len() {
        return Err("mismatched point array lengths".into());
    }
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                network,
                raw_bytes,
                dto,
            } => {
                let curve = network
                    .curves
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or_else(|| format!("curve '{}' not found", id))?;
                if curve.kind == hydra::CurveKind::PumpHead && xs.len() < 2 {
                    return Err("pump-head curves require at least 2 points".into());
                }
                curve.points = if curve.kind == hydra::CurveKind::PumpHead {
                    xs.iter()
                        .zip(ys.iter())
                        .map(|(&x, &y)| hydra::CurvePoint {
                            x: x / CFS_TO_LS,
                            y: y / FT_TO_M,
                        })
                        .collect()
                } else {
                    xs.iter()
                        .zip(ys.iter())
                        .map(|(&x, &y)| hydra::CurvePoint { x, y })
                        .collect()
                };
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Create a new time pattern with flat multipliers (all 1.0) at 24 hourly steps.
///
/// `id` must be unique within the network.
#[tauri::command]
pub fn create_pattern(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                network,
                raw_bytes,
                dto,
            } => {
                if network.patterns.iter().any(|p| p.id == id) {
                    return Err(format!("pattern '{}' already exists", id));
                }
                network.patterns.push(hydra::Pattern {
                    id: id.clone(),
                    factors: vec![1.0; 24],
                });
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

/// Delete a time pattern from the network.
///
/// Fails if any junction demand, reservoir/tank head pattern, pump
/// speed/price pattern, or the global default/energy-price pattern (from
/// `[OPTIONS]`) still references it — the reference must be cleared first so
/// the network never ends up with a dangling pattern ID that would fail to
/// parse on the next INP round-trip.
#[tauri::command]
pub fn delete_pattern(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    let result = {
        let mut guard = state.0.lock().unwrap();
        match &mut *guard {
            NetworkStateInner::Loaded {
                network,
                raw_bytes,
                dto,
            } => {
                if !network.patterns.iter().any(|p| p.id == id) {
                    return Err(format!("pattern '{}' not found", id));
                }

                let mut referenced_by: Vec<String> = Vec::new();
                for n in &network.nodes {
                    match &n.kind {
                        hydra::NodeKind::Junction(j) => {
                            if j.demands
                                .iter()
                                .any(|d| d.pattern.as_deref() == Some(id.as_str()))
                            {
                                referenced_by.push(n.base.id.clone());
                            }
                        }
                        hydra::NodeKind::Reservoir(r) => {
                            if r.head_pattern.as_deref() == Some(id.as_str()) {
                                referenced_by.push(n.base.id.clone());
                            }
                        }
                        hydra::NodeKind::Tank(t) => {
                            if t.head_pattern.as_deref() == Some(id.as_str()) {
                                referenced_by.push(n.base.id.clone());
                            }
                        }
                    }
                }
                for l in &network.links {
                    if let hydra::LinkKind::Pump(p) = &l.kind {
                        if p.speed_pattern.as_deref() == Some(id.as_str())
                            || p.price_pattern.as_deref() == Some(id.as_str())
                        {
                            referenced_by.push(l.base.id.clone());
                        }
                    }
                }
                if network.options.default_pattern.as_deref() == Some(id.as_str()) {
                    referenced_by.push("global default pattern (Options)".into());
                }
                if network.options.energy_price_pattern.as_deref() == Some(id.as_str()) {
                    referenced_by.push("global energy price pattern (Options)".into());
                }
                if !referenced_by.is_empty() {
                    return Err(format!(
                        "pattern '{}' is still attached to {}; detach it first",
                        id,
                        referenced_by.join(", ")
                    ));
                }

                network.patterns.retain(|p| p.id != id);
                *raw_bytes = hydra::write_inp(network);
                *dto = network_to_dto(network);
                Ok(())
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    if result.is_ok() {
        let _ = app.emit(NETWORK_CHANGED_EVENT, ());
    }
    result
}

///
/// Used by the "Preview changes" diff dialog so the frontend can compare the
/// saved file against a prospective patched version.
#[tauri::command]
/// Return the raw INP text for a project or scenario.
pub fn get_project_inp(app: tauri::AppHandle, project_id: String) -> Result<String, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let path = bundle::base_model_path(&app_data, &project_id);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// A single patch entry passed to `preview_patches`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchItem {
    pub kind: String,
    pub id: String,
    pub field: String,
    pub value: serde_json::Value,
}

/// Apply a list of patches to a temporary clone of the in-memory network and
/// return the resulting INP text, **without** mutating `NetworkState`.
///
/// Used by the "Preview changes" diff dialog so the frontend can show a diff
/// between the on-disk file and what would be written after saving.
#[tauri::command]
/// Return the INP text that would result from applying pending patches.
pub fn preview_patches(
    state: tauri::State<'_, NetworkState>,
    patches: Vec<PatchItem>,
) -> Result<String, String> {
    let mut network = {
        let guard = state.0.lock().unwrap();
        match &*guard {
            NetworkStateInner::Loaded { network, .. } => network.clone(),
            NetworkStateInner::Empty => return Err("no network loaded".into()),
        }
    };

    for patch in patches {
        apply_patch_to_network(
            &mut network,
            &patch.kind,
            &patch.id,
            &patch.field,
            patch.value,
        )?;
    }

    let new_bytes = hydra::write_inp(&network);
    String::from_utf8(new_bytes).map_err(|e| e.to_string())
}

/// Compile-time version string for the Hydra engine library.
const HYDRA_VERSION: &str = hydra::HYDRA_VERSION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    /// Version of the hydra engine library.
    pub hydra: &'static str,
    /// Version of this application binary (hydra-gui crate).
    pub app: &'static str,
}

#[tauri::command]
/// Return available scenario versions (result timestamps).
pub fn get_versions() -> Versions {
    Versions {
        hydra: HYDRA_VERSION,
        app: env!("CARGO_PKG_VERSION"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_modified ───────────────────────────────────────────────────

    #[test]
    fn format_modified_just_now() {
        let label = format_modified(meta::now_secs());
        assert_eq!(label, "just now");
    }

    #[test]
    fn format_modified_minutes() {
        let label = format_modified(meta::now_secs() - 300); // 5 minutes ago
        assert_eq!(label, "5m ago");
    }

    #[test]
    fn format_modified_hours() {
        let label = format_modified(meta::now_secs() - 7_200); // 2 hours ago
        assert_eq!(label, "2h ago");
    }

    #[test]
    fn format_modified_days() {
        let label = format_modified(meta::now_secs() - 3 * 86_400); // 3 days ago
        assert_eq!(label, "3d ago");
    }

    #[test]
    fn format_modified_months() {
        let label = format_modified(meta::now_secs() - 31 * 86_400); // 31 days ago
        assert_eq!(label, "1mo ago");
    }

    #[test]
    fn format_modified_two_months() {
        let label = format_modified(meta::now_secs() - 65 * 86_400); // ~2 months ago
        assert_eq!(label, "2mo ago");
    }

    // ── project_to_dto state derivation ──────────────────────────────────

    fn sample_meta(nodes: u32, links: u32) -> meta::ProjectMeta {
        meta::ProjectMeta {
            name: "test".into(),
            description: None,
            source_crs: "EPSG:4326".into(),
            node_count: nodes,
            link_count: links,
            analysis_options: None,
        }
    }

    #[test]
    fn dto_state_draft_when_no_nodes_no_sim() {
        let dto = project_to_dto("d", &sample_meta(0, 0), 0, None, "not-run", false, 0);
        assert_eq!(dto.state, "draft");
    }

    #[test]
    fn dto_state_ready_when_nodes_present_no_sim() {
        let dto = project_to_dto("r", &sample_meta(5, 4), 0, None, "not-run", false, 0);
        assert_eq!(dto.state, "ready");
    }

    #[test]
    fn dto_state_simulated_when_done() {
        let dto = project_to_dto("s", &sample_meta(5, 4), 0, None, "done", false, 0);
        assert_eq!(dto.state, "simulated");
    }

    #[test]
    fn dto_folder_missing_propagated() {
        let dto = project_to_dto("m", &sample_meta(0, 0), 0, None, "not-run", true, 0);
        assert!(dto.folder_missing);
    }

    #[test]
    fn dto_last_run_label_absent_when_no_sim() {
        let dto = project_to_dto("nr", &sample_meta(3, 2), 0, None, "not-run", false, 0);
        assert!(dto.last_run_label.is_none());
    }

    // ── mtime_secs ────────────────────────────────────────────────────────

    #[test]
    fn mtime_secs_returns_none_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = meta::mtime_secs(&dir.path().join("nonexistent.txt"));
        assert!(result.is_none());
    }

    #[test]
    fn mtime_secs_returns_some_for_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"hello").unwrap();
        let result = meta::mtime_secs(&path);
        assert!(result.is_some());
        let t = result.unwrap();
        assert!(t > 0);
    }

    // ── sim_state_from_results ────────────────────────────────────────────

    #[test]
    fn sim_state_done_when_results_exist() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("results.out");
        std::fs::write(&p, b"dummy").unwrap();
        assert_eq!(meta::sim_state_from_results(&p), "done");
    }

    #[test]
    fn sim_state_not_run_when_no_results() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("results.out");
        assert_eq!(meta::sim_state_from_results(&p), "not-run");
    }
}
