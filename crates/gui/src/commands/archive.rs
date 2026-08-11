//! Archive import: many models in, one project each out.
//!
//! An archive chosen by the user — `.zip`, `.7z`, `.tar`, or `.tar.gz` —
//! is scanned entry by entry: every entry whose
//! extension any GUI-openable engine imports is read, recognised
//! (hydra-common spec §2.5.1), and tolerant-parsed exactly as a single-file
//! import would be, producing a manifest the review step renders. Creating
//! then re-reads the archive and writes one ordinary project bundle per
//! selected entry. Nothing passes through the single-slot `NetworkState`:
//! the scan describes, the create persists, and the wizard's own import path
//! is untouched.
//!
//! Failures are per-entry throughout. One unreadable or unrecognised entry
//! never poisons the archive: the scan reports it against its path, and a
//! create returns an outcome per selection, successes and failures side by
//! side — partial success loudly reported rather than rolled back.

use std::io::Read;

use super::projects::{
    app_data_dir, parse_model_bytes, persist_new_project, require_gui_openable_engine, validate_id,
    ParsedModel, Project,
};

/// Entry-count ceiling per archive: far above any real model collection,
/// low enough to bound a malicious central directory.
const MAX_ENTRIES: usize = 2_048;
/// Decompressed ceiling per entry (bytes). Model text runs to tens of
/// megabytes at the extreme; a quarter gigabyte is not a model.
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
/// Decompressed ceiling across the whole scan (bytes) — the zip-bomb stop.
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

/// One archive entry that looked like a model, described for the review
/// table. `engine` names the single definite recognition claim;
/// `candidates` the plausible ones when no engine was definite (the user
/// picks, mirroring the CLI's `--engine`); `error` the reason an entry that
/// looked importable is not.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveModelEntry {
    /// Path inside the archive, the entry's identity for the create step.
    pub path: String,
    /// File stem, the default project name.
    pub stem: String,
    /// The recognised engine key, when exactly one engine was definite.
    pub engine: Option<String>,
    /// GUI-openable candidate keys when recognition was ambiguous.
    pub candidates: Vec<String>,
    pub node_count: u32,
    pub link_count: u32,
    /// §2.9 findings count — the wizard reports the number; the Issues
    /// panel lists them once the project opens.
    pub finding_count: u32,
    /// §14.10 repairs applied during the trial parse, one message each.
    /// Surfaced per the repair contract; the same repairs apply at create.
    pub repairs: Vec<String>,
    /// External files the model references (rain, climate, interface):
    /// carried into the project when the archive holds them, warned about
    /// when it does not — said here, before the user commits.
    pub sidecars: Vec<SidecarRef>,
    /// Why this entry cannot be imported, when it cannot.
    pub error: Option<String>,
}

/// One referenced auxiliary file: the name as the model wrote it, a
/// human label saying what role it plays, and whether this archive holds
/// it (matched by trailing file name).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRef {
    pub file: String,
    pub label: String,
    pub carried: bool,
}

/// What a scan found: the model-shaped entries, described, and every other
/// file (the likely sidecars), listed so the review step can say what will
/// *not* be imported.
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveScan {
    /// The archive's own path, echoed back for the create call.
    pub archive_path: String,
    pub models: Vec<ArchiveModelEntry>,
    pub others: Vec<String>,
}

/// One selected entry of a create call: which entry, what to name the
/// project, and which engine parses it (the recognised one, or the user's
/// choice for an ambiguous entry).
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSelection {
    pub path: String,
    pub id: String,
    pub name: String,
    pub engine: String,
}

/// The fate of one selection: a created project, or the reason there is
/// none. The create call succeeds as a whole whenever the archive itself
/// was readable — per-entry failure is data, not an exception.
#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveImportOutcome {
    pub path: String,
    pub name: String,
    pub project: Option<Project>,
    pub error: Option<String>,
}

/// The lowercase extensions any GUI-openable engine imports — the same
/// facts the single-file picker builds its filters from, so the scan and
/// the picker agree about what looks like a model.
fn model_extensions() -> Vec<String> {
    let mut out = Vec::new();
    for engine in hydra::common::ENGINES {
        if require_gui_openable_engine(engine.key).is_err() {
            continue;
        }
        for format in engine.import {
            for ext in format.extensions {
                let ext = ext.to_ascii_lowercase();
                if !out.contains(&ext) {
                    out.push(ext);
                }
            }
        }
    }
    out
}

