//! Project, scenario, CRS-catalog, and file-manager commands, plus the shared
//! bundle-path/id helpers and project DTO derivation from on-disk state.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::meta::{self, bundle};

use super::binary_codec::{encode_network_snapshot, encode_network_snapshot_absent};
use super::network_dto::{
    format_inp_parse_error, network_to_dto, NetworkDto, NetworkState, NetworkStateInner,
};
use super::simulation::try_acquire_run_target;

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
    /// Engine key from the `hydra::common` registry (`"wds"`, …). The
    /// frontend resolves it against `list_engines`; an unresolvable key is
    /// an explicit unsupported state, never a fallback.
    pub engine: String,
    pub scenario_count: u32,
    pub state: String,
    pub modified_label: String,
    /// Epoch seconds of the last modification (mtime of `base/model.inp`,
    /// falling back to the project directory mtime). Used for sorting.
    pub modified_at: i64,
    /// Epoch milliseconds of the last modification — derived from the same
    /// mtime as `modified_at` / `modified_label`. `None` only when the
    /// timestamp is not representable (negative epoch seconds).
    pub modified_at_ms: Option<u64>,
    /// Relative label for the last completed simulation, e.g. "2h ago".
    /// `None` when the project has never been simulated.
    pub last_run_label: Option<String>,
    /// Epoch milliseconds of the last completed simulation (mtime of
    /// `results.out`) — derived from the same timestamp as `last_run_label`.
    /// `None` when the project has never been simulated.
    pub last_run_at_ms: Option<u64>,
    pub node_count: u32,
    pub link_count: u32,
    /// EPSG code for the coordinate reference system of the INP \[COORDINATES\].
    pub source_crs: String,
    pub insights: Option<ProjectInsights>,
    /// `true` when the project's on-disk bundle directory is absent. Always
    /// `false` now that projects are discovered by scanning the filesystem;
    /// kept for wire-format compatibility. The frontend renders such rows
    /// muted and offers "Remove from list" instead of "Open folder".
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
        projects.push(project_dto_from_disk(&app_data, &path, &id, &meta));
    }
    sort_projects_most_recent_first(&mut projects);
    Ok(projects)
}

/// Resolve an engine key, refusing anything this build cannot actually run
/// (hydra-common spec §2.3).
///
/// The two failure modes are deliberately distinct in the message: an
/// unknown key means the project came from a newer Hydra, while a planned
/// key means the engine is registered but unimplemented. Collapsing them
/// would leave a user unable to tell "upgrade Hydra" from "wait for it".
fn require_available_engine(key: &str) -> Result<&'static hydra::common::EngineDescriptor, String> {
    let descriptor = hydra::common::engine_by_key(key)
        .map_err(|_| format!("unknown engine {key:?} — this build of Hydra does not have it"))?;
    if !descriptor.is_available() {
        return Err(format!(
            "{} modelling is not available yet in this build of Hydra",
            descriptor.label
        ));
    }
    Ok(descriptor)
}

/// The model a project starts from when the user imports nothing.
///
/// Hydra cannot represent a network with *no* elements: validation requires
/// at least one fixed-grade node, so a truly empty model fails to parse
/// (`NoReservoir`) and a project with no `model.inp` at all has no in-memory
/// network for the editor to mutate — which is why adding a junction to one
/// used to fail outright.
///
/// A single reservoir is therefore the smallest thing that is a network. It
/// is also the right one to hand someone: a distribution model needs a
/// source, so this is the element they would have had to draw first anyway.
/// Everything downstream — the editor, simulation settings, scenarios,
/// export, the topology digest — then works on a new project with no special
/// cases.
///
/// LPS/H-W because the GUI presents SI throughout; `Duration 0` starts the
/// project as a single-period steady-state run, the cheapest thing to solve
/// while a model is still being drawn. The project's own name is
/// deliberately NOT written as the `[TITLE]`: it is user input, and a name
/// beginning with `[` would open a section and inject content into the file.
const STARTER_INP: &[u8] = b"\
[RESERVOIRS]
;ID   Head
 R1   100

[COORDINATES]
;Node X    Y
 R1   0    0

[OPTIONS]
 Units      LPS
 Headloss   H-W

[TIMES]
 Duration   0

[END]
";

/// Node count of [`STARTER_INP`]. Asserted against the parsed model in
/// `starter_inp_is_a_valid_minimal_network`, so the two cannot drift.
const STARTER_NODE_COUNT: u32 = 1;

