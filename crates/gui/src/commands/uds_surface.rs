//! Surface results provider: the §14.16 sidecar served to the canvas.
//!
//! A mesh run's surface results live in `results.2d.out` beside
//! `results.out` (see `simulation::surface_results_path`). This module
//! serves them in three pieces sized to how the canvas consumes them: a
//! JSON meta with the engine's surface-variable catalog and sampled
//! ranges (once per scenario), the mesh geometry as a compact binary
//! payload (once per scenario — it can run to megabytes), and one
//! instant's cell values per timeline step. Values are SI (§14.16); the
//! frontend converts at the render boundary, the same discipline as
//! every other served result.
//!
//! "Engines describe, applications render": the variable ids, labels,
//! symbols, quantities and ramp hints all come from the engine's surface
//! catalog (`hydra::uds::descriptors::surface_variables`) — nothing is
//! invented here.
//!
//! # Geometry payload layout (version 1)
//!
//! Little-endian:
//!
//! ```text
//! u32 version   u32 n_vertices   u32 n_cells
//! f64 × 3 per vertex: x, y, z (m)
//! u32 × 3 per cell: vertex indices
//! ```
//!
//! # Period payload layout (version 1)
//!
//! Little-endian, columnar in the surface catalog's order:
//!
//! ```text
//! u32 version   u32 n_cells   f64 t (run seconds)
//! f32 × n_cells depth (m)
//! f32 × n_cells water surface elevation (m)
//! f32 × n_cells speed (m/s)
//! ```

use std::path::Path;

use serde::Serialize;

use hydra::swmm::out_reader::OverlandResults;

use super::generic_results::GenericVariableDto;
use super::projects::{
    app_data_dir, model_path_for, project_engine_key, results_path_for, validate_target_ids,
};
use super::simulation::surface_results_path;
use super::uds_results::quantity_descriptor;

/// Version stamped into the geometry payload header.
const SURFACE_GEOMETRY_VERSION: u32 = 1;
/// Version stamped into the period payload header.
const SURFACE_PERIOD_VERSION: u32 = 2;
/// Records sampled for the per-variable ranges. A record is the whole
/// surface, so unlike the network scan this is bounded by bytes read,
/// not periods visited.
const RANGE_SCAN_MAX_RECORDS: usize = 32;

/// What the frontend needs before it asks for geometry or values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceMetaDto {
    pub n_vertices: u32,
    pub n_cells: u32,
    pub periods: u32,
    pub report_step_s: f64,
    pub first_report_t_s: f64,
    /// The engine's surface catalog with this run's sampled SI ranges,
    /// in presentation order — the period payload's column order.
    pub variables: Vec<GenericVariableDto>,
}

/// Evenly spread sample indexes: all of `n` when it fits the budget.
fn sample_indexes(n: usize, budget: usize) -> Vec<usize> {
    if n <= budget {
        (0..n).collect()
    } else {
        (0..budget).map(|i| i * (n - 1) / (budget - 1)).collect()
    }
}

