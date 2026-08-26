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
use super::projects::{app_data_dir, results_path_for, validate_target_ids};
use super::simulation::surface_results_path;
use super::uds_results::quantity_descriptor;

/// Version stamped into the geometry payload header.
const SURFACE_GEOMETRY_VERSION: u32 = 1;
/// Version stamped into the period payload header.
const SURFACE_PERIOD_VERSION: u32 = 1;
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
pub(crate) fn surface_meta_of(path: &Path) -> Result<SurfaceMetaDto, String> {
    let r = OverlandResults::open(path)?;
    // One accumulator per catalog variable, in catalog order.
    let vars = hydra::uds::descriptors::surface_variables();
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
    }
    let variables = vars
        .iter()
        .enumerate()
        .map(|(k, v)| GenericVariableDto::from_descriptor(v, lo[k], hi[k], quantity_descriptor))
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

/// The geometry payload for a sidecar on disk (layout above).
pub(crate) fn surface_geometry_of(path: &Path) -> Result<Vec<u8>, String> {
    let r = OverlandResults::open(path)?;
    let mut out = Vec::with_capacity(12 + 24 * r.verts.len() + 12 * r.cells.len());
    out.extend_from_slice(&SURFACE_GEOMETRY_VERSION.to_le_bytes());
    out.extend_from_slice(&(r.verts.len() as u32).to_le_bytes());
    out.extend_from_slice(&(r.cells.len() as u32).to_le_bytes());
    for (x, y, z) in &r.verts {
        for f in [x, y, z] {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    for c in &r.cells {
        for i in c {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }
    Ok(out)
}

/// One instant's cell values (layout above).
pub(crate) fn surface_period_of(path: &Path, period: usize) -> Result<Vec<u8>, String> {
    let r = OverlandResults::open(path)?;
    let rec = r.record(period)?;
    let nc = rec.cells.len();
    let mut out = Vec::with_capacity(16 + 12 * nc);
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
    Ok(out)
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
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Option<SurfaceMetaDto>, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => surface_meta_of(&path).map(Some),
        None => Ok(None),
    }
}

/// The mesh geometry payload. Ask only after `load_surface_meta` said the
/// target has surface results; a missing sidecar is an error here.
#[tauri::command(async)]
pub fn load_surface_geometry(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<u8>, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => surface_geometry_of(&path),
        None => Err("this target has no surface results".into()),
    }
}

/// One instant's surface values, by period index.
#[tauri::command(async)]
pub fn load_surface_period(
    app: tauri::AppHandle,
    project_id: String,
    scenario_id: Option<String>,
    period: u32,
) -> Result<Vec<u8>, String> {
    match sidecar_for(&app, &project_id, scenario_id.as_deref())? {
        Some(path) => surface_period_of(&path, period as usize),
        None => Err("this target has no surface results".into()),
    }
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

    #[test]
    fn meta_carries_the_engine_catalog_with_sampled_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let meta = surface_meta_of(&sidecar(dir.path())).expect("meta");
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