/// Persist a new project. Called from the frontend's "New Project" wizard.
///
/// The INP bytes currently held in managed state are copied into the bundle
/// as the project's canonical base model, so the bundle is self-contained on
/// disk even if the original source file is later moved or deleted. When
/// nothing was imported, [`STARTER_INP`] is written instead — a project
/// always has a model.
#[tauri::command(async)]
/// Create a new project directory with `meta.json` and `base/` subdirectories.
pub fn create_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    name: String,
    engine: String,
) -> Result<Project, String> {
    validate_id(&id)?;
    // The engine key is persisted into meta.json and never rewritten, so a
    // key that this build cannot run must be refused here rather than
    // producing a project that opens into a permanent unsupported state.
    require_available_engine(&engine)?;
    let app_data = app_data_dir(&app)?;

    // Snapshot the currently loaded network (if any). `up_to_date_raw_bytes`
    // re-serialises first when in-memory edits have not been flushed yet.
    let imported = {
        let mut guard = state.0.lock();
        let bytes = guard.up_to_date_raw_bytes().cloned();
        match (&*guard, bytes) {
            (NetworkStateInner::Loaded { dto, .. }, Some(bytes)) => {
                Some((bytes, dto.nodes.len() as u32, dto.links.len() as u32))
            }
            _ => None,
        }
    };
    // Counts must describe the bytes actually written, starter model included
    // — they are what `state` ("draft" vs "ready") and every has-a-network
    // check downstream are derived from.
    let (inp_bytes, node_count, link_count) =
        imported.unwrap_or_else(|| (STARTER_INP.to_vec(), STARTER_NODE_COUNT, 0));

    let project_dir = bundle::project_dir(&app_data, &id);
    let base_dir = bundle::base_dir(&app_data, &id);
    let scenarios_dir = project_dir.join("scenarios");
    std::fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&scenarios_dir).map_err(|e| e.to_string())?;

    let meta = meta::ProjectMeta {
        version: 1,
        name,
        engine,
        source_crs: "EPSG:4326".into(),
        node_count,
        link_count,
    };
    meta::write_project_meta(&project_dir, &meta)?;

    bundle::atomic_write(&bundle::base_model_path(&app_data, &id), &inp_bytes)
        .map_err(|e| e.to_string())?;

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
    Ok(Some(project_dto_from_disk(
        &app_data,
        &project_dir,
        &id,
        &project_meta,
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
    /// CRS definition string. Despite the name, this usually holds a **WKT**
    /// definition rather than a proj4 string: the curated catalog
    /// (`resources/crs-catalog.json`) ships WKT, and proj4js on the frontend
    /// accepts both formats interchangeably. Kept as `proj4` for wire-format
    /// compatibility — do not rename.
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
    /// CRS definition string. Despite the name, curated entries carry **WKT**
    /// from `resources/crs-catalog.json` (proj4js accepts WKT as well as
    /// proj4 strings). Kept as `proj4` for wire-format compatibility — do not
    /// rename.
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
        let raw = include_str!("../../resources/crs-catalog.json");
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

pub(crate) fn app_data_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Reject any string that is not a valid UUID v4, preventing path traversal via
/// `project_id` / `scenario_id` parameters supplied by the frontend.
pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| format!("invalid id: expected UUID, got {:?}", id))
}

/// Validate the `(project_id, optional scenario_id)` pair that every
/// project/scenario-target command receives — both must be UUIDs.
pub(crate) fn validate_target_ids(
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<(), String> {
    validate_id(project_id)?;
    if let Some(sid) = scenario_id {
        validate_id(sid)?;
    }
    Ok(())
}

/// The project's analysis criteria, or `null` when it has none saved.
///
/// Never fails: a project that has never had criteria edited simply has no
/// file, which is the normal state, and a corrupt one reads as absent.
#[tauri::command]
pub fn get_project_criteria(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Option<meta::ProjectCriteria>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    Ok(meta::read_project_criteria(&bundle::project_dir(
        &app_data,
        &project_id,
    )))
}

/// Persist the project's analysis criteria.
#[tauri::command]
pub fn update_project_criteria(
    app: tauri::AppHandle,
    project_id: String,
    criteria: meta::ProjectCriteria,
) -> Result<(), String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    meta::write_project_criteria(&bundle::project_dir(&app_data, &project_id), &criteria)
}

/// `results.out` path for a project's base model (`scenario_id == None`) or
/// one of its scenarios.
pub(crate) fn results_path_for(
    app_data: &std::path::Path,
    project_id: &str,
    scenario_id: Option<&str>,
) -> std::path::PathBuf {
    match scenario_id {
        Some(sid) => bundle::scenario_results_path(app_data, project_id, sid),
        None => bundle::base_results_path(app_data, project_id),
    }
}

/// Delete a target's simulation results, returning it to its unsimulated
/// state. Returns `true` when a results file was removed, `false` when the
/// target had none (already unsimulated — not an error, so repeating the
/// action is harmless).
///
/// `results.out` is the *only* artifact a run produces, and every derived
/// notion of "simulated" is read back from it: the project and scenario sim
/// state, the last-run timestamp, result metadata, and the analytics cache
/// (keyed on the file's path, length and mtime, so its entries can never
/// match a later file). Removing it is therefore the whole operation — there
/// is no second place where a stale "simulated" flag could survive.
///
/// The run lock is taken first. Deleting the file out from under a running
/// simulation would leave the queue writing to an unlinked inode and report
/// success for results nobody can read.
#[tauri::command(async)]
/// Delete a project or scenario's `results.out`, returning it to "not-run".
pub fn delete_simulation(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<bool, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let _run_guard = try_acquire_run_target(&project_id, scenario_id.as_deref())?;
    let path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    remove_results_file(&path)
}

/// Remove one run's artifacts, treating "already absent" as success.
///
/// Takes `warnings.json` with the results it describes. The warnings writer
/// maintains the invariant that "warnings can never exist without results";
/// deleting only `results.out` would leave the previous run's warnings being
/// served for a target that now reports as unsimulated.
fn remove_results_file(path: &std::path::Path) -> Result<bool, String> {
    let removed = match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(format!("Cannot delete simulation results: {e}")),
    };
    // Best-effort, like every other write of this file: an orphaned warnings
    // file is a diagnostic annoyance, not a reason to fail the clear.
    let _ = std::fs::remove_file(super::simulation::run_warnings_path(path));
    Ok(removed)
}

/// Bytes on disk for one target's run artifacts — `results.out` plus the
/// `warnings.json` beside it. Zero when the target has never been simulated.
///
/// Counts exactly what a clear would remove, so the figure shown before
/// confirming is the space actually reclaimed.
fn results_bytes(path: &std::path::Path) -> u64 {
    let file_len = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    file_len(path) + file_len(&super::simulation::run_warnings_path(path))
}