/// The meta for a sidecar on disk: counts, clock, and the catalog with
/// sampled ranges.
pub(crate) fn surface_meta_of(
    path: &Path,
    net: Option<&hydra::uds::model::Network>,
) -> Result<SurfaceMetaDto, String> {
    let r = OverlandResults::open(path)?;
    // hydra-common §6.3: pollutant series are named by the model, so the
    // catalog is the model's. The sidecar says how many series it holds,
    // and a model edited since the run may no longer declare the same
    // ones. Publishing the model's names against the file's columns would
    // then label one pollutant with another's name, so the mismatch
    // publishes the fixed catalog alone and the concentrations go unshown
    // until the model is run again. An absent variable is a state §6.2
    // already asks applications to handle; a mislabelled one is not.
    let vars: Vec<hydra::common::ModelVariable> = match net {
        Some(net) if net.constituents.len() == r.constituents => {
            hydra::uds::descriptors::surface_variables_for(net)
        }
        _ => hydra::uds::descriptors::surface_variables()
            .iter()
            .map(hydra::common::ModelVariable::from)
            .collect(),
    };
    // One accumulator per catalog variable, in catalog order.
    let fixed = hydra::uds::descriptors::surface_variables().len();
    let mut lo = vec![f64::INFINITY; vars.len()];
    let mut hi = vec![f64::NEG_INFINITY; vars.len()];
    for i in sample_indexes(r.periods, RANGE_SCAN_MAX_RECORDS) {
        let rec = r.record(i)?;
        for c in &rec.cells {
            let [depth, eta, u, v] = *c;
            let values = [
                f64::from(depth),
                f64::from(eta),
                f64::from(u).hypot(f64::from(v)),
            ];
            for (k, val) in values.into_iter().enumerate() {
                if val.is_finite() {
                    lo[k] = lo[k].min(val);
                    hi[k] = hi[k].max(val);
                }
            }
        }
        for (k, (conc, _)) in rec.constituents.iter().enumerate() {
            let Some(slot) = (fixed + k < vars.len()).then_some(fixed + k) else {
                break;
            };
            for c in conc {
                let val = f64::from(*c);
                if val.is_finite() {
                    lo[slot] = lo[slot].min(val);
                    hi[slot] = hi[slot].max(val);
                }
            }
        }
    }
    let variables = vars
        .iter()
        .enumerate()
        .map(|(k, v)| GenericVariableDto::from_model_variable(v, lo[k], hi[k], quantity_descriptor))
        .collect();
    Ok(SurfaceMetaDto {
        n_vertices: r.verts.len() as u32,
        n_cells: r.cells.len() as u32,
        periods: r.periods as u32,
        report_step_s: r.report_step,
        first_report_t_s: r.first_report_t,
        variables,
    })
}

/// The geometry payload (layout above), from whichever mesh the caller
/// holds. Written once so the two sources — the run's sidecar and the
/// model itself — cannot drift into two dialects of one layout.
fn encode_geometry(
    n_vertices: usize,
    n_cells: usize,
    verts: impl Iterator<Item = (f64, f64, f64)>,
    tris: impl Iterator<Item = [u32; 3]>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + 24 * n_vertices + 12 * n_cells);
    out.extend_from_slice(&SURFACE_GEOMETRY_VERSION.to_le_bytes());
    out.extend_from_slice(&(n_vertices as u32).to_le_bytes());
    out.extend_from_slice(&(n_cells as u32).to_le_bytes());
    for (x, y, z) in verts {
        for f in [x, y, z] {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    for c in tris {
        for i in c {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }
    out
}

/// The geometry payload for a sidecar on disk (layout above).
pub(crate) fn surface_geometry_of(path: &Path) -> Result<Vec<u8>, String> {
    let r = OverlandResults::open(path)?;
    Ok(encode_geometry(
        r.verts.len(),
        r.cells.len(),
        r.verts.iter().copied(),
        r.cells.iter().copied(),
    ))
}

// ── The model's own mesh ──────────────────────────────────────────────────────
//
// A mesh is a property of the model, present from import; the sidecar
// carries a copy only so a viewer can render a run without one. In the
// app the model is already in memory, so the canvas takes its geometry
// from here — which is what lets a mesh model show its surface before it
// has ever been run.

/// What the app needs to know a model carries a surface at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshInfoDto {
    pub n_vertices: u32,
    pub n_cells: u32,
    /// The engine's mesh-property catalog with this mesh's own ranges —
    /// the ground it sits on, which a reader can be shown before there
    /// is any run to colour it with.
    pub properties: Vec<GenericVariableDto>,
}

/// The loaded model's mesh counts and properties, or `None` when it
/// carries no mesh (every water model, and every drainage model without
/// 2D sections).
pub(crate) fn mesh_info_of(net: &hydra::uds::model::Network) -> Option<MeshInfoDto> {
    let mesh = net.overland.as_ref()?;
    // The bed's range over the mesh's own vertices (§15.2), so the ramp
    // spans the terrain that is there rather than an assumed datum.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in &mesh.verts {
        if v.z.is_finite() {
            lo = lo.min(v.z);
            hi = hi.max(v.z);
        }
    }
    let properties = hydra::uds::descriptors::surface_properties()
        .iter()
        .map(|v| GenericVariableDto::from_descriptor(v, lo, hi, quantity_descriptor))
        .collect();
    Some(MeshInfoDto {
        n_vertices: mesh.verts.len() as u32,
        n_cells: mesh.cells.len() as u32,
        properties,
    })
}