/// External files a drainage model references but an archive import does
/// not carry into the project: rain gage records, climate files, and
/// interface files read at run time. Named so the review table can warn
/// before the user commits, instead of the run refusing after.
fn uds_sidecar_refs(network: &hydra::uds::model::Network) -> Vec<(String, String)> {
    use hydra::uds::model::{FileMode, GageSource, TemperatureSource};
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |file: &str, role: &str| {
        if !out.iter().any(|(f, _)| f == file) {
            out.push((file.to_string(), format!("{role} \"{file}\"")));
        }
    };
    for gage in &network.gages {
        if let GageSource::File { file, .. } = &gage.source {
            push(file, "rain file");
        }
    }
    if let Some(TemperatureSource::File { name, .. }) = &network.climate.temperature {
        push(name, "climate file");
    }
    let iface = &network.interface_files;
    for (slot, role) in [
        (&iface.rainfall, "rainfall interface file"),
        (&iface.runoff, "runoff interface file"),
        (&iface.rdii, "RDII interface file"),
    ] {
        if let Some((FileMode::Use, name)) = slot {
            push(name, role);
        }
    }
    if let Some(name) = &iface.hotstart_use {
        push(name, "hotstart file");
    }
    if let Some(name) = &iface.inflows {
        push(name, "routing inflows file");
    }
    out
}

/// Describe one model-shaped entry: recognise, then trial-parse under the
/// recognised engine. Every failure lands in the entry's `error`.
fn describe_model_entry(path: String, stem: String, bytes: Vec<u8>) -> ArchiveModelEntry {
    let mut entry = ArchiveModelEntry {
        path,
        stem: stem.clone(),
        engine: None,
        candidates: Vec::new(),
        node_count: 0,
        link_count: 0,
        finding_count: 0,
        repairs: Vec::new(),
        sidecars: Vec::new(),
        error: None,
    };
    let engine_key = match hydra::engines::route(&bytes) {
        Ok(descriptor) => descriptor.key,
        Err(hydra::engines::RouteError::Ambiguous { candidates, .. }) => {
            entry.candidates = candidates
                .iter()
                .filter(|key| require_gui_openable_engine(key).is_ok())
                .map(|key| (*key).to_string())
                .collect();
            if entry.candidates.is_empty() {
                entry.error = Some("no engine this GUI opens recognises this file".into());
            }
            return entry;
        }
        Err(e) => {
            entry.error = Some(e.to_string());
            return entry;
        }
    };
    if let Err(e) = require_gui_openable_engine(engine_key) {
        entry.error = Some(e);
        return entry;
    }
    match parse_model_bytes(engine_key, bytes, stem) {
        Ok((parsed, imported)) => {
            entry.engine = Some(engine_key.to_string());
            entry.node_count = imported.node_count;
            entry.link_count = imported.link_count;
            entry.finding_count = imported.findings.len() as u32;
            entry.repairs = imported.repairs;
            if let ParsedModel::Uds { network, .. } = &parsed {
                entry.sidecars = uds_sidecar_refs(network)
                    .into_iter()
                    .map(|(file, label)| SidecarRef {
                        file,
                        label,
                        carried: false,
                    })
                    .collect();
            }
        }
        Err(e) => {
            entry.engine = Some(engine_key.to_string());
            entry.error = Some(e);
        }
    }
    entry
}

/// The archive formats the import reads, told apart by file name — the
/// same way the picker filters, so the two cannot disagree.
enum ArchiveKind {
    Zip,
    SevenZ,
    Tar,
    TarGz,
}

fn archive_kind(path: &std::path::Path) -> Result<ArchiveKind, String> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".zip") {
        Ok(ArchiveKind::Zip)
    } else if name.ends_with(".7z") {
        Ok(ArchiveKind::SevenZ)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Ok(ArchiveKind::TarGz)
    } else if name.ends_with(".tar") {
        Ok(ArchiveKind::Tar)
    } else {
        Err(format!(
            "{name:?} is not a supported archive — this import reads \
             .zip, .7z, .tar, and .tar.gz"
        ))
    }
}

