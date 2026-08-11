//! Project, scenario, CRS-catalog, and file-manager commands, plus the shared
//! bundle-path/id helpers and project DTO derivation from on-disk state.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::meta::{self, bundle};

use super::binary_codec::{encode_network_snapshot, encode_network_snapshot_absent};
use super::mutations::{validation_findings, ValidationFindingDto};
use super::network_dto::{
    format_read_error, network_to_dto, NetworkDto, NetworkState, NetworkStateInner,
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
    /// This project's display-unit override — `"source"`, `"si"`, `"us"`,
    /// or absent when it follows the app-wide default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_system: Option<String>,
    pub insights: Option<ProjectInsights>,
    /// `true` when the project's on-disk bundle directory is absent. Always
    /// `false` now that projects are discovered by scanning the filesystem;
    /// kept for wire-format compatibility. The frontend renders such rows
    /// muted and offers "Remove from list" instead of "Open folder".
    pub folder_missing: bool,
}

// Off the main thread: this stats every project, reads every `meta.json`
// and counts every scenario directory, so its cost grows with the library
// rather than staying constant — and it is the first thing the home page
// asks for.
#[tauri::command(async)]
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

/// Engines whose projects this GUI can create, edit, and run.
///
/// The registry says what this *build of Hydra* can simulate; these lists
/// say what this *GUI* can do with each engine, in two tiers. An engine can
/// be **openable** (projects can be created from an imported model, viewed,
/// and run through the queue) before it is **editable** (tables, inspector
/// writes, element creation). The tiers differ while an engine's viewer
/// ships ahead of its editor.
pub(crate) const GUI_OPENABLE_ENGINES: &[&str] = &["wds", "uds"];
const GUI_EDITABLE_ENGINES: &[&str] = &["wds"];

/// Resolve an engine key, refusing anything this GUI cannot open at all
/// (hydra-common spec §2.3).
///
/// The three failure modes are deliberately distinct in the message: an
/// unknown key means the project came from a newer Hydra, a planned key
/// means the engine is registered but unimplemented, and an available but
/// unopenable key means the engine runs from the CLI only. Collapsing them
/// would leave a user unable to tell "upgrade Hydra" from "wait for it"
/// from "use the CLI".
pub(crate) fn require_gui_openable_engine(
    key: &str,
) -> Result<&'static hydra::common::EngineDescriptor, String> {
    let descriptor = hydra::common::engine_by_key(key)
        .map_err(|_| format!("unknown engine {key:?} — this build of Hydra does not have it"))?;
    if !descriptor.is_available() {
        return Err(format!(
            "{} modelling is not available yet in this build of Hydra",
            descriptor.label
        ));
    }
    if !GUI_OPENABLE_ENGINES.contains(&descriptor.key) {
        return Err(format!(
            "{} projects are not supported in the Hydra GUI yet — run these \
             models with the hydra CLI",
            descriptor.label
        ));
    }
    Ok(descriptor)
}

/// The engine key a project's metadata declares; `"wds"` for projects
/// predating the field, matching `ProjectMeta`'s own default.
pub(crate) fn project_engine_key(app_data: &std::path::Path, project_id: &str) -> String {
    let dir = bundle::project_dir(app_data, project_id);
    meta::read_project_meta(&dir)
        .map(|m| m.engine)
        .unwrap_or_else(|_| "wds".to_string())
}

/// Whether this GUI can edit the given engine's models. Openable-but-not-
/// editable engines get read-only projects: viewable and runnable, with
/// every mutating command refusing.
pub(crate) fn engine_is_gui_editable(key: &str) -> bool {
    GUI_EDITABLE_ENGINES.contains(&key)
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

/// Choose the base model for a new project, and the element counts that
/// describe it.
///
/// Returns the loaded network's bytes only when `import` is true **and** a
/// network is actually loaded; otherwise the starter model. The counts always
/// describe the bytes returned — they are what `state` ("draft" vs "ready")
/// and every has-a-network check downstream are derived from, so a mismatch
/// would mislabel the project.
///
/// Split out of `create_project` so the choice can be tested without a Tauri
/// handle: this is the decision that, when inferred from managed state alone,
/// copied a previously-opened project into a project asked to be empty.
type NewProjectModel = (Vec<u8>, u32, u32, Vec<(String, Vec<u8>)>);

fn new_project_model(import: bool, guard: &mut NetworkStateInner) -> NewProjectModel {
    if !import {
        return (STARTER_INP.to_vec(), STARTER_NODE_COUNT, 0, Vec::new());
    }
    // `up_to_date_raw_bytes` re-serialises first when in-memory edits have
    // not been flushed yet.
    let bytes = guard.current_model_bytes();
    match (&*guard, bytes) {
        (NetworkStateInner::Loaded { dto, .. }, Some(bytes)) => (
            bytes,
            dto.nodes.len() as u32,
            dto.links.len() as u32,
            Vec::new(),
        ),
        // A uds import: the model text as imported, counted from the parsed
        // network, together with whatever auxiliary records the import
        // gathered beside the model or the user attached. Falling through
        // to the starter here would silently write an EPANET model into an
        // urban-drainage project.
        (
            NetworkStateInner::LoadedUds {
                network, aux_files, ..
            },
            Some(bytes),
        ) => (
            bytes,
            network.vertices.len() as u32,
            network.links.len() as u32,
            aux_files.clone(),
        ),
        _ => (STARTER_INP.to_vec(), STARTER_NODE_COUNT, 0, Vec::new()),
    }
}

/// Persist a new project. Called from the frontend's "New Project" wizard.
///
/// `import_loaded_network` states the caller's **intent** and is never
/// inferred. When true, the INP bytes currently held in managed state are
/// copied into the bundle as its canonical base model, so the bundle is
/// self-contained even if the original source file is later moved or deleted.
/// When false — or when nothing is loaded — [`STARTER_INP`] is written
/// instead; a project always has a model.
///
/// The flag exists because managed state is ambient: it holds whichever
/// network was last opened and is not cleared by leaving a project. Deriving
/// "the user imported something" from "a network is loaded" silently wrote a
/// previously-opened project's model into a project the user asked to be
/// empty.
#[tauri::command(async)]
/// Create a new project directory with `meta.json` and `base/` subdirectories.
pub fn create_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    name: String,
    engine: String,
    import_loaded_network: bool,
) -> Result<Project, String> {
    validate_id(&id)?;
    // The engine key is persisted into meta.json and never rewritten, so a
    // key that this GUI cannot open must be refused here rather than
    // producing a project that opens into a permanent unsupported state.
    let descriptor = require_gui_openable_engine(&engine)?;
    // A read-only engine has no editor to grow a model in, so a blank
    // start would create a project that can never hold anything: its
    // projects begin from an imported model or not at all.
    if !engine_is_gui_editable(descriptor.key) && !import_loaded_network {
        return Err(format!(
            "{} projects start from an imported model — editing is not \
             available in the GUI yet",
            descriptor.label
        ));
    }
    let app_data = app_data_dir(&app)?;

    let (inp_bytes, node_count, link_count, aux_files) =
        new_project_model(import_loaded_network, &mut state.0.lock());

    let project = persist_new_project(
        &app_data, &id, name, engine, &inp_bytes, node_count, link_count,
    )?;
    // The auxiliary records gathered at import travel into the bundle,
    // where the run queue reads them (§12.1).
    super::aux_files::write_aux_files(&app_data, &id, &aux_files)?;
    Ok(project)
}

