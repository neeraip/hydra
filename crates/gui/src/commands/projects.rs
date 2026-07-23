//! Project, scenario, CRS-catalog, and file-manager commands, plus the shared
//! bundle-path/id helpers and project DTO derivation from on-disk state.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::meta::{self, bundle};

use super::binary_codec::{encode_network_snapshot, encode_network_snapshot_absent};
use super::network_dto::{
    format_inp_parse_error, network_to_dto, NetworkDto, NetworkState, NetworkStateInner,
};

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

/// Persist a new project. Called from the frontend's "New Project" wizard
/// once a network has been loaded into [`NetworkState`]. The INP bytes
/// currently held in managed state are copied into the bundle as the
/// project's canonical base model so the bundle is self-contained on disk
/// even if the original source file is later moved or deleted.
#[tauri::command(async)]
/// Create a new project directory with `meta.json` and `base/` subdirectories.
pub fn create_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    name: String,
) -> Result<Project, String> {
    validate_id(&id)?;
    let app_data = app_data_dir(&app)?;

    // Snapshot the currently loaded network (if any). `up_to_date_raw_bytes`
    // re-serialises first when in-memory edits have not been flushed yet.
    let (inp_bytes, node_count, link_count) = {
        let mut guard = state.0.lock();
        let bytes = guard.up_to_date_raw_bytes().cloned();
        match &*guard {
            NetworkStateInner::Loaded { dto, .. } => {
                (bytes, dto.nodes.len() as u32, dto.links.len() as u32)
            }
            NetworkStateInner::Empty => (None, 0, 0),
        }
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
    /// Always `None` — the frontend fetches the network separately via
    /// `load_project_network`, so the full DTO is no longer serialised into
    /// this payload. The field is kept for wire-format compatibility.
    pub network: Option<NetworkDto>,
}

/// Open an existing project from disk. Reads the metadata, and if a base model
/// is present, parses it into [`NetworkState`] so the rest of the app can read
/// nodes/links/results from it.
#[tauri::command(async)]
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
    let project = project_dto_from_disk(&app_data, &project_dir, &id, &meta);

    // If the bundle has a base model on disk, parse it and populate state.
    // The parsed network and its DTO are intentionally *not* returned to the
    // caller: the frontend fetches the snapshot separately via
    // `load_project_network` / `get_network_snapshot`, so returning the full
    // network here would serialise tens of MB that are immediately discarded.
    let model_path = bundle::base_model_path(&app_data, &id);
    if model_path.exists() {
        let bytes = std::fs::read(&model_path).map_err(|e| e.to_string())?;
        let net = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;
        let dto = network_to_dto(&net);
        *state.0.lock() = NetworkStateInner::Loaded {
            raw_bytes: bytes,
            dirty: false,
            network: net,
            dto,
            owner_project_id: Some(id.clone()),
            owner_scenario_id: None,
        };
    } else {
        *state.0.lock() = NetworkStateInner::Empty;
    }

    Ok(Some(LoadedProject {
        project,
        network: None,
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

/// Build a [`Project`] DTO from a project's on-disk bundle state: scenario
/// count, sim state + last-run time derived from `base/results.out`, and
/// `modified_at` from `base/model.inp` (falling back to the project directory
/// mtime, then to "now"). Shared by `list_projects` / `load_project` /
/// `rename_project` so the three always derive identical rows.
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
    /// Always 0 — the filesystem is the source of truth, so nothing can be
    /// orphaned; kept for wire-format compatibility.
    pub recovered: u32,
    /// Project IDs whose on-disk folder is missing. Always empty; kept for
    /// wire-format compatibility.
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

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("Field data (CSV, Excel)", &["csv", "xlsx", "xls"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;

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

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter("EPANET Input File", &["inp"])
            .blocking_pick_file()
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
    let network = hydra::io::parse(&bytes).map_err(format_inp_parse_error)?;

    let mut dto = network_to_dto(&network);
    dto.file_stem = file_stem;

    *state.0.lock() = NetworkStateInner::Loaded {
        raw_bytes: bytes,
        dirty: false,
        network,
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
/// Reject a `save_project` call whose target project does not own the network
/// currently held in `NetworkState`. `owner_project_id` is `None` only for
/// networks loaded from the file picker (no owning project yet), which are
/// allowed through to preserve the draft/`create_project` flow.
fn check_save_target(owner_project_id: Option<&str>, id: &str) -> Result<(), String> {
    match owner_project_id {
        Some(owner) if owner != id => Err(format!(
            "save_project refused: the loaded network belongs to project {owner}, not {id}; \
             reload the project before saving"
        )),
        _ => Ok(()),
    }
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
                owner_project_id, ..
            } => check_save_target(owner_project_id.as_deref(), &id)?,
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
/// `NetworkState`, making it available to `get_nodes` / `get_links`.
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
        network,
        dto,
        owner_project_id: Some(project_id.clone()),
        owner_scenario_id: scenario_id.clone(),
    };
    Ok(tauri::ipc::Response::new(encoded))
}

/// Return the raw INP text of a project's base model (`base/model.inp`).
/// Scenario INPs are not addressable through this command.
///
/// Used by the "Preview changes" diff dialog so the frontend can compare the
/// saved file against a prospective patched version.
#[tauri::command(async)]
/// Return the raw INP text for a project's base model.
pub fn get_project_inp(app: tauri::AppHandle, project_id: String) -> Result<String, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let path = bundle::base_model_path(&app_data, &project_id);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
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
            std::fs::read(&path).map_err(|e| format!("Cannot read model: {e}"))?
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
/// Return the hydra engine and application version strings.
pub fn get_versions() -> Versions {
    Versions {
        hydra: HYDRA_VERSION,
        app: env!("CARGO_PKG_VERSION"),
    }
}

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
        let err = check_save_target(Some("owner-a"), "other-b").unwrap_err();
        assert!(err.contains("owner-a"));
        assert!(err.contains("other-b"));
    }

    #[test]
    fn check_save_target_allows_matching_or_unowned() {
        assert!(check_save_target(Some("owner-a"), "owner-a").is_ok());
        // File-picker loads have no owner yet (pre-create_project draft flow).
        assert!(check_save_target(None, "owner-a").is_ok());
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
                description: None,
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
