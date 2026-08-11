//! Archive import: many models in, one project each out.
//!
//! A `.zip` chosen by the user is scanned entry by entry — every entry whose
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
    /// External files the model references (rain, climate, interface).
    /// The engine runs entirely from supplied bytes, so a run will refuse
    /// until these are inlined — said here, before the user commits.
    pub sidecars: Vec<String>,
    /// Why this entry cannot be imported, when it cannot.
    pub error: Option<String>,
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
fn uds_sidecar_refs(network: &hydra::uds::model::Network) -> Vec<String> {
    use hydra::uds::model::{FileMode, GageSource, TemperatureSource};
    let mut out: Vec<String> = Vec::new();
    let mut push = |label: String| {
        if !out.contains(&label) {
            out.push(label);
        }
    };
    for gage in &network.gages {
        if let GageSource::File { file, .. } = &gage.source {
            push(format!("rain file \"{file}\""));
        }
    }
    if let Some(TemperatureSource::File { name, .. }) = &network.climate.temperature {
        push(format!("climate file \"{name}\""));
    }
    let iface = &network.interface_files;
    for (slot, label) in [
        (&iface.rainfall, "rainfall interface file"),
        (&iface.runoff, "runoff interface file"),
        (&iface.rdii, "RDII interface file"),
    ] {
        if let Some((FileMode::Use, name)) = slot {
            push(format!("{label} \"{name}\""));
        }
    }
    if let Some(name) = &iface.hotstart_use {
        push(format!("hotstart file \"{name}\""));
    }
    if let Some(name) = &iface.inflows {
        push(format!("routing inflows file \"{name}\""));
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
                entry.sidecars = uds_sidecar_refs(network);
            }
        }
        Err(e) => {
            entry.engine = Some(engine_key.to_string());
            entry.error = Some(e);
        }
    }
    entry
}

/// Read one entry's bytes, bounded: an entry whose *declared* size exceeds
/// the cap is refused before a byte is decompressed, and the read itself is
/// clamped so a lying header cannot overshoot either.
fn read_entry_bytes(
    file: &mut zip::read::ZipFile<'_, impl Read + std::io::Seek>,
    budget: &mut u64,
) -> Result<Vec<u8>, String> {
    let declared = file.size();
    if declared > MAX_ENTRY_BYTES {
        return Err(format!(
            "entry is {declared} bytes decompressed — larger than any model \
             this import accepts"
        ));
    }
    if declared > *budget {
        return Err("archive exceeds the import's total decompressed-size budget".into());
    }
    let mut bytes = Vec::with_capacity(declared.min(1024 * 1024) as usize);
    let mut clamped = file.take(declared + 1);
    clamped.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > declared {
        return Err("entry decompresses past its declared size".into());
    }
    *budget -= bytes.len() as u64;
    Ok(bytes)
}

/// Scan an archive on disk into the review manifest.
pub(crate) fn scan_archive_file(path: &std::path::Path) -> Result<ArchiveScan, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("not a readable zip archive: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!(
            "archive holds {} entries — more than the {MAX_ENTRIES} this import accepts",
            zip.len()
        ));
    }
    let extensions = model_extensions();
    let mut models = Vec::new();
    let mut others = Vec::new();
    let mut budget = MAX_TOTAL_BYTES;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let entry_path = entry.name().to_string();
        let file_name = entry_path.rsplit(['/', '\\']).next().unwrap_or("");
        // Archive helpers' bookkeeping (macOS resource forks and the like),
        // silently irrelevant rather than listed as leftovers.
        if file_name.starts_with("._") || file_name == ".DS_Store" {
            continue;
        }
        let (stem, ext) = match file_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), ext.to_ascii_lowercase()),
            _ => (file_name.to_string(), String::new()),
        };
        if !extensions.contains(&ext) {
            others.push(entry_path);
            continue;
        }
        match read_entry_bytes(&mut entry, &mut budget) {
            Ok(bytes) => models.push(describe_model_entry(entry_path, stem, bytes)),
            Err(e) => models.push(ArchiveModelEntry {
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
    Ok(ArchiveScan {
        archive_path: path.display().to_string(),
        models,
        others,
    })
}

/// Create one project per selection, against an `app_data` root — the
/// command body, testable without a Tauri handle.
pub(crate) fn create_projects_from_archive_at(
    app_data: &std::path::Path,
    archive_path: &std::path::Path,
    selections: Vec<ArchiveSelection>,
) -> Result<Vec<ArchiveImportOutcome>, String> {
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| format!("not a readable zip archive: {e}"))?;
    let mut outcomes = Vec::with_capacity(selections.len());
    for selection in selections {
        let outcome = create_one(app_data, &mut zip, &selection);
        outcomes.push(ArchiveImportOutcome {
            path: selection.path,
            name: selection.name,
            project: outcome.as_ref().ok().cloned(),
            error: outcome.err(),
        });
    }
    Ok(outcomes)
}

/// The whole life of one selection: read, parse, persist. Any failure is
/// this entry's alone.
fn create_one(
    app_data: &std::path::Path,
    zip: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>,
    selection: &ArchiveSelection,
) -> Result<Project, String> {
    validate_id(&selection.id)?;
    require_gui_openable_engine(&selection.engine)?;
    let mut entry = zip
        .by_name(&selection.path)
        .map_err(|_| format!("archive has no entry {:?}", selection.path))?;
    let mut budget = MAX_TOTAL_BYTES;
    let bytes = read_entry_bytes(&mut entry, &mut budget)?;
    let stem = entry
        .name()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .rsplit_once('.')
        .map_or_else(|| selection.name.clone(), |(stem, _)| stem.to_string());
    let (parsed, imported) = parse_model_bytes(&selection.engine, bytes, stem)?;
    persist_new_project(
        app_data,
        &selection.id,
        selection.name.clone(),
        selection.engine.clone(),
        &parsed.served_bytes(),
        imported.node_count,
        imported.link_count,
    )
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
            .add_filter("Model archive", &["zip"])
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
        let file = tempfile::NamedTempFile::new().expect("temp file");
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
        assert_eq!(drainage.sidecars, vec!["rain file \"rain.dat\""]);

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