/// One walked entry: its path inside the archive, and — when the walk's
/// `read` callback asked for it — its bytes or its own read error. `None`
/// means the entry was seen but deliberately not read.
type WalkedEntry = (String, Option<Result<Vec<u8>, String>>);

/// The walk's ceilings, spent as entries pass. One instance per walk, so
/// the caps mean the same thing whatever the container format.
struct WalkBudget {
    entries: usize,
    bytes: u64,
}

impl WalkBudget {
    fn new() -> Self {
        WalkBudget {
            entries: MAX_ENTRIES,
            bytes: MAX_TOTAL_BYTES,
        }
    }

    /// Count one entry against the archive-wide ceiling; a breach fails
    /// the whole walk (a malicious directory, not a bad entry).
    fn count_entry(&mut self) -> Result<(), String> {
        if self.entries == 0 {
            return Err(format!(
                "archive holds more than the {MAX_ENTRIES} entries this import accepts"
            ));
        }
        self.entries -= 1;
        Ok(())
    }
}

/// Read one entry's bytes from any decompressed stream, bounded: a
/// declared size over the cap is refused before a byte is read, and the
/// read is clamped so a lying header cannot overshoot either.
fn read_bounded(
    reader: &mut dyn Read,
    declared: u64,
    budget: &mut WalkBudget,
) -> Result<Vec<u8>, String> {
    if declared > MAX_ENTRY_BYTES {
        return Err(format!(
            "entry is {declared} bytes decompressed — larger than any model \
             this import accepts"
        ));
    }
    if declared > budget.bytes {
        return Err("archive exceeds the import's total decompressed-size budget".into());
    }
    let mut bytes = Vec::with_capacity(declared.min(1024 * 1024) as usize);
    let mut clamped = reader.take(declared + 1);
    clamped.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > declared {
        return Err("entry decompresses past its declared size".into());
    }
    budget.bytes -= bytes.len() as u64;
    Ok(bytes)
}

/// Walk every file entry of the archive at `path`, in order, reading the
/// ones `read` asks for. The one place the container formats differ;
/// everything above it — scan, create, caps — is format-blind.
fn walk_archive(
    path: &std::path::Path,
    read: &mut dyn FnMut(&str) -> bool,
) -> Result<Vec<WalkedEntry>, String> {
    match archive_kind(path)? {
        ArchiveKind::Zip => walk_zip(path, read),
        ArchiveKind::SevenZ => walk_7z(path, read),
        ArchiveKind::Tar => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            walk_tar(std::io::BufReader::new(file), read)
        }
        ArchiveKind::TarGz => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            walk_tar(
                flate2::read::GzDecoder::new(std::io::BufReader::new(file)),
                read,
            )
        }
    }
}

fn walk_zip(
    path: &std::path::Path,
    read: &mut dyn FnMut(&str) -> bool,
) -> Result<Vec<WalkedEntry>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("not a readable zip archive: {e}"))?;
    let mut budget = WalkBudget::new();
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        budget.count_entry()?;
        let entry_path = entry.name().to_string();
        let declared = entry.size();
        let bytes = read(&entry_path)
            .then(|| read_bounded(&mut entry, declared, &mut budget))
            .transpose();
        out.push((
            entry_path,
            match bytes {
                Ok(b) => b.map(Ok),
                Err(e) => Some(Err(e)),
            },
        ));
    }
    Ok(out)
}

fn walk_tar(
    reader: impl Read,
    read: &mut dyn FnMut(&str) -> bool,
) -> Result<Vec<WalkedEntry>, String> {
    let mut archive = tar::Archive::new(reader);
    let mut budget = WalkBudget::new();
    let mut out = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| format!("not a readable tar archive: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("unreadable tar entry: {e}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        budget.count_entry()?;
        let entry_path = entry
            .path()
            .map_err(|e| e.to_string())?
            .display()
            .to_string();
        let declared = entry.header().size().map_err(|e| e.to_string())?;
        let bytes = read(&entry_path)
            .then(|| read_bounded(&mut entry, declared, &mut budget))
            .transpose();
        out.push((
            entry_path,
            match bytes {
                Ok(b) => b.map(Ok),
                Err(e) => Some(Err(e)),
            },
        ));
    }
    Ok(out)
}

