//! What Hydra has written to disk, and how to take the largest part of it
//! back.
//!
//! Settings could reveal the data folder but never said what was in it,
//! which is the question people actually bring: results are by far the
//! biggest thing a run produces — a `.out` file grows with elements ×
//! reporting periods and reaches gigabytes on a real network — and
//! clearing them meant visiting projects one at a time.
//!
//! Results are safe to delete in a way models are not: they are derived,
//! reproducible by running again, and `results.out` is the single artifact
//! every "has this been run" answer is read back from (see
//! `delete_simulation`). Nothing else here is offered for deletion.
//!
//! Both commands delegate rather than reimplement. The per-project clear
//! already exists, already takes every run lock up front, and already
//! knows that a scenario list must come from disk rather than from a
//! caller's cache; a second walk of the same layout would be a second
//! place for those rules to be got wrong.

use serde::Serialize;
use std::path::Path;

use super::projects::{app_data_dir, delete_all_simulations, project_ids, projects_results_size};

/// How much disk the app data folder is using.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataUsage {
    /// Everything under the data folder, results included.
    pub total_bytes: u64,
    /// The part of it that is simulation results — what a clear reclaims.
    pub results_bytes: u64,
    /// How many projects that is spread across.
    pub project_count: u32,
}

/// What a clear actually did.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClearedResults {
    /// Results files removed.
    pub removed: u32,
    /// Projects left untouched because a simulation was writing to one of
    /// their targets. The per-project clear is all-or-nothing, so such a
    /// project keeps every one of its results.
    pub skipped: u32,
}

/// Where the diagnostic log is being written, or `None` when no file could
/// be opened. Managed state rather than recomputed: whether logging to a
/// file succeeded is a fact about this run, not something to ask the
/// filesystem about twice.
pub struct LogLocation(pub Option<std::path::PathBuf>);

#[tauri::command]
/// Reveal the folder holding the diagnostic log.
///
/// Fails with something a reader can act on when there is no log rather
/// than opening their home directory: "logging to a file is not working"
/// and "here are your logs" must not look the same.
pub fn open_log_folder(
    app: tauri::AppHandle,
    location: tauri::State<'_, LogLocation>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let Some(dir) = location.0.clone() else {
        return Err(
            "This run is not writing a log file: the log folder could not be created.".into(),
        );
    };
    // The newest file rather than the folder it is in: a week of dated
    // files is a directory listing to interpret, and the one the reader
    // wants is almost always today's. Revealing it selects it in the file
    // manager, so the folder is right there anyway.
    let target = crate::logging::log_files(&dir)
        .pop()
        .unwrap_or_else(|| dir.clone());
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|e| e.to_string())
}

/// Total size of everything under `dir`, following no symlinks.
///
/// Unreadable entries count as nothing rather than failing the walk: this
/// figure is shown to someone deciding whether to clear results, and a
/// permission error on one file is not a reason to answer "unknown".
pub(crate) fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            total += dir_size(&entry.path());
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

#[tauri::command(async)]
/// How much disk the app data folder uses, and how much of it is results.
pub fn get_data_usage(app: tauri::AppHandle) -> Result<DataUsage, String> {
    let app_data = app_data_dir(&app)?;
    let ids = project_ids(&app_data);
    Ok(DataUsage {
        total_bytes: dir_size(&app_data),
        results_bytes: projects_results_size(app.clone(), ids.clone())?,
        project_count: ids.len() as u32,
    })
}

#[tauri::command(async)]
/// Delete every project's results, leaving models and reports untouched.
///
/// A project whose target is mid-run is skipped rather than failing the
/// whole operation: the run lock exists because deleting a file out from
/// under a running simulation leaves the queue writing to an unlinked
/// inode. One busy project is not a reason to leave the other forty
/// gigabytes on disk, so what was skipped is reported back instead.
pub fn clear_all_results(app: tauri::AppHandle) -> Result<ClearedResults, String> {
    let app_data = app_data_dir(&app)?;
    let mut cleared = ClearedResults {
        removed: 0,
        skipped: 0,
    };
    for id in project_ids(&app_data) {
        match delete_all_simulations(app.clone(), id) {
            Ok(removed) => cleared.removed += removed,
            // The expected failure is "a simulation is running for this
            // target". A project that refuses for any other reason is
            // still a project we did not clear, and saying so is more use
            // than aborting the ones that would have worked.
            Err(_) => cleared.skipped += 1,
        }
    }
    Ok(cleared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::bundle;

    #[test]
    fn dir_size_counts_every_file_beneath_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let pid = "11111111-1111-4111-8111-111111111111";
        let sid = "22222222-2222-4222-8222-222222222222";
        std::fs::create_dir_all(bundle::base_dir(root, pid)).unwrap();
        std::fs::write(bundle::base_model_path(root, pid), vec![b'm'; 40]).unwrap();
        std::fs::write(bundle::base_results_path(root, pid), vec![0u8; 100]).unwrap();
        std::fs::create_dir_all(bundle::scenario_dir(root, pid, sid)).unwrap();
        std::fs::write(bundle::scenario_results_path(root, pid, sid), vec![0u8; 30]).unwrap();

        // The models are on disk too, so the total has to exceed the part
        // a clear would free — a figure that said otherwise would be
        // offering to empty the folder.
        assert_eq!(dir_size(root), 170);
    }

    #[test]
    fn an_absent_folder_measures_zero_rather_than_failing() {
        // A fresh install has written nothing yet, and the Data section is
        // rendered before anything has been created.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(dir_size(&dir.path().join("nothing-here")), 0);
    }

    #[test]
    fn a_symlinked_tree_is_not_counted_twice() {
        // Nothing in the bundle layout creates one, but a user's data
        // folder is a real directory they can put anything in — and a
        // symlink to its own parent would otherwise recurse forever.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/file"), vec![0u8; 10]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root, root.join("loop")).unwrap();
        assert_eq!(dir_size(root), 10);
    }
}