/// Scenario ids of `project_id`, discovered from the bundle directory.
///
/// Read straight from disk rather than taken from the caller: this backs a
/// clear-everything action, and a list supplied by a frontend whose scenario
/// cache was stale would silently leave results behind on whichever scenario
/// it had not heard about.
fn scenario_ids(app_data: &std::path::Path, project_id: &str) -> Result<Vec<String>, String> {
    let scenarios_dir = bundle::project_dir(app_data, project_id).join("scenarios");
    if !scenarios_dir.exists() {
        return Ok(vec![]);
    }
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&scenarios_dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !path.is_dir() {
            continue;
        }
        // A directory without readable metadata is not a scenario the rest of
        // the app will show, so `list_scenarios` skips it — skip it here too,
        // rather than reaching into something we cannot identify.
        if meta::read_scenario_meta(&path).is_err() {
            continue;
        }
        if let Some(id) = path.file_name().and_then(|n| n.to_str()) {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// Delete the simulation results of a project's base model **and** every one
/// of its scenarios. Returns how many results files were removed.
///
/// All run locks are taken up front, before a single file is touched, so the
/// operation is all-or-nothing: if any target is mid-run the call fails
/// having changed nothing. Clearing target-by-target and stopping at the busy
/// one would leave the project in a state the user did not ask for and cannot
/// easily identify — some results gone, some not, and no record of which.
#[tauri::command(async)]
/// Delete every `results.out` in a project (base model and all scenarios).
pub fn delete_all_simulations(app: tauri::AppHandle, project_id: String) -> Result<u32, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;

    let mut targets: Vec<Option<String>> = vec![None];
    targets.extend(scenario_ids(&app_data, &project_id)?.into_iter().map(Some));

    // Held for the whole function: dropping a guard early would let a run
    // start on a target already cleared, so its fresh results would be
    // deleted by nothing but would exist inside an operation reporting that
    // everything was cleared.
    let _guards = targets
        .iter()
        .map(|sid| try_acquire_run_target(&project_id, sid.as_deref()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut removed = 0u32;
    for sid in &targets {
        let path = results_path_for(&app_data, &project_id, sid.as_deref());
        if remove_results_file(&path)? {
            removed += 1;
        }
    }
    Ok(removed)
}
/// Every target's run-artifact size for one project, in bytes.
///
/// Batched deliberately: the scenarios panel labels one clear action per row,
/// and a per-row command would cost an IPC round trip each. The work itself is
/// two `stat` calls per target — metadata only, never opening the file — so a
/// 650 MB result costs the same as an empty one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResultsSizes {
    /// Bytes held by the base model's run.
    pub base: u64,
    /// Scenario id → bytes held by that scenario's run.
    pub scenarios: std::collections::HashMap<String, u64>,
    /// Base plus every scenario — what a project-wide clear reclaims.
    pub total: u64,
}

#[tauri::command(async)]
pub fn project_results_sizes(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<ProjectResultsSizes, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let base = results_bytes(&results_path_for(&app_data, &project_id, None));
    let mut scenarios = std::collections::HashMap::new();
    for sid in scenario_ids(&app_data, &project_id)? {
        let bytes = results_bytes(&results_path_for(&app_data, &project_id, Some(&sid)));
        scenarios.insert(sid, bytes);
    }
    let total = base + scenarios.values().sum::<u64>();
    Ok(ProjectResultsSizes {
        base,
        scenarios,
        total,
    })
}

/// Bytes a clear across `project_ids` would reclaim.
///
/// Takes the whole selection so a bulk action costs one round trip rather
/// than one per project — selections are unbounded, round trips are not free,
/// and the `stat` work behind them is.
///
/// Unknown or unreadable ids contribute zero rather than failing: a stale id
/// in a selection should not deny the user a figure for the rest.
#[tauri::command(async)]
pub fn projects_results_size(
    app: tauri::AppHandle,
    project_ids: Vec<String>,
) -> Result<u64, String> {
    let app_data = app_data_dir(&app)?;
    let mut total = 0u64;
    for project_id in project_ids {
        if validate_id(&project_id).is_err() {
            continue;
        }
        total += results_bytes(&results_path_for(&app_data, &project_id, None));
        for sid in scenario_ids(&app_data, &project_id).unwrap_or_default() {
            total += results_bytes(&results_path_for(&app_data, &project_id, Some(&sid)));
        }
    }
    Ok(total)
}

/// `model.inp` path for a project's base model (`scenario_id == None`) or
/// one of its scenarios.
pub(crate) fn model_path_for(
    app_data: &std::path::Path,
    project_id: &str,
    scenario_id: Option<&str>,
) -> std::path::PathBuf {
    match scenario_id {
        Some(sid) => bundle::scenario_model_path(app_data, project_id, sid),
        None => bundle::base_model_path(app_data, project_id),
    }
}

/// Read a target's model bytes, distinguishing "there is no model yet" from
/// a genuine read failure.
///
/// A project created without importing a source model has no `model.inp` at
/// all — that is its normal resting state, not a fault, and the "start with
/// an empty network" path exists precisely to produce it. Commands that
/// merely *describe* a model (validation, topology digest) must answer
/// "nothing to describe" for one, or every blank project greets the user
/// with backend-error toasts the moment it opens.
///
/// Only `NotFound` is folded into `Ok(None)`. A permission error or a bad
/// symlink still fails loudly: those mean the model exists and is
/// unreachable, which is exactly the situation a user needs told about.
pub(crate) fn read_model_bytes(path: &std::path::Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot read model: {e}")),
    }
}

/// Count the scenario subdirectories under `<app_data>/projects/<id>/scenarios/`
/// that hold a readable `meta.json` — the same criterion `list_scenarios`
/// applies, so project-card counts always match the scenario list.
fn count_scenario_dirs(app_data: &std::path::Path, project_id: &str) -> u32 {
    let scenarios_dir = bundle::project_dir(app_data, project_id).join("scenarios");
    if !scenarios_dir.exists() {
        return 0;
    }
    std::fs::read_dir(&scenarios_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    path.is_dir() && meta::read_scenario_meta(&path).is_ok()
                })
                .count() as u32
        })
        .unwrap_or(0)
}

/// Return a list of scenario IDs (directory names) under `<app_data>/projects/<id>/scenarios/`.
pub(crate) fn list_scenario_ids(app_data: &std::path::Path, project_id: &str) -> Vec<String> {
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
    sort_scenarios_by_name(&mut result);
    Ok(result)
}

/// Order scenarios by name, case-insensitively, with the (unique) id as a
/// deterministic tie-breaker for equal names.
fn sort_scenarios_by_name(scenarios: &mut [ScenarioDto]) {
    scenarios.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Create a new scenario under `project_id`. If `parent_scenario_id` is
/// `Some`, the parent's model.inp is copied into the new scenario directory
/// as a starting point; otherwise the base model is used. Returns the new
/// `ScenarioDto`.
#[tauri::command(async)]
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

    let src = scenario_source_model(&app_data, &project_id, parent_scenario_id.as_deref())?;

    let id = uuid::Uuid::new_v4().to_string();
    let sc_dir = bundle::scenario_dir(&app_data, &project_id, &id);
    std::fs::create_dir_all(&sc_dir).map_err(|e| e.to_string())?;

    let sc_meta = meta::ScenarioMeta {
        name,
        parent_scenario_id: parent_scenario_id.clone(),
    };
    meta::write_scenario_meta(&sc_dir, &sc_meta)?;

    let dest = bundle::scenario_model_path(&app_data, &project_id, &id);
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;

    Ok(scenario_meta_to_dto(&id, &project_id, &sc_meta, "not-run"))
}

