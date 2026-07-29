//! Filesystem-based metadata for Hydra GUI.
//!
//! Each project stores user-assigned metadata in
//! `<app_data>/projects/<id>/meta.json`. Each scenario stores its metadata
//! in `<app_data>/projects/<id>/scenarios/<sc-id>/meta.json`.
//!
//! Everything that can be derived at runtime (IDs from directory names, sim
//! state from `results.out` existence, counts from directory enumeration, and
//! timestamps from file mtimes) is NOT stored here.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Project metadata ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// File-format version. Defaulted rather than required: projects written
    /// before the field existed are v1 by definition, and a required field
    /// would make every one of them fail to parse — which `list_projects`
    /// turns into the project vanishing from the list entirely.
    #[serde(default = "v1")]
    pub version: u32,
    pub name: String,
    /// Engine key from the `hydra::common` registry (`"wds"`, …). Projects
    /// written before the field existed default to `"wds"` — the only
    /// engine that existed then.
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default = "default_crs")]
    pub source_crs: String,
    #[serde(default)]
    pub node_count: u32,
    #[serde(default)]
    pub link_count: u32,
}

fn v1() -> u32 {
    1
}

fn default_crs() -> String {
    "EPSG:4326".into()
}

fn default_engine() -> String {
    "wds".into()
}

// ── Project analysis criteria ─────────────────────────────────────────────────

/// A three-stop threshold band, in SI display units (m, m/s, or L/s).
///
/// The middle stop is named per quantity — pressure has a *required* service
/// level, velocity and flow have a *target* — so the two shapes are kept
/// distinct rather than collapsed into one with a vague middle field.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredBand {
    pub low: f64,
    pub required: f64,
    pub high: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetBand {
    pub low: f64,
    pub target: f64,
    pub high: f64,
}

/// User-defined analysis criteria for one project, stored in
/// `<project>/criteria.json`.
///
/// Deliberately NOT part of the manifest: these are analysis *inputs*, not
/// facts needed to load a project. A project with no criteria file opens
/// normally on the defaults below, whereas one with no manifest cannot be
/// listed at all. Every field defaults, so a partially written or older file
/// degrades field-by-field instead of failing.
///
/// Project-scoped rather than per-scenario on purpose: criteria are the ruler,
/// scenarios are what is measured with it. Per-scenario criteria would make
/// two scenarios' compliance figures incomparable, which is the reason
/// scenarios exist.
///
/// Values are SI — metres, m/s, and L/s — matching what the canvas and the
/// analytics command already exchange, so nothing converts at this boundary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCriteria {
    #[serde(default = "v1")]
    pub version: u32,
    /// Minimum service pressure (m) for the compliance figures.
    #[serde(default = "default_min_pressure")]
    pub min_pressure_m: f64,
    #[serde(default = "default_pressure_band")]
    pub pressure: RequiredBand,
    #[serde(default = "default_velocity_band")]
    pub velocity: TargetBand,
    #[serde(default = "default_flow_band")]
    pub flow: TargetBand,
}

/// EPANET/AWWA-typical minimum service pressure, ~20 psi.
fn default_min_pressure() -> f64 {
    14.0
}

fn default_pressure_band() -> RequiredBand {
    RequiredBand {
        low: 24.0,
        required: 35.0,
        high: 45.0,
    }
}

fn default_velocity_band() -> TargetBand {
    TargetBand {
        low: 0.1,
        target: 0.5,
        high: 1.5,
    }
}

fn default_flow_band() -> TargetBand {
    TargetBand {
        low: 0.1,
        target: 1.0,
        high: 10.0,
    }
}

impl Default for ProjectCriteria {
    fn default() -> Self {
        Self {
            version: 1,
            min_pressure_m: default_min_pressure(),
            pressure: default_pressure_band(),
            velocity: default_velocity_band(),
            flow: default_flow_band(),
        }
    }
}