fn walk_7z(
    path: &std::path::Path,
    read: &mut dyn FnMut(&str) -> bool,
) -> Result<Vec<WalkedEntry>, String> {
    let mut seven = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
        .map_err(|e| format!("not a readable 7z archive: {e}"))?;
    let mut budget = WalkBudget::new();
    let mut out: Vec<WalkedEntry> = Vec::new();
    let mut walk_error: Option<String> = None;
    seven
        .for_each_entries(|entry, reader| {
            if entry.is_directory {
                return Ok(true);
            }
            if let Err(e) = budget.count_entry() {
                walk_error = Some(e);
                return Ok(false);
            }
            let entry_path = entry.name.clone();
            if read(&entry_path) {
                let bytes = read_bounded(reader, entry.size, &mut budget);
                out.push((entry_path, Some(bytes)));
            } else {
                // A solid block decodes front to back: later entries need
                // the skipped one's data consumed, whether or not it was
                // wanted. Drained to nowhere rather than held.
                std::io::copy(reader, &mut std::io::sink()).ok();
                out.push((entry_path, None));
            }
            Ok(true)
        })
        .map_err(|e| format!("not a readable 7z archive: {e}"))?;
    if let Some(e) = walk_error {
        return Err(e);
    }
    Ok(out)
}

/// The junk archive helpers scatter (macOS resource forks and the like) —
/// silently irrelevant rather than listed as leftovers.
fn is_archive_junk(file_name: &str) -> bool {
    file_name.starts_with("._") || file_name == ".DS_Store"
}

/// An entry path's file name, stem, and lowercased extension.
fn split_entry_name(entry_path: &str) -> (&str, String, String) {
    let file_name = entry_path.rsplit(['/', '\\']).next().unwrap_or("");
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            (file_name, stem.to_string(), ext.to_ascii_lowercase())
        }
        _ => (file_name, file_name.to_string(), String::new()),
    }
}

/// Scan an archive on disk into the review manifest.
pub(crate) fn scan_archive_file(path: &std::path::Path) -> Result<ArchiveScan, String> {
    let extensions = model_extensions();
    let wanted = |entry_path: &str| {
        let (file_name, _, ext) = split_entry_name(entry_path);
        !is_archive_junk(file_name) && extensions.contains(&ext)
    };
    let walked = walk_archive(path, &mut |p| wanted(p))?;

    let mut models = Vec::new();
    let mut others = Vec::new();
    for (entry_path, bytes) in walked {
        let (file_name, stem, _) = split_entry_name(&entry_path);
        if is_archive_junk(file_name) {
            continue;
        }
        match bytes {
            None => others.push(entry_path),
            Some(Ok(bytes)) => models.push(describe_model_entry(entry_path, stem, bytes)),
            Some(Err(e)) => models.push(ArchiveModelEntry {
                path: entry_path,
                stem,
                engine: None,
                candidates: Vec::new(),
                node_count: 0,
                link_count: 0,
                finding_count: 0,
                repairs: Vec::new(),
                sidecars: Vec::new(),
                error: Some(e),
            }),
        }
    }
    // A referenced auxiliary file counts as carried when the archive holds
    // an entry with the same trailing file name — the create step copies
    // it into the project by exactly this match.
    let other_basenames: Vec<String> = others
        .iter()
        .map(|p| split_entry_name(p).0.to_ascii_lowercase())
        .collect();
    for model in &mut models {
        for sidecar in &mut model.sidecars {
            let base = split_entry_name(&sidecar.file).0.to_ascii_lowercase();
            sidecar.carried = other_basenames.contains(&base);
        }
    }
    Ok(ArchiveScan {
        archive_path: path.display().to_string(),
        models,
        others,
    })
}