/// Resolve the model a new scenario branches from — the parent scenario's,
/// or the base model — failing when there is none.
///
/// A scenario is a variant of a model, so there has to be a model to vary.
/// Branching a project with none used to succeed and silently skip the copy,
/// producing a scenario with no model of its own: indistinguishable on disk
/// from the empty parent it came from, and equally unrunnable. Refusing is
/// the honest answer, and it keeps an empty project from quietly growing a
/// tree of empty children.
fn scenario_source_model(
    app_data: &std::path::Path,
    project_id: &str,
    parent_scenario_id: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let src = match parent_scenario_id {
        Some(pid) => bundle::scenario_model_path(app_data, project_id, pid),
        None => bundle::base_model_path(app_data, project_id),
    };
    if !src.exists() {
        return Err(match parent_scenario_id {
            Some(_) => "That scenario has no network to branch from".to_string(),
            None => {
                "This project has no network yet — import or build one before creating scenarios"
                    .to_string()
            }
        });
    }
    Ok(src)
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

/// Scenarios descended from `scenario_id`, at any depth, read from the bundle.
///
/// Enumerated from disk rather than accepted from the caller for the same
/// reason `delete_all_simulations` reads its own target list: a frontend list
/// that had gone stale would leave behind exactly the scenarios the
/// confirmation promised to remove.
///
/// Walks with a visited set. Parent links live in each scenario's own
/// `meta.json` and nothing enforces acyclicity across them, so a cycle is
/// representable on disk and a naive walk would not terminate.
fn scenario_descendants(
    app_data: &std::path::Path,
    project_id: &str,
    scenario_id: &str,
) -> Result<Vec<String>, String> {
    let mut children: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for id in scenario_ids(app_data, project_id)? {
        let dir = bundle::scenario_dir(app_data, project_id, &id);
        if let Ok(meta) = meta::read_scenario_meta(&dir) {
            if let Some(parent) = meta.parent_scenario_id {
                children.entry(parent).or_default().push(id);
            }
        }
    }
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::from([scenario_id.to_string()]);
    let mut queue = vec![scenario_id.to_string()];
    while let Some(id) = queue.pop() {
        for child in children.get(&id).cloned().unwrap_or_default() {
            if seen.insert(child.clone()) {
                found.push(child.clone());
                queue.push(child);
            }
        }
    }
    Ok(found)
}

/// Permanently delete a scenario and its on-disk bundle.
///
/// With `cascade`, every scenario descended from it goes too. Without it they
/// survive: each is a complete copy of its parent's model rather than a delta,
/// so the link is lineage only — which is why cascading is opt-in and the
/// caller defaults it off.
///
/// Descendants are removed before their parent, so an interrupted cascade
/// leaves the parent standing with the remainder of its subtree still
/// attached, rather than a set of orphans promoted to roots.
///
/// Returns how many scenarios were removed; `0` when the id was not found.
#[tauri::command]
/// Remove the scenario directory tree, optionally with its descendants.
pub fn delete_scenario(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: String,
    cascade: bool,
) -> Result<u32, String> {
    validate_id(&project_id)?;
    validate_id(&scenario_id)?;
    let app_data = app_data_dir(&app)?;
    if !bundle::scenario_dir(&app_data, &project_id, &scenario_id).exists() {
        return Ok(0);
    }
    let mut removed = 0u32;
    if cascade {
        for id in scenario_descendants(&app_data, &project_id, &scenario_id)? {
            bundle::delete_scenario_dir(&app_data, &project_id, &id).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    bundle::delete_scenario_dir(&app_data, &project_id, &scenario_id).map_err(|e| e.to_string())?;
    Ok(removed + 1)
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

/// Build a [`Project`] DTO from a project's on-disk bundle state: scenario
/// count, sim state + last-run time derived from `base/results.out`, and
/// `modified_at` from `base/model.inp` (falling back to the project directory
/// mtime, then to "now"). Shared by `list_projects` / `rename_project` so
/// both always derive identical rows.
fn project_dto_from_disk(
    app_data: &std::path::Path,
    project_dir: &std::path::Path,
    id: &str,
    meta: &meta::ProjectMeta,
) -> Project {
    let scenario_count = count_scenario_dirs(app_data, id);
    let results_path = bundle::base_results_path(app_data, id);
    let sim_state = meta::sim_state_from_results(&results_path);
    let last_run_at = if results_path.exists() {
        meta::mtime_secs(&results_path)
    } else {
        None
    };
    let modified_at = meta::mtime_secs(&bundle::base_model_path(app_data, id))
        .or_else(|| meta::mtime_secs(project_dir))
        .unwrap_or_else(meta::now_secs);
    project_to_dto(
        id,
        meta,
        scenario_count,
        last_run_at,
        sim_state,
        false,
        modified_at,
    )
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
        modified_at,
        modified_at_ms: epoch_secs_to_ms(modified_at),
        last_run_label,
        last_run_at_ms: last_run_at.and_then(epoch_secs_to_ms),
        id: id.to_string(),
        name: meta.name.clone(),
        engine: meta.engine.clone(),
        scenario_count,
        state: state.into(),
        node_count: meta.node_count,
        link_count: meta.link_count,
        source_crs: meta.source_crs.clone(),
        insights: None,
        folder_missing,
    }
}

/// Sort projects most-recently-modified first, by the epoch `modified_at`
/// (never by the human-readable label, which does not sort chronologically).
fn sort_projects_most_recent_first(projects: &mut [Project]) {
    projects.sort_by_key(|p| std::cmp::Reverse(p.modified_at));
}

/// Epoch seconds → epoch milliseconds; `None` for negative (pre-1970) values,
/// which cannot be represented in the frontend's unsigned-ms contract.
fn epoch_secs_to_ms(secs: i64) -> Option<u64> {
    u64::try_from(secs).ok()?.checked_mul(1000)
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

/// Open a native file-open dialog filtered to `engine`'s source-model
/// formats, parse the chosen file with that engine, store the result in
/// managed state, and return the `NetworkDto` to the caller.
///
/// Returns `null` to the frontend when the dialog is cancelled.
///
/// The picker filter comes from the engine descriptor rather than being
/// hardcoded, but the filter is only a filter: `wds` and `uds` both claim
/// `.inp` (hydra-common spec §2.2), so the parse below is what actually
/// decides whether the file is the right kind of model.
#[tauri::command]
/// Open a native file-picker, parse the chosen model file, and hold it in `NetworkState`.
pub async fn open_and_load_network(
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
    engine: String,
) -> Result<Option<NetworkDto>, String> {
    use tauri_plugin_dialog::DialogExt;

    let descriptor = require_available_engine(&engine)?;

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = dialog_app.dialog().file();
        for format in descriptor.import {
            dialog = dialog.add_filter(format.label, format.extensions);
        }
        dialog.blocking_pick_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;

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
    // `require_available_engine` has already narrowed this to the one engine
    // with an implementation. Matching on the key rather than calling the wds
    // parser unconditionally makes the next engine a compile-time decision
    // instead of a silently wrong parse.
    let network = match descriptor.key {
        "wds" => hydra::io::parse(&bytes).map_err(format_inp_parse_error)?,
        other => return Err(format!("no importer for engine {other:?}")),
    };

    let mut dto = network_to_dto(&network);
    dto.file_stem = file_stem;

    *state.0.lock() = NetworkStateInner::Loaded {
        raw_bytes: bytes,
        dirty: false,
        network: std::sync::Arc::new(network),
        dto: dto.clone(),
        owner_project_id: None,
        owner_scenario_id: None,
    };
    Ok(Some(dto))
}

/// Persist the currently loaded network (`NetworkState`) back into the named
/// project as `base/model.inp`.
///
/// Returns `true` when the file was written, `false` when no network is loaded
/// in managed state (i.e. the project is a draft with no INP attached yet).
///
/// When `scenario_id` is `Some`, writes to the scenario's INP file instead of
/// the base model file (and skips the base-model node/link count update).
/// Reject a `save_project` call whose target does not own the network
/// currently held in `NetworkState`.
///
/// BOTH halves of the target are checked. The project half stops a stale
/// `activeProjectId` from overwriting another project's `model.inp`; the
/// scenario half stops the much easier mistake of writing the base model's
/// bytes into a scenario (or one scenario's into a sibling's) during the
/// window between the frontend switching `activeScenarioId` and the new
/// target's INP actually being loaded. A project-only check passes that case
/// happily, because the project does match.
///
/// `owner_project_id` is `None` only for networks loaded from the file picker
/// (no owning project yet), which are allowed through to preserve the
/// draft/`create_project` flow.
fn check_save_target(
    owner_project_id: Option<&str>,
    owner_scenario_id: Option<&str>,
    id: &str,
    scenario_id: Option<&str>,
) -> Result<(), String> {
    let Some(owner) = owner_project_id else {
        return Ok(());
    };
    if owner != id {
        return Err(format!(
            "save_project refused: the loaded network belongs to project {owner}, not {id}; \
             reload the project before saving"
        ));
    }
    if owner_scenario_id != scenario_id {
        let name = |s: Option<&str>| s.map_or_else(|| "the base model".to_string(), str::to_string);
        return Err(format!(
            "save_project refused: the loaded network belongs to {}, not {}; \
             wait for the target to finish loading before saving",
            name(owner_scenario_id),
            name(scenario_id)
        ));
    }
    Ok(())
}

#[tauri::command(async)]
/// Flush in-memory patches to `base/model.inp`; update node/link counts in `meta.json`.
pub fn save_project(
    id: String,
    scenario_id: Option<String>,
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    validate_target_ids(&id, scenario_id.as_deref())?;
    let (raw, node_count, link_count) = {
        let mut guard = state.0.lock();
        match &*guard {
            NetworkStateInner::Loaded {
                owner_project_id,
                owner_scenario_id,
                ..
            } => check_save_target(
                owner_project_id.as_deref(),
                owner_scenario_id.as_deref(),
                &id,
                scenario_id.as_deref(),
            )?,
            NetworkStateInner::Empty => return Ok(false),
        }
        // Serialise pending in-memory edits (dirty flag) exactly once, here at
        // the save point, instead of on every mutation.
        let raw = match guard.up_to_date_raw_bytes() {
            Some(bytes) => bytes.clone(),
            None => return Ok(false),
        };
        match &*guard {
            NetworkStateInner::Loaded { dto, .. } => {
                (raw, dto.nodes.len() as u32, dto.links.len() as u32)
            }
            NetworkStateInner::Empty => return Ok(false),
        }
    };
    let app_data = app_data_dir(&app)?;
    bundle::atomic_write(
        &model_path_for(&app_data, &id, scenario_id.as_deref()),
        &raw,
    )
    .map_err(|e| e.to_string())?;
    if scenario_id.is_none() {
        // Update cached node/link counts in meta.json (base model only).
        let project_dir = bundle::project_dir(&app_data, &id);
        if let Ok(mut project_meta) = meta::read_project_meta(&project_dir) {
            project_meta.node_count = node_count;
            project_meta.link_count = link_count;
            let _ = meta::write_project_meta(&project_dir, &project_meta);
        }
    }
    Ok(true)
}

/// Load the INP for a project's base model or a named scenario into
/// `NetworkState`, making it available to the read-only `get_*` commands.
///
/// Returns a compact binary nodes+links snapshot when loaded (see
/// [`encode_network_snapshot`] for the byte layout). When the target INP does
/// not exist on disk yet, the payload is a header with the "present" flag
/// clear, which the frontend decodes as `null`.
#[tauri::command(async)]
/// Parse the project bundle's INP and load it into `NetworkState`.
pub fn load_project_network(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let path = model_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !path.exists() {
        *state.0.lock() = NetworkStateInner::Empty;
        return Ok(tauri::ipc::Response::new(encode_network_snapshot_absent()));
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let network = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;
    let dto = network_to_dto(&network);
    // Encode before taking the state lock — serialisation work happens
    // outside the mutex, and (unlike the old JSON path) no nodes/links clone
    // is needed to build the response.
    let encoded = encode_network_snapshot(&dto);
    *state.0.lock() = NetworkStateInner::Loaded {
        raw_bytes: bytes,
        dirty: false,
        network: std::sync::Arc::new(network),
        dto,
        owner_project_id: Some(project_id.clone()),
        owner_scenario_id: scenario_id.clone(),
    };
    Ok(tauri::ipc::Response::new(encoded))
}

/// Export the current INP for a project's base model or a scenario via a
/// native save dialog (default filename `<project-name>.inp`).
///
/// When `NetworkState` holds exactly this target, the exported bytes come
/// from the in-memory network — `up_to_date_raw_bytes` re-serialises first
/// when unsaved edits are pending (`dirty`), the same dirtiness handling
/// `save_project` uses — so the export always reflects the current editor
/// state. Otherwise the on-disk `model.inp` is exported as-is.
///
/// Returns `Ok(Some(path))` with the written file's path, or `Ok(None)` when
/// the user cancels the dialog.
#[tauri::command]
/// Save the target's INP to a user-chosen path via a native save dialog.
pub async fn export_project_inp(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;

    // Resolve the INP bytes up front so a missing model errors before any
    // dialog is shown. Cache path: only when the loaded network is exactly
    // this (project, scenario) target.
    let cached: Option<Vec<u8>> = {
        let mut guard = state.0.lock();
        let matches_target = matches!(
            &*guard,
            NetworkStateInner::Loaded {
                owner_project_id: Some(owner),
                owner_scenario_id,
                ..
            } if *owner == project_id && owner_scenario_id.as_deref() == scenario_id.as_deref()
        );
        if matches_target {
            guard.up_to_date_raw_bytes().cloned()
        } else {
            None
        }
    };
    let bytes = match cached {
        Some(b) => b,
        None => {
            let path = model_path_for(&app_data, &project_id, scenario_id.as_deref());
            // Unlike the describe-only commands, exporting genuinely needs a
            // model — but say so in the user's terms rather than handing them
            // a raw errno for a project they simply have not built yet.
            read_model_bytes(&path)?
                .ok_or("This project has no network yet — import or build one before exporting")?
        }
    };

    let default_name = meta::read_project_meta(&bundle::project_dir(&app_data, &project_id))
        .map(|m| m.name)
        .unwrap_or_else(|_| "model".to_string());

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("EPANET Input File", &["inp"])
            .set_file_name(format!("{default_name}.inp"))
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;

    let file_path = match picked {
        Some(p) => p,
        None => return Ok(None), // user cancelled
    };
    let path_buf = file_path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path_buf, &bytes).map_err(|e| format!("Cannot write INP: {e}"))?;
    Ok(Some(path_buf.to_string_lossy().into_owned()))
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
/// The engine registry: every engine compiled into this build, in
/// presentation order (hydra-common spec §2.2). The frontend derives all
/// engine-identity presentation (label, pill, accent) from this instead of
/// hardcoding it.
pub fn list_engines() -> &'static [hydra::common::EngineDescriptor] {
    hydra::common::ENGINES
}

#[tauri::command]
/// Return the hydra engine and application version strings.
pub fn get_versions() -> Versions {
    Versions {
        hydra: HYDRA_VERSION,
        app: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
/// Whether this install can self-update via the updater plugin.
///
/// False in dev builds (there is no installed bundle to replace) and on
/// Linux when not running from an AppImage — the updater plugin supports
/// only AppImage there, so deb/rpm installs must hide all updater UI and
/// keep updating through their package manager.
pub fn updater_supported() -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    if cfg!(target_os = "linux") {
        return std::env::var_os("APPIMAGE").is_some();
    }
    true
}

#[cfg(test)]
mod tests {

    #[test]
    fn meta_with_removed_description_field_still_parses() {
        // Older meta.json files carried a never-displayed `description`
        // field and an `analysisOptions` field that was never once populated
        // — always null, never read. Both are gone from the struct; serde
        // ignores unknown fields, so those files must keep loading.
        let json = r#"{"name":"Legacy","description":"old text","sourceCrs":"EPSG:4326","nodeCount":1,"linkCount":2,"analysisOptions":null}"#;
        let m: meta::ProjectMeta = serde_json::from_str(json).unwrap();
        assert_eq!(m.name, "Legacy");
        assert_eq!(m.node_count, 1);
        let scenario = r#"{"name":"S1","description":"old","parentScenarioId":null}"#;
        let sm: meta::ScenarioMeta = serde_json::from_str(scenario).unwrap();
        assert_eq!(sm.name, "S1");
    }
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

    // ── scenario cascade ─────────────────────────────────────────────────

    /// Write a scenario directory with the given parent link.
    fn put_scenario(app_data: &std::path::Path, project: &str, id: &str, parent: Option<&str>) {
        let dir = bundle::scenario_dir(app_data, project, id);
        std::fs::create_dir_all(&dir).unwrap();
        meta::write_scenario_meta(
            &dir,
            &meta::ScenarioMeta {
                name: id.to_string(),
                parent_scenario_id: parent.map(str::to_string),
            },
        )
        .unwrap();
    }

    #[test]
    fn descendants_reach_every_depth_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        // a → b → c, a → d, plus an unrelated root.
        put_scenario(dir.path(), "p1", "a", None);
        put_scenario(dir.path(), "p1", "b", Some("a"));
        put_scenario(dir.path(), "p1", "c", Some("b"));
        put_scenario(dir.path(), "p1", "d", Some("a"));
        put_scenario(dir.path(), "p1", "z", None);

        let mut found = scenario_descendants(dir.path(), "p1", "a").unwrap();
        found.sort();
        assert_eq!(found, ["b", "c", "d"]);
        assert!(scenario_descendants(dir.path(), "p1", "c")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn descendants_terminate_on_a_parent_cycle() {
        // Parent links live in each scenario's own meta.json and nothing
        // enforces acyclicity across them, so a cycle is representable on
        // disk. A naive walk would hang the command.
        let dir = tempfile::tempdir().unwrap();
        put_scenario(dir.path(), "p1", "a", Some("b"));
        put_scenario(dir.path(), "p1", "b", Some("a"));
        put_scenario(dir.path(), "p1", "c", Some("b"));

        let mut found = scenario_descendants(dir.path(), "p1", "a").unwrap();
        found.sort();
        assert_eq!(found, ["b", "c"]);
    }

    // ── project criteria ─────────────────────────────────────────────────

    #[test]
    fn criteria_default_when_the_file_is_absent_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let project = bundle::project_dir(dir.path(), "p1");
        std::fs::create_dir_all(&project).unwrap();

        // Never edited — the normal state, reported as absent so callers can
        // tell it apart from a saved file holding the same values.
        assert!(meta::read_project_criteria(&project).is_none());

        // A corrupt file reads as absent rather than taking the project's
        // analysis view down with it.
        bundle::atomic_write(&project.join("criteria.json"), b"{ not json").unwrap();
        assert!(meta::read_project_criteria(&project).is_none());

        // Defaults are still what an absent file means to a caller.
        let c = meta::ProjectCriteria::default();
        assert_eq!(c.min_pressure_m, 14.0);
        assert_eq!(c.velocity.target, 0.5);
    }

    #[test]
    fn criteria_round_trip_and_fill_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let project = bundle::project_dir(dir.path(), "p1");
        std::fs::create_dir_all(&project).unwrap();

        let c = meta::ProjectCriteria {
            min_pressure_m: 30.0,
            velocity: meta::TargetBand {
                high: 2.0,
                ..meta::ProjectCriteria::default().velocity
            },
            ..meta::ProjectCriteria::default()
        };
        meta::write_project_criteria(&project, &c).unwrap();

        let back = meta::read_project_criteria(&project).expect("just written");
        assert_eq!(back.min_pressure_m, 30.0);
        assert_eq!(back.velocity.high, 2.0);
        assert_eq!(back.pressure.required, 35.0);

        // A file written by an older build carries fewer fields; each one
        // missing falls back on its own rather than discarding the file.
        bundle::atomic_write(
            &project.join("criteria.json"),
            br#"{"version":1,"minPressureM":21.5}"#,
        )
        .unwrap();
        let partial = meta::read_project_criteria(&project).expect("partial file parses");
        assert_eq!(partial.min_pressure_m, 21.5);
        assert_eq!(partial.flow.target, 1.0);
    }

    #[test]
    fn criteria_live_beside_the_manifest_not_inside_it() {
        // The manifest gates whether a project is listed at all, so criteria
        // must not share its file: a bad criteria write cannot be allowed to
        // hide the project.
        let dir = tempfile::tempdir().unwrap();
        let project = bundle::project_dir(dir.path(), "p1");
        std::fs::create_dir_all(&project).unwrap();
        meta::write_project_criteria(&project, &meta::ProjectCriteria::default()).unwrap();
        assert!(project.join("criteria.json").exists());
        assert!(!project.join("meta.json").exists());
    }

    // ── engine gating ────────────────────────────────────────────────────

    #[test]
    fn only_an_implemented_engine_may_back_a_new_project() {
        assert_eq!(require_available_engine("wds").unwrap().key, "wds");

        // Planned: registered and presentable, but nothing can run it. The
        // wizard disables these cards; this is the backstop for a caller
        // that ignores the card state.
        for planned in ["uds", "och"] {
            let err = require_available_engine(planned).unwrap_err();
            assert!(
                err.contains("not available yet"),
                "{planned} rejection should say it is unimplemented, got: {err}"
            );
        }

        // Unknown: a different failure with a different remedy (upgrade).
        let err = require_available_engine("zzz").unwrap_err();
        assert!(err.contains("unknown engine"), "got: {err}");
        assert!(!err.contains("not available yet"), "got: {err}");
    }

    // ── deleting simulation results ──────────────────────────────────────

    #[test]
    fn sim_state_follows_the_results_file_both_ways() {
        // `delete_simulation` removes exactly one file, which works only
        // because every notion of "simulated" is read back from that file's
        // existence. This pins that relationship: if a second source of
        // truth for sim state ever appears, deleting results.out would stop
        // being enough and this test is where it should be noticed.
        let dir = tempfile::tempdir().unwrap();
        let path = results_path_for(dir.path(), "p1", None);
        assert_eq!(meta::sim_state_from_results(&path), "not-run");

        bundle::atomic_write(&path, b"fake results").unwrap();
        assert_eq!(meta::sim_state_from_results(&path), "done");

        std::fs::remove_file(&path).unwrap();
        assert_eq!(meta::sim_state_from_results(&path), "not-run");
    }

    #[test]
    fn deleting_results_is_idempotent_and_scenario_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let base = results_path_for(dir.path(), "p1", None);
        let scenario = results_path_for(dir.path(), "p1", Some("s1"));
        bundle::atomic_write(&base, b"base results").unwrap();
        bundle::atomic_write(&scenario, b"scenario results").unwrap();

        // Clearing one target must not touch its siblings — the command
        // resolves a single path, and these are the paths it resolves.
        assert_ne!(base, scenario);
        std::fs::remove_file(&scenario).unwrap();
        assert!(base.exists(), "base results must survive");
        assert_eq!(meta::sim_state_from_results(&scenario), "not-run");

        // Second delete finds nothing; the command reports `false` rather
        // than failing, so repeating the action is harmless.
        assert_eq!(
            std::fs::remove_file(&scenario).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn scenario_ids_reads_the_bundle_not_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scenario_ids(dir.path(), "p1").unwrap().is_empty());

        for (sid, name) in [("s1", "Alpha"), ("s2", "Beta")] {
            let sc_dir = bundle::scenario_dir(dir.path(), "p1", sid);
            std::fs::create_dir_all(&sc_dir).unwrap();
            meta::write_scenario_meta(
                &sc_dir,
                &meta::ScenarioMeta {
                    name: name.into(),
                    parent_scenario_id: None,
                },
            )
            .unwrap();
        }
        // A directory with no readable meta.json is not a scenario anywhere
        // else in the app, so clear-all must not reach into it either.
        std::fs::create_dir_all(bundle::scenario_dir(dir.path(), "p1", "junk")).unwrap();

        let mut ids = scenario_ids(dir.path(), "p1").unwrap();
        ids.sort();
        assert_eq!(ids, ["s1", "s2"]);
    }

    #[test]
    fn clearing_results_takes_the_warnings_with_them() {
        // The warnings writer maintains "warnings can never exist without
        // results". Deleting only results.out would leave the last run's
        // warnings being served for a target that now reports unsimulated.
        let dir = tempfile::tempdir().unwrap();
        let results = results_path_for(dir.path(), "p1", None);
        let warnings = results.with_file_name("warnings.json");
        bundle::atomic_write(&results, b"out").unwrap();
        bundle::atomic_write(&warnings, b"[]").unwrap();

        assert!(remove_results_file(&results).unwrap());
        assert!(!results.exists());
        assert!(!warnings.exists(), "warnings outlived their results");
    }

    #[test]
    fn results_size_counts_exactly_what_a_clear_removes() {
        let dir = tempfile::tempdir().unwrap();
        let results = results_path_for(dir.path(), "p1", None);
        assert_eq!(results_bytes(&results), 0, "never simulated → nothing");

        bundle::atomic_write(&results, &[0u8; 500]).unwrap();
        bundle::atomic_write(&results.with_file_name("warnings.json"), &[0u8; 24]).unwrap();
        // Both files go, so both are counted — the figure shown before
        // confirming is the space actually reclaimed.
        assert_eq!(results_bytes(&results), 524);

        remove_results_file(&results).unwrap();
        assert_eq!(results_bytes(&results), 0);
    }

    // ── starter model ────────────────────────────────────────────────────

    #[test]
    fn starter_inp_is_a_valid_minimal_network() {
        // The whole point of the starter model is that it parses: a project
        // written with a model that does not load is worse than no model.
        let network = hydra::io::parse(STARTER_INP).expect("starter model must parse");
        assert_eq!(network.nodes.len() as u32, STARTER_NODE_COUNT);
        assert_eq!(network.links.len(), 0);
        assert!(
            matches!(network.nodes[0].kind, hydra::NodeKind::Reservoir(_)),
            "the starter node must be a fixed-grade source, or validation fails"
        );
        // Coordinates matter: a node the canvas cannot place is a node the
        // user cannot see or build from.
        assert!(network.coordinates.contains_key(&network.nodes[0].base.id));
    }

    #[test]
    fn a_lone_junction_would_not_have_been_a_valid_starter() {
        // Records why the starter is a reservoir rather than the junction one
        // might reach for first: an unreachable junction fails validation, so
        // "one junction" is not a smaller valid model — it is an invalid one.
        let inp = b"[JUNCTIONS]\n J1  10\n\n[OPTIONS]\n Units LPS\n\n[END]\n";
        assert!(hydra::io::parse(inp).is_err());
    }

    #[test]
    fn the_starter_model_round_trips_through_a_write() {
        // The editor re-serialises the in-memory network on save, so a
        // starter that cannot survive parse → write → parse would break on
        // the user's first edit.
        let network = hydra::io::parse(STARTER_INP).unwrap();
        let written = hydra::io::write_inp(&network);
        let reparsed = hydra::io::parse(&written).expect("re-serialised model must load");
        assert_eq!(reparsed.nodes.len(), network.nodes.len());
        assert_eq!(reparsed.coordinates, network.coordinates);
    }

    // ── scenario branching ───────────────────────────────────────────────

    #[test]
    fn a_scenario_cannot_branch_from_a_project_with_no_network() {
        let dir = tempfile::tempdir().unwrap();

        // Empty project: base model absent.
        let err = scenario_source_model(dir.path(), "p1", None).unwrap_err();
        assert!(err.contains("no network yet"), "got: {err}");

        // Same for branching a scenario that has no model of its own.
        let err = scenario_source_model(dir.path(), "p1", Some("s1")).unwrap_err();
        assert!(err.contains("no network to branch"), "got: {err}");

        // With a base model present, branching resolves to it.
        bundle::atomic_write(&bundle::base_model_path(dir.path(), "p1"), b"[JUNCTIONS]\n").unwrap();
        assert_eq!(
            scenario_source_model(dir.path(), "p1", None).unwrap(),
            bundle::base_model_path(dir.path(), "p1")
        );
    }

    // ── project_to_dto state derivation ──────────────────────────────────

    fn sample_meta(nodes: u32, links: u32) -> meta::ProjectMeta {
        meta::ProjectMeta {
            version: 1,
            name: "test".into(),
            engine: "wds".into(),
            source_crs: "EPSG:4326".into(),
            node_count: nodes,
            link_count: links,
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

    // ── project list sorting ──────────────────────────────────────────────

    #[test]
    fn projects_sort_by_epoch_not_label() {
        let now = meta::now_secs();
        // "20m ago" vs "5h ago": lexicographic label comparison would put
        // "5h ago" first; epoch comparison must put "20m ago" first.
        let older = project_to_dto(
            "old",
            &sample_meta(1, 1),
            0,
            None,
            "not-run",
            false,
            now - 5 * 3_600,
        );
        let newer = project_to_dto(
            "new",
            &sample_meta(1, 1),
            0,
            None,
            "not-run",
            false,
            now - 20 * 60,
        );
        assert_eq!(older.modified_label, "5h ago");
        assert_eq!(newer.modified_label, "20m ago");
        let mut projects = vec![older, newer];
        sort_projects_most_recent_first(&mut projects);
        assert_eq!(projects[0].id, "new");
        assert_eq!(projects[1].id, "old");
    }

    // ── save_project ownership check ──────────────────────────────────────

    #[test]
    fn check_save_target_rejects_mismatched_project() {
        let err = check_save_target(Some("owner-a"), None, "other-b", None).unwrap_err();
        assert!(err.contains("owner-a"));
        assert!(err.contains("other-b"));
    }

    #[test]
    fn check_save_target_allows_matching_or_unowned() {
        assert!(check_save_target(Some("owner-a"), None, "owner-a", None).is_ok());
        assert!(check_save_target(Some("a"), Some("s1"), "a", Some("s1")).is_ok());
        // File-picker loads have no owner yet (pre-create_project draft flow).
        assert!(check_save_target(None, None, "owner-a", None).is_ok());
    }

    /// The scenario half of the guard. Without it, a save issued between the
    /// frontend switching scenario and the new INP loading writes the OLD
    /// target's bytes into the NEW target's model.inp — the project matches,
    /// so a project-only check waves it through.
    #[test]
    fn check_save_target_rejects_mismatched_scenario_within_one_project() {
        // Base model loaded, save aimed at a scenario.
        let err = check_save_target(Some("a"), None, "a", Some("s1")).unwrap_err();
        assert!(err.contains("base model"), "got: {err}");
        assert!(err.contains("s1"), "got: {err}");
        // Scenario loaded, save aimed at the base model.
        let err = check_save_target(Some("a"), Some("s1"), "a", None).unwrap_err();
        assert!(err.contains("s1"), "got: {err}");
        assert!(err.contains("base model"), "got: {err}");
        // Scenario loaded, save aimed at a sibling scenario.
        let err = check_save_target(Some("a"), Some("s1"), "a", Some("s2")).unwrap_err();
        assert!(err.contains("s1") && err.contains("s2"), "got: {err}");
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

    // ── target path helpers ───────────────────────────────────────────────

    #[test]
    fn target_path_helpers_resolve_base_and_scenario() {
        let app_data = std::path::Path::new("/app-data");
        assert_eq!(
            results_path_for(app_data, "p1", None),
            bundle::base_results_path(app_data, "p1")
        );
        assert_eq!(
            results_path_for(app_data, "p1", Some("s1")),
            bundle::scenario_results_path(app_data, "p1", "s1")
        );
        assert_eq!(
            model_path_for(app_data, "p1", None),
            bundle::base_model_path(app_data, "p1")
        );
        assert_eq!(
            model_path_for(app_data, "p1", Some("s1")),
            bundle::scenario_model_path(app_data, "p1", "s1")
        );
    }

    #[test]
    fn validate_target_ids_rejects_non_uuid_parts() {
        let pid = uuid::Uuid::new_v4().to_string();
        let sid = uuid::Uuid::new_v4().to_string();
        assert!(validate_target_ids(&pid, None).is_ok());
        assert!(validate_target_ids(&pid, Some(&sid)).is_ok());
        assert!(validate_target_ids("../escape", None).is_err());
        assert!(validate_target_ids(&pid, Some("../escape")).is_err());
    }

    // ── numeric project timestamps ────────────────────────────────────────

    #[test]
    fn project_dto_carries_epoch_ms_alongside_labels() {
        let now = meta::now_secs();
        let dto = project_to_dto(
            "p",
            &sample_meta(1, 1),
            0,
            Some(now - 60),
            "done",
            false,
            now,
        );
        assert_eq!(dto.modified_at_ms, Some(now as u64 * 1000));
        assert_eq!(dto.last_run_at_ms, Some((now - 60) as u64 * 1000));
        // Labels are unchanged by the numeric fields.
        assert_eq!(dto.modified_label, "just now");
        assert_eq!(dto.last_run_label.as_deref(), Some("1m ago"));

        let dto = project_to_dto("p", &sample_meta(1, 1), 0, None, "not-run", false, now);
        assert_eq!(dto.last_run_at_ms, None);
        assert_eq!(epoch_secs_to_ms(-1), None);
    }

    // ── count_scenario_dirs requires readable meta.json ───────────────────

    #[test]
    fn count_scenario_dirs_counts_only_dirs_with_readable_meta() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        let scenarios = bundle::project_dir(app_data, "p1").join("scenarios");
        let with_meta = scenarios.join("with-meta");
        let no_meta = scenarios.join("no-meta");
        let bad_meta = scenarios.join("bad-meta");
        std::fs::create_dir_all(&with_meta).unwrap();
        std::fs::create_dir_all(&no_meta).unwrap();
        std::fs::create_dir_all(&bad_meta).unwrap();
        meta::write_scenario_meta(
            &with_meta,
            &meta::ScenarioMeta {
                name: "s1".into(),
                parent_scenario_id: None,
            },
        )
        .unwrap();
        std::fs::write(bad_meta.join("meta.json"), b"{not json").unwrap();
        // Only the directory list_scenarios would also return is counted.
        assert_eq!(count_scenario_dirs(app_data, "p1"), 1);
        // Missing scenarios dir: zero, not an error.
        assert_eq!(count_scenario_dirs(app_data, "p2"), 0);
    }

    // ── normalize_epsg ────────────────────────────────────────────────────

    #[test]
    fn normalize_epsg_handles_bare_codes_prefixes_and_case() {
        assert_eq!(normalize_epsg("4326"), "EPSG:4326");
        assert_eq!(normalize_epsg(" epsg:27700 "), "EPSG:27700");
        assert_eq!(normalize_epsg("EPSG:3857"), "EPSG:3857");
        // Non-EPSG authorities are upper-cased but not prefixed.
        assert_eq!(normalize_epsg("esri:102100"), "ESRI:102100");
        assert_eq!(normalize_epsg("   "), "");
    }

    // ── parse_wkt_label ───────────────────────────────────────────────────

    #[test]
    fn parse_wkt_label_extracts_first_quoted_name_or_falls_back() {
        assert_eq!(
            parse_wkt_label("GEOGCS[\"WGS 84\",DATUM[\"WGS_1984\"]]", "EPSG:4326"),
            "WGS 84 (EPSG:4326)"
        );
        // No quoted name: falls back to the EPSG code.
        assert_eq!(parse_wkt_label("+proj=longlat", "EPSG:9999"), "EPSG:9999");
        assert_eq!(parse_wkt_label("PROJCS[\"\"]", "EPSG:9998"), "EPSG:9998");
    }

    // ── scenario ordering ─────────────────────────────────────────────────

    #[test]
    fn scenarios_sort_by_name_case_insensitively_not_by_id() {
        let sc = |id: &str, name: &str| ScenarioDto {
            id: id.into(),
            project_id: "p1".into(),
            parent_scenario_id: None,
            name: name.into(),
            state: "not-run".into(),
        };
        // Ids deliberately ordered against the names.
        let mut items = vec![
            sc("aaa", "zeta"),
            sc("zzz", "Alpha"),
            sc("mmm", "beta"),
            sc("bbb", "alpha"),
        ];
        sort_scenarios_by_name(&mut items);
        let names: Vec<&str> = items.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Alpha", "beta", "zeta"]);
        // Case-insensitive equal names tie-break deterministically by id.
        assert_eq!(items[0].id, "bbb");
        assert_eq!(items[1].id, "zzz");
    }

    // ── meta.json atomic writes ───────────────────────────────────────────

    #[test]
    fn write_project_meta_is_atomic_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().join("proj");
        let m = sample_meta(7, 6);
        // Creates the directory as needed, like the previous implementation.
        meta::write_project_meta(&project_dir, &m).unwrap();
        let back = meta::read_project_meta(&project_dir).unwrap();
        assert_eq!(back.node_count, 7);
        assert_eq!(back.link_count, 6);
        // No temp file left behind by the atomic write.
        let leftovers: Vec<_> = std::fs::read_dir(&project_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != "meta.json")
            .collect();
        assert!(leftovers.is_empty(), "unexpected files: {leftovers:?}");

        // Overwrite in place.
        let mut m2 = sample_meta(1, 1);
        m2.name = "renamed".into();
        meta::write_project_meta(&project_dir, &m2).unwrap();
        assert_eq!(
            meta::read_project_meta(&project_dir).unwrap().name,
            "renamed"
        );
    }
}
