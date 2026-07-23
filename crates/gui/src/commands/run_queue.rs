//! In-memory run queue: queue state and pruning, the queue commands, and the
//! background processor that drains it one simulation at a time.

use serde::Serialize;
use tauri::Manager;

use crate::meta::{self, bundle};

use super::network_dto::format_inp_parse_error;
use super::projects::{app_data_dir, results_path_for, validate_id};
use super::simulation::{
    emit_or_warn, progress_percent, run_loop_outcome, run_sim_loops, try_acquire_run_target,
    RunLoopError, SimulationProgressDto, SIMULATION_PROGRESS_EVENT,
};

const RUN_QUEUE_UPDATE_EVENT: &str = "run_queue_update";

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
    inner: parking_lot::Mutex<RunQueueInner>,
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
        let mut g = self.inner.lock();
        g.items.push(item);
        Self::prune_terminal_history_locked(&mut g, meta::now_secs());
    }

    /// Atomically claim the processor slot. Returns `true` if the caller
    /// should spawn the queue processor (i.e. it was not already running).
    pub fn try_claim_processor(&self) -> bool {
        let mut g = self.inner.lock();
        if g.processor_running {
            return false;
        }
        g.processor_running = true;
        true
    }

    /// Atomically fetch the next "queued" item (global FIFO across all
    /// projects) or, when none exists, release the processor slot.
    ///
    /// Performing the emptiness check and the release under a single lock
    /// closes the shutdown race where `enqueue_runs` inserts an item after the
    /// processor's final emptiness check but before the release — with a
    /// separate `release_processor()` call that item would sit queued forever
    /// because `try_claim_processor` still saw the slot as taken.
    fn next_queued_or_release(&self) -> Option<RunQueueItem> {
        let mut g = self.inner.lock();
        let next = g.items.iter().find(|i| i.status == "queued").cloned();
        if next.is_none() {
            g.processor_running = false;
        }
        next
    }

    fn get_for_project(&self, project_id: &str) -> Vec<RunQueueItem> {
        self.inner
            .lock()
            .items
            .iter()
            .filter(|i| i.project_id == project_id)
            .cloned()
            .collect()
    }

    fn mark_running(&self, id: &str, started_at: i64) {
        let mut g = self.inner.lock();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "running".into();
            item.started_at = Some(started_at);
        }
    }

    fn mark_done(&self, id: &str, finished_at: i64) {
        let mut g = self.inner.lock();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "done".into();
            item.finished_at = Some(finished_at);
        }
        g.cancel_requested.remove(id);
        Self::prune_terminal_history_locked(&mut g, finished_at);
    }

    fn mark_failed(&self, id: &str, finished_at: i64, error: &str) {
        let mut g = self.inner.lock();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            item.status = "failed".into();
            item.finished_at = Some(finished_at);
            item.error = Some(error.into());
        }
        g.cancel_requested.remove(id);
        Self::prune_terminal_history_locked(&mut g, finished_at);
    }

    fn mark_cancelled(&self, id: &str, finished_at: i64) {
        let mut g = self.inner.lock();
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
        let now = meta::now_secs();
        let mut g = self.inner.lock();
        let mut running_ids: Vec<String> = Vec::new();
        for item in g.items.iter_mut() {
            if item.project_id == project_id && item.status == "queued" {
                item.status = "cancelled".into();
                // Stamp `finished_at` like `mark_cancelled` does so TTL
                // pruning and the frontend see a real completion time.
                item.finished_at = Some(now);
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
            Self::prune_terminal_history_locked(&mut g, now);
        }
        count
    }

    /// Cancel a single queued item, or request cancellation for a running item.
    /// Returns `(cancelled_or_requested, project_id)`.
    fn cancel_item(&self, id: &str) -> (bool, Option<String>) {
        let mut g = self.inner.lock();
        if let Some(item) = g.items.iter_mut().find(|i| i.id == id) {
            let pid = item.project_id.clone();
            if item.status == "queued" {
                let now = meta::now_secs();
                item.status = "cancelled".into();
                item.finished_at = Some(now);
                Self::prune_terminal_history_locked(&mut g, now);
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
        self.inner.lock().cancel_requested.contains(id)
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

// ── Queue commands ────────────────────────────────────────────────────────────

/// Return the current run queue for `project_id`, ordered by `queued_at`.
#[tauri::command]
/// Return the current run queue items.
pub fn get_run_queue(
    run_queue: tauri::State<'_, RunQueue>,
    project_id: String,
) -> Result<Vec<RunQueueItemDto>, String> {
    validate_id(&project_id)?;
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
    emit_or_warn(&app, RUN_QUEUE_UPDATE_EVENT, &project_id);

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
    validate_id(&project_id)?;
    let n = run_queue.cancel_for_project(&project_id);
    emit_or_warn(&app, RUN_QUEUE_UPDATE_EVENT, &project_id);
    Ok(n)
}

/// Cancel a single run item by its run ID. A `queued` item is moved straight
/// to `cancelled`; for a `running` item cancellation is requested (advisory —
/// the current simulation step completes before the item is marked
/// cancelled). Returns `true` when the item was cancelled or the cancel
/// request was newly accepted.
#[tauri::command]
/// Cancel a queued item, or request cancellation of a running one.
pub fn cancel_run_item(
    app: tauri::AppHandle,
    run_queue: tauri::State<'_, RunQueue>,
    run_id: String,
) -> Result<bool, String> {
    validate_id(&run_id)?;
    let (cancelled, project_id) = run_queue.cancel_item(&run_id);
    if cancelled {
        if let Some(pid) = project_id {
            emit_or_warn(&app, RUN_QUEUE_UPDATE_EVENT, &pid);
        }
    }
    Ok(cancelled)
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
/// The caller claims the processor slot (via `try_claim_processor`) before
/// spawning this task; the slot is released atomically with the final
/// queue-emptiness check inside `next_queued_or_release`, so at most one
/// processor is active at any time and no queued item can be stranded.
async fn process_queue(app: tauri::AppHandle) {
    let rq = app.state::<RunQueue>();
    // `next_queued_or_release` releases the processor slot atomically with
    // the queue-emptiness check, so an item enqueued concurrently is either
    // seen here or triggers a fresh processor spawn in `enqueue_runs`.
    while let Some(item) = rq.next_queued_or_release() {
        let now = meta::now_secs();
        rq.mark_running(&item.id, now);
        emit_or_warn(&app, RUN_QUEUE_UPDATE_EVENT, &item.project_id);

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
        emit_or_warn(&app, RUN_QUEUE_UPDATE_EVENT, &item.project_id);
    }
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

    let network = hydra::io::parse(&raw_bytes).map_err(format_inp_parse_error)?;
    let run_quality = network.options.quality_mode != QualityMode::None;
    let duration_seconds = network.options.duration;

    let out_path = results_path_for(&app_data, project_id, scenario_id);

    // Exclusive write access to this target's results.out — fails the queue
    // item with a clear error if a direct run is currently writing it.
    let _run_guard = try_acquire_run_target(project_id, scenario_id)?;

    let mut sim = Simulation::create();
    sim.load(network).map_err(|e| format!("{e:?}"))?;

    let run_id_owned = run_id.to_string();
    let app_emit = app.clone();
    let app_cancel = app.clone();
    let (_, run_err, wall_ms, hyd_steps) = tauri::async_runtime::spawn_blocking(move || {
        run_sim_loops(
            sim,
            Some(out_path),
            duration_seconds,
            run_quality,
            |phase, ss, done, failed, msg| {
                emit_or_warn(
                    &app_emit,
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

    tracing::info!(
        run_id,
        project_id,
        scenario_id = scenario_id.unwrap_or("-"),
        wall_ms,
        hyd_steps,
        outcome = run_loop_outcome(&run_err),
        "queued simulation run finished"
    );

    if let Some(err) = run_err {
        return match err {
            RunLoopError::Failed(msg) => Err(msg),
            // No results cleanup needed: `run_sim_loops` streams to a temp
            // file and discards it on cancellation, so `results.out` still
            // holds the previous successful run (if any).
            RunLoopError::Cancelled => Ok(QueueRunResult::Cancelled),
        };
    }

    Ok(QueueRunResult::Done)
}

#[derive(Debug, Clone, Copy)]
enum QueueRunResult {
    Done,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── run queue ─────────────────────────────────────────────────────────

    fn queued_item(id: &str, project_id: &str) -> RunQueueItem {
        RunQueueItem {
            id: id.into(),
            project_id: project_id.into(),
            target_id: None,
            target_name: None,
            status: "queued".into(),
            queued_at: meta::now_secs(),
            started_at: None,
            finished_at: None,
            error: None,
        }
    }

    #[test]
    fn processor_release_is_atomic_with_empty_check() {
        let rq = RunQueue::default();
        assert!(rq.try_claim_processor());
        // Queue empty: the fetch must release the processor slot...
        assert!(rq.next_queued_or_release().is_none());
        // ...so a subsequent enqueue can reclaim it (no stranded items).
        assert!(rq.try_claim_processor());
        // An item enqueued while the slot is claimed is returned, and the
        // slot stays claimed.
        rq.enqueue(queued_item("r1", "p1"));
        assert_eq!(rq.next_queued_or_release().unwrap().id, "r1");
        assert!(!rq.try_claim_processor());
    }

    #[test]
    fn cancel_for_project_stamps_finished_at() {
        let rq = RunQueue::default();
        rq.enqueue(queued_item("r1", "p1"));
        assert_eq!(rq.cancel_for_project("p1"), 1);
        let items = rq.get_for_project("p1");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, "cancelled");
        assert!(items[0].finished_at.is_some());
    }

    #[test]
    fn cancel_item_stamps_finished_at() {
        let rq = RunQueue::default();
        rq.enqueue(queued_item("r1", "p1"));
        let (cancelled, pid) = rq.cancel_item("r1");
        assert!(cancelled);
        assert_eq!(pid.as_deref(), Some("p1"));
        let items = rq.get_for_project("p1");
        assert_eq!(items[0].status, "cancelled");
        assert!(items[0].finished_at.is_some());
    }
}