/// The loaded model's mesh as a geometry payload (layout above) — the
/// same bytes the sidecar path serves, from the model instead.
pub(crate) fn mesh_geometry_of(net: &hydra::uds::model::Network) -> Vec<u8> {
    let Some(mesh) = net.overland.as_ref() else {
        return encode_geometry(0, 0, std::iter::empty(), std::iter::empty());
    };
    encode_geometry(
        mesh.verts.len(),
        mesh.cells.len(),
        mesh.verts.iter().map(|v| (v.x, v.y, v.z)),
        mesh.cells.iter().map(|c| c.v),
    )
}

/// One instant's cell values (layout above).
pub(crate) fn surface_period_of(path: &Path, period: usize) -> Result<Vec<u8>, String> {
    let r = OverlandResults::open(path)?;
    Ok(encode_surface_period(&r.record(period)?))
}

/// The period payload for one record (layout above).
///
/// Separate from reading a file so a fixture can be encoded from a
/// record written here rather than from a run: results are bit-identical
/// on one platform and not across, so bytes taken from a simulation
/// would be a fixture only the machine that made it could match.
fn encode_surface_period(rec: &hydra::swmm::out_reader::OverlandRecord) -> Vec<u8> {
    let nc = rec.cells.len();
    let mut out = Vec::with_capacity(16 + 4 * nc * (3 + rec.constituents.len()));
    out.extend_from_slice(&SURFACE_PERIOD_VERSION.to_le_bytes());
    out.extend_from_slice(&(nc as u32).to_le_bytes());
    out.extend_from_slice(&rec.t.to_le_bytes());
    for c in &rec.cells {
        out.extend_from_slice(&c[0].to_le_bytes());
    }
    for c in &rec.cells {
        out.extend_from_slice(&c[1].to_le_bytes());
    }
    for c in &rec.cells {
        out.extend_from_slice(&c[2].hypot(c[3]).to_le_bytes());
    }
    // §15.11 concentrations, one column per constituent, in the order the
    // per-model catalog publishes them. The column count is not stated in
    // the header: the payload is a whole number of equal columns, so a
    // decoder divides, and one that expected three refuses the length
    // rather than reading a concentration as a depth.
    for (conc, _) in &rec.constituents {
        for c in conc {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

/// The target's sidecar path, or `None` when it has none (never run,
/// not a mesh model, or a wds project — none of which write one).
fn sidecar_for(
    app: &tauri::AppHandle,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<Option<std::path::PathBuf>, String> {
    validate_target_ids(project_id, scenario_id)?;
    let app_data = app_data_dir(app)?;
    let path = surface_results_path(&results_path_for(&app_data, project_id, scenario_id));
    Ok(path.exists().then_some(path))
}

/// Surface result metadata for a project or scenario, or `None` when the
/// target has no surface results — the normal state for every non-mesh
/// run, not an error.
#[tauri::command(async)]
pub fn load_surface_meta(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::network_dto::NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<SurfaceMetaDto>, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => {
            let net = mesh_network_for(&app, &state, &project_id, scenario_id.as_deref())?;
            surface_meta_of(&path, net.as_deref()).map(Some)
        }
        None => Ok(None),
    }
}

/// The mesh geometry payload. Ask only after `load_surface_meta` said the
/// target has surface results; a missing sidecar is an error here.
///
/// Returned as `tauri::ipc::Response` like every binary command: a bare
/// `Vec<u8>` serialises as a JSON number array, which reaches the
/// frontend as an `Array` where its decoder expects an `ArrayBuffer` —
/// the payload survives, the type does not.
#[tauri::command(async)]
pub fn load_surface_geometry(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => surface_geometry_of(&path).map(tauri::ipc::Response::new),
        None => Err("this target has no surface results".into()),
    }
}

/// One instant's surface values, by period index (binary, see above).
#[tauri::command(async)]
pub fn load_surface_period(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
    period: u32,
) -> Result<tauri::ipc::Response, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => surface_period_of(&path, period as usize).map(tauri::ipc::Response::new),
        None => Err("this target has no surface results".into()),
    }
}