/// Create one project per selection, against an `app_data` root — the
/// command body, testable without a Tauri handle.
///
/// One walk serves every selection: streaming formats have no by-name
/// access, and even for those that do, one pass is the honest cost.
pub(crate) fn create_projects_from_archive_at(
    app_data: &std::path::Path,
    archive_path: &std::path::Path,
    selections: Vec<ArchiveSelection>,
) -> Result<Vec<ArchiveImportOutcome>, String> {
    let wanted: std::collections::HashSet<&str> =
        selections.iter().map(|s| s.path.as_str()).collect();
    let mut entries: std::collections::HashMap<String, Result<Vec<u8>, String>> =
        walk_archive(archive_path, &mut |p| wanted.contains(p))?
            .into_iter()
            .filter_map(|(path, bytes)| bytes.map(|b| (path, b)))
            .collect();
    let mut outcomes = Vec::with_capacity(selections.len());
    // Auxiliary files each created project references, by lowercased
    // trailing name: filled per selection, served by a second walk.
    let mut aux_wanted: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for selection in selections {
        let bytes = entries
            .remove(&selection.path)
            .unwrap_or_else(|| Err(format!("archive has no entry {:?}", selection.path)));
        let outcome = bytes.and_then(|bytes| create_one(app_data, bytes, &selection));
        if let Ok((_, sidecars)) = &outcome {
            for file in sidecars {
                let base = split_entry_name(file).0.to_ascii_lowercase();
                aux_wanted
                    .entry(base)
                    .or_default()
                    .push(selection.id.clone());
            }
        }
        outcomes.push(ArchiveImportOutcome {
            path: selection.path,
            name: selection.name,
            project: outcome.as_ref().ok().map(|(p, _)| p.clone()),
            error: outcome.err(),
        });
    }

    // Carry the referenced auxiliary files (§12.1: runs read them from the
    // bundle). A copy failure is loud but non-fatal: the project exists,
    // and its runs will name exactly what is missing.
    if !aux_wanted.is_empty() {
        let aux_entries = walk_archive(archive_path, &mut |p| {
            let base = split_entry_name(p).0.to_ascii_lowercase();
            aux_wanted.contains_key(&base)
        })?;
        for (entry_path, bytes) in aux_entries {
            let base_name = split_entry_name(&entry_path).0.to_string();
            let Some(Ok(bytes)) = bytes else { continue };
            let Some(project_ids) = aux_wanted.get(&base_name.to_ascii_lowercase()) else {
                continue;
            };
            for id in project_ids {
                let dir = crate::meta::bundle::aux_dir(app_data, id);
                let write = std::fs::create_dir_all(&dir)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        crate::meta::bundle::atomic_write(&dir.join(&base_name), &bytes)
                            .map_err(|e| e.to_string())
                    });
                if let Err(e) = write {
                    for outcome in &mut outcomes {
                        if outcome.project.as_ref().is_some_and(|p| p.id == *id) {
                            outcome.error =
                                Some(format!("project created, but {base_name:?}: {e}"));
                        }
                    }
                }
            }
        }
    }
    Ok(outcomes)
}

/// The whole life of one selection: parse, persist. Any failure is this
/// entry's alone. Returns the project and the auxiliary file names its
/// model references, so the caller can carry them.
fn create_one(
    app_data: &std::path::Path,
    bytes: Vec<u8>,
    selection: &ArchiveSelection,
) -> Result<(Project, Vec<String>), String> {
    validate_id(&selection.id)?;
    require_gui_openable_engine(&selection.engine)?;
    let (_, stem, _) = split_entry_name(&selection.path);
    let (parsed, imported) = parse_model_bytes(&selection.engine, bytes, stem)?;
    let sidecars = match &parsed {
        ParsedModel::Uds { network, .. } => uds_sidecar_refs(network)
            .into_iter()
            .map(|(file, _)| file)
            .collect(),
        ParsedModel::Wds { .. } => Vec::new(),
    };
    let project = persist_new_project(
        app_data,
        &selection.id,
        selection.name.clone(),
        selection.engine.clone(),
        &parsed.served_bytes(),
        imported.node_count,
        imported.link_count,
    )?;
    Ok((project, sidecars))
}