/// Write a new project bundle to disk: directories, `meta.json`, and the
/// base model. The write half of [`create_project`], shared with the
/// archive import, which creates one project per selected entry without
/// anything passing through managed state.
///
/// Callers have already validated the id and the engine key; this only
/// persists.
pub(crate) fn persist_new_project(
    app_data: &std::path::Path,
    id: &str,
    name: String,
    engine: String,
    inp_bytes: &[u8],
    node_count: u32,
    link_count: u32,
) -> Result<Project, String> {
    let project_dir = bundle::project_dir(app_data, id);
    let base_dir = bundle::base_dir(app_data, id);
    let scenarios_dir = project_dir.join("scenarios");
    std::fs::create_dir_all(&base_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&scenarios_dir).map_err(|e| e.to_string())?;

    let meta = meta::ProjectMeta {
        version: 1,
        name,
        engine,
        source_crs: source_crs_for_model(inp_bytes),
        node_count,
        link_count,
        // No override: a new project follows the app-wide default until
        // someone says otherwise.
        unit_system: None,
    };
    meta::write_project_meta(&project_dir, &meta)?;

    bundle::atomic_write(&bundle::base_model_path(app_data, id), inp_bytes)
        .map_err(|e| e.to_string())?;

    let modified_at = meta::mtime_secs(&bundle::base_model_path(app_data, id))
        .or_else(|| meta::mtime_secs(&project_dir))
        .unwrap_or_else(meta::now_secs);
    Ok(project_to_dto(
        id,
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
// Off the main thread: the tree holds `results.out` files that run to
// gigabytes, and unlinking those is not work to do where a window is
// waiting to redraw.
#[tauri::command(async)]
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

/// The CRS sentinel for a model whose plan coordinates are a local
/// drawing grid rather than a georeferenced system. Not an EPSG code
/// precisely because no EPSG code is true of such a model.
pub(crate) const LOCAL_CRS: &str = "LOCAL";

/// The CRS a freshly imported model should start on.
///
/// SWMM states its coordinate basis in `[MAP] UNITS` — `DEGREES` means the
/// plan coordinates are geographic, while `FEET`, `METERS` and `NONE` mean
/// a linear grid whose origin is the drawing canvas, not a datum. Reading
/// it beats defaulting everything to WGS84, which silently asserts that a
/// site grid in feet is longitude and latitude. Anything we cannot read
/// keeps the old default: WGS84 is the right guess for a model that says
/// nothing, and the user can still correct it.
fn source_crs_for_model(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    // The model's own declaration wins where it exists: [MAP] UNITS.
    let mut in_map = false;
    let mut in_coords = false;
    // What the coordinates themselves say, absent a declaration: `None`
    // until a non-placeholder point is seen, then whether every one so
    // far fits inside longitude/latitude ranges.
    let mut all_lat_lng: Option<bool> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let header = trimmed.to_ascii_uppercase();
            in_map = header.starts_with("[MAP");
            in_coords = header.starts_with("[COORDINATES");
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with(';') {
            continue;
        }
        if in_map {
            let mut it = trimmed.split_whitespace();
            if !it.next().is_some_and(|k| k.eq_ignore_ascii_case("UNITS")) {
                continue;
            }
            match it.next().map(str::to_ascii_uppercase).as_deref() {
                Some("DEGREES") => return "EPSG:4326".to_string(),
                Some("FEET") | Some("METERS") | Some("NONE") => return LOCAL_CRS.to_string(),
                // An unrecognised word declares nothing; the coordinates
                // decide below.
                _ => {}
            }
        }
        if in_coords {
            let mut it = trimmed.split_whitespace();
            let _id = it.next();
            let (Some(Ok(x)), Some(Ok(y))) = (
                it.next().map(str::parse::<f64>),
                it.next().map(str::parse::<f64>),
            ) else {
                continue;
            };
            // The importer writes (0, 0) for an element with no
            // coordinate at all; placeholders say nothing.
            if x == 0.0 && y == 0.0 {
                continue;
            }
            let fits = (-180.0..=180.0).contains(&x) && (-90.0..=90.0).contains(&y);
            all_lat_lng = Some(all_lat_lng.unwrap_or(true) && fits);
        }
    }
    // Undeclared: geographic only when the numbers could actually be
    // degrees — every coordinate inside longitude/latitude ranges. A
    // model whose coordinates rule that out, or that carries none at
    // all, is a drawing grid until its author says otherwise: painting a
    // basemap under a synthesized schematic — or placing a metric survey
    // at null island — asserts an earth placement nobody chose.
    match all_lat_lng {
        Some(true) => "EPSG:4326".to_string(),
        _ => LOCAL_CRS.to_string(),
    }
}

/// The unit system a target's own model declares — `"si"` or `"us"`.
///
/// This is what the `"source"` display preference resolves to. Both engines
/// declare a named flow unit (GPM, CFS, LPS, CMS, …) that falls into one of
/// two coherent groups, and that group is the finest distinction the §5
/// quantity descriptors can express: they carry one SI label and one US
/// label, so a CFS model and a GPM model both resolve to `"us"` and both
/// display gpm. Reports are finer-grained — they name the model's exact
/// flow unit — which is why the setting this feeds is about a *system*,
/// not about matching the file's every label.
///
/// `None` when the target has no model yet, or its engine declares none.
#[tauri::command(async)]
pub fn get_model_unit_system(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<String>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let si = |b: bool| Ok(Some(if b { "si" } else { "us" }.to_string()));
    match project_engine_key(&app_data, &project_id).as_str() {
        "wds" => {
            let network = super::results::network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            si(hydra::io::units::is_si(network.options.flow_units))
        }
        "uds" => {
            let network = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            si(!network.options.flow_units.is_us())
        }
        _ => Ok(None),
    }
}

/// Set a project's display unit system, or clear the override back to the
/// app-wide default with `None`. Returns `true` when the metadata was
/// updated, `false` when the project is not found on disk.
#[tauri::command]
pub fn update_project_units(
    app: tauri::AppHandle,
    id: String,
    unit_system: Option<String>,
) -> Result<bool, String> {
    validate_id(&id)?;
    if let Some(v) = unit_system.as_deref() {
        if !matches!(v, "source" | "si" | "us") {
            return Err(format!("unknown unit system '{v}'"));
        }
    }
    let app_data = app_data_dir(&app)?;
    let project_dir = bundle::project_dir(&app_data, &id);
    if !project_dir.exists() {
        return Ok(false);
    }
    let mut project_meta = meta::read_project_meta(&project_dir)?;
    // `None` clears the override — back to following the default, which is
    // not the same as pinning the value the default currently holds.
    project_meta.unit_system = unit_system;
    meta::write_project_meta(&project_dir, &project_meta)?;
    Ok(true)
}

/// Update the source CRS for a project. Returns `true` when the metadata was
/// updated, `false` when the project is not found on disk.
#[tauri::command]
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

/// The project's saved criteria valuation for its engine (hydra-common
/// spec §7.3), or `null` when none is saved. Engine-generic: the wds
/// legacy store is separate (`get_project_criteria`).
#[tauri::command]
pub fn get_criteria_valuation(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Option<serde_json::Value>, String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let engine = project_engine_key(&app_data, &project_id);
    Ok(meta::read_criteria_valuation(
        &bundle::project_dir(&app_data, &project_id),
        &engine,
    ))
}

/// Persist the project's criteria valuation for its engine.
#[tauri::command]
pub fn update_criteria_valuation(
    app: tauri::AppHandle,
    project_id: String,
    valuation: serde_json::Value,
) -> Result<(), String> {
    validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    let engine = project_engine_key(&app_data, &project_id);
    meta::write_criteria_valuation(
        &bundle::project_dir(&app_data, &project_id),
        &engine,
        &valuation,
    )
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
pub(crate) fn remove_results_file(path: &std::path::Path) -> Result<bool, String> {
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

/// Every project id on disk, in directory order.
///
/// Deliberately tolerant where `list_projects` is not: this backs the
/// data-folder figures and the clear-everything action, both of which
/// should describe what is actually there. A project whose `meta.json` is
/// missing or unreadable still occupies disk and still holds results, so
/// it is counted and cleared — `list_projects` skips it, because a project
/// it cannot describe is one it cannot render.
pub(crate) fn project_ids(app_data: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(bundle::projects_root(app_data)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect()
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

/// Reveal the application data directory in the system file manager.
///
/// This is the root every bundle lives under — `projects/<id>/` with each
/// project's manifest, criteria and models, plus `custom_crs.json`. Nothing
/// else in the app can reach it, so inspecting a project's files previously
/// meant knowing the Tauri identifier and typing the path by hand.
#[tauri::command]
/// Open the app data directory in the OS file manager.
pub fn open_data_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let app_data = app_data_dir(&app)?;
    // A fresh install has no directory until the first project is created,
    // and revealing nothing would look like a broken button.
    if !app_data.exists() {
        std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    }
    app.opener()
        .reveal_item_in_dir(&app_data)
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
        unit_system: meta.unit_system.clone(),
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

/// A model just read by the import dialog, with whatever stands between it and
/// being simulable.
///
/// The findings travel with the network rather than being fetched afterwards
/// because at this point there is no project to fetch them for —
/// `validate_network` is addressed by project and scenario, and neither exists
/// until the user finishes the wizard.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedModel {
    /// The recovered network. Empty (bar the file name) for engines whose
    /// element data reaches the frontend through the viewer snapshot rather
    /// than this DTO — which is why the counts below are explicit.
    pub network: NetworkDto,
    /// Element counts of the imported model, for the wizard's preview.
    pub node_count: u32,
    /// See `node_count`.
    pub link_count: u32,
    /// §2.9 violations, empty when the model is ready to run. Non-empty means
    /// the project will open with these listed in the Issues panel.
    pub findings: Vec<ValidationFindingDto>,
    /// Repairs applied during import (repair by omission, uds interop
    /// §14.10): one human-readable entry per line the importer commented
    /// out. Empty when the file imported as written. Must be surfaced —
    /// the repair contract forbids applying these silently.
    pub repairs: Vec<String>,
    /// Whether the model's own coordinates rule out longitude/latitude
    /// (see [`coordinates_are_projected`]). Answered here rather than in
    /// the wizard because a uds import returns no elements at all — the
    /// one engine whose models most often carry projected coordinates
    /// could never have been asked on the frontend.
    pub coordinates_projected: bool,
    /// The engine that owns this model. Echoed even when the caller named
    /// it, so the one path that *discovers* it (recognition, §2.5.1) and
    /// the one that is told it return the same shape.
    pub engine: String,
    /// Auxiliary files the model references: carried when the import has
    /// their bytes in hand (found beside the model, or attached), warned
    /// about otherwise — the wizard says which before the user commits.
    pub sidecars: Vec<super::aux_files::SidecarRef>,
}

/// Whether a model's own coordinates rule out longitude and latitude.
///
/// The strongest fact available before any coordinate system is chosen,
/// and deliberately the same test the canvas applies once one is —
/// `crsInference`'s `projected` on the frontend, and the WGS84 range check
/// in `useCrsReprojection`. A point outside ±180/±90 is not degrees,
/// whatever else it may be.
///
/// Placeholder points are ignored: the importer writes (0, 0) for an
/// element with no coordinate at all, and counting those would drag every
/// reading towards null island.
fn coordinates_are_projected(points: impl Iterator<Item = (f64, f64)>) -> bool {
    points
        .filter(|(x, y)| !(*x == 0.0 && *y == 0.0))
        .any(|(x, y)| !(-180.0..=180.0).contains(&x) || !(-90.0..=90.0).contains(&y))
}

/// Select every element for reporting when the model selects none.
///
/// A drainage model's `[REPORT]` selections default to nothing — the
/// predecessor's behaviour, which the engine keeps faithfully — so a file
/// that never names one runs to completion and writes a *valid* results
/// file containing only the system-wide series. Opened here that reads as
/// a total failure: every element grey, every value absent, and nothing on
/// screen to say why, because as far as the file is concerned nothing was
/// asked about.
///
/// Selecting everything is the repair, and it is safe in the way §14.10's
/// omissions are: it asks for output, changes no physics, and leaves the
/// run identical in every other respect. Appended rather than edited in
/// place — a later directive overrides an earlier one, so the author's own
/// text survives untouched and readable.
///
/// Only when *nothing* is selected. A model naming some elements has made
/// a choice, and widening it would be guessing at what it meant.
fn ensure_uds_reporting(
    text: String,
    network: hydra::uds::model::Network,
) -> (String, hydra::uds::model::Network, Option<String>) {
    use hydra::uds::model::ReportSelection;
    let unset = |s: &ReportSelection| matches!(s, ReportSelection::None);
    if !(unset(&network.report.parcels)
        && unset(&network.report.vertices)
        && unset(&network.report.links))
    {
        return (text, network, None);
    }
    let widened = format!(
        "{}\n\n[REPORT]\n\
         ; [added by Hydra import] the model selected no elements to report\n\
         SUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n",
        text.trim_end()
    );
    let (reparsed, diags) = hydra::uds::io::objects::parse_network(&widened);
    if diags.iter().any(|d| d.kind.is_error()) {
        // Cannot happen for an appended section on text that already
        // parsed; keep the file as written rather than serve one that no
        // longer reads.
        return (text, network, None);
    }
    (
        widened,
        reparsed,
        Some(
            "Selected all elements for reporting — the model selected none, \
             so the run would have produced no per-element results"
                .into(),
        ),
    )
}

/// Parse uds model text, applying §14.10 repair-by-omission when that is
/// the only thing standing between the file and a clean import: when every
/// refusal is repairable, the offending lines are commented out (original
/// text preserved behind the `;`) and the text re-read. A model that
/// selects nothing for reporting is then widened to select everything (see
/// [`ensure_uds_reporting`]). Returns the served text, the parsed network,
/// and one message per repair applied.
fn import_uds_text(
    text: String,
) -> Result<(String, hydra::uds::model::Network, Vec<String>), String> {
    let (network, diags) = hydra::uds::io::objects::parse_network(&text);
    let errors: Vec<_> = diags.iter().filter(|d| d.kind.is_error()).collect();
    if errors.is_empty() {
        let (text, network, widened) = ensure_uds_reporting(text, network);
        return Ok((text, network, widened.into_iter().collect()));
    }
    if !errors.iter().all(|d| d.kind.repairable_by_omission()) {
        // At least one refusal carries meaning omission would change —
        // report the first, exactly as before repairs existed.
        let first = errors
            .iter()
            .find(|d| !d.kind.repairable_by_omission())
            .expect("checked above");
        return Err(format!("Cannot import this model: {first}"));
    }

    let lines_to_comment: std::collections::HashSet<usize> =
        errors.iter().map(|d| d.line).collect();
    let repaired: String = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if lines_to_comment.contains(&(i + 1)) {
                format!("; [commented out by Hydra import] {line}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let (network, diags) = hydra::uds::io::objects::parse_network(&repaired);
    if let Some(first) = diags.iter().find(|d| d.kind.is_error()) {
        // The repair did not converge (should not happen for line-scoped
        // omissions) — refuse rather than loop.
        return Err(format!("Cannot import this model: {first}"));
    }
    let mut repairs: Vec<String> = errors
        .iter()
        .map(|d| format!("Commented out {d}"))
        .collect();
    let (repaired, network, widened) = ensure_uds_reporting(repaired, network);
    repairs.extend(widened);
    Ok((repaired, network, repairs))
}

/// Open a native file-open dialog filtered to `engine`'s source-model
/// formats, parse the chosen file with that engine, store the result in
/// managed state, and return it to the caller.
///
/// Returns `null` to the frontend when the dialog is cancelled.
///
/// The picker filter comes from the engine descriptor rather than being
/// hardcoded, but the filter is only a filter: `wds` and `uds` both claim
/// `.inp` (hydra-common spec §2.2), so the parse below is what actually
/// decides whether the file is the right kind of model.
///
/// Fails only on a model that cannot be read at all; one that is readable but
/// not yet simulable imports, and the reasons come back alongside it as
/// `findings` so the wizard can say so before the user commits to creating the
/// project.
#[tauri::command]
/// Open a native file-picker, parse the chosen model file, and hold it in `NetworkState`.
pub async fn open_and_load_network(
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
    engine: String,
) -> Result<Option<ImportedModel>, String> {
    use tauri_plugin_dialog::DialogExt;

    let descriptor = require_gui_openable_engine(&engine)?;

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
    // `require_gui_openable_engine` has already narrowed this to engines
    // the GUI opens. Matching on the key rather than calling the wds
    // parser unconditionally makes the next engine a compile-time decision
    // instead of a silently wrong parse.
    //
    // Tolerant (model spec §4.1.2), matching `load_network`. Importing
    // strictly meant a network that is readable but not yet simulable could be
    // *reopened* once it was already a project, yet could not be imported to
    // become one — so the file a user most needs to open in order to fix was
    // the one the wizard refused.
    //
    // Runs still use the strict `parse`, so an unsimulable network cannot
    // reach the solver.
    load_model_bytes(descriptor.key, bytes, file_stem, &state, path_buf.parent())
}

/// A parsed import before it is stored or persisted anywhere: the parsed
/// network in its engine's own shape, plus the bytes that become the
/// project's `model.inp` — the original file for wds, the §14.10-repaired
/// text for uds.
///
/// Split from the storing so an import can be *described* without touching
/// the single-slot `NetworkState` — the archive scan parses many models,
/// and holding each briefly in the one global slot would race the wizard.
pub(crate) enum ParsedModel {
    /// A water-distribution model and its element DTOs.
    Wds {
        raw_bytes: Vec<u8>,
        network: Box<hydra::Network>,
        dto: Box<NetworkDto>,
    },
    /// A drainage model: the served (possibly repaired) text.
    Uds {
        raw_text: String,
        network: Box<hydra::uds::model::Network>,
    },
}

impl ParsedModel {
    /// The bytes a project created from this import persists as
    /// `base/model.inp`.
    pub(crate) fn served_bytes(&self) -> Vec<u8> {
        match self {
            ParsedModel::Wds { raw_bytes, .. } => raw_bytes.clone(),
            ParsedModel::Uds { raw_text, .. } => raw_text.clone().into_bytes(),
        }
    }
}

/// Parse model bytes with the engine that owns them and describe the result
/// for the wizard, without storing anything.
///
/// Shared by every way a model reaches the app: choosing an engine and then
/// a file, choosing a file and letting the recognition contract name the
/// engine (hydra-common spec §2.5.1), and each entry of an archive import.
/// All must produce the same project from the same bytes, which one body is
/// the only way to guarantee.
pub(crate) fn parse_model_bytes(
    engine_key: &str,
    bytes: Vec<u8>,
    file_stem: String,
) -> Result<(ParsedModel, ImportedModel), String> {
    let network = match engine_key {
        "wds" => {
            let (network, _validation_errors) =
                hydra::io::parse_tolerant(&bytes).map_err(format_read_error)?;
            network
        }
        "uds" => {
            // Read-only import: parse errors refuse the file — unless every
            // refusal is repairable by omission (interop §14.10), in which
            // case the offending lines are commented out, the repaired text
            // becomes the project's model.inp, and the repairs are reported.
            // The wizard preview and Issues panel for uds arrive with the
            // viewer snapshot — the returned DTO is empty apart from the
            // file name.
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let (text, network, repairs) = import_uds_text(text)?;
            let dto = NetworkDto {
                file_stem,
                ..Default::default()
            };
            let (node_count, link_count) =
                (network.vertices.len() as u32, network.links.len() as u32);
            let coordinates_projected =
                coordinates_are_projected(super::uds_view::model_coordinates(&network));
            let imported = ImportedModel {
                network: dto,
                node_count,
                link_count,
                findings: Vec::new(),
                repairs,
                coordinates_projected,
                engine: engine_key.to_string(),
                sidecars: super::aux_files::sidecar_status(&network, &[]),
            };
            return Ok((
                ParsedModel::Uds {
                    raw_text: text,
                    network: Box::new(network),
                },
                imported,
            ));
        }
        other => return Err(format!("no importer for engine {other:?}")),
    };

    // Re-derived from the network rather than mapped from the errors the parse
    // returned, so these are the same DTOs — same codes, same element ids —
    // that the Issues panel will list once the project opens. The wizard's
    // count and the panel's contents cannot disagree.
    let findings = validation_findings(&network);

    let mut dto = network_to_dto(&network);
    dto.file_stem = file_stem;
    let coordinates_projected = coordinates_are_projected(network.coordinates.values().copied());
    let (node_count, link_count) = (dto.nodes.len() as u32, dto.links.len() as u32);
    let imported = ImportedModel {
        network: dto.clone(),
        node_count,
        link_count,
        findings,
        repairs: Vec::new(),
        coordinates_projected,
        engine: engine_key.to_string(),
        sidecars: Vec::new(),
    };
    Ok((
        ParsedModel::Wds {
            raw_bytes: bytes,
            network: Box::new(network),
            dto: Box::new(dto),
        },
        imported,
    ))
}

/// Parse model bytes, hold the result in managed state, and describe it for
/// the wizard — [`parse_model_bytes`] plus the storing that the single-model
/// import paths want.
///
/// `source_dir` is where the model file came from, when it came from disk:
/// auxiliary files a drainage model references are looked for there — the
/// name as written first, its trailing file name second — and every one
/// found is gathered for `create_project` to write into the bundle, with
/// the wizard told which references are covered and which are not.
fn load_model_bytes(
    engine_key: &str,
    bytes: Vec<u8>,
    file_stem: String,
    state: &tauri::State<'_, NetworkState>,
    source_dir: Option<&std::path::Path>,
) -> Result<Option<ImportedModel>, String> {
    let (parsed, mut imported) = parse_model_bytes(engine_key, bytes, file_stem)?;
    *state.0.lock() = match parsed {
        ParsedModel::Wds {
            raw_bytes,
            network,
            dto,
        } => NetworkStateInner::Loaded {
            raw_bytes,
            dirty: false,
            network: std::sync::Arc::new(*network),
            dto: *dto,
            owner_project_id: None,
            owner_scenario_id: None,
        },
        ParsedModel::Uds { raw_text, network } => {
            let mut aux_files: Vec<(String, Vec<u8>)> = Vec::new();
            if let Some(dir) = source_dir {
                for source in super::aux_files::uds_sidecar_refs(&network) {
                    // Only what a run can consume is gathered; an
                    // unsupported reference is named to the wizard but
                    // never quietly held.
                    if !source.supported {
                        continue;
                    }
                    let base = super::aux_files::aux_basename(&source.file).to_string();
                    let found = std::fs::read(dir.join(&source.file))
                        .or_else(|_| std::fs::read(dir.join(&base)));
                    if let Ok(bytes) = found {
                        // Stored under the name the *model* wrote, which
                        // is what the run path derives its read from.
                        aux_files.push((base, bytes));
                    }
                }
            }
            let gathered: Vec<String> = aux_files.iter().map(|(n, _)| n.clone()).collect();
            imported.sidecars = super::aux_files::sidecar_status(&network, &gathered);
            NetworkStateInner::LoadedUds {
                raw_text,
                network: std::sync::Arc::new(*network),
                aux_files,
                owner_project_id: None,
                owner_scenario_id: None,
            }
        }
    };
    Ok(Some(imported))
}

/// Attach one auxiliary file's bytes to the drainage model currently held
/// for import (§12.1): stored under its trailing name for `create_project`
/// to write into the bundle, replacing an earlier attachment of the same
/// name. Refuses a file the model never references — attaching it would
/// silently do nothing, and the picker was probably aimed at the wrong
/// file. Returns the refreshed sidecar status the wizard renders.
///
/// The decision half of `attach_aux_file`, callable without a dialog.
pub(crate) fn attach_aux_bytes(
    inner: &mut NetworkStateInner,
    file_name: &str,
    bytes: Vec<u8>,
) -> Result<Vec<super::aux_files::SidecarRef>, String> {
    let NetworkStateInner::LoadedUds {
        network, aux_files, ..
    } = inner
    else {
        return Err("no drainage model is loaded to attach files to".into());
    };
    let base = super::aux_files::aux_basename(file_name).to_string();
    let reference = super::aux_files::uds_sidecar_refs(network)
        .into_iter()
        .find(|s| super::aux_files::aux_basename(&s.file).eq_ignore_ascii_case(&base));
    let Some(reference) = reference else {
        return Err(format!(
            "the model does not reference a file named {base:?} — check the \
             model's [RAINGAGES] and climate declarations for the expected name"
        ));
    };
    if !reference.supported {
        return Err(format!(
            "{} is declared by the model, but this format is not served yet — \
             attaching it would change nothing",
            reference.label
        ));
    }
    // Stored under the name the *model* wrote, not the name of the file
    // the user happened to pick: the run path derives its read from the
    // model's reference, so `RAIN.DAT` attached for a model saying
    // `rain.dat` must land as `rain.dat` — a difference invisible on a
    // case-insensitive filesystem and fatal on any other.
    let stored = super::aux_files::aux_basename(&reference.file).to_string();
    aux_files.retain(|(name, _)| !name.eq_ignore_ascii_case(&stored));
    aux_files.push((stored, bytes));
    let gathered: Vec<String> = aux_files.iter().map(|(n, _)| n.clone()).collect();
    Ok(super::aux_files::sidecar_status(network, &gathered))
}

/// Open a native file-picker for an auxiliary file the loaded drainage
/// model references, and attach its bytes for `create_project` to carry.
/// Returns `null` when the dialog is cancelled.
#[tauri::command]
pub async fn attach_aux_file(
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
) -> Result<Option<Vec<super::aux_files::SidecarRef>>, String> {
    use tauri_plugin_dialog::DialogExt;

    let dialog_app = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        dialog_app.dialog().file().blocking_pick_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;
    let Some(path) = path else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    attach_aux_bytes(&mut state.0.lock(), &file_name, bytes).map(Some)
}

/// Open a native file-picker across every model format the GUI can open,
/// and let the file say which engine owns it.
///
/// The inverse of [`open_and_load_network`], which is told the engine and
/// filters the picker to its formats. Here the user has a file and may not
/// know — or care — which tool wrote it, so the recognition contract
/// answers instead (hydra-common spec §2.5.1): every engine judges the
/// bytes, and only a single definite claim routes. A file two engines call
/// plausible is *not* routed, because "I cannot tell this from another
/// engine's model" is not a basis for choosing — the refusal names the
/// candidates so the caller can ask the user.
///
/// Returns `null` when the dialog is cancelled.
#[tauri::command]
pub async fn open_and_recognise_network(
    state: tauri::State<'_, NetworkState>,
    app: tauri::AppHandle,
) -> Result<Option<ImportedModel>, String> {
    use tauri_plugin_dialog::DialogExt;

    // Every format every openable engine imports, as one filter: the point
    // of this path is that the user does not have to know whose file it is.
    let mut filters: Vec<(&'static str, &'static [&'static str])> = Vec::new();
    for engine in hydra::common::ENGINES {
        if require_gui_openable_engine(engine.key).is_err() {
            continue;
        }
        for format in engine.import {
            filters.push((format.label, format.extensions));
        }
    }

    let dialog_app = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = dialog_app.dialog().file();
        for (label, extensions) in filters {
            dialog = dialog.add_filter(label, extensions);
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

    let descriptor = hydra::engines::route(&bytes).map_err(|e| e.to_string())?;
    // Routing says whose the file is, not that this build can open it.
    require_gui_openable_engine(descriptor.key)?;
    load_model_bytes(descriptor.key, bytes, file_stem, &state, path_buf.parent())
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
            }
            | NetworkStateInner::LoadedUds {
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
        // the save point, instead of on every mutation. (A uds model is
        // read-only, so its text is always current.)
        let raw = match guard.current_model_bytes() {
            Some(bytes) => bytes,
            None => return Ok(false),
        };
        match &*guard {
            NetworkStateInner::Loaded { dto, .. } => {
                (raw, dto.nodes.len() as u32, dto.links.len() as u32)
            }
            NetworkStateInner::LoadedUds { network, .. } => (
                raw,
                network.vertices.len() as u32,
                network.links.len() as u32,
            ),
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

    // A read-only engine's project loads into its own state variant; the
    // frontend receives an empty snapshot until the descriptor-driven
    // viewer snapshot lands, so the project page opens (engine pill, runs,
    // reports) with a blank canvas.
    let project_dir = bundle::project_dir(&app_data, &project_id);
    let engine = meta::read_project_meta(&project_dir)
        .map(|m| m.engine)
        .unwrap_or_else(|_| "wds".to_string());
    if engine == "uds" {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let (network, diags) = hydra::uds::io::objects::parse_network(&text);
        if let Some(first) = diags.iter().find(|d| d.kind.is_error()) {
            return Err(format!("Cannot open this model: {first}"));
        }
        let view = super::uds_view::build_view(&network);
        // Same outline for the home page as the distribution path draws,
        // from the viewer's own geometry rather than a network DTO.
        super::sketch::refresh_uds(&app_data, &project_id, &view);
        let encoded = super::uds_view::encode_uds_snapshot(&view);
        *state.0.lock() = NetworkStateInner::LoadedUds {
            raw_text: text,
            network: std::sync::Arc::new(network),
            // Project-owned: aux files live on disk in base/aux/.
            aux_files: Vec::new(),
            owner_project_id: Some(project_id.clone()),
            owner_scenario_id: scenario_id.clone(),
        };
        return Ok(tauri::ipc::Response::new(encoded));
    }

    // Tolerant (model spec §4.1.2): a network under construction is not
    // simulable — a junction exists for some interval before anything
    // connects it — and refusing to load one would mean the editor could not
    // reopen work it had itself saved. The validation errors are not
    // discarded: `validate_network` reports them to the Issues panel from the
    // same in-memory network this stores.
    //
    // Runs still use the strict `parse`, so an unsimulable network cannot
    // reach the solver.
    let (network, _validation_errors) =
        hydra::io::parse_tolerant(&bytes).map_err(format_read_error)?;
    let dto = network_to_dto(&network);
    // Encode before taking the state lock — serialisation work happens
    // outside the mutex, and (unlike the old JSON path) no nodes/links clone
    // is needed to build the response.
    let encoded = encode_network_snapshot(&dto);
    // Draw the project's outline for the home page while its geometry is
    // already in hand. Once per open, and skipped entirely when the model
    // has not moved since the last drawing. A failure here is not a load
    // failure: the home page falls back to the engine's mark.
    super::sketch::refresh(&app_data, &project_id, &dto);
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

    // Filter label from the engine's own import format — a SWMM project's
    // save dialog must not say "EPANET Input File".
    let engine_key = project_engine_key(&app_data, &project_id);
    let (filter_label, filter_exts): (String, Vec<String>) = hydra::common::ENGINES
        .iter()
        .find(|e| e.key == engine_key)
        .and_then(|e| e.import.first())
        .map(|f| {
            (
                f.label.to_string(),
                f.extensions.iter().map(|x| (*x).to_string()).collect(),
            )
        })
        .unwrap_or_else(|| ("Model input file".to_string(), vec!["inp".to_string()]));

    // The dialog call blocks until the user answers — run it on the blocking
    // pool so it does not tie up an async runtime worker for that whole time.
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let exts: Vec<&str> = filter_exts.iter().map(String::as_str).collect();
        dialog_app
            .dialog()
            .file()
            .add_filter(filter_label, &exts)
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
    /// The platform this build runs on, as `os/arch` — `macos/aarch64`.
    ///
    /// Here because every bug report needs it and nothing in the app said
    /// it: the frontend can read a user agent, but that names the webview,
    /// which is not the same question as which binary is running.
    pub platform: String,
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
/// The element-kind catalog of one engine (hydra-common spec §4.1): every
/// kind it models, with the class it belongs to and the engine-authored
/// singular/plural labels and badge glyph.
///
/// Static per engine — a property of the domain, not of any model — so the
/// frontend may cache it and use it for chrome that must be correct before
/// a model is loaded (tab headings, legends, badges). Empty for an engine
/// with no catalog, which reads as "nothing to describe".
pub fn list_element_kinds(engine: String) -> &'static [hydra::common::ElementKind] {
    match engine.as_str() {
        "wds" => hydra::descriptors::ELEMENT_KINDS,
        "uds" => hydra::uds::descriptors::ELEMENT_KINDS,
        _ => &[],
    }
}

#[tauri::command]
/// Return the hydra engine and application version strings.
pub fn get_versions() -> Versions {
    Versions {
        hydra: HYDRA_VERSION,
        app: env!("CARGO_PKG_VERSION"),
        platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
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

    // ── Frontend registry mirror ─────────────────────────────────────────────

    /// The frontend keeps `FALLBACK_ENGINES`, a hand-written copy of the
    /// registry, for when it runs outside a Tauri shell (plain `vite` dev
    /// server) and `list_engines` is unreachable. Nothing makes the copy
    /// follow the original, so an engine renamed, recoloured, or flipped to
    /// available in Rust would silently keep its old identity in dev — the
    /// exact per-engine hardcoding the registry exists to abolish, reintroduced
    /// one layer up.
    ///
    /// Rust is the source of truth; this fails the build when the mirror drifts.
    /// The frontend's badge table (`types/elementTypes.ts`) duplicates the
    /// letters each engine declares in its §4.1 element-kind catalog — and
    /// had already drifted from it once, silently, because the frontend is
    /// what renders. The engine catalog is the source of truth; this fails
    /// the build when the mirror disagrees.
    ///
    /// Colours are deliberately not checked: they are presentation the
    /// contract does not describe, and belong to the frontend alone.
    /// SWMM states its coordinate basis in `[MAP] UNITS`, so a model on a
    /// local drawing grid must not be handed WGS84 — which would assert
    /// that a site grid in feet is longitude and latitude.
    #[test]
    fn map_units_then_coordinates_decide_a_new_project_crs() {
        let with_units = |u: &str| {
            format!("[TITLE]\nx\n[MAP]\nDIMENSIONS 0 0 100 100\nUnits {u}\n[JUNCTIONS]\nJ1 1 1\n")
        };
        for linear in ["Feet", "METERS", "none"] {
            assert_eq!(
                source_crs_for_model(with_units(linear).as_bytes()),
                LOCAL_CRS,
                "{linear:?} is a linear drawing grid, not a datum",
            );
        }
        assert_eq!(
            source_crs_for_model(with_units("Degrees").as_bytes()),
            "EPSG:4326",
        );
        // Nothing to read at all: a drawing grid, never a datum nobody
        // chose — a coordinate-less model has no earth placement.
        assert_eq!(
            source_crs_for_model(b"[TITLE]\nx\n[JUNCTIONS]\nJ1 1 1\n"),
            LOCAL_CRS,
        );
        // Undeclared units, coordinates that fit degrees: geographic.
        assert_eq!(
            source_crs_for_model(
                b"[JUNCTIONS]\nJ1 1 1\n[COORDINATES]\nJ1 -122.41 37.77\nJ2 -122.40 37.78\n"
            ),
            "EPSG:4326",
        );
        // Undeclared units, coordinates degrees cannot hold: a grid.
        assert_eq!(
            source_crs_for_model(
                b"[JUNCTIONS]\nJ1 1 1\n[COORDINATES]\nJ1 5500 8300\nJ2 -122.40 37.78\n"
            ),
            LOCAL_CRS,
        );
        // Placeholders alone say nothing: still a grid.
        assert_eq!(
            source_crs_for_model(b"[COORDINATES]\nJ1 0 0\nJ2 0 0\n"),
            LOCAL_CRS,
        );
        // A declaration outranks the numbers, in both directions.
        assert_eq!(
            source_crs_for_model(b"[MAP]\nUnits Degrees\n[COORDINATES]\nJ1 5500 8300\n"),
            "EPSG:4326",
        );
        // UNITS belongs to [MAP]; the same word elsewhere must not steal it.
        assert_eq!(
            source_crs_for_model(b"[OPTIONS]\nUNITS FEET\n[MAP]\nUnits Degrees\n"),
            "EPSG:4326",
        );
    }

    #[test]
    fn frontend_badges_mirror_the_engine_catalogs() {
        let ts = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/frontend/src/types/elementTypes.ts"
        ))
        .expect("frontend elementTypes.ts is readable");

        for kind in hydra::descriptors::ELEMENT_KINDS
            .iter()
            .chain(hydra::uds::descriptors::ELEMENT_KINDS)
        {
            // Only spatial kinds carry a row in the frontend table; the
            // non-spatial ones are listed by their own editors.
            if kind.class == hydra::common::ElementClass::Collection {
                continue;
            }
            let expected = format!("{}: {{ label: \"{}\"", kind.id, kind.badge);
            assert!(
                ts.contains(&expected),
                "frontend badge table is missing or disagrees with the engine \
                 catalog for {:?} (expected `{}`); update \
                 frontend/src/types/elementTypes.ts to match",
                kind.id,
                expected,
            );
        }
    }

    #[test]
    fn frontend_fallback_registry_mirrors_the_rust_registry() {
        let ts = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/frontend/src/hooks/engines.ts"
        ))
        .expect("frontend engines.ts is readable");
        // Bound the slice to the array literal: everything after it also
        // contains `key:` and would inflate the count guard below.
        let after = ts
            .split("export const FALLBACK_ENGINES")
            .nth(1)
            .expect("FALLBACK_ENGINES is declared");
        let end = after
            .find("\n];")
            .expect("FALLBACK_ENGINES array is closed");
        let fallback = &after[..end];

        // Slice the array into one block per engine, so a value can be
        // attributed to the engine that declares it. Checking the whole array
        // for `"planned"` would pass while the wrong engine carried it — with
        // three engines present the string is in the text either way.
        let block_for = |key: &str| -> String {
            let start = fallback
                .find(&format!("key: \"{key}\""))
                .unwrap_or_else(|| panic!("engine {key:?} is missing from FALLBACK_ENGINES"));
            let rest = &fallback[start + 1..];
            let end = rest
                .find("key: \"")
                .map_or(fallback.len(), |e| start + 1 + e);
            fallback[start..end].to_string()
        };

        for engine in hydra::common::ENGINES {
            let block = block_for(engine.key);
            let status = if engine.is_available() {
                "available"
            } else {
                "planned"
            };
            for (field, value) in [
                ("label", engine.label.to_string()),
                ("pill", engine.pill.to_string()),
                ("accent", engine.accent.to_string()),
                ("status", status.to_string()),
            ] {
                assert!(
                    block.contains(&format!("\"{value}\"")),
                    "engine {:?}: {field} should be {value:?} in FALLBACK_ENGINES \
                     (frontend/src/hooks/engines.ts) to match the Rust registry",
                    engine.key,
                );
            }
            // Summaries wrap across lines in the TS source, so compare on a
            // distinctive opening slice rather than the whole string.
            let head: String = engine
                .summary
                .split_whitespace()
                .take(4)
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                block.contains(&head),
                "engine {:?}: summary should start {head:?} in FALLBACK_ENGINES",
                engine.key,
            );
            for format in engine.import {
                for ext in format.extensions {
                    assert!(
                        block.contains(&format!("\"{ext}\"")),
                        "engine {:?}: import extension {ext:?} is missing from FALLBACK_ENGINES",
                        engine.key,
                    );
                }
            }
        }

        // Count guard: the loop above cannot notice an engine the mirror has
        // but the registry does not.
        let mirrored = fallback.matches("key:").count();
        assert_eq!(
            mirrored,
            hydra::common::ENGINES.len(),
            "FALLBACK_ENGINES lists {mirrored} engines, the registry has {}",
            hydra::common::ENGINES.len(),
        );
    }

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

    /// A created uds project must contain the imported SWMM text, never the
    /// Repair by omission (uds interop §14.10): a vendor-dialect option the
    /// predecessor would also refuse is commented out, reported, and the
    /// repaired text imports cleanly; anything else still refuses.
    #[test]
    fn uds_import_repairs_unknown_options_by_commenting_them_out() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\nDATA_STEP 00:05:00\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (text, network, repairs) =
            import_uds_text(model.to_string()).expect("repairable import");
        assert!(
            repairs
                .iter()
                .any(|r| r.contains("DATA_STEP") && r.contains("line 3")),
            "repair names the line and token: {repairs:?}"
        );
        // The original line survives behind a comment marker, and the
        // repaired text parses without refusals.
        assert!(text.contains("; [commented out by Hydra import] DATA_STEP 00:05:00"));
        assert_eq!(network.vertices.len(), 2);

        // A refusal that is NOT repairable (bad value for a real option)
        // still refuses — omission would change meaning.
        let bad = "[OPTIONS]\nFLOW_UNITS FURLONGS\n[JUNCTIONS]\nJ1 100 4\n";
        let err = import_uds_text(bad.to_string()).unwrap_err();
        assert!(err.contains("Cannot import this model"), "{err}");
    }

    /// A drainage model that names no elements to report runs fine and
    /// writes a results file with nothing in it but the system series —
    /// which opened here is a grey network, no values anywhere, and no
    /// stated reason. The importer widens the selection and says so.
    #[test]
    fn uds_import_selects_every_element_when_the_model_selects_none() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (text, network, repairs) = import_uds_text(model.to_string()).expect("import");
        assert!(
            repairs
                .iter()
                .any(|r| r.contains("all elements for reporting")),
            "the widening must be reported, never silent: {repairs:?}"
        );
        use hydra::uds::model::ReportSelection;
        assert_eq!(network.report.vertices, ReportSelection::All);
        assert_eq!(network.report.links, ReportSelection::All);
        assert_eq!(network.report.parcels, ReportSelection::All);
        // The author's own text is untouched; the request is appended.
        assert!(text.starts_with("[OPTIONS]"));
        assert!(text.contains("[added by Hydra import]"));
    }

    /// The coordinate reading the import wizard asks its question from.
    /// The same rule the canvas applies once a system is chosen, so the
    /// two cannot disagree about whether a model is in degrees.
    #[test]
    fn projected_coordinates_are_told_from_degrees() {
        // Longitude and latitude: nothing to ask about.
        assert!(!coordinates_are_projected(
            [(-1.55, 53.80), (-1.54, 53.81)].into_iter()
        ));
        // Eastings and northings: not degrees, whatever else they are.
        assert!(coordinates_are_projected(
            [(429000.0, 434000.0)].into_iter()
        ));
        // Latitude out of range alone is enough.
        assert!(coordinates_are_projected([(10.0, 95.0)].into_iter()));
    }

    /// The importer writes (0, 0) for an element with no coordinate, so a
    /// model with no geometry at all must not read as anything.
    #[test]
    fn placeholder_coordinates_say_nothing() {
        assert!(!coordinates_are_projected(
            [(0.0, 0.0), (0.0, 0.0)].into_iter()
        ));
        assert!(!coordinates_are_projected(std::iter::empty()));
        // And they do not mask a real projected point beside them.
        assert!(coordinates_are_projected(
            [(0.0, 0.0), (500000.0, 180000.0)].into_iter()
        ));
    }

    /// A drainage model's coordinates live in a preserved display section
    /// rather than the parsed model, and its import returns no elements at
    /// all — so this is the only place the question can be answered for it.
    #[test]
    fn a_drainage_model_is_read_from_its_coordinates_section() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [COORDINATES]\nJ1 429000 434000\nO1 429100 434100\n";
        let (_, network, _) = import_uds_text(model.to_string()).expect("import");
        assert!(coordinates_are_projected(
            super::super::uds_view::model_coordinates(&network)
        ));

        let degrees = model.replace(
            "[COORDINATES]\nJ1 429000 434000\nO1 429100 434100",
            "[COORDINATES]\nJ1 -1.55 53.80\nO1 -1.54 53.81",
        );
        let (_, network, _) = import_uds_text(degrees).expect("import");
        assert!(!coordinates_are_projected(
            super::super::uds_view::model_coordinates(&network)
        ));
    }

    /// A model that names some elements has made a choice. Widening it
    /// would be guessing at what it meant, so nothing is touched.
    #[test]
    fn uds_import_leaves_a_deliberate_report_selection_alone() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n\
                     [JUNCTIONS]\nJ1 100 4\n[OUTFALLS]\nO1 98 FREE\n\
                     [CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n\
                     [REPORT]\nNODES J1\n";
        let (text, network, repairs) = import_uds_text(model.to_string()).expect("import");
        assert!(repairs.is_empty(), "nothing to repair: {repairs:?}");
        assert!(!text.contains("[added by Hydra import]"));
        use hydra::uds::model::ReportSelection;
        assert_ne!(network.report.vertices, ReportSelection::None);
        assert_eq!(network.report.links, ReportSelection::None);
    }

    /// EPANET starter — the fall-through that once wrote the starter into a
    /// uds project silently discarded the user's model.
    #[test]
    fn a_uds_import_creates_the_project_from_the_imported_model() {
        let model = "[OPTIONS]\nFLOW_UNITS CFS\n[JUNCTIONS]\nJ1 100 4\n\
                     [OUTFALLS]\nO1 98 FREE\n[CONDUITS]\nC1 J1 O1 400 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1.5 0 0 0\n";
        let (network, diags) = hydra::uds::io::objects::parse_network(model);
        assert!(!diags.iter().any(|d| d.kind.is_error()));
        let mut guard = NetworkStateInner::LoadedUds {
            raw_text: model.to_string(),
            network: std::sync::Arc::new(network),
            aux_files: Vec::new(),
            owner_project_id: None,
            owner_scenario_id: None,
        };

        let (bytes, node_count, link_count, _) = new_project_model(true, &mut guard);
        assert_eq!(bytes, model.as_bytes(), "must persist the imported model");
        assert_eq!(node_count, 2, "J1 + O1");
        assert_eq!(link_count, 1, "C1");
        assert_ne!(bytes, STARTER_INP.to_vec());
    }

    #[test]
    fn only_a_gui_openable_engine_may_back_a_new_project() {
        assert_eq!(require_gui_openable_engine("wds").unwrap().key, "wds");
        // uds opens read-only: creatable from an import, viewable, runnable.
        assert_eq!(require_gui_openable_engine("uds").unwrap().key, "uds");
        assert!(engine_is_gui_editable("wds"));
        assert!(!engine_is_gui_editable("uds"));

        // Registered but not openable — planned engines. The wizard disables
        // these cards; this is the backstop for a caller that ignores the
        // card state.
        let err = require_gui_openable_engine("och").unwrap_err();
        assert!(
            err.contains("not available yet") || err.contains("not supported in the Hydra GUI"),
            "och rejection should say why it cannot back a project, got: {err}"
        );
        assert!(!err.contains("unknown engine"), "got: {err}");

        // Unknown: a different failure with a different remedy (upgrade).
        let err = require_gui_openable_engine("zzz").unwrap_err();
        assert!(err.contains("unknown engine"), "got: {err}");
        assert!(!err.contains("not available yet"), "got: {err}");
    }

    /// The wds Editor's rail is declared by hand rather than built from
    /// this catalog — each of its sections is a bespoke editable table, so
    /// there is nothing to derive a section from that would not still need
    /// a hand-written kind→component map beside it.
    ///
    /// The cost of declaring it is drift, and it has already been paid
    /// once: the rail called the Curves section "Pump curves" long after
    /// this catalog called it "Curves" and the curve payload distinguished
    /// tank-volume and valve-headloss curves from pump ones.
    ///
    /// So the claim is pinned on both sides. This half notices the engine
    /// changing; the frontend half — `editorRail.test.ts` — notices the
    /// rail changing, and asserts every kind listed here reaches exactly
    /// one section under its own label. Changing this list without
    /// changing that one fails the pair, which is the point.
    #[test]
    fn the_gui_editor_rail_mirrors_this_catalog() {
        let catalog: Vec<(&str, &str)> = list_element_kinds("wds".into())
            .iter()
            .map(|k| (k.id, k.label_plural))
            .collect();
        assert_eq!(
            catalog,
            vec![
                ("junction", "Junctions"),
                ("reservoir", "Reservoirs"),
                ("tank", "Tanks"),
                ("pipe", "Pipes"),
                ("pump", "Pumps"),
                ("valve", "Valves"),
                ("pattern", "Patterns"),
                ("curve", "Curves"),
                ("control", "Controls"),
                ("rule", "Rules"),
            ],
            "the wds catalog changed — update the Editor rail in \
             crates/gui/frontend/src/pages/project/NetworkEditor/editorRail.ts \
             and the CATALOG mirror in its test, then update this list"
        );
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
    fn a_project_asked_to_be_empty_never_adopts_the_loaded_network() {
        // The reported bug: open a project, return to the projects page, then
        // create an "empty" project — and it came back holding the previous
        // project's model. Managed state still held that network, and the
        // command inferred "a network is loaded" to mean "the user imported
        // one". The intent is now explicit, so a loaded network is ignored.
        const LOADED_INP: &str = "\
[JUNCTIONS]
J1  10  5

[RESERVOIRS]
R1  100

[PIPES]
P1  R1  J1  1000  12  100  0  Open

[COORDINATES]
J1  1.0  2.0
R1  0.0  0.0

[OPTIONS]
 Units      LPS
 Headloss   H-W

[END]
";
        let network = hydra::io::parse(LOADED_INP.as_bytes()).expect("fixture must parse");
        let dto = network_to_dto(&network);
        let mut loaded = NetworkStateInner::Loaded {
            raw_bytes: LOADED_INP.as_bytes().to_vec(),
            dirty: false,
            network: std::sync::Arc::new(network),
            dto,
            owner_project_id: Some("previously-open-project".into()),
            owner_scenario_id: None,
        };

        let (bytes, nodes, links, _) = new_project_model(false, &mut loaded);
        assert_eq!(
            bytes, STARTER_INP,
            "an empty project gets the starter model"
        );
        assert_eq!((nodes, links), (STARTER_NODE_COUNT, 0));

        // Counts must describe the bytes written: reporting the loaded
        // network's counts here would mark the project "ready" and make every
        // has-a-network check downstream disagree with the file on disk.
        let (bytes, nodes, links, _) = new_project_model(true, &mut loaded);
        assert_eq!(bytes, LOADED_INP.as_bytes(), "an import gets those bytes");
        assert_eq!((nodes, links), (2, 1));
    }

    #[test]
    fn importing_with_nothing_loaded_falls_back_to_the_starter_model() {
        let mut empty = NetworkStateInner::Empty;
        let (bytes, nodes, links, _) = new_project_model(true, &mut empty);
        assert_eq!(bytes, STARTER_INP);
        assert_eq!((nodes, links), (STARTER_NODE_COUNT, 0));
    }

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

    // ── import tolerance (model spec §4.1.2) ─────────────────────────────

    #[test]
    fn import_reads_a_model_that_is_not_yet_simulable() {
        // The parse `open_and_load_network` performs. A lone unreachable
        // junction is the resting state of a network under construction, and
        // the wizard is now the only way in — importing strictly meant the
        // file a user most needs to open in order to fix it was the one they
        // could not open.
        let inp = b"[JUNCTIONS]\n J1  10\n\n[OPTIONS]\n Units LPS\n\n[END]\n";
        let (network, errors) =
            hydra::io::parse_tolerant(inp).expect("an unsimulable model must still import");
        assert_eq!(network.nodes.len(), 1);
        assert!(
            !errors.is_empty(),
            "the reason it is unsimulable must be reported, not swallowed"
        );
    }

    #[test]
    fn import_still_refuses_a_model_that_cannot_be_read() {
        // The other side of the line: tolerance extends to networks that are
        // readable but incomplete, never to bytes no network can be built
        // from. A duplicated id makes every reference to it ambiguous, so
        // there is no well-defined network to be tolerant *with*.
        let dup = b"[JUNCTIONS]\n J1  10\n J1  20\n\n[RESERVOIRS]\n R1  100\n\n[PIPES]\n P1  R1  J1  100  300  100  0  Open\n\n[OPTIONS]\n Units LPS\n\n[END]\n";
        assert!(
            hydra::io::parse_tolerant(dup).is_err(),
            "a duplicate id must fail even the tolerant parse"
        );

        let swmm = b"[TITLE]\n\n[SUBCATCHMENTS]\n S1  RG1  J1  10  50  500  0.5  0\n\n[END]\n";
        assert!(
            hydra::io::parse_tolerant(swmm).is_err(),
            "another tool's dialect must fail even the tolerant parse"
        );
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
            unit_system: None,
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

/// What `get_model_unit_system` reports, and what the project override
/// persists — the two inputs the GUI's display-unit resolution reads.
#[cfg(test)]
mod unit_preference {
    use super::*;

    fn group_of(inp: &str) -> bool {
        let network = hydra::io::parse(inp.as_bytes()).expect("fixture parses");
        hydra::io::units::is_si(network.options.flow_units)
    }

    const MODEL: &str = "[JUNCTIONS]\nJ1 100 0\n\n[RESERVOIRS]\nR1 200\n\n\
                         [PIPES]\nP1 R1 J1 1000 12 100 0 Open\n\n\
                         [OPTIONS]\nUnits  ";

    /// Every named flow unit falls into one of the two groups the §5
    /// descriptors can express. This is the mapping `"source"` resolves
    /// through, so a variant landing in the wrong group would show a whole
    /// model in the wrong system.
    #[test]
    fn every_flow_unit_resolves_to_the_right_group() {
        for us in ["CFS", "GPM", "MGD", "IMGD", "AFD"] {
            assert!(
                !group_of(&format!("{MODEL}{us}\n")),
                "{us} is a US customary flow unit"
            );
        }
        for si in ["LPS", "LPM", "MLD", "CMH", "CMD", "CMS"] {
            assert!(
                group_of(&format!("{MODEL}{si}\n")),
                "{si} is an SI flow unit"
            );
        }
    }

    /// `None` clears the override rather than storing a value, because
    /// "follow the default" and "pin the value the default currently
    /// holds" must stay distinguishable — they diverge the moment the
    /// default changes.
    #[test]
    fn clearing_the_override_is_distinct_from_pinning_its_value() {
        let mut meta = meta::ProjectMeta {
            version: 1,
            name: "p".into(),
            engine: "wds".into(),
            source_crs: "EPSG:4326".into(),
            node_count: 0,
            link_count: 0,
            unit_system: None,
        };
        assert_eq!(meta.unit_system, None, "a new project inherits");

        meta.unit_system = Some("source".into());
        let pinned = serde_json::to_string(&meta).unwrap();
        assert!(pinned.contains("unitSystem"), "a pin is written");

        meta.unit_system = None;
        let inherited = serde_json::to_string(&meta).unwrap();
        assert!(
            !inherited.contains("unitSystem"),
            "inheriting is the absence of the field, not a value: {inherited}"
        );
    }
}