/// The drainage model a mesh question is about.
///
/// Named, and target-addressed, because the two commands below used to
/// answer from whichever network the backend happened to hold, with no
/// way to say which model that was. The canvas asks the moment the active
/// project changes — before that project's network has finished
/// loading — so through the whole of a project switch both commands
/// described the *previous* project's mesh. The canvas drew it, and worse,
/// framed its camera around its extent, so switching projects left the new
/// network fitted to a mesh belonging to a model no longer on screen.
///
/// `None` is the ordinary answer for a water project, a project with no
/// model imported yet, and a drainage model without 2D sections.
fn mesh_network_for(
    app: &tauri::AppHandle,
    state: &super::network_dto::NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<Option<std::sync::Arc<hydra::uds::model::Network>>, String> {
    mesh_network_at(&app_data_dir(app)?, state, project_id, scenario_id)
}

/// The decision itself, with the app handle resolved away so it can be
/// asked in a test.
fn mesh_network_at(
    app_data: &Path,
    state: &super::network_dto::NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
) -> Result<Option<std::sync::Arc<hydra::uds::model::Network>>, String> {
    validate_target_ids(project_id, scenario_id)?;
    // Only a drainage model can carry a mesh, and reading a water model
    // with the drainage parser to discover that would be nonsense.
    if project_engine_key(app_data, project_id) != "uds" {
        return Ok(None);
    }
    // A project created but never imported into has no model to have a
    // mesh — an ordinary state, not a failure.
    if !model_path_for(app_data, project_id, scenario_id).is_file() {
        return Ok(None);
    }
    // Serves the loaded network when it owns this target and reads the
    // model from disk when it does not, which is exactly the window a
    // project switch opens.
    super::results::uds_network_for_target(app_data, state, project_id, scenario_id).map(Some)
}

/// Whether this target's model carries a 2D surface, and how big it is.
///
/// Answered from the model, so it is true from import — before any run,
/// and for a model that will never be run. `None` for a model with no
/// mesh, which is every water model and most drainage ones.
#[tauri::command(async)]
pub fn load_mesh_info(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::network_dto::NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<MeshInfoDto>, String> {
    Ok(
        mesh_network_for(&app, &state, &project_id, scenario_id.as_deref())?
            .as_deref()
            .and_then(mesh_info_of),
    )
}

/// This target's mesh geometry (binary, see `load_surface_geometry`).
/// Empty counts when the model carries no mesh.
#[tauri::command(async)]
pub fn load_mesh_geometry(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::network_dto::NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = match mesh_network_for(&app, &state, &project_id, scenario_id.as_deref())? {
        Some(net) => mesh_geometry_of(&net),
        None => encode_geometry(0, 0, std::iter::empty(), std::iter::empty()),
    };
    Ok(tauri::ipc::Response::new(bytes))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A finished mesh run's sidecar, produced through the same dialect
    /// doors the run queue uses.
    fn sidecar(dir: &Path) -> std::path::PathBuf {
        let model = "[OPTIONS]\nFLOW_UNITS CMS\nFLOW_ROUTING DYNWAVE\n\
                     START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
                     END_DATE 01/01/2024\nEND_TIME 00:10:00\nREPORT_STEP 00:05:00\n\
                     [2D_VERTICES]\n0 0 10.0\n1 0 10.2\n1 1 10.4\n0 1 10.6\n\
                     [2D_TRIANGLES]\n0 1 2 0.02 0.05\n0 2 3 0.03 0.05\n\
                     [2D_VERTEX_NODE_MAP]\n0 J1\n\
                     [JUNCTIONS]\nJ1 9 4 0 0 0\n[OUTFALLS]\nO1 8 FREE\n\
                     [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
                     [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n";
        let (mut sim, _, _) = hydra::swmm::session::open(model).expect("open");
        let path = dir.join("results.2d.out");
        let sink = Box::new(std::fs::File::create(&path).expect("create"));
        hydra::swmm::session::begin_overland_results(&mut sim, sink).expect("begin");
        sim.run();
        sim.finish_results().expect("finish");
        path
    }

    /// The same little mesh model, declaring a pollutant, so its run
    /// writes §15.11 concentration columns.
    const POLLUTED_MODEL: &str = "[OPTIONS]\nFLOW_UNITS CMS\nFLOW_ROUTING DYNWAVE\n\
         START_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
         END_DATE 01/01/2024\nEND_TIME 00:10:00\nREPORT_STEP 00:05:00\n\
         [POLLUTANTS]\nTSS MG/L 0 0 0 0 NO\n\
         [2D_VERTICES]\n0 0 10.0\n1 0 10.2\n1 1 10.4\n0 1 10.6\n\
         [2D_TRIANGLES]\n0 1 2 0.02 0.05\n0 2 3 0.03 0.05\n\
         [2D_VERTEX_NODE_MAP]\n0 J1\n\
         [JUNCTIONS]\nJ1 9 4 0 0 0\n[OUTFALLS]\nO1 8 FREE\n\
         [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
         [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n";

    fn polluted_sidecar(dir: &Path) -> std::path::PathBuf {
        let (mut sim, _, _) = hydra::swmm::session::open(POLLUTED_MODEL).expect("open");
        let path = dir.join("polluted.2d.out");
        let sink = Box::new(std::fs::File::create(&path).expect("create"));
        hydra::swmm::session::begin_overland_results(&mut sim, sink).expect("begin");
        sim.run();
        sim.finish_results().expect("finish");
        path
    }

    /// hydra-common §6.3: a pollutant's series is named by the model, so
    /// the catalog the meta publishes is the model's, and it carries the
    /// quantity that pollutant's own declared units name.
    #[test]
    fn a_declared_pollutant_becomes_a_surface_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = polluted_sidecar(dir.path());
        let (net, _) = hydra::swmm::objects::parse_network(POLLUTED_MODEL);
        let meta = surface_meta_of(&path, Some(&net)).expect("meta");
        let ids: Vec<&str> = meta.variables.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["depth", "elevation", "speed", "pollutant:TSS"]);
        let tss = meta.variables.last().expect("the pollutant series");
        assert_eq!(tss.label, "TSS");
        assert_eq!(
            tss.quantity.as_ref().map(|q| q.si_label),
            Some("mg/L"),
            "the legend must say what the concentration is in"
        );
        // The payload carries a column for it, in catalog order.
        let bytes = surface_period_of(&path, 0).expect("period");
        let nc = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 16 + 4 * nc * meta.variables.len());
    }

    /// The sidecar's column count is the file's, the names are the
    /// model's, and an edit between the run and the reading can part
    /// them. The concentrations then go unshown rather than being
    /// labelled with the wrong pollutant's name.
    #[test]
    fn a_model_edited_since_the_run_publishes_no_pollutant_series() {
        let dir = tempfile::tempdir().unwrap();
        let path = polluted_sidecar(dir.path());
        let fixed = ["depth", "elevation", "speed"];

        // The model gained a second pollutant after the run.
        let (mut net, _) = hydra::swmm::objects::parse_network(POLLUTED_MODEL);
        let extra = net.constituents[0].clone();
        net.constituents.push(extra);
        let meta = surface_meta_of(&path, Some(&net)).expect("meta");
        let ids: Vec<&str> = meta.variables.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, fixed, "a changed pollutant list publishes neither");

        // And with no model to ask at all.
        let meta = surface_meta_of(&path, None).expect("meta");
        let ids: Vec<&str> = meta.variables.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, fixed);
    }

    /// The record the fixture and the frontend both describe. Written
    /// here rather than taken from a run, so the bytes are the same on
    /// every platform.
    fn fixture_record() -> hydra::swmm::out_reader::OverlandRecord {
        hydra::swmm::out_reader::OverlandRecord {
            t: 300.0,
            cells: vec![[0.5, 10.5, 0.3, 0.4], [0.0, 10.0, 0.0, 0.0]],
            exchange: vec![0.25],
            ledger: Default::default(),
            constituents: vec![(vec![2.0, 0.0], [0.0; 11])],
        }
    }

    /// The frontend decodes this payload, and nothing in either language
    /// can check the other's idea of the layout. Only a file both read
    /// fails when they diverge — the counterpart is in
    /// `hooks/surface.test.ts`. Regenerate with
    /// `UPDATE_SNAPSHOT_FIXTURE=1 cargo test -p hydra-gui fixture`.
    #[test]
    fn surface_period_fixture_is_the_bytes_the_frontend_decodes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/surface-period.bin");
        let encoded = encode_surface_period(&fixture_record());

        if std::env::var_os("UPDATE_SNAPSHOT_FIXTURE").is_some() {
            std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create dir");
            std::fs::write(&path, &encoded).expect("write fixture");
            return;
        }
        let stored = std::fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "cannot read {}: {e}. Regenerate with UPDATE_SNAPSHOT_FIXTURE=1.",
                path.display()
            )
        });
        assert!(
            stored == encoded,
            "surface-period bytes changed ({} stored, {} encoded); regenerate the \
             fixture and update the frontend decoder to match",
            stored.len(),
            encoded.len()
        );
    }

    #[test]
    fn meta_carries_the_engine_catalog_with_sampled_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let meta = surface_meta_of(&sidecar(dir.path()), None).expect("meta");
        assert_eq!(meta.n_vertices, 4);
        assert_eq!(meta.n_cells, 2);
        assert!(meta.periods > 0);
        assert!((meta.report_step_s - 300.0).abs() < 1e-9);
        let ids: Vec<&str> = meta.variables.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["depth", "elevation", "speed"], "catalog order");
        let depth = &meta.variables[0];
        assert!(depth.max > 0.0, "the wet mesh has depth somewhere");
        assert!(depth.quantity.is_some(), "quantity resolved for display");
        // Elevation sits at the terrain's scale, never collapsed to 0..0.
        assert!(meta.variables[1].max >= 10.0);
    }

    #[test]
    fn geometry_payload_round_trips_the_mesh() {
        let dir = tempfile::tempdir().unwrap();
        let path = sidecar(dir.path());
        let bytes = surface_geometry_of(&path).expect("geometry");
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f64_at = |o: usize| f64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
        assert_eq!(u32_at(0), SURFACE_GEOMETRY_VERSION);
        let (nv, nc) = (u32_at(4) as usize, u32_at(8) as usize);
        assert_eq!((nv, nc), (4, 2));
        assert_eq!(bytes.len(), 12 + 24 * nv + 12 * nc);
        // First vertex (0, 0, 10.0); first cell (0, 1, 2).
        assert_eq!(f64_at(12), 0.0);
        assert_eq!(f64_at(28), 10.0);
        let cells_at = 12 + 24 * nv;
        assert_eq!(
            [u32_at(cells_at), u32_at(cells_at + 4), u32_at(cells_at + 8)],
            [0, 1, 2]
        );
    }

    #[test]
    fn period_payload_columns_match_the_reader_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = sidecar(dir.path());
        let rec = OverlandResults::open(&path)
            .expect("open")
            .record(0)
            .expect("record");
        let bytes = surface_period_of(&path, 0).expect("period");
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        assert_eq!(u32_at(0), SURFACE_PERIOD_VERSION);
        let nc = u32_at(4) as usize;
        assert_eq!(nc, rec.cells.len());
        assert_eq!(f64::from_le_bytes(bytes[8..16].try_into().unwrap()), rec.t);
        assert_eq!(bytes.len(), 16 + 12 * nc);
        for (ci, c) in rec.cells.iter().enumerate() {
            assert_eq!(f32_at(16 + 4 * ci), c[0], "depth column");
            assert_eq!(f32_at(16 + 4 * (nc + ci)), c[1], "elevation column");
            assert_eq!(
                f32_at(16 + 4 * (2 * nc + ci)),
                c[2].hypot(c[3]),
                "speed column"
            );
        }
        // Out of range is a named refusal, not a panic.
        let r = OverlandResults::open(&path).expect("open");
        assert!(surface_period_of(&path, r.periods).is_err());
    }

    // ── The model's own mesh ──────────────────────────────────────────

    const MESH_MODEL: &str = "[OPTIONS]\nFLOW_UNITS CMS\n\
         [2D_VERTICES]\n0 0 10.0\n1 0 10.2\n1 1 10.4\n0 1 10.6\n\
         [2D_TRIANGLES]\n0 1 2 0.02\n0 2 3 0.03\n\
         [JUNCTIONS]\nJ1 9 4 0 0 0\n[OUTFALLS]\nO1 8 FREE\n\
         [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
         [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n";

    const FLAT_MODEL: &str = "[OPTIONS]\nFLOW_UNITS CMS\n\
         [JUNCTIONS]\nJ1 9 4 0 0 0\n[OUTFALLS]\nO1 8 FREE\n\
         [CONDUITS]\nC1 J1 O1 100 0.013 0 0\n\
         [XSECTIONS]\nC1 CIRCULAR 1 0 0 0\n";

    /// A project bundle on disk: meta naming the engine, and a model.
    fn project_on_disk(app_data: &Path, project_id: &str, engine: &str, model: &str) {
        let dir = crate::meta::bundle::project_dir(app_data, project_id);
        std::fs::create_dir_all(&dir).unwrap();
        crate::meta::write_project_meta(
            &dir,
            &crate::meta::ProjectMeta {
                version: 1,
                name: project_id.to_string(),
                engine: engine.to_string(),
                source_crs: "LOCAL".into(),
                node_count: 0,
                link_count: 0,
                unit_system: None,
            },
        )
        .unwrap();
        crate::meta::bundle::atomic_write(
            &super::model_path_for(app_data, project_id, None),
            model.as_bytes(),
        )
        .unwrap();
    }

    /// Project ids are validated as UUIDs before anything else happens,
    /// so a test's projects need real ones.
    const FLAT_ID: &str = "11111111-1111-4111-8111-111111111111";
    const MESH_ID: &str = "22222222-2222-4222-8222-222222222222";
    const WDS_ID: &str = "33333333-3333-4333-8333-333333333333";
    const EMPTY_ID: &str = "44444444-4444-4444-8444-444444444444";

    /// A drainage network held in the state, owned by `owner`.
    fn loaded_uds(owner: &str, model: &str) -> super::super::network_dto::NetworkState {
        let (net, _) = hydra::swmm::objects::parse_network(model);
        super::super::network_dto::NetworkState(parking_lot::Mutex::new(
            super::super::network_dto::NetworkStateInner::LoadedUds {
                raw_text: model.to_string(),
                dirty: false,
                network: std::sync::Arc::new(net),
                aux_files: Vec::new(),
                owner_project_id: Some(owner.to_string()),
                owner_scenario_id: None,
            },
        ))
    }

    /// The defect this addresses: both mesh commands used to answer from
    /// whatever network the backend held, so through a project switch —
    /// the canvas asks the moment the active project changes, before that
    /// project's network has loaded — they described the project being
    /// left. The canvas drew that mesh and fitted its camera to the
    /// union of it and the new network, which is what "switching projects
    /// does not fit the network" was.
    #[test]
    fn a_mesh_question_is_answered_about_the_project_it_names() {
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path();
        // Two drainage projects: the one asked about has no mesh, the one
        // loaded in the state has one.
        project_on_disk(app_data, FLAT_ID, "uds", FLAT_MODEL);
        let state = loaded_uds(MESH_ID, MESH_MODEL);

        let net = mesh_network_at(app_data, &state, FLAT_ID, None)
            .expect("a readable model is not an error")
            .expect("the project has a model");
        assert!(
            mesh_info_of(&net).is_none(),
            "the answer must describe flat-project, not whatever is loaded"
        );

        // And the loaded network is still what serves its own target.
        project_on_disk(app_data, MESH_ID, "uds", MESH_MODEL);
        let net = mesh_network_at(app_data, &state, MESH_ID, None)
            .unwrap()
            .unwrap();
        assert_eq!(mesh_info_of(&net).map(|m| m.n_cells), Some(2));
    }

    #[test]
    fn a_water_project_is_not_read_with_the_drainage_parser() {
        let dir = tempfile::tempdir().unwrap();
        project_on_disk(
            dir.path(),
            WDS_ID,
            "wds",
            "[JUNCTIONS]
 J1 100
",
        );
        let state = loaded_uds(MESH_ID, MESH_MODEL);
        assert!(
            mesh_network_at(dir.path(), &state, WDS_ID, None)
                .unwrap()
                .is_none(),
            "no water model can carry a mesh"
        );
    }

    #[test]
    fn a_project_with_no_model_yet_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        project_on_disk(dir.path(), EMPTY_ID, "uds", MESH_MODEL);
        std::fs::remove_file(super::model_path_for(dir.path(), EMPTY_ID, None)).unwrap();
        let state = loaded_uds(MESH_ID, MESH_MODEL);
        assert!(
            mesh_network_at(dir.path(), &state, EMPTY_ID, None)
                .unwrap()
                .is_none(),
            "a project created but never imported into has nothing to describe"
        );
    }

    /// A mesh is known from the model, before any run — the fact the app
    /// needs to say "this model is 2D" on a project nobody has simulated.
    #[test]
    fn a_models_mesh_is_known_without_running_it() {
        let (net, _) = hydra::swmm::objects::parse_network(MESH_MODEL);
        let info = mesh_info_of(&net).expect("the model carries a mesh");
        assert_eq!((info.n_vertices, info.n_cells), (4, 2));
        // The ground comes with the mesh, ranged over the mesh's own
        // bed: a reader can be shown the terrain with no run at all.
        let ground = info
            .properties
            .iter()
            .find(|p| p.id == "ground")
            .expect("the mesh publishes its ground");
        assert!((ground.min - 10.0).abs() < 1e-9, "{}", ground.min);
        assert!((ground.max - 10.6).abs() < 1e-9, "{}", ground.max);
        assert!(ground.quantity.is_some(), "resolved for display");

        let (flat, _) = hydra::swmm::objects::parse_network(FLAT_MODEL);
        assert!(
            mesh_info_of(&flat).is_none(),
            "a model without 2D sections carries no surface"
        );
    }

    /// Both sources write one layout: the model's payload decodes by the
    /// same rules as the sidecar's, and says the same thing about the
    /// same mesh.
    #[test]
    fn the_model_and_the_sidecar_encode_one_geometry_layout() {
        let (net, _) = hydra::swmm::objects::parse_network(MESH_MODEL);
        let from_model = mesh_geometry_of(&net);
        let dir = tempfile::tempdir().unwrap();
        let from_run = surface_geometry_of(&sidecar(dir.path())).expect("geometry");
        // Same mesh, same bytes — the sidecar's copy is the model's mesh.
        assert_eq!(from_model, from_run);

        // A model with no mesh answers in the same layout, empty.
        let (flat, _) = hydra::swmm::objects::parse_network(FLAT_MODEL);
        let empty = mesh_geometry_of(&flat);
        assert_eq!(empty.len(), 12);
        assert_eq!(
            u32::from_le_bytes(empty[0..4].try_into().unwrap()),
            SURFACE_GEOMETRY_VERSION
        );
        assert_eq!(u32::from_le_bytes(empty[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn sampling_spreads_across_the_run_and_takes_all_when_small() {
        assert_eq!(sample_indexes(3, 32), vec![0, 1, 2]);
        let s = sample_indexes(1000, 32);
        assert_eq!(s.len(), 32);
        assert_eq!(s[0], 0);
        assert_eq!(*s.last().unwrap(), 999);
        assert!(s.windows(2).all(|w| w[0] < w[1]), "strictly increasing");
    }
}