/// Open a native file-picker for a model archive and scan it. Returns
/// `null` when the dialog is cancelled. The scan parses every model in the
/// archive, so all of it runs on the blocking pool.
#[tauri::command]
pub async fn open_and_scan_archive(app: tauri::AppHandle) -> Result<Option<ArchiveScan>, String> {
    use tauri_plugin_dialog::DialogExt;

    let dialog_app = app.clone();
    let scan = tauri::async_runtime::spawn_blocking(move || {
        let path = dialog_app
            .dialog()
            .file()
            .add_filter("Model archive", &["zip", "7z", "tar", "gz", "tgz"])
            .blocking_pick_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        scan_archive_file(&path).map(Some)
    })
    .await
    .map_err(|e| format!("archive scan task panicked: {e}"))??;
    Ok(scan)
}

/// Create one project per selected archive entry. Succeeds whenever the
/// archive is readable; each selection's own failure comes back in its
/// outcome. Ids are minted here — the caller names projects, not
/// directories.
#[tauri::command(async)]
pub fn create_projects_from_archive(
    app: tauri::AppHandle,
    archive_path: String,
    selections: Vec<ArchiveSelectionInput>,
) -> Result<Vec<ArchiveImportOutcome>, String> {
    let app_data = app_data_dir(&app)?;
    let selections = selections
        .into_iter()
        .map(|s| ArchiveSelection {
            id: uuid::Uuid::new_v4().to_string(),
            path: s.path,
            name: s.name,
            engine: s.engine,
        })
        .collect();
    create_projects_from_archive_at(&app_data, std::path::Path::new(&archive_path), selections)
}

/// What the frontend sends per selection — everything but the id, which the
/// backend mints.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSelectionInput {
    pub path: String,
    pub name: String,
    pub engine: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A definite uds model with a file-sourced rain gage — the sidecar
    /// case an archive is most likely to carry.
    const UDS_INP: &str = "\
[OPTIONS]
FLOW_UNITS  CMS
FLOW_ROUTING  DYNWAVE

[RAINGAGES]
rg1  VOLUME  0:01  1.0  FILE  \"rain.dat\"  sta1  MM

[JUNCTIONS]
J1  10  2

[OUTFALLS]
O1  9  FREE

