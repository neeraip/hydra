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
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_crs")]
    pub source_crs: String,
    #[serde(default)]
    pub node_count: u32,
    #[serde(default)]
    pub link_count: u32,
    #[serde(default)]
    pub analysis_options: Option<serde_json::Value>,
}

fn default_crs() -> String {
    "EPSG:4326".into()
}

// ── Scenario metadata ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioMeta {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_scenario_id: Option<String>,
}

// ── I/O helpers ───────────────────────────────────────────────────────────────

pub fn read_project_meta(dir: &Path) -> Result<ProjectMeta, String> {
    let path = dir.join("meta.json");
    let bytes =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("cannot parse {}: {}", path.display(), e))
}

pub fn write_project_meta(dir: &Path, meta: &ProjectMeta) -> Result<(), String> {
    let path = dir.join("meta.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create dir {}: {}", parent.display(), e))?;
    }
    let json =
        serde_json::to_string_pretty(meta).map_err(|e| format!("cannot serialise meta: {e}"))?;
    std::fs::write(&path, json.as_bytes())
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

pub fn read_scenario_meta(dir: &Path) -> Result<ScenarioMeta, String> {
    let path = dir.join("meta.json");
    let bytes =
        std::fs::read(&path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("cannot parse {}: {}", path.display(), e))
}

pub fn write_scenario_meta(dir: &Path, meta: &ScenarioMeta) -> Result<(), String> {
    let path = dir.join("meta.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create dir {}: {}", parent.display(), e))?;
    }
    let json =
        serde_json::to_string_pretty(meta).map_err(|e| format!("cannot serialise meta: {e}"))?;
    std::fs::write(&path, json.as_bytes())
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))
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

    #[allow(dead_code)]
    pub fn base_reports_dir(app_data: &Path, project_id: &str) -> PathBuf {
        base_dir(app_data, project_id).join("reports")
    }

    #[allow(dead_code)]
    pub fn scenario_reports_dir(app_data: &Path, project_id: &str, scenario_id: &str) -> PathBuf {
        scenario_dir(app_data, project_id, scenario_id).join("reports")
    }

    /// Atomically write `bytes` to `path` by writing to a sibling temp file
    /// and renaming. Creates parent directories as needed.
    pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = match path.extension().and_then(|s| s.to_str()) {
            Some(ext) => path.with_extension(format!("{ext}.tmp")),
            None => path.with_extension("tmp"),
        };
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, path)?;
        Ok(())
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
