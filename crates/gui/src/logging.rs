//! Where the app's diagnostics go.
//!
//! They went to stderr, which in a packaged desktop app goes nowhere at
//! all: no console window on Windows, no Console.app entry on macOS, and
//! a Linux user only sees it if they happened to launch from a terminal.
//! So every warning the app has ever emitted about a failed read, a
//! rejected command or a corrupt file was written to a stream with no
//! reader — and a bug report could carry version numbers and nothing
//! about what actually happened.
//!
//! Now the same records also go to a dated file the reader can reveal
//! from Settings. Two rules make that safe to leave switched on:
//!
//! *It is bounded.* One file per day, seven kept. A log that grows
//! without limit is a support problem of its own, and the interesting
//! window for "what happened just now" is short.
//!
//! *It is best-effort.* If the log directory cannot be created or opened,
//! the app logs to stderr and starts anyway. Refusing to launch because a
//! diagnostic file could not be written would be the tail wagging the dog.

use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

/// Files older than this are deleted as new ones are created.
const KEPT_FILES: usize = 7;

/// Every log file's name begins with this, which is also what makes
/// pruning safe: nothing else in the directory is ours to delete.
const PREFIX: &str = "hydra";

/// Start logging: stderr always, and a dated file when one can be opened.
///
/// Returns the guard for the file writer's worker thread — logging stops
/// when it is dropped, so the caller must hold it for the life of the
/// process — and the directory being written to, for the command that
/// reveals it.
///
/// Called once, from `setup`, because the log directory is a question only
/// the app handle can answer.
pub fn init(
    log_dir: Option<PathBuf>,
) -> (
    Option<tracing_appender::non_blocking::WorkerGuard>,
    Option<PathBuf>,
) {
    let filter = || EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    let Some(dir) = log_dir else {
        tracing_subscriber::fmt().with_env_filter(filter()).init();
        return (None, None);
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing_subscriber::fmt().with_env_filter(filter()).init();
        tracing::warn!("no log file: cannot create {}: {e}", dir.display());
        return (None, None);
    }

    let appender = tracing_appender::rolling::Builder::new()
        .filename_prefix(PREFIX)
        .filename_suffix("log")
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .max_log_files(KEPT_FILES)
        .build(&dir);

    match appender {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            // Both destinations, not one: a developer running from a
            // terminal should not have to tail a file to see what they
            // already had, and the file exists for the run nobody watched.
            //
            // ANSI off in the file — colour codes in a log someone pastes
            // into an issue are noise wearing escape sequences.
            use tracing_subscriber::layer::SubscriberExt as _;
            use tracing_subscriber::util::SubscriberInitExt as _;
            tracing_subscriber::registry()
                .with(filter())
                .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(writer),
                )
                .init();
            (Some(guard), Some(dir))
        }
        Err(e) => {
            tracing_subscriber::fmt().with_env_filter(filter()).init();
            tracing::warn!("no log file: {e}");
            (None, None)
        }
    }
}

/// The log files currently on disk, newest name last.
///
/// Named by the day they cover, so lexical order is chronological order —
/// which is the whole reason the date is the suffix rather than the
/// prefix of anything.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(PREFIX))
        })
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_only_our_own_files_in_date_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "hydra.2026-08-07.log",
            "hydra.2026-08-09.log",
            "hydra.2026-08-08.log",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        // Pruning deletes what this lists, so it must never claim a file
        // the app did not write.
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();

        let files: Vec<String> = log_files(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            files,
            [
                "hydra.2026-08-07.log",
                "hydra.2026-08-08.log",
                "hydra.2026-08-09.log"
            ]
        );
    }

    #[test]
    fn an_absent_directory_lists_nothing_rather_than_failing() {
        // The first launch reveals the folder before anything has rotated
        // into it.
        let dir = tempfile::tempdir().unwrap();
        assert!(log_files(&dir.path().join("nope")).is_empty());
    }
}