[CONDUITS]
C1  J1  O1  100  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.0  0  0  0
";

    /// Write a zip holding the given (name, bytes) entries to a temp file.
    fn zip_of(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::with_suffix(".zip").expect("temp file");
        let mut zip = zip::ZipWriter::new(file.reopen().expect("reopen"));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            zip.start_file(*name, options).expect("start entry");
            zip.write_all(bytes).expect("write entry");
        }
        zip.finish().expect("finish zip");
        file
    }

    #[test]
    fn scan_recognises_each_model_and_lists_the_rest() {
        let archive = zip_of(&[
            (
                "nets/water.inp",
                super::super::test_fixtures::TEST_INP.as_bytes(),
            ),
            ("nets/drainage.inp", UDS_INP.as_bytes()),
            ("nets/rain.dat", b"sta1 2020 1 1 0 0 1.0"),
            ("._resource-fork.inp", b"junk"),
            ("readme.txt", b"hello"),
        ]);
        let scan = scan_archive_file(archive.path()).expect("scan");

        assert_eq!(scan.models.len(), 2, "{:?}", scan.models);
        let water = scan
            .models
            .iter()
            .find(|m| m.path == "nets/water.inp")
            .expect("water entry");
        assert_eq!(water.engine.as_deref(), Some("wds"));
        assert_eq!(water.stem, "water");
        assert_eq!((water.node_count, water.link_count), (3, 2));
        assert_eq!(water.error, None);
        assert!(water.sidecars.is_empty());

        let drainage = scan
            .models
            .iter()
            .find(|m| m.path == "nets/drainage.inp")
            .expect("drainage entry");
        assert_eq!(drainage.engine.as_deref(), Some("uds"));
        assert_eq!((drainage.node_count, drainage.link_count), (2, 1));
        // The review step must be able to warn about the rain file before
        // the user commits — a run will refuse until it is inlined.
        assert_eq!(drainage.sidecars.len(), 1);
        assert_eq!(drainage.sidecars[0].file, "rain.dat");
        assert_eq!(drainage.sidecars[0].label, "rain file \"rain.dat\"");
        // The archive holds nets/rain.dat, so the reference is carried.
        assert!(drainage.sidecars[0].carried);

        // Non-models are listed (the likely sidecars), resource-fork junk
        // is not.
        assert_eq!(scan.others, vec!["nets/rain.dat", "readme.txt"]);
    }

    #[test]
    fn create_writes_one_ordinary_project_per_selection() {
        let archive = zip_of(&[
            (
                "water.inp",
                super::super::test_fixtures::TEST_INP.as_bytes(),
            ),
            ("drainage.inp", UDS_INP.as_bytes()),
        ]);
        let app_data = tempfile::tempdir().expect("app data");
        let outcomes = create_projects_from_archive_at(
            app_data.path(),
            archive.path(),
            vec![
                ArchiveSelection {
                    path: "water.inp".into(),
                    id: "11111111-1111-1111-1111-111111111111".into(),
                    name: "Water".into(),
                    engine: "wds".into(),
                },
                ArchiveSelection {
                    path: "drainage.inp".into(),
                    id: "22222222-2222-2222-2222-222222222222".into(),
                    name: "Drainage".into(),
                    engine: "uds".into(),
                },
                // A selection that cannot succeed fails alone.
                ArchiveSelection {
                    path: "missing.inp".into(),
                    id: "33333333-3333-3333-3333-333333333333".into(),
                    name: "Ghost".into(),
                    engine: "wds".into(),
                },
            ],
        )
        .expect("create");

        assert_eq!(outcomes.len(), 3);
        assert!(outcomes[0].project.is_some(), "{:?}", outcomes[0].error);
        assert!(outcomes[1].project.is_some(), "{:?}", outcomes[1].error);
        assert!(outcomes[2].project.is_none());
        assert!(outcomes[2].error.as_deref().unwrap().contains("no entry"));

        // Each project is an ordinary bundle: meta.json names its engine,
        // and base/model.inp holds the model.
        for (id, engine, name) in [
            ("11111111-1111-1111-1111-111111111111", "wds", "Water"),
            ("22222222-2222-2222-2222-222222222222", "uds", "Drainage"),
        ] {
            let dir = app_data.path().join("projects").join(id);
            let meta: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(dir.join("meta.json")).expect("meta"),
            )
            .expect("meta json");
            assert_eq!(meta["engine"], engine);
            assert_eq!(meta["name"], name);
            let model =
                std::fs::read_to_string(dir.join("base").join("model.inp")).expect("model.inp");
            assert!(!model.is_empty());
        }

        // The ghost selection left no half-written bundle behind.
        assert!(!app_data
            .path()
            .join("projects")
            .join("33333333-3333-3333-3333-333333333333")
            .exists());
    }

    /// Build the same entries as a `.tar.gz` archive.
    fn targz_of(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::with_suffix(".tar.gz").expect("temp file");
        let gz = flate2::write::GzEncoder::new(
            file.reopen().expect("reopen"),
            flate2::Compression::default(),
        );
        let mut tar = tar::Builder::new(gz);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, *bytes).expect("append");
        }
        tar.into_inner()
            .expect("tar finish")
            .finish()
            .expect("gz finish");
        file
    }

    /// Build the same entries as a `.7z` archive.
    fn sevenz_of(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::with_suffix(".7z").expect("temp file");
        let mut writer =
            sevenz_rust2::ArchiveWriter::new(file.reopen().expect("reopen")).expect("writer");
        for (name, bytes) in entries {
            writer
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file(name),
                    Some(std::io::Cursor::new(bytes.to_vec())),
                )
                .expect("push entry");
        }
        writer.finish().expect("finish 7z");
        file
    }

    /// Every container format answers the same scan the same way: the
    /// walk is the only place formats differ, and this holds it to that.
    #[test]
    fn every_archive_format_scans_identically() {
        let entries: &[(&str, &[u8])] = &[
            (
                "nets/water.inp",
                super::super::test_fixtures::TEST_INP.as_bytes(),
            ),
            ("nets/drainage.inp", UDS_INP.as_bytes()),
            ("nets/rain.dat", b"sta1 2020 1 1 0 0 1.0"),
        ];
        let archives: Vec<tempfile::NamedTempFile> =
            vec![zip_of(entries), targz_of(entries), sevenz_of(entries)];
        for archive in &archives {
            let path = archive.path();
            let scan = scan_archive_file(path).expect("scan");
            let mut engines: Vec<_> = scan
                .models
                .iter()
                .map(|m| (m.path.as_str(), m.engine.as_deref()))
                .collect();
            engines.sort_unstable();
            assert_eq!(
                engines,
                vec![
                    ("nets/drainage.inp", Some("uds")),
                    ("nets/water.inp", Some("wds")),
                ],
                "{path:?}"
            );
            assert_eq!(scan.others, vec!["nets/rain.dat"], "{path:?}");
            let drainage = scan
                .models
                .iter()
                .find(|m| m.path == "nets/drainage.inp")
                .unwrap();
            assert_eq!(drainage.sidecars.len(), 1, "{path:?}");
            assert!(drainage.sidecars[0].carried, "{path:?}");
        }
    }

    /// Streaming formats have no by-name access; the create path's single
    /// walk must serve them identically.
    #[test]
    fn create_reads_a_tar_gz_selection() {
        let archive = targz_of(&[(
            "water.inp",
            super::super::test_fixtures::TEST_INP.as_bytes(),
        )]);
        let app_data = tempfile::tempdir().expect("app data");
        let outcomes = create_projects_from_archive_at(
            app_data.path(),
            archive.path(),
            vec![ArchiveSelection {
                path: "water.inp".into(),
                id: "44444444-4444-4444-4444-444444444444".into(),
                name: "Water".into(),
                engine: "wds".into(),
            }],
        )
        .expect("create");
        assert!(outcomes[0].project.is_some(), "{:?}", outcomes[0].error);
        assert!(app_data
            .path()
            .join("projects/44444444-4444-4444-4444-444444444444/base/model.inp")
            .exists());
    }

    /// The auxiliary files a model references travel with it: the create
    /// copies them into the project's `base/aux/`, where the run path
    /// reads them (§12.1).
    #[test]
    fn create_carries_referenced_sidecars_into_the_bundle() {
        let rain: &[u8] = b"sta1 2020 1 1 0 0 1.0\n";
        let archive = zip_of(&[
            ("nets/drainage.inp", UDS_INP.as_bytes()),
            ("forcing/rain.dat", rain),
            ("forcing/unrelated.dat", b"noise"),
        ]);
        let app_data = tempfile::tempdir().expect("app data");
        let outcomes = create_projects_from_archive_at(
            app_data.path(),
            archive.path(),
            vec![ArchiveSelection {
                path: "nets/drainage.inp".into(),
                id: "55555555-5555-5555-5555-555555555555".into(),
                name: "Drainage".into(),
                engine: "uds".into(),
            }],
        )
        .expect("create");
        assert!(outcomes[0].project.is_some(), "{:?}", outcomes[0].error);
        assert_eq!(outcomes[0].error, None);
        let aux = app_data
            .path()
            .join("projects/55555555-5555-5555-5555-555555555555/base/aux");
        assert_eq!(
            std::fs::read(aux.join("rain.dat")).expect("carried"),
            rain.to_vec(),
            "the referenced record travels with the project"
        );
        // Only referenced files travel; the archive's other data does not.
        assert!(!aux.join("unrelated.dat").exists());
    }

    /// A file that is no archive at all is refused by name, before any
    /// container library guesses at its bytes.
    #[test]
    fn an_unsupported_extension_is_refused_with_the_supported_list() {
        let err = scan_archive_file(std::path::Path::new("/tmp/models.rar")).unwrap_err();
        assert!(err.contains(".zip, .7z, .tar, and .tar.gz"), "{err}");
    }

    #[test]
    fn an_unrecognisable_model_entry_reports_instead_of_poisoning_the_scan() {
        let archive = zip_of(&[
            ("fine.inp", super::super::test_fixtures::TEST_INP.as_bytes()),
            ("noise.inp", b"this is not a model of anything"),
        ]);
        let scan = scan_archive_file(archive.path()).expect("scan");
        assert_eq!(scan.models.len(), 2);
        let noise = scan
            .models
            .iter()
            .find(|m| m.path == "noise.inp")
            .expect("noise entry");
        assert_eq!(noise.engine, None);
        assert!(noise.error.is_some());
        let fine = scan.models.iter().find(|m| m.path == "fine.inp").unwrap();
        assert_eq!(fine.engine.as_deref(), Some("wds"));
        assert_eq!(fine.error, None);
    }
}
