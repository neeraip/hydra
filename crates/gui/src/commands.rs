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
//! exists. `scenario_count` is the count of subdirectories under `scenarios/`
//! that hold a readable `meta.json` (the same criterion `list_scenarios`
//! applies). While a simulation is running, results stream to a sibling
//! `results.out.tmp` that is renamed onto `results.out` only on success, so
//! `results.out` always holds the last *complete* run.
//!
//! # Backend events
//!
//! | Event | Payload | Throttle |
//! |---|---|---|
//! | `simulation_progress` | `SimulationProgressDto` | on each whole-percent progress bucket or ≥125 ms since the last emit + always on completion/failure |
//! | `run_queue_update` | `String` — the affected project's id; the frontend refetches items via `get_run_queue` | on every queue state change |
//! | `network-changed` | `NetworkChangedPayload` (delta) or `null` | on every mutating command, emitted while the mutator still holds the `NetworkState` lock so event order matches mutation commit order. Element-scoped edits (`patch_element`, `patch_elements`, `patch_node_position`) carry a `NetworkChangedPayload` whose `elements` are the updated element DTOs so the frontend can patch in place; structural mutations emit `null`, which triggers a full snapshot refetch. |
//!
//! # Run queue
//!
//! The queue processor is a single background task; at most one simulation runs
//! at a time. Items are processed FIFO. Each item transitions:
//! `queued` → `running` → `done` | `failed` | `cancelled`.
//! Cancellation of a running item is advisory: the current simulation step
//! completes before the item is marked cancelled. Closing the application
//! discards all pending items (queue state is not persisted).

mod binary_codec;
mod mutations;
mod network_dto;
mod projects;
mod results;
mod run_queue;
mod sim_params;
mod simulation;
#[cfg(test)]
mod test_fixtures;

pub use binary_codec::*;
pub use mutations::*;
pub use network_dto::*;
pub use projects::*;
pub use results::*;
pub use run_queue::*;
pub use sim_params::*;
pub use simulation::*;