/// Read `<dir>/criteria.json`, or `None` when the project has none.
///
/// Never fails. `None` covers both "never edited" — the normal state — and an
/// unreadable or corrupt file, which degrades to defaults rather than taking
/// the project's analysis view down with it.
///
/// Returning `Option` rather than defaults lets callers tell an absent file
/// from a saved one that happens to hold the default values. The canvas needs
/// that distinction: it seeds pressure bands from the model's own service
/// pressures only for a project that has never had criteria saved, and
/// seeding over deliberately chosen bands would discard them on every load.
pub fn read_project_criteria(dir: &Path) -> Option<ProjectCriteria> {
    let bytes = std::fs::read(dir.join("criteria.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_project_criteria(dir: &Path, criteria: &ProjectCriteria) -> Result<(), String> {
    write_meta_named(dir, "criteria.json", criteria)
}

// ── Scenario metadata ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioMeta {
    pub name: String,
    #[serde(default)]
    pub parent_scenario_id: Option<String>,
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

/// Read and parse `<dir>/meta.json`.
fn read_meta<T: serde::de::DeserializeOwned>(dir: &Path) -> Result<T, String> {
    let path = dir.join("meta.json");
    let bytes =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("cannot parse {}: {}", path.display(), e))
}

/// Serialise `meta` and write it to `<dir>/meta.json`, creating `dir` as needed.
///
/// Written atomically (temp file + rename via [`bundle::atomic_write`]) so a
/// crash mid-write can never leave a truncated `meta.json` behind.
fn write_meta<T: Serialize>(dir: &Path, meta: &T) -> Result<(), String> {
    write_meta_named(dir, "meta.json", meta)
}

/// Serialise `value` and write it atomically to `<dir>/<file>`.
fn write_meta_named<T: Serialize>(dir: &Path, file: &str, value: &T) -> Result<(), String> {
    let path = dir.join(file);
    let json =
        serde_json::to_string_pretty(value).map_err(|e| format!("cannot serialise {file}: {e}"))?;
    bundle::atomic_write(&path, json.as_bytes())
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

pub fn read_project_meta(dir: &Path) -> Result<ProjectMeta, String> {
    read_meta(dir)
}

pub fn write_project_meta(dir: &Path, meta: &ProjectMeta) -> Result<(), String> {
    write_meta(dir, meta)
}

pub fn read_scenario_meta(dir: &Path) -> Result<ScenarioMeta, String> {
    read_meta(dir)
}

pub fn write_scenario_meta(dir: &Path, meta: &ScenarioMeta) -> Result<(), String> {
    write_meta(dir, meta)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Current epoch seconds.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Return the file/directory modification time as epoch seconds, or `None` on error.
pub fn mtime_secs(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Returns `"done"` if `results_path` exists on disk, `"not-run"` otherwise.
pub fn sim_state_from_results(results_path: &Path) -> &'static str {
    if results_path.exists() {
        "done"
    } else {
        "not-run"
    }
}

// ── Bundle path helpers ───────────────────────────────────────────────────────

/// Filesystem path helpers for project bundles.
/// The filesystem is the source of truth; these helpers locate on-disk
/// artifacts (model, results, reports).
#[allow(dead_code)]
pub mod bundle {
    use super::*;

    pub fn projects_root(app_data: &Path) -> PathBuf {
        app_data.join("projects")
    }

    pub fn project_dir(app_data: &Path, project_id: &str) -> PathBuf {
        projects_root(app_data).join(project_id)
    }

    pub fn base_dir(app_data: &Path, project_id: &str) -> PathBuf {
        project_dir(app_data, project_id).join("base")
    }

    pub fn scenario_dir(app_data: &Path, project_id: &str, scenario_id: &str) -> PathBuf {
        project_dir(app_data, project_id)
            .join("scenarios")
            .join(scenario_id)
    }

    pub fn base_model_path(app_data: &Path, project_id: &str) -> PathBuf {
        base_dir(app_data, project_id).join("model.inp")
    }

    pub fn scenario_model_path(app_data: &Path, project_id: &str, scenario_id: &str) -> PathBuf {
        scenario_dir(app_data, project_id, scenario_id).join("model.inp")
    }

    pub fn base_results_path(app_data: &Path, project_id: &str) -> PathBuf {
        base_dir(app_data, project_id).join("results.out")
    }

    pub fn scenario_results_path(app_data: &Path, project_id: &str, scenario_id: &str) -> PathBuf {
        scenario_dir(app_data, project_id, scenario_id).join("results.out")
    }

    // Note: the reports helpers are currently unused (covered by the
    // module-level `allow(dead_code)`); they document the bundle layout for
    // the upcoming reports feature.
    pub fn base_reports_dir(app_data: &Path, project_id: &str) -> PathBuf {
        base_dir(app_data, project_id).join("reports")
    }

    pub fn scenario_reports_dir(app_data: &Path, project_id: &str, scenario_id: &str) -> PathBuf {
        scenario_dir(app_data, project_id, scenario_id).join("reports")
    }

    /// Atomically write `bytes` to `path` by writing to a sibling temp file
    /// and renaming. Creates parent directories as needed.
    ///
    /// The temp file carries a per-call unique suffix. A name derived only
    /// from the destination would be shared by two concurrent writers to the
    /// same path — `save_project` racing `update_sim_params`, both of which
    /// write `base/model.inp` — and `std::fs::write` is not atomic within the
    /// temp file, so their interleaved bytes would be renamed into place as a
    /// corrupt model. Distinct temp names make the rename the only contended
    /// step, and rename is atomic: one writer wins whole.
    pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let unique = format!("{}.{}", std::process::id(), next_temp_seq());
        let tmp = match path.extension().and_then(|s| s.to_str()) {
            Some(ext) => path.with_extension(format!("{ext}.{unique}.tmp")),
            None => path.with_extension(format!("{unique}.tmp")),
        };
        // A failed write leaves the temp file behind; clear it so a full disk
        // or permission error cannot litter the bundle with partial models.
        if let Err(e) = std::fs::write(&tmp, bytes) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }

    /// Monotonic counter making concurrent `atomic_write` temp names unique
    /// within this process (the pid disambiguates across processes).
    fn next_temp_seq() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        SEQ.fetch_add(1, Ordering::Relaxed)
    }

    /// Recursively delete the on-disk project bundle. No-op if it doesn't exist.
    pub fn delete_project_dir(app_data: &Path, project_id: &str) -> std::io::Result<()> {
        let dir = project_dir(app_data, project_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Recursively delete the on-disk scenario directory. No-op if it doesn't exist.
    pub fn delete_scenario_dir(
        app_data: &Path,
        project_id: &str,
        scenario_id: &str,
    ) -> std::io::Result<()> {
        let dir = scenario_dir(app_data, project_id, scenario_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }
}
