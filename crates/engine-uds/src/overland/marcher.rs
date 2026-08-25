//! §15.4: the explicit local-inertial marcher.
//!
//! Faces fire first, cells second, boundaries at the owning cell's
//! firing — every update a pure function of the last published state,
//! every reduction folded in fixed order, so the march is deterministic
//! and, when the §6.4 team takes the phases (a later slice),
//! byte-identical at any width. This slice runs every cell on the global
//! step; the power-of-two tiers of §15.4.4 layer on top without changing
//! a value the single-tier march produces at the same steps.

use super::closure::{face_depth_mean, face_depth_vfr, flat_eta, vfr_eta, CellBed};
use super::coupling::{exchange_conductance, exchange_q};
use super::{
    BoundaryCondition, CellClosure, CouplingRow, FaceReconstruction, OverlandMesh, SeriesOrValue,
    Topology,
};

/// §15.1: standard gravity (m/s²).
const G: f64 = 9.80665;

/// §15.4.2: closure round-off must not masquerade as slope (m).
const ETA_DEADBAND: f64 = 1e-12;

/// §15.4.2: the exporting cell's per-face volume share β.
const BETA: f64 = 0.8;

/// §15.4.5: elements per worker below which a phase runs serial — a
/// dispatch costs more than a small map saves, and byte-identity makes
/// the two paths interchangeable.
#[cfg(feature = "threads")]
const PAR_GRAIN: usize = 256;

/// One interior face: a single prognostic discharge shared by two cells,
/// oriented from the lower-indexed cell to the higher.
#[derive(Debug, Clone)]
struct Face {
    cl: u32,
    cr: u32,
    /// Planimetric length ξ (m).
    xi: f64,
    /// Unit normal, oriented cl → cr.
    nx: f64,
    ny: f64,
    /// Face bed: max of the two centroid beds (m).
    z_face: f64,
    /// Sorted endpoint beds (m).
    z_lo: f64,
    z_hi: f64,
    /// Edge midpoint (m), for the Perot offset (§15.4.3).
    mx: f64,
    my: f64,
    /// Inverse normal distance 1/dₙ (1/m).
    inv_dn: f64,
    /// Conveyance ψ.
    psi: f64,
    /// Mean Manning n squared.
    n2: f64,
}

/// One boundary slot with a non-wall condition, evaluated serially at
/// tier-0 cadence (§15.5).
#[derive(Debug, Clone)]
struct Boundary {
    cell: u32,
    xi: f64,
    /// The §15.5 law, with its authored constant. Series and curve
    /// parameters resolve to per-advance constants at the wiring layer;
    /// the marcher sees only values.
    law: BoundaryLaw,
    /// Edge midpoint, for the Perot completion (§15.5).
    mx: f64,
    my: f64,
    /// Ghost-arm inverse distance for the stage law: 3ξ/(2A).
    inv_dn_ghost: f64,
    /// Prognostic discharge of the stage law's inertial arm (m²/s,
    /// inflow positive).
    q: f64,
    /// The edge's sill (higher endpoint elevation, m), the rating law's
    /// datum (§15.5).
    z_sill: f64,
    /// §15.5: a series- or curve-driven slot is a wall until its first
    /// resolution.
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BoundaryLaw {
    /// §15.5 `RATING_CURVE`: per-metre discharge from a curve of stage
    /// above the edge sill, re-read every firing.
    Rating { curve: u32 },
    /// Manning outfall at the authored bed slope.
    NormalFlow { slope: f64 },
    /// Held per-metre discharge (m³/s per m, authored outward positive).
    Flow { q_per_m: f64 },
    /// Held stage (m).
    Stage { eta: f64 },
}

/// The marcher: geometry frozen at build, state advanced by
/// [`Marcher::advance`].
pub struct Marcher {
    // ── Geometry ────────────────────────────────────────────────────
    /// Cell planimetric areas (m²) — public for §15.6 footprint sums.
    pub area: Vec<f64>,
    /// Cell centroids (m), for the Perot offset (§15.4.3).
    cx: Vec<f64>,
    cy: Vec<f64>,
    bed: Vec<CellBed>,
    z_mean: Vec<f64>,
    n_cell: Vec<f64>,
    faces: Vec<Face>,
    /// Cell → incident interior faces, CSR, ascending face index: the
    /// §15.4.3 fixed gather order. Sign +1 when the cell is `cl`.
    cf_off: Vec<u32>,
    cf_face: Vec<(u32, f64)>,
    boundaries: Vec<Boundary>,
    /// Σ ξ/dₙ over each cell's interior faces, the §15.4.4 L² metric.
    metric: Vec<f64>,

    // ── Options (§14.15) ────────────────────────────────────────────
    theta: f64,
    cfl: f64,
    froude_max: f64,
    dry_depth: f64,
    h_move: f64,
    max_dt: f64,
    closure: CellClosure,
    face_rec: FaceReconstruction,
    vfr_eps: f64,
    lts_tiers: u32,

    // ── State ───────────────────────────────────────────────────────
    /// Conserved volumes (m³).
    pub vol: Vec<f64>,
    /// Derived surface elevations (m).
    pub eta: Vec<f64>,
    /// Derived mean depths (m).
    pub depth: Vec<f64>,
    /// Perot velocity proxies (m²/s).
    qcx: Vec<f64>,
    qcy: Vec<f64>,
    /// Face discharges (m²/s) and pending mass per side (m³).
    q: Vec<f64>,
    facc_l: Vec<f64>,
    facc_r: Vec<f64>,
    /// §15.4.4 active set with hysteresis, one-ring halo, and pinned
    /// boundary cells.
    active: Vec<bool>,
    /// §15.4.4 tier per cell (0 = every base substep); boundary cells
    /// pinned to 0, inactive cells parked at the coarsest.
    tier: Vec<u8>,
    /// A face fires at the finer of its two cells' cadences.
    face_tier: Vec<u8>,
    macro_cycles: u64,
    /// §15.4.4 whole-run counts for the §14.9 time-step summary.
    substeps: u64,
    rebuilds: u64,
    min_dt0: f64,
    advanced: f64,
    peak_active: usize,
    /// Initial storage (m³), the §15.8 ledger's opening term.
    storage0: f64,
    /// Per-cell ledger scratch for the ∥ cell phase: (rain, coupling,
    /// evaporation take), reduced serially in index order so the sums
    /// are byte-identical at every width (§15.4.5).
    led: Vec<[f64; 3]>,
    /// §15.4.5: the worker team, when the model asked for width.
    #[cfg(feature = "threads")]
    team: Option<crate::hydraulics::team::Team>,

    // ── Sources, set by the caller before an advance (m/s) ─────────
    pub rain: Vec<f64>,
    pub evap: Vec<f64>,
    pub coupling: Vec<f64>,

    // ── §15.6 coupling ─────────────────────────────────────────────
    couplings: Vec<CouplingPoint>,
    /// Distinct coupled node names, in slot order.
    node_names: Vec<String>,
    /// Per-slot drive, set before an advance: node hydraulic grade,
    /// node water depth, rim elevation (all m), and the volume
    /// available to spill over the whole advance (m³).
    node_grade: Vec<f64>,
    node_depth: Vec<f64>,
    node_rim: Vec<f64>,
    node_spill_avail: Vec<f64>,
    /// §15.6 per-advance spill ledger: the same water cannot spill
    /// twice within one routing step.
    node_drawn: Vec<f64>,
    /// §15.6: slots naming an outfall exchange through the boundary
    /// stage and injection paths, never the junction orifice law.
    outfall_slot: Vec<bool>,
    /// Signed exchanged volume per point over the last advance (m³,
    /// positive = drained into the node).
    exchange: Vec<f64>,

    // ── §15.5 driven boundaries ────────────────────────────────────
    /// Slots whose stage or flow the session resolves per advance.
    driven: Vec<DrivenBoundary>,
    /// Distinct rating-curve names, in first-appearance order, and the
    /// resolved curves (stage above sill → per-metre discharge).
    rating_names: Vec<String>,
    rating_curves: Vec<Vec<(f64, f64)>>,

    // ── §15.8 ledger (m³ since construction) ───────────────────────
    pub rain_in: f64,
    pub evap_out: f64,
    pub boundary_in: f64,
    pub boundary_out: f64,
    /// Junction exchange (§15.6 orifice law): spills in, drains out.
    pub coupling_in: f64,
    pub coupling_out: f64,
    /// Outfall exchange (§15.6 injection path): discharges in,
    /// withdrawals out — booked separately per §15.8.
    pub outfall_in: f64,
    pub outfall_out: f64,
}

/// §15.5: a boundary slot whose law the session resolves per advance
/// from an authored time series.
#[derive(Debug, Clone)]
pub struct DrivenBoundary {
    /// Index into the marcher's boundary list.
    pub boundary: usize,
    /// The authored series name, resolved by the session.
    pub series: String,
    /// Stage series (`true`) or flow series (`false`).
    pub is_stage: bool,
}

/// §15.6: a resolved coupling point. A vertex row collapses to the
/// lowest-bed cell of its stencil for the orifice exchange; the stencil
/// and the vertex remain for outfall-injection scattering.
#[derive(Debug, Clone)]
pub struct CouplingPoint {
    /// The cell both directions of the exchange apply at.
    pub cell: u32,
    /// The vertex's incident cells (a cell row: the cell alone).
    pub stencil: Vec<u32>,
    /// The vertex position and ground elevation, `None` for a cell row.
    pub vertex: Option<(f64, f64, f64)>,
    /// The exchange slot: points naming one network node share a slot,
    /// its drive, and its spill ledger.
    pub node_slot: u32,
    /// The network node as authored, for the session to resolve (§2.6).
    pub node: String,
    /// Discharge coefficient (§15.6).
    pub cd: f64,
    /// Exchange area (m²).
    pub area: f64,
    /// Unauthored areas are eligible for `COUPLING_AREA AUTO` (§15.6).
    pub area_authored: bool,
}

/// The §15.4.3 cell phase's taken-out state, as raw pointers.
struct CellPtrs {
    vol: *mut f64,
    depth: *mut f64,
    eta: *mut f64,
    qcx: *mut f64,
    qcy: *mut f64,
    fl: *mut f64,
    fr: *mut f64,
    led: *mut [f64; 3],
}

// SAFETY: the cell phase's writes are per-cell disjoint (see
// [`Marcher::fire_cell_at`]), which is what makes sharing the pointers
// across the team sound.
#[cfg(feature = "threads")]
unsafe impl Sync for CellPtrs {}

impl Marcher {
    /// Build from a validated mesh and its topology. Initial volumes come
    /// from the authored depths, initial face discharges from the
    /// authored velocities projected onto the face normals.
    pub fn build(mesh: &OverlandMesh, topo: &Topology) -> Marcher {
        let nc = mesh.cells.len();
        let o = &mesh.options;

        let bed: Vec<CellBed> = mesh
            .cells
            .iter()
            .map(|c| {
                let [a, b, d] = c.v.map(|v| mesh.verts[v as usize].z);
                CellBed::new(a, b, d)
            })
            .collect();

        // Interior faces, one per shared edge, from the lower-indexed
        // side's slot (its normal already points cl → cr).
        let mut faces = Vec::new();
        for ci in 0..nc {
            for e in 0..3 {
                let slot = ci * 3 + e;
                let Some(nj) = topo.neighbour[slot] else {
                    continue;
                };
                if (nj as usize) < ci {
                    continue;
                }
                let [nx, ny] = topo.edge_normal[slot];
                faces.push(Face {
                    cl: ci as u32,
                    cr: nj,
                    xi: topo.edge_len[slot],
                    nx,
                    ny,
                    z_face: topo.centroid[ci][2].max(topo.centroid[nj as usize][2]),
                    z_lo: topo.edge_z[slot][0],
                    z_hi: topo.edge_z[slot][1],
                    mx: topo.edge_mid[slot][0],
                    my: topo.edge_mid[slot][1],
                    inv_dn: topo.inv_dn[slot],
                    psi: topo.conveyance[slot],
                    n2: {
                        let n = 0.5 * (mesh.cells[ci].n + mesh.cells[nj as usize].n);
                        n * n
                    },
                });
            }
        }

        // Cell → faces CSR in ascending face order.
        let mut counts = vec![0u32; nc + 1];
        for f in &faces {
            counts[f.cl as usize + 1] += 1;
            counts[f.cr as usize + 1] += 1;
        }
        for i in 0..nc {
            counts[i + 1] += counts[i];
        }
        let mut cursor = counts.clone();
        let mut cf_face = vec![(0u32, 0.0); *counts.last().unwrap_or(&0) as usize];
        for (fi, f) in faces.iter().enumerate() {
            cf_face[cursor[f.cl as usize] as usize] = (fi as u32, 1.0);
            cursor[f.cl as usize] += 1;
            cf_face[cursor[f.cr as usize] as usize] = (fi as u32, -1.0);
            cursor[f.cr as usize] += 1;
        }

        let mut metric = vec![0.0; nc];
        for f in &faces {
            let m = f.xi * f.inv_dn;
            metric[f.cl as usize] += m;
            metric[f.cr as usize] += m;
        }

        // Boundary laws from the validated rows; walls stay implicit.
        let mut boundaries = Vec::new();
        let mut driven: Vec<DrivenBoundary> = Vec::new();
        let mut rating_names: Vec<String> = Vec::new();
        for row in &mesh.boundaries {
            // §15.5: series and curve parameters resolve at the wiring
            // layer; a slot is a wall until its first resolution.
            let mut enabled = true;
            let mut series: Option<(String, bool)> = None;
            let law = match &row.condition {
                BoundaryCondition::Wall => continue,
                BoundaryCondition::NormalFlow { slope } => {
                    BoundaryLaw::NormalFlow { slope: *slope }
                }
                BoundaryCondition::Flow(SeriesOrValue::Value(v)) => {
                    BoundaryLaw::Flow { q_per_m: *v }
                }
                BoundaryCondition::Stage(SeriesOrValue::Value(v)) => BoundaryLaw::Stage { eta: *v },
                BoundaryCondition::Flow(SeriesOrValue::Series(name)) => {
                    enabled = false;
                    series = Some((name.clone(), false));
                    BoundaryLaw::Flow { q_per_m: 0.0 }
                }
                BoundaryCondition::Stage(SeriesOrValue::Series(name)) => {
                    enabled = false;
                    series = Some((name.clone(), true));
                    BoundaryLaw::Stage { eta: 0.0 }
                }
                BoundaryCondition::RatingCurve { curve } => {
                    enabled = false;
                    let slot = match rating_names.iter().position(|n| n == curve) {
                        Some(i) => i,
                        None => {
                            rating_names.push(curve.clone());
                            rating_names.len() - 1
                        }
                    };
                    BoundaryLaw::Rating { curve: slot as u32 }
                }
            };
            let slot = row.cell as usize * 3 + row.edge as usize;
            let xi = topo.edge_len[slot];
            if let Some((name, is_stage)) = series {
                driven.push(DrivenBoundary {
                    boundary: boundaries.len(),
                    series: name,
                    is_stage,
                });
            }
            boundaries.push(Boundary {
                cell: row.cell,
                xi,
                law,
                mx: topo.edge_mid[slot][0],
                my: topo.edge_mid[slot][1],
                inv_dn_ghost: 3.0 * xi / (2.0 * topo.area[row.cell as usize]),
                q: 0.0,
                z_sill: topo.edge_z[slot][0].max(topo.edge_z[slot][1]),
                enabled,
            });
        }

        let area: Vec<f64> = topo.area.clone();
        let vol: Vec<f64> = mesh
            .cells
            .iter()
            .enumerate()
            .map(|(ci, c)| c.h0 * area[ci])
            .collect();

        // Authored velocities → face discharges: the mean of the two
        // incident cells' (h·u, h·v), projected on the face normal.
        let mut hu = vec![(0.0, 0.0); nc];
        for r in &mesh.init_velocity {
            let h0 = mesh.cells[r.cell as usize].h0;
            hu[r.cell as usize] = (h0 * r.u, h0 * r.v);
        }
        let q: Vec<f64> = faces
            .iter()
            .map(|f| {
                let (ul, vl) = hu[f.cl as usize];
                let (ur, vr) = hu[f.cr as usize];
                0.5 * ((ul + ur) * f.nx + (vl + vr) * f.ny)
            })
            .collect();

        // §15.6: resolve coupling rows. Two rows resolving to one
        // vertex (or cell) are one coupling, the last authored winning.
        let z_mean_of = |ci: usize| bed[ci].mean();
        let mut resolved: Vec<(Option<u32>, Option<u32>, &CouplingRow)> = Vec::new();
        for row in &mesh.vertex_couplings {
            if let Some(v) = mesh.resolve_vertex(&row.address) {
                resolved.retain(|(rv, _, _)| *rv != Some(v));
                resolved.push((Some(v), None, row));
            }
        }
        for row in &mesh.cell_couplings {
            if let Some(c) = mesh.resolve_cell(&row.address) {
                resolved.retain(|(_, rc, _)| *rc != Some(c));
                resolved.push((None, Some(c), row));
            }
        }
        let mut couplings: Vec<CouplingPoint> = Vec::new();
        let mut node_names: Vec<String> = Vec::new();
        for (v, c, row) in resolved {
            let (cell, stencil, vertex) = if let Some(v) = v {
                let stencil: Vec<u32> = (0..nc)
                    .filter(|&ci| mesh.cells[ci].v.contains(&v))
                    .map(|ci| ci as u32)
                    .collect();
                let Some(&cell) = stencil
                    .iter()
                    .min_by(|&&a, &&b| z_mean_of(a as usize).total_cmp(&z_mean_of(b as usize)))
                else {
                    continue;
                };
                let vr = &mesh.verts[v as usize];
                (cell, stencil, Some((vr.x, vr.y, vr.z)))
            } else {
                let cell = c.unwrap_or(0);
                (cell, vec![cell], None)
            };
            let slot = match node_names.iter().position(|n| n == &row.node) {
                Some(i) => i,
                None => {
                    node_names.push(row.node.clone());
                    node_names.len() - 1
                }
            } as u32;
            couplings.push(CouplingPoint {
                cell,
                stencil,
                vertex,
                node_slot: slot,
                node: row.node.clone(),
                cd: row.cd,
                area: row.area,
                area_authored: row.area_authored,
            });
        }
        let ns = node_names.len();
        let np = couplings.len();

        let nf = faces.len();
        let mut m = Marcher {
            cx: topo.centroid.iter().map(|c| c[0]).collect(),
            cy: topo.centroid.iter().map(|c| c[1]).collect(),
            z_mean: bed.iter().map(CellBed::mean).collect(),
            n_cell: mesh.cells.iter().map(|c| c.n).collect(),
            area,
            bed,
            faces,
            cf_off: counts,
            cf_face,
            boundaries,
            metric,
            theta: o.theta,
            cfl: o.cfl_number,
            froude_max: o.froude_max,
            dry_depth: o.dry_depth,
            h_move: o.h_move,
            max_dt: o.max_timestep,
            closure: o.cell_closure,
            face_rec: o.face_reconstruction,
            vfr_eps: o.vfr_min_wet_frac,
            lts_tiers: o.lts_tiers,
            vol,
            eta: vec![0.0; nc],
            depth: vec![0.0; nc],
            qcx: vec![0.0; nc],
            qcy: vec![0.0; nc],
            q,
            facc_l: vec![0.0; nf],
            facc_r: vec![0.0; nf],
            active: vec![false; nc],
            tier: vec![0; nc],
            face_tier: Vec::new(),
            macro_cycles: 0,
            substeps: 0,
            rebuilds: 0,
            min_dt0: f64::INFINITY,
            advanced: 0.0,
            peak_active: 0,
            storage0: 0.0,
            led: vec![[0.0; 3]; nc],
            #[cfg(feature = "threads")]
            team: None,
            rain: vec![0.0; nc],
            evap: vec![0.0; nc],
            coupling: vec![0.0; nc],
            couplings,
            node_names,
            node_grade: vec![0.0; ns],
            node_depth: vec![0.0; ns],
            node_rim: vec![0.0; ns],
            node_spill_avail: vec![0.0; ns],
            node_drawn: vec![0.0; ns],
            outfall_slot: vec![false; ns],
            exchange: vec![0.0; np],
            driven,
            rating_curves: vec![Vec::new(); rating_names.len()],
            rating_names,
            rain_in: 0.0,
            evap_out: 0.0,
            boundary_in: 0.0,
            boundary_out: 0.0,
            coupling_in: 0.0,
            coupling_out: 0.0,
            outfall_in: 0.0,
            outfall_out: 0.0,
        };
        for ci in 0..nc {
            m.reclose(ci);
        }
        m.face_tier = vec![0; m.faces.len()];
        m.refresh_perot();
        m.rebuild_active();
        m.rebuilds = 0;
        m.storage0 = m.storage();
        m
    }

    /// A cell's (depth, surface) closure for a volume (§15.3), pure.
    fn close_of(&self, ci: usize, vol: f64) -> (f64, f64) {
        let h = vol.max(0.0) / self.area[ci];
        let eta = match self.closure {
            CellClosure::Flat => flat_eta(&self.bed[ci], h),
            CellClosure::Vfr => vfr_eta(&self.bed[ci], h, self.vfr_eps),
        };
        (h, eta)
    }

    /// Re-derive a cell's surface from its volume (§15.3).
    fn reclose(&mut self, ci: usize) {
        let (h, eta) = self.close_of(ci, self.vol[ci]);
        self.depth[ci] = h;
        self.eta[ci] = eta;
    }

    /// §15.4.3 Perot reconstruction over each cell's faces, fixed order:
    /// $\mathbf{q}_c = \frac1A \sum_e s_e q_e \xi_e (\mathbf{m}_e -
    /// \mathbf{c})$ — the midpoint offset, not the normal; the two differ
    /// on any skewed cell, and the normal form was measured to destabilise
    /// the march (waves growing from the θ-blend feeding on itself).
    fn refresh_perot(&mut self) {
        for ci in 0..self.area.len() {
            self.perot_cell(ci);
        }
        self.perot_complete_boundaries();
    }

    /// One cell's interior-face reconstruction, at its firing.
    fn perot_cell(&mut self, ci: usize) {
        let (x, y) = self.perot_of(ci);
        self.qcx[ci] = x;
        self.qcy[ci] = y;
    }

    /// The reconstruction itself, pure over the face discharges.
    fn perot_of(&self, ci: usize) -> (f64, f64) {
        let (mut sx, mut sy) = (0.0, 0.0);
        let lo = self.cf_off[ci] as usize;
        let hi = self.cf_off[ci + 1] as usize;
        for &(fi, sign) in &self.cf_face[lo..hi] {
            let f = &self.faces[fi as usize];
            let flux = sign * self.q[fi as usize] * f.xi;
            sx += flux * (f.mx - self.cx[ci]);
            sy += flux * (f.my - self.cy[ci]);
        }
        (sx / self.area[ci], sy / self.area[ci])
    }

    /// §15.5 completion: a boundary edge's discharge belongs to its
    /// cell's reconstruction too, in the same outward-flux convention
    /// (the prognostic is inflow-positive, so outward is its negation).
    fn perot_complete_boundaries(&mut self) {
        for b in &self.boundaries {
            let ci = b.cell as usize;
            let flux = -b.q * b.xi;
            self.qcx[ci] += flux * (b.mx - self.cx[ci]) / self.area[ci];
            self.qcy[ci] += flux * (b.my - self.cy[ci]) / self.area[ci];
        }
    }

    /// §15.4.4 activation: hysteresis about `h_move`, a one-ring halo,
    /// and boundary cells pinned.
    fn rebuild_active(&mut self) {
        let band = (0.001_f64).min(self.h_move / 2.0);
        let core: Vec<bool> = (0..self.area.len())
            .map(|ci| {
                if self.active[ci] {
                    self.depth[ci] >= self.h_move - band
                } else {
                    self.depth[ci] > self.h_move + band
                }
            })
            .collect();
        let mut next = core.clone();
        for f in &self.faces {
            if core[f.cl as usize] {
                next[f.cr as usize] = true;
            }
            if core[f.cr as usize] {
                next[f.cl as usize] = true;
            }
        }
        for b in &self.boundaries {
            next[b.cell as usize] = true;
        }
        // §15.4.4: cells with coupling points or held injection are
        // always active.
        for cp in &self.couplings {
            next[cp.cell as usize] = true;
        }
        for (ci, r) in self.coupling.iter().enumerate() {
            if *r != 0.0 {
                next[ci] = true;
            }
        }
        self.rebuilds += 1;
        self.peak_active = self.peak_active.max(next.iter().filter(|a| **a).count());
        self.active = next;
    }

    /// §15.4.4: a cell's stable step, `max_dt` where no wave constrains.
    fn cell_dt(&self, ci: usize) -> f64 {
        if !self.active[ci] || self.depth[ci] <= self.dry_depth {
            return self.max_dt;
        }
        let h = self.depth[ci];
        let speed = (G * h).sqrt() + (self.qcx[ci].hypot(self.qcy[ci])) / h;
        if self.metric[ci] > 0.0 && speed > 0.0 {
            let l = (2.0 * self.area[ci] / self.metric[ci]).sqrt();
            (self.cfl * l / speed).min(self.max_dt)
        } else {
            self.max_dt
        }
    }

    /// §15.4.4: the stable base step over active cells.
    fn stable_dt(&self) -> f64 {
        (0..self.area.len())
            .map(|ci| self.cell_dt(ci))
            .fold(self.max_dt, f64::min)
    }

    /// §15.4.4 published picture: the observable discharge of face
    /// `fi`, re-limited against the published surfaces — dry faces read
    /// zero and the Froude cap is re-applied. The prognostic value is
    /// untouched.
    pub fn published_face_q(&self, fi: usize) -> f64 {
        let f = &self.faces[fi];
        let (cl, cr) = (f.cl as usize, f.cr as usize);
        let h_f = match self.face_rec {
            FaceReconstruction::Mean => face_depth_mean(self.eta[cl], self.eta[cr], f.z_face),
            FaceReconstruction::VfrFace => {
                face_depth_vfr(self.eta[cl], self.eta[cr], f.z_lo, f.z_hi)
            }
        };
        if h_f <= self.dry_depth {
            return 0.0;
        }
        let cap = self.froude_max * h_f * (G * h_f).sqrt();
        self.q[fi].clamp(-cap, cap)
    }

    /// §15.6: junction exchange at tier-0 cadence, sequentially, in
    /// point order. A drain takes at most the β share of its source
    /// cell per substep; a spill draws against the node's advance-wide
    /// ledger — the same water cannot spill twice.
    fn fire_couplings(&mut self, dt: f64) {
        for k in 0..self.couplings.len() {
            let cp = &self.couplings[k];
            let ci = cp.cell as usize;
            let slot = cp.node_slot as usize;
            if self.outfall_slot[slot] {
                // §15.6: outfall coupling is asymmetric — no orifice law.
                continue;
            }
            let mut q = exchange_q(
                self.eta[ci],
                self.node_grade[slot],
                self.node_rim[slot],
                cp.cd,
                cp.area,
                self.depth[ci],
                self.node_depth[slot],
                self.dry_depth,
            );
            if q == 0.0 {
                continue;
            }
            if q > 0.0 {
                q = q.min(BETA * self.vol[ci].max(0.0) / dt);
            } else {
                let avail = (self.node_spill_avail[slot] - self.node_drawn[slot]).max(0.0);
                if avail <= 0.0 {
                    continue;
                }
                let take = (-q * dt).min(avail);
                self.node_drawn[slot] += take;
                q = -take / dt;
            }
            let dv = q * dt;
            self.vol[ci] = (self.vol[ci] - dv).max(0.0);
            if dv > 0.0 {
                self.coupling_out += dv;
            } else {
                self.coupling_in += -dv;
            }
            self.exchange[k] += dv;
            self.reclose(ci);
        }
    }

    /// §15.4.5: give the march the §6.4 worker width. Widths below 2
    /// stay serial; results are byte-identical either way.
    #[cfg(feature = "threads")]
    pub fn set_width(&mut self, width: usize) {
        self.team = crate::hydraulics::team::Team::new(width);
    }

    /// The §15.4.4 drying depth (m), for callers keying wetness ramps.
    pub fn dry_depth(&self) -> f64 {
        self.dry_depth
    }

    /// §15.6: mark a slot as naming an outfall — its points leave the
    /// junction orifice law to the boundary-stage and injection paths.
    pub fn mark_outfall_slot(&mut self, slot: usize) {
        self.outfall_slot[slot] = true;
    }

    /// Whether a slot was marked as an outfall.
    pub fn is_outfall_slot(&self, slot: usize) -> bool {
        self.outfall_slot[slot]
    }

    /// §15.5: the boundary slots whose stage or flow the session must
    /// resolve per advance.
    pub fn driven_boundaries(&self) -> &[DrivenBoundary] {
        &self.driven
    }

    /// §15.5: resolve a driven slot's flow (m³/s per metre, outward
    /// positive as authored). The slot conveys from now on.
    pub fn set_boundary_flow(&mut self, bi: usize, q_per_m: f64) {
        self.boundaries[bi].law = BoundaryLaw::Flow { q_per_m };
        self.boundaries[bi].enabled = true;
    }

    /// §15.5: resolve a driven slot's stage (m). The slot conveys from
    /// now on.
    pub fn set_boundary_stage(&mut self, bi: usize, eta: f64) {
        self.boundaries[bi].law = BoundaryLaw::Stage { eta };
        self.boundaries[bi].enabled = true;
    }

    /// Distinct rating-curve names, in slot order.
    pub fn rating_curve_names(&self) -> &[String] {
        &self.rating_names
    }

    /// §15.5: supply a rating curve's points (stage above the edge sill
    /// (m) → per-metre discharge, sorted by stage). Every boundary slot
    /// reading this curve conveys from now on.
    pub fn set_rating_curve(&mut self, slot: usize, points: Vec<(f64, f64)>) {
        self.rating_curves[slot] = points;
        for b in &mut self.boundaries {
            if matches!(b.law, BoundaryLaw::Rating { curve } if curve as usize == slot) {
                b.enabled = true;
            }
        }
    }

    /// The resolved §15.6 coupling points, in authored (post last-wins)
    /// order.
    pub fn coupling_points(&self) -> &[CouplingPoint] {
        &self.couplings
    }

    /// Distinct coupled node names, in slot order; drives and spill
    /// budgets are addressed by these slots.
    pub fn coupling_nodes(&self) -> &[String] {
        &self.node_names
    }

    /// §15.6: set a node slot's drive for the coming advance — its
    /// hydraulic grade, water depth and rim elevation (m), and the
    /// volume available to spill over the whole advance (m³).
    pub fn set_node_drive(&mut self, slot: usize, grade: f64, depth: f64, rim: f64, spill: f64) {
        self.node_grade[slot] = grade;
        self.node_depth[slot] = depth;
        self.node_rim[slot] = rim;
        self.node_spill_avail[slot] = spill;
    }

    /// Resolve a `COUPLING_AREA AUTO` derivation (§15.6): the session
    /// overrides unauthored areas from the node's largest connected
    /// conduit before the first advance.
    pub fn set_coupling_area(&mut self, point: usize, area: f64) {
        self.couplings[point].area = area;
    }

    /// Signed exchanged volume per point over the last advance (m³,
    /// positive = drained into the node).
    pub fn exchanged(&self) -> &[f64] {
        &self.exchange
    }

    /// §15.6: the exchange conductance of a point against the current
    /// surface, for the §6.4 vertex damping. Never negative.
    pub fn coupling_conductance(&self, point: usize) -> f64 {
        let cp = &self.couplings[point];
        let ci = cp.cell as usize;
        let slot = cp.node_slot as usize;
        exchange_conductance(
            self.eta[ci],
            self.node_grade[slot],
            self.node_rim[slot],
            cp.cd,
            cp.area,
            self.depth[ci],
            self.node_depth[slot],
            self.dry_depth,
        )
    }

    /// Clear every held injection rate (§15.6 outfall network→surface).
    pub fn clear_injection(&mut self) {
        for r in &mut self.coupling {
            *r = 0.0;
        }
    }

    /// §15.6: scatter an outfall's injection (m³/s) across a point's
    /// stencil, weighted by the surface slope from the vertex down
    /// toward each cell, falling back to area weights on a flat or dry
    /// surface. The rates persist until cleared.
    pub fn inject(&mut self, point: usize, rate: f64) {
        let cp = self.couplings[point].clone();
        let weights: Vec<f64> = if let Some((vx, vy, vz)) = cp.vertex {
            // η_v: wet-depth-weighted mean surface of the wet stencil
            // cells, the vertex ground elevation when all are dry.
            let (mut num, mut den) = (0.0, 0.0);
            for &t in &cp.stencil {
                let t = t as usize;
                if self.depth[t] >= self.dry_depth {
                    num += self.depth[t] * self.eta[t];
                    den += self.depth[t];
                }
            }
            let eta_v = if den > 0.0 { num / den } else { vz };
            cp.stencil
                .iter()
                .map(|&t| {
                    let t = t as usize;
                    let d = (self.cx[t] - vx).hypot(self.cy[t] - vy);
                    if d < 1e-9 {
                        0.0
                    } else {
                        ((eta_v - self.eta[t]) / d).max(0.0)
                    }
                })
                .collect()
        } else {
            vec![1.0]
        };
        let wsum: f64 = weights.iter().sum();
        if wsum > 1e-30 {
            for (&t, w) in cp.stencil.iter().zip(&weights) {
                let t = t as usize;
                self.coupling[t] += rate * (w / wsum) / self.area[t];
            }
        } else {
            let asum: f64 = cp.stencil.iter().map(|&t| self.area[t as usize]).sum();
            for &t in &cp.stencil {
                let t = t as usize;
                self.coupling[t] += rate / asum;
            }
        }
    }

    /// §15.4.4: re-tiering invalidates the positivity bookkeeping of
    /// in-flight accumulators, so gather every pending side into its
    /// cell first, and re-close the cells that changed.
    fn settle_accumulators(&mut self) {
        for ci in 0..self.area.len() {
            let lo = self.cf_off[ci] as usize;
            let hi = self.cf_off[ci + 1] as usize;
            let mut pending = 0.0;
            for &(fi, sign) in &self.cf_face[lo..hi] {
                let fi = fi as usize;
                if sign > 0.0 {
                    pending += self.facc_l[fi];
                    self.facc_l[fi] = 0.0;
                } else {
                    pending += self.facc_r[fi];
                    self.facc_r[fi] = 0.0;
                }
            }
            if pending != 0.0 {
                self.vol[ci] = (self.vol[ci] + pending).max(0.0);
                self.reclose(ci);
            }
        }
    }

    /// §15.4.4: assign every cell its power-of-two tier from its own
    /// stable step against the base step; boundary cells pin to 0,
    /// inactive cells park at the coarsest; a face takes the finer of
    /// its two cells' cadences.
    fn assign_tiers(&mut self, dt0: f64) {
        let k_max = self.lts_tiers.saturating_sub(1).min(7) as u8;
        for ci in 0..self.area.len() {
            self.tier[ci] = if !self.active[ci] {
                k_max
            } else {
                let ratio = (self.cell_dt(ci) / dt0).max(1.0);
                (ratio.log2().floor() as u8).min(k_max)
            };
        }
        for b in &self.boundaries {
            self.tier[b.cell as usize] = 0;
        }
        // §15.4.4: coupling and injection cells are pinned to tier 0.
        for cp in &self.couplings {
            self.tier[cp.cell as usize] = 0;
        }
        for (ci, r) in self.coupling.iter().enumerate() {
            if *r != 0.0 {
                self.tier[ci] = 0;
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            self.face_tier[fi] = self.tier[f.cl as usize].min(self.tier[f.cr as usize]);
            // §15.4.4: a face walled by deactivation surrenders its
            // discharge; stale momentum must not survive re-activation.
            if !(self.active[f.cl as usize] && self.active[f.cr as usize]) {
                self.q[fi] = 0.0;
            }
        }
    }

    /// One face's §15.4.2 firing. The face arrays are taken out of
    /// `self` for the phase and passed as raw pointers.
    ///
    /// # Safety
    /// Reads and writes only index `fi` of `q`, `fl` and `fr`; callers
    /// run disjoint `fi` concurrently and nothing else touches those
    /// arrays during the phase.
    #[allow(clippy::too_many_arguments)]
    unsafe fn fire_face_at(
        &self,
        fi: usize,
        dt0: f64,
        s: u64,
        q: *mut f64,
        fl: *mut f64,
        fr: *mut f64,
    ) {
        if !s.is_multiple_of(1u64 << self.face_tier[fi]) {
            return;
        }
        let dt = dt0 * f64::from(1u32 << self.face_tier[fi]);
        let f = &self.faces[fi];
        let (cl, cr) = (f.cl as usize, f.cr as usize);
        // Both-active gating: a one-sided face loses basin volume.
        if !self.active[cl] || !self.active[cr] {
            *q.add(fi) = 0.0;
            return;
        }
        let h_f = match self.face_rec {
            FaceReconstruction::Mean => face_depth_mean(self.eta[cl], self.eta[cr], f.z_face),
            FaceReconstruction::VfrFace => {
                face_depth_vfr(self.eta[cl], self.eta[cr], f.z_lo, f.z_hi)
            }
        };
        if h_f <= self.dry_depth {
            *q.add(fi) = 0.0;
            return;
        }
        let q_old = *q.add(fi);
        let mut d_eta = self.eta[cr] - self.eta[cl];
        if d_eta.abs() < ETA_DEADBAND {
            d_eta = 0.0;
        }
        let slope = d_eta * f.inv_dn;
        let q_perot =
            0.5 * ((self.qcx[cl] + self.qcx[cr]) * f.nx + (self.qcy[cl] + self.qcy[cr]) * f.ny);
        let q_hat = self.theta * q_old + (1.0 - self.theta) * q_perot;
        let q_mag = q_old
            .abs()
            .max(0.5 * (self.qcx[cl] + self.qcx[cr]).hypot(self.qcy[cl] + self.qcy[cr]));
        let h73 = h_f * h_f * h_f.cbrt();
        let mut q_new = (q_hat - dt * G * h_f * slope) / (1.0 + G * dt * f.n2 * q_mag / h73);
        // Froude cap.
        let q_cap = self.froude_max * h_f * (G * h_f).sqrt();
        q_new = q_new.clamp(-q_cap, q_cap);
        // Positivity: the exporter grants at most β/3 of its volume
        // per cell cycle, divided by the times this face fires
        // within the exporter's cycle (§15.4.4) — repeated takes at
        // a tier interface would otherwise drain the cell into the
        // backstop.
        let exporter = if q_new > 0.0 { cl } else { cr };
        let refire = 1u32 << (self.tier[exporter] - self.face_tier[fi]);
        let budget = BETA / 3.0 / f64::from(refire) * self.vol[exporter].max(0.0);
        let take = q_new.abs() * f.psi * f.xi * dt;
        if take > budget && take > 0.0 {
            q_new *= budget / take;
        }
        *q.add(fi) = q_new;
        let dm = f.psi * q_new * f.xi * dt;
        *fl.add(fi) -= dm;
        *fr.add(fi) += dm;
    }

    /// §15.4.2 ∥ face phase: fire every face whose tier is due at base
    /// substep `s`, each over its own tier's interval — across the
    /// worker team when the model asked for width, byte-identical at
    /// any (§15.4.5).
    fn fire_faces(&mut self, dt0: f64, s: u64) {
        let mut q = std::mem::take(&mut self.q);
        let mut fl = std::mem::take(&mut self.facc_l);
        let mut fr = std::mem::take(&mut self.facc_r);
        let (qp, flp, frp) = (q.as_mut_ptr(), fl.as_mut_ptr(), fr.as_mut_ptr());
        #[cfg(feature = "threads")]
        {
            if let Some(mut team) = self.team.take() {
                if self.faces.len() >= PAR_GRAIN * team.width() {
                    use crate::hydraulics::team::SendPtr;
                    let (qs, fls, frs) = (SendPtr::new(qp), SendPtr::new(flp), SendPtr::new(frp));
                    let me = &*self;
                    // SAFETY: per-face disjoint reads/writes, as the
                    // body's contract states.
                    team.run(me.faces.len(), |fi| unsafe {
                        me.fire_face_at(fi, dt0, s, qs.get(), fls.get(), frs.get());
                    });
                    self.team = Some(team);
                    self.q = q;
                    self.facc_l = fl;
                    self.facc_r = fr;
                    return;
                }
                self.team = Some(team);
            }
        }
        for fi in 0..self.faces.len() {
            // SAFETY: serial — the pointers are exclusive here.
            unsafe { self.fire_face_at(fi, dt0, s, qp, flp, frp) };
        }
        self.q = q;
        self.facc_l = fl;
        self.facc_r = fr;
    }

    /// One cell's §15.4.3 firing. The cell-state arrays and the face
    /// accumulators are taken out of `self` for the phase and passed as
    /// raw pointers; ledger contributions land in the per-cell scratch
    /// for the driver's serial reduction.
    ///
    /// # Safety
    /// Reads and writes only this cell's indices — its own slots of
    /// `vol`/`depth`/`eta`/`qcx`/`qcy`/`led`, and its **own sides** of
    /// its incident faces' accumulators, which no other cell owns.
    /// Callers run disjoint `ci` concurrently and nothing else touches
    /// these arrays during the phase.
    #[allow(clippy::too_many_arguments)]
    unsafe fn fire_cell_at(&self, ci: usize, dt0: f64, s: u64, p: &CellPtrs) {
        if !s.is_multiple_of(1u64 << self.tier[ci]) {
            return;
        }
        let dt = dt0 * f64::from(1u32 << self.tier[ci]);
        let lo = self.cf_off[ci] as usize;
        let hi = self.cf_off[ci + 1] as usize;
        let mut flux = 0.0;
        for &(fi, sign) in &self.cf_face[lo..hi] {
            let fi = fi as usize;
            if sign > 0.0 {
                flux += *p.fl.add(fi);
                *p.fl.add(fi) = 0.0;
            } else {
                flux += *p.fr.add(fi);
                *p.fr.add(fi) = 0.0;
            }
        }
        let a = self.area[ci];
        let rain = self.rain[ci] * a * dt;
        let coup = self.coupling[ci] * a * dt;
        // §15.4.3: evaporation shuts off C¹ as the cell dries, and
        // takes no more than the cell holds.
        let t = (*p.depth.add(ci) / self.dry_depth).clamp(0.0, 1.0);
        let ramp = t * t * (3.0 - 2.0 * t);
        let want = self.evap[ci] * ramp * a * dt;
        let before = *p.vol.add(ci) + flux + rain + coup;
        let take = want.min(before.max(0.0));
        let v_new = (before - take).max(0.0);
        *p.vol.add(ci) = v_new;
        let (h, eta) = self.close_of(ci, v_new);
        *p.depth.add(ci) = h;
        *p.eta.add(ci) = eta;
        let (qx, qy) = self.perot_of(ci);
        *p.qcx.add(ci) = qx;
        *p.qcy.add(ci) = qy;
        *p.led.add(ci) = [rain, coup, take];
    }

    /// §15.4.3 ∥ cell phase: every cell whose tier is due at base
    /// substep `s` gathers its own accumulator sides in fixed order,
    /// applies its sources over its own tier's interval, clamps,
    /// re-closes, and refreshes its velocity reconstruction — across
    /// the worker team when the model asked for width. The §15.8
    /// ledger reduces serially in index order afterwards, so its sums
    /// are byte-identical at every width (§15.4.5).
    fn fire_cells(&mut self, dt0: f64, s: u64) {
        let nc = self.area.len();
        let mut vol = std::mem::take(&mut self.vol);
        let mut depth = std::mem::take(&mut self.depth);
        let mut eta = std::mem::take(&mut self.eta);
        let mut qcx = std::mem::take(&mut self.qcx);
        let mut qcy = std::mem::take(&mut self.qcy);
        let mut fl = std::mem::take(&mut self.facc_l);
        let mut fr = std::mem::take(&mut self.facc_r);
        let mut led = std::mem::take(&mut self.led);
        let ptrs = CellPtrs {
            vol: vol.as_mut_ptr(),
            depth: depth.as_mut_ptr(),
            eta: eta.as_mut_ptr(),
            qcx: qcx.as_mut_ptr(),
            qcy: qcy.as_mut_ptr(),
            fl: fl.as_mut_ptr(),
            fr: fr.as_mut_ptr(),
            led: led.as_mut_ptr(),
        };
        let mut ran = false;
        #[cfg(feature = "threads")]
        {
            if let Some(mut team) = self.team.take() {
                if nc >= PAR_GRAIN * team.width() {
                    let shared = &ptrs;
                    let me = &*self;
                    // SAFETY: per-cell disjoint reads/writes — a cell
                    // touches only its own slots and its own sides of
                    // its incident faces' accumulators.
                    team.run(nc, |ci| unsafe {
                        me.fire_cell_at(ci, dt0, s, shared);
                    });
                    ran = true;
                }
                self.team = Some(team);
            }
        }
        if !ran {
            for ci in 0..nc {
                // SAFETY: serial — the pointers are exclusive here.
                unsafe { self.fire_cell_at(ci, dt0, s, &ptrs) };
            }
        }
        self.vol = vol;
        self.depth = depth;
        self.eta = eta;
        self.qcx = qcx;
        self.qcy = qcy;
        self.facc_l = fl;
        self.facc_r = fr;
        self.led = led;
        // §15.8: the ledger reduction, serial and in index order.
        for ci in 0..nc {
            if !s.is_multiple_of(1u64 << self.tier[ci]) {
                continue;
            }
            let [rain, coup, take] = self.led[ci];
            self.rain_in += rain;
            // The injection path books separately from the junction
            // orifice exchange.
            if coup >= 0.0 {
                self.outfall_in += coup;
            } else {
                self.outfall_out += -coup;
            }
            self.evap_out += take;
        }
    }

    /// §15.5: boundary laws, serially, in slot order, clamped in volume
    /// space and booked as applied.
    fn fire_boundaries(&mut self, dt: f64) {
        for bi in 0..self.boundaries.len() {
            let b = &self.boundaries[bi];
            if !b.enabled {
                // §15.5: an unresolved series or curve slot is a wall.
                continue;
            }
            let ci = b.cell as usize;
            let h = self.depth[ci];
            // Inflow-positive volume the law asks to move this substep.
            let asked: f64 = match b.law {
                BoundaryLaw::Rating { curve } => {
                    // §15.5: stage above the edge sill, re-read every
                    // firing; linear between points, held at the ends.
                    let head = (self.eta[ci] - b.z_sill).max(0.0);
                    let pts = &self.rating_curves[curve as usize];
                    let q = match pts.iter().position(|p| p.0 >= head) {
                        _ if pts.is_empty() => 0.0,
                        None => pts[pts.len() - 1].1,
                        Some(0) => pts[0].1,
                        Some(i) => {
                            let (x0, y0) = pts[i - 1];
                            let (x1, y1) = pts[i];
                            y0 + (y1 - y0) * (head - x0) / (x1 - x0).max(1e-30)
                        }
                    };
                    if h <= self.dry_depth {
                        0.0
                    } else {
                        -q * b.xi * dt
                    }
                }
                BoundaryLaw::NormalFlow { slope } => {
                    if h <= self.dry_depth || slope <= 0.0 {
                        0.0
                    } else {
                        -(h.powf(5.0 / 3.0) * slope.sqrt() / self.n_cell[ci]) * b.xi * dt
                    }
                }
                BoundaryLaw::Flow { q_per_m } => -q_per_m * b.xi * dt,
                BoundaryLaw::Stage { eta } => {
                    // §15.5: the interior momentum law against a ghost
                    // cell held at the stage.
                    let h_f = match self.face_rec {
                        FaceReconstruction::Mean => eta.max(self.eta[ci]) - self.z_mean[ci],
                        FaceReconstruction::VfrFace => {
                            face_depth_vfr(eta, self.eta[ci], self.bed[ci].z1, self.bed[ci].z3)
                        }
                    };
                    if h_f <= self.dry_depth {
                        self.boundaries[bi].q = 0.0;
                        continue;
                    }
                    let mut d_eta = self.eta[ci] - eta;
                    if d_eta.abs() < ETA_DEADBAND {
                        d_eta = 0.0;
                    }
                    let slope = d_eta * b.inv_dn_ghost;
                    let n2 = self.n_cell[ci] * self.n_cell[ci];
                    let h73 = h_f * h_f * h_f.cbrt();
                    let q_prev = self.boundaries[bi].q;
                    let mut q_new =
                        (q_prev - dt * G * h_f * slope) / (1.0 + G * dt * n2 * q_prev.abs() / h73);
                    let q_cap = self.froude_max * h_f * (G * h_f).sqrt();
                    q_new = q_new.clamp(-q_cap, q_cap);
                    self.boundaries[bi].q = q_new;
                    let b = &self.boundaries[bi];
                    q_new * b.xi * dt
                }
            };
            let b = &self.boundaries[bi];
            // Volume clamps (§15.5): a cell cannot be driven negative,
            // and one substep of a stage boundary moves the cell at most
            // TO the stage, from either side.
            let v_old = self.vol[ci];
            let mut v_new = v_old + asked;
            if let BoundaryLaw::Stage { eta } = b.law {
                let h_eq = match self.closure {
                    CellClosure::Flat => (eta - self.z_mean[ci]).max(0.0),
                    CellClosure::Vfr => super::closure::vfr_mean_depth(&self.bed[ci], eta),
                };
                let v_eq = h_eq * self.area[ci];
                v_new = if asked < 0.0 {
                    v_new.max(v_old.min(v_eq))
                } else {
                    v_new.min(v_old.max(v_eq))
                };
            }
            v_new = v_new.max(0.0);
            let applied = v_new - v_old;
            // §15.5: momentum matches applied mass — a clamped exchange
            // that kept its unclamped momentum would wind up and drive
            // the basin as an oscillator.
            self.boundaries[bi].q = if b.xi > 1e-12 && dt > 0.0 {
                applied / (b.xi * dt)
            } else {
                0.0
            };
            // Booking follows application exactly (§15.5).
            if applied >= 0.0 {
                self.boundary_in += applied;
            } else {
                self.boundary_out -= applied;
            }
            self.vol[ci] = v_new;
            self.reclose(ci);
            self.perot_cell(ci);
        }
        self.perot_complete_boundaries();
    }

    /// Advance to exactly `span` seconds from now (§15.4.4: an advance
    /// always reaches its target).
    pub fn advance(&mut self, span: f64) {
        // §15.6 per-advance state: the spill ledger and the exchange
        // totals reset; the same water cannot spill twice within one
        // routing step.
        for d in &mut self.node_drawn {
            *d = 0.0;
        }
        for e in &mut self.exchange {
            *e = 0.0;
        }
        let mut remaining = span;
        let nsub = 1u64 << self.lts_tiers.saturating_sub(1).min(7);
        let mut dt0 = self.stable_dt();
        while remaining > 1e-12 {
            // §15.4.4: the active set and the tiers rebuild every fourth
            // macro cycle; between rebuilds the base step may only
            // tighten, at macro-cycle boundaries, so every firing within
            // a cycle shares one consistent dt0 and every cell
            // integrates exactly nsub·dt0 per cycle.
            if self.macro_cycles.is_multiple_of(4) {
                self.settle_accumulators();
                self.rebuild_active();
                dt0 = self.stable_dt();
                self.assign_tiers(dt0);
            } else {
                dt0 = dt0.min(self.stable_dt());
            }
            if dt0 * nsub as f64 > remaining {
                // Tail: collapse every cell to tier 0 and land exactly.
                // The collapse is a re-tiering, so settle first (§15.4.4).
                self.settle_accumulators();
                for t in &mut self.tier {
                    *t = 0;
                }
                for t in &mut self.face_tier {
                    *t = 0;
                }
                while remaining > 1e-12 {
                    let dt = dt0.min(remaining);
                    self.fire_faces(dt, 0);
                    self.fire_cells(dt, 0);
                    self.fire_boundaries(dt);
                    self.fire_couplings(dt);
                    self.substeps += 1;
                    self.min_dt0 = self.min_dt0.min(dt);
                    self.advanced += dt;
                    remaining -= dt;
                }
                self.macro_cycles += 1;
                break;
            }
            for s in 0..nsub {
                self.fire_faces(dt0, s);
                self.fire_cells(dt0, s);
                self.fire_boundaries(dt0);
                self.fire_couplings(dt0);
            }
            self.substeps += nsub;
            self.min_dt0 = self.min_dt0.min(dt0);
            self.advanced += dt0 * nsub as f64;
            remaining -= dt0 * nsub as f64;
            self.macro_cycles += 1;
        }
    }

    /// §15.4.4 whole-run march counts for the §14.9 time-step summary:
    /// (base substeps, macro cycles, active-set rebuilds, minimum base
    /// step (s), average base step (s), peak active cells). The minimum
    /// and average read zero before any substep has fired.
    pub fn statistics(&self) -> (u64, u64, u64, f64, f64, usize) {
        let min = if self.substeps == 0 {
            0.0
        } else {
            self.min_dt0
        };
        let avg = if self.substeps == 0 {
            0.0
        } else {
            self.advanced / self.substeps as f64
        };
        (
            self.substeps,
            self.macro_cycles,
            self.rebuilds,
            min,
            avg,
            self.peak_active,
        )
    }

    /// A cell's §15.4.3 Perot flow proxy $\mathbf{q}_c$ (m²/s); its
    /// velocity is this over the depth.
    pub fn cell_velocity_proxy(&self, ci: usize) -> (f64, f64) {
        (self.qcx[ci], self.qcy[ci])
    }

    /// The §15.8 ledger's opening storage (m³).
    pub fn initial_storage(&self) -> f64 {
        self.storage0
    }

    /// §15.8 continuity error as a signed volume (m³): storage now
    /// against everything the ledger says arrived and left.
    pub fn ledger_error(&self) -> f64 {
        self.storage()
            - (self.storage0 + self.rain_in + self.coupling_in + self.outfall_in + self.boundary_in
                - self.evap_out
                - self.coupling_out
                - self.outfall_out
                - self.boundary_out)
    }

    /// Total stored volume (m³), the conservation ledger's storage term.
    pub fn storage(&self) -> f64 {
        self.vol.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overland::{MeshCell, MeshVertex, OverlandMesh};

    /// A 1 m wide strip of `nx` quads split into triangles, bed `z(x)`.
    fn strip(nx: usize, dx: f64, z: impl Fn(f64) -> f64) -> OverlandMesh {
        let mut mesh = OverlandMesh::default();
        for i in 0..=nx {
            let x = dx * i as f64;
            for y in [0.0, 1.0] {
                mesh.verts.push(MeshVertex {
                    x,
                    y,
                    z: z(x),
                    tag: None,
                });
            }
        }
        for i in 0..nx {
            let (a, b) = ((2 * i) as u32, (2 * i + 1) as u32);
            let (c, d) = ((2 * i + 2) as u32, (2 * i + 3) as u32);
            // Alternate the quad split so the diagonals herringbone:
            // one orientation biases the strip's effective conveyance.
            let (t1, t2) = if i % 2 == 0 {
                ([a, b, c], [b, d, c])
            } else {
                ([a, b, d], [a, d, c])
            };
            mesh.cells.push(MeshCell {
                v: t1,
                n: 0.03,
                h0: 0.0,
                tag: None,
            });
            mesh.cells.push(MeshCell {
                v: t2,
                n: 0.03,
                h0: 0.0,
                tag: None,
            });
        }
        mesh
    }

    fn fill_to_stage(mesh: &mut OverlandMesh, eta0: f64) {
        let cells = std::mem::take(&mut mesh.cells);
        mesh.cells = cells
            .into_iter()
            .map(|mut c| {
                let zm = c.v.iter().map(|&v| mesh.verts[v as usize].z).sum::<f64>() / 3.0;
                c.h0 = (eta0 - zm).max(0.0);
                c
            })
            .collect();
    }

    fn build(mesh: &OverlandMesh) -> Marcher {
        let topo = Topology::build(mesh).expect("valid mesh");
        Marcher::build(mesh, &topo)
    }

    /// §15.9: a lake at rest over an immersed bump stays exactly at
    /// rest, under both closures — the well-balancing property.
    #[test]
    fn a_lake_at_rest_stays_at_rest() {
        for closure in [CellClosure::Flat, CellClosure::Vfr] {
            let mut mesh = strip(20, 1.0, |x| {
                10.0 + 0.3 * (-((x - 10.0) / 3.0) * ((x - 10.0) / 3.0)).exp()
            });
            mesh.options.cell_closure = closure;
            fill_to_stage(&mut mesh, 10.6);
            let mut m = build(&mesh);
            let v0 = m.storage();
            m.advance(60.0);
            assert!(
                m.q.iter().all(|q| q.abs() < 1e-10),
                "{closure:?}: max q {}",
                m.q.iter().fold(0.0_f64, |a, q| a.max(q.abs()))
            );
            assert!(((m.storage() - v0) / v0).abs() < 1e-12, "{closure:?}");
        }
    }

    /// §15.9: an emerged bump keeps its dry crest dry — no uphill creep
    /// — while the wet flanks stay still.
    #[test]
    fn an_emerged_bump_stays_dry_and_still() {
        let mut mesh = strip(20, 1.0, |x| {
            10.0 + 0.5 * (-((x - 10.0) / 2.5) * ((x - 10.0) / 2.5)).exp()
        });
        fill_to_stage(&mut mesh, 10.2);
        let mut m = build(&mesh);
        let dry0: Vec<bool> = m.depth.iter().map(|&d| d == 0.0).collect();
        assert!(dry0.iter().any(|&d| d), "the crest starts dry");
        m.advance(60.0);
        for (ci, was_dry) in dry0.iter().enumerate() {
            if *was_dry {
                assert_eq!(m.vol[ci], 0.0, "cell {ci} crept uphill");
            }
        }
        assert!(m.q.iter().all(|q| q.abs() < 1e-10));
    }

    /// §15.9: a closed basin conserves — storage change equals the
    /// ledger exactly, rain and evaporation included.
    #[test]
    fn a_closed_basin_conserves_to_its_ledger() {
        let mut mesh = strip(12, 1.0, |x| 10.0 - 0.02 * x);
        fill_to_stage(&mut mesh, 10.05);
        let mut m = build(&mesh);
        let v0 = m.storage();
        for r in &mut m.rain {
            *r = 5.0e-6;
        }
        for e in &mut m.evap {
            *e = 1.0e-6;
        }
        m.advance(600.0);
        let expected = v0 + m.rain_in - m.evap_out + m.boundary_in - m.boundary_out;
        let scale = m.storage().max(1.0);
        assert!(
            ((m.storage() - expected) / scale).abs() < 1e-10,
            "storage {} vs ledger {expected}",
            m.storage()
        );
    }

    /// A rectangular grid of split quads, the reference configuration
    /// the steady-slope gates run on: `nx` by `ny` quads of side `dx`.
    fn grid(nx: usize, ny: usize, dx: f64, z: impl Fn(f64, f64) -> f64) -> OverlandMesh {
        let mut mesh = OverlandMesh::default();
        let nvx = nx + 1;
        for j in 0..=ny {
            for i in 0..=nx {
                let (x, y) = (dx * i as f64, dx * j as f64);
                mesh.verts.push(MeshVertex {
                    x,
                    y,
                    z: z(x, y),
                    tag: None,
                });
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                let v00 = (j * nvx + i) as u32;
                let v10 = (j * nvx + i + 1) as u32;
                let v01 = ((j + 1) * nvx + i) as u32;
                let v11 = ((j + 1) * nvx + i + 1) as u32;
                mesh.cells.push(MeshCell {
                    v: [v00, v10, v11],
                    n: 0.03,
                    h0: 0.0,
                    tag: None,
                });
                mesh.cells.push(MeshCell {
                    v: [v00, v11, v01],
                    n: 0.03,
                    h0: 0.0,
                    tag: None,
                });
            }
        }
        mesh
    }

    /// §15.9: steady rain on a sloped plane with a Manning outfall along
    /// the downhill boundary reaches the normal-depth equilibrium:
    /// outflow within 10% of the rain rate, mid-strip mean depth within
    /// 6% of Manning normal depth — the reference configuration and
    /// envelope (about 2.5% first-order staggering plus about 1% from
    /// the vector-magnitude friction at this resolution).
    #[test]
    fn steady_rain_on_a_slope_finds_normal_depth() {
        let (nx, ny, dx, s) = (40usize, 4usize, 1.0, 0.01);
        let rain = 2.0e-4; // 720 mm/h: normal depth ~18 mm, above the bands
        let mut mesh = grid(nx, ny, dx, |x, _| 10.0 + s * x);
        let topo = Topology::build(&mesh).expect("valid");
        // NORMAL_FLOW along the whole downhill (x = 0) boundary.
        let mut outlets = 0;
        for ci in 0..mesh.cells.len() {
            for e in 0..3usize {
                let slot = ci * 3 + e;
                if topo.neighbour[slot].is_none() && topo.edge_mid[slot][0] < 1e-9 {
                    mesh.boundaries.push(crate::overland::BoundaryRow {
                        cell: ci as u32,
                        edge: e as u8,
                        condition: BoundaryCondition::NormalFlow { slope: s },
                        group: None,
                    });
                    outlets += 1;
                }
            }
        }
        assert!(outlets > 0);
        let mut m = build(&mesh);
        for r in &mut m.rain {
            *r = rain;
        }
        m.advance(6000.0);
        let v_start = m.storage();
        let b_out_0 = m.boundary_out;
        let window = 1000.0;
        m.advance(window);
        // Exact ledger identity over the window.
        let rain_window = rain * m.area.iter().sum::<f64>() * window;
        let out_window = m.boundary_out - b_out_0;
        let ds = m.storage() - v_start;
        assert!(
            (rain_window - out_window - ds).abs() < 1e-8 * rain_window,
            "ledger: rain {rain_window} out {out_window} ds {ds}"
        );
        // Rate: outflow tracks the rain input.
        assert!(
            (out_window / rain_window - 1.0).abs() < 0.10,
            "outflow {out_window} vs rain {rain_window}"
        );
        // Mid-strip mean depth against the local normal depth.
        let l = nx as f64 * dx;
        let x_mid = 0.5 * l;
        let q_mid = rain * (l - x_mid);
        let h_n = (0.03 * q_mid / s.sqrt()).powf(3.0 / 5.0);
        let topo2 = Topology::build(&mesh).expect("valid");
        let (mut h_sum, mut h_cnt) = (0.0, 0u32);
        for ci in 0..m.depth.len() {
            if (topo2.centroid[ci][0] - x_mid).abs() < dx {
                h_sum += m.depth[ci];
                h_cnt += 1;
            }
        }
        let h_avg = h_sum / f64::from(h_cnt);
        assert!(
            (h_avg / h_n - 1.0).abs() < 0.06,
            "mid-strip depth {h_avg} vs normal {h_n}"
        );
    }

    /// A geometrically graded strip: cell sizes span a decade, so the
    /// tier ladder genuinely spreads. `ratio` grades each quad's width.
    fn graded_strip(nx: usize, ny: usize, dx0: f64, ratio: f64) -> OverlandMesh {
        let mut mesh = OverlandMesh::default();
        let mut xs = vec![0.0];
        let mut dx = dx0;
        for _ in 0..nx {
            xs.push(xs.last().unwrap() + dx);
            dx *= ratio;
        }
        let nvx = nx + 1;
        for j in 0..=ny {
            for &x in &xs {
                mesh.verts.push(MeshVertex {
                    x,
                    y: j as f64,
                    z: 10.0,
                    tag: None,
                });
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                let v00 = (j * nvx + i) as u32;
                let v10 = (j * nvx + i + 1) as u32;
                let v01 = ((j + 1) * nvx + i) as u32;
                let v11 = ((j + 1) * nvx + i + 1) as u32;
                mesh.cells.push(MeshCell {
                    v: [v00, v10, v11],
                    n: 0.10,
                    h0: 0.0,
                    tag: None,
                });
                mesh.cells.push(MeshCell {
                    v: [v00, v11, v01],
                    n: 0.10,
                    h0: 0.0,
                    tag: None,
                });
            }
        }
        mesh
    }

    /// The graded dam break at K tiers: 1.5 m charged over the fine end,
    /// fronts crossing every tier interface. Returns (volumes at t_mid,
    /// time-mean volumes over the last window, relative volume drift).
    fn graded_dam_break(k: u32, t_mid: f64, t_end: f64) -> (Vec<f64>, Vec<f64>, f64) {
        let mut mesh = graded_strip(24, 4, 1.0, 1.08);
        mesh.options.lts_tiers = k;
        let topo = Topology::build(&mesh).expect("valid");
        let cells = std::mem::take(&mut mesh.cells);
        mesh.cells = cells
            .into_iter()
            .enumerate()
            .map(|(ci, mut c)| {
                if topo.centroid[ci][0] < 4.0 {
                    c.h0 = 1.5;
                }
                c
            })
            .collect();
        let mut m = build(&mesh);
        let v0 = m.storage();
        let mut t = 0.0;
        while t < t_mid {
            m.advance(5.0f64.min(t_mid - t));
            t += 5.0;
        }
        let v_mid = m.vol.clone();
        // Time-mean over the last 60 s: the closed basin sustains a
        // weakly damped seiche, and an instantaneous sample aliases the
        // K-dependent phase; the mean is the settled solution.
        let mut acc = vec![0.0; m.vol.len()];
        let mut n_samples = 0u32;
        while t < t_end {
            m.advance(5.0f64.min(t_end - t));
            t += 5.0;
            if t >= t_end - 60.0 {
                for (a, v) in acc.iter_mut().zip(&m.vol) {
                    *a += v;
                }
                n_samples += 1;
            }
        }
        for a in &mut acc {
            *a /= f64::from(n_samples);
        }
        let drift = ((m.storage() - v0) / v0).abs();
        (v_mid, acc, drift)
    }

    /// §15.9: the march conserves exactly at every tier count, through a
    /// multiscale dam break with fronts crossing tier interfaces.
    #[test]
    fn lts_conserves_at_every_tier_count() {
        for k in [1u32, 2, 4, 6] {
            let (_, _, drift) = graded_dam_break(k, 60.0, 120.0);
            assert!(drift < 1e-10, "volume drift {drift} at K={k}");
        }
    }

    /// §15.9: the tiered march agrees with the global-step march — the
    /// mid-transient divergence decays rather than grows, and the
    /// settled solutions match to centimetres.
    #[test]
    fn lts_agrees_with_the_global_step() {
        let (mid1, end1, _) = graded_dam_break(1, 120.0, 600.0);
        let (mid4, end4, _) = graded_dam_break(4, 120.0, 600.0);
        let mesh = graded_strip(24, 4, 1.0, 1.08);
        let topo = Topology::build(&mesh).expect("valid");
        let depth_div = |a: &[f64], b: &[f64]| {
            a.iter()
                .zip(b)
                .enumerate()
                .map(|(ci, (x, y))| (x - y).abs() / topo.area[ci])
                .fold(0.0_f64, f64::max)
        };
        let max_mid = depth_div(&mid1, &mid4);
        let max_end = depth_div(&end1, &end4);
        assert!(
            max_end < max_mid,
            "tier divergence must decay, not grow: mid {max_mid} end {max_end}"
        );
        assert!(max_end <= 0.025, "settled max depth divergence {max_end}");
        // Peak settled depths agree within 5%.
        let peak = |v: &[f64]| {
            v.iter()
                .enumerate()
                .map(|(ci, x)| x / topo.area[ci])
                .fold(0.0_f64, f64::max)
        };
        let (p1, p4) = (peak(&end1), peak(&end4));
        assert!(
            (p4 - p1).abs() <= 0.05 * p1.max(1e-12),
            "peaks: global {p1} tiered {p4}"
        );
    }

    /// §15.4.2: at steady state the free surface is spatially smooth —
    /// no standing cell-to-cell stair from the split-quad centroid
    /// staggering. Guards the shared-edge sill datum, both closures.
    #[test]
    fn a_steady_slope_carries_no_checkerboard() {
        for closure in [CellClosure::Flat, CellClosure::Vfr] {
            let (nx, ny, dx, slope) = (40usize, 4usize, 1.0, 0.01);
            let mut mesh = grid(nx, ny, dx, |x, _| 10.0 + slope * x);
            mesh.options.cell_closure = closure;
            let topo = Topology::build(&mesh).expect("valid");
            for ci in 0..mesh.cells.len() {
                for e in 0..3usize {
                    let slot = ci * 3 + e;
                    if topo.neighbour[slot].is_none() && topo.edge_mid[slot][0] < 1e-9 {
                        mesh.boundaries.push(crate::overland::BoundaryRow {
                            cell: ci as u32,
                            edge: e as u8,
                            condition: BoundaryCondition::NormalFlow { slope },
                            group: None,
                        });
                    }
                }
            }
            let mut m = build(&mesh);
            for r in &mut m.rain {
                *r = 2.0e-4;
            }
            m.advance(6000.0);
            // Roughness of η against the face-neighbour mean over the
            // strip interior (the outlet drawdown and the upslope crest
            // carry genuine one-sided curvature).
            let (mut ss, mut mx, mut n) = (0.0_f64, 0.0_f64, 0u32);
            for ci in 0..m.eta.len() {
                let x = topo.centroid[ci][0];
                if !(5.0..=35.0).contains(&x) {
                    continue;
                }
                let (mut hs, mut hn) = (0.0, 0u32);
                for e in 0..3usize {
                    if let Some(nb) = topo.neighbour[ci * 3 + e] {
                        hs += m.eta[nb as usize];
                        hn += 1;
                    }
                }
                if hn < 3 {
                    continue;
                }
                let r = m.eta[ci] - hs / f64::from(hn);
                ss += r * r;
                mx = mx.max(r.abs());
                n += 1;
            }
            let rms = (ss / f64::from(n.max(1))).sqrt();
            assert!(rms < 1.2e-3, "{closure:?}: surface roughness rms {rms}");
            assert!(mx < 3.5e-3, "{closure:?}: surface roughness max {mx}");
        }
    }

    /// §15.4.2: on a steep dam-break every published face discharge obeys
    /// the Froude cap against the clamp's own face depth, and nothing
    /// runs away.
    #[test]
    fn the_froude_clamp_bounds_every_face() {
        let mut mesh = grid(20, 3, 1.0, |x, _| 10.0 - 0.10 * x);
        let topo = Topology::build(&mesh).expect("valid");
        let cells = std::mem::take(&mut mesh.cells);
        mesh.cells = cells
            .into_iter()
            .enumerate()
            .map(|(ci, mut c)| {
                c.n = 0.015;
                if topo.centroid[ci][0] < 3.0 {
                    c.h0 = 1.0;
                }
                c
            })
            .collect();
        let mut m = build(&mesh);
        for _ in 0..40 {
            m.advance(1.0);
            for &v in &m.vol {
                assert!(v.is_finite());
            }
            for (fi, f) in m.faces.iter().enumerate() {
                let q = m.published_face_q(fi).abs();
                if q == 0.0 {
                    continue;
                }
                let h_f = m.eta[f.cl as usize].max(m.eta[f.cr as usize]) - f.z_face;
                assert!(h_f > 0.0, "published flux across a dry face");
                let cap = m.froude_max * h_f * (G * h_f).sqrt();
                assert!(q <= cap * (1.0 + 1e-9), "face {fi}: {q} above cap {cap}");
            }
        }
    }

    /// §15.3: a one-vertex-wide ridge holds a pool under `VFR_FACE` (the
    /// edge's own endpoint beds wall it) and leaks under `MEAN` (the
    /// centroid-diluted face bed conveys) — the documented defect the
    /// option exists to fix.
    #[test]
    fn a_thin_crest_holds_under_vfr_face_and_leaks_under_mean() {
        let ridge = |x: f64, _: f64| if (x - 8.0).abs() < 1e-9 { 1.0 } else { 0.0 };
        let seed = |rec: FaceReconstruction| {
            let mut mesh = grid(8, 4, 2.0, ridge);
            mesh.options.face_reconstruction = rec;
            fill_to_stage(&mut mesh, 0.9);
            let topo = Topology::build(&mesh).expect("valid");
            let cells = std::mem::take(&mut mesh.cells);
            mesh.cells = cells
                .into_iter()
                .enumerate()
                .map(|(ci, mut c)| {
                    if topo.centroid[ci][0] > 8.0 {
                        c.h0 = 0.0;
                    }
                    c
                })
                .collect();
            (build(&mesh), topo)
        };

        // VFR_FACE walls the crest: the right half stays bone dry and the
        // pool keeps its volume.
        let (mut m, topo) = seed(FaceReconstruction::VfrFace);
        let v0 = m.vol.clone();
        m.advance(600.0);
        for (ci, (&v, &v_init)) in m.vol.iter().zip(&v0).enumerate() {
            if topo.centroid[ci][0] > 8.0 {
                assert_eq!(v, 0.0, "VFR_FACE leaked at cell {ci}");
            } else {
                assert!(
                    (v - v_init).abs() <= 1e-9 * (v_init + 1.0),
                    "pool cell {ci} moved: {v} vs {v_init}"
                );
            }
        }

        // MEAN leaks through the centroid-diluted crest.
        let (mut m, topo) = seed(FaceReconstruction::Mean);
        m.advance(600.0);
        let crossed: f64 = (0..m.vol.len())
            .filter(|&ci| topo.centroid[ci][0] > 8.0)
            .map(|ci| m.vol[ci])
            .sum();
        assert!(crossed > 0.0, "expected the centroid face bed to leak");
    }

    /// §15.9 SWASHES: subcritical flow over a bump, against the exact
    /// Bernoulli cubic. The scheme omits convective acceleration, so its
    /// frictionless steady surface is flat and the residual is exactly
    /// the velocity-head dip over the bump; the grade bounds the
    /// relative L1 depth error at 1.5%.
    #[test]
    fn swashes_subcritical_flow_over_a_bump() {
        let (nx, ny, dx) = (100usize, 2usize, 0.25);
        let l = nx as f64 * dx;
        let (q_in, h_out) = (4.42, 2.0);
        let bump = |x: f64, _: f64| {
            if x > 8.0 && x < 12.0 {
                0.2 - 0.05 * (x - 10.0) * (x - 10.0)
            } else {
                0.0
            }
        };
        let mut mesh = grid(nx, ny, dx, bump);
        let cells = std::mem::take(&mut mesh.cells);
        mesh.cells = cells
            .into_iter()
            .map(|mut c| {
                c.n = 1e-6; // the case is frictionless
                c
            })
            .collect();
        fill_to_stage(&mut mesh, h_out);
        let topo = Topology::build(&mesh).expect("valid");
        for ci in 0..mesh.cells.len() {
            for e in 0..3usize {
                let slot = ci * 3 + e;
                if topo.neighbour[slot].is_some() {
                    continue;
                }
                let x = topo.edge_mid[slot][0];
                if x < 1e-9 {
                    // Inflow: authored outward-positive, so inflow is
                    // negative (§15.5).
                    mesh.boundaries.push(crate::overland::BoundaryRow {
                        cell: ci as u32,
                        edge: e as u8,
                        condition: BoundaryCondition::Flow(SeriesOrValue::Value(-q_in)),
                        group: None,
                    });
                } else if x > l - 1e-9 {
                    mesh.boundaries.push(crate::overland::BoundaryRow {
                        cell: ci as u32,
                        edge: e as u8,
                        condition: BoundaryCondition::Stage(SeriesOrValue::Value(h_out)),
                        group: None,
                    });
                }
            }
        }
        let mut m = build(&mesh);
        // §15.9: between a specified-flow inlet and a stage outlet the
        // frictionless case holds an undamped standing wave; the grade
        // reads the time-mean field, which is the steady solution.
        m.advance(300.0);
        let mut mean = vec![0.0; m.depth.len()];
        let samples = 300u32;
        for _ in 0..samples {
            m.advance(1.0);
            for (a, h) in mean.iter_mut().zip(&m.depth) {
                *a += h;
            }
        }
        for a in &mut mean {
            *a /= f64::from(samples);
        }

        // The exact profile: h³ + (z − H)h² + q²/2g = 0, subcritical
        // root, H the outlet's total head.
        let head = h_out + q_in * q_in / (2.0 * G * h_out * h_out);
        let exact = |z: f64| {
            let mut h = (head - z).max(0.1);
            for _ in 0..60 {
                let f = h * h * h + (z - head) * h * h + q_in * q_in / (2.0 * G);
                let df = 3.0 * h * h + 2.0 * (z - head) * h;
                h -= f / df;
            }
            h
        };
        let (mut err, mut norm) = (0.0, 0.0);
        for (ci, &h) in mean.iter().enumerate() {
            let x = topo.centroid[ci][0];
            let h_ref = exact(bump(x, 0.0));
            err += (h - h_ref).abs() * topo.area[ci];
            norm += h_ref * topo.area[ci];
        }
        assert!(
            err / norm < 0.0125,
            "SWASHES bump relative L1 depth error {}",
            err / norm
        );
    }

    /// §15.9 SWASHES 3.2.1: the MacDonald 1000 m subcritical profile.
    /// The bed is derived from the full steady momentum equation for a
    /// prescribed depth profile, so the analytic solution is exact by
    /// construction; the scheme omits convective acceleration, and its
    /// steady surface differs by the integrated velocity-head term. The
    /// grade bounds the relative L1 depth error of the settled
    /// time-mean field at 2.5% (the predecessor's own marcher measures
    /// 2.0% on this case).
    #[test]
    fn macdonald_subcritical_profile() {
        let (l, q_in, n_man) = (1000.0, 2.0, 0.033);
        let a_h = (4.0 / G).powf(1.0 / 3.0);
        let h_ex = |x: f64| a_h * (1.0 + 0.5 * (-16.0 * (x / l - 0.5).powi(2)).exp());
        let dh_ex = |x: f64| {
            a_h * 0.5 * (-16.0 * (x / l - 0.5).powi(2)).exp() * (-32.0 * (x / l - 0.5) / l)
        };
        // Bed from the full steady momentum equation, z(L) = 0:
        // z(x) = ∫ₓᴸ [(1 − Fr²) h′ + n² q² / h^{10/3}] ds.
        let integrand = |x: f64| {
            let h = h_ex(x);
            let fr2 = q_in * q_in / (G * h * h * h);
            (1.0 - fr2) * dh_ex(x) + n_man * n_man * q_in * q_in / h.powf(10.0 / 3.0)
        };
        // Cumulative trapezoid on a fine grid, then linear lookup.
        let m = 4000usize;
        let ds = l / m as f64;
        let mut z_tab = vec![0.0; m + 1];
        for i in (0..m).rev() {
            let x0 = ds * i as f64;
            z_tab[i] = z_tab[i + 1] + 0.5 * ds * (integrand(x0) + integrand(x0 + ds));
        }
        let z_of = |x: f64| {
            let f = (x / ds).clamp(0.0, m as f64);
            let i = (f as usize).min(m - 1);
            let t = f - i as f64;
            z_tab[i] * (1.0 - t) + z_tab[i + 1] * t
        };

        let (nx, ny, dx) = (200usize, 2usize, 5.0);
        let mut mesh = grid(nx, ny, dx, |x, _| z_of(x));
        let topo = Topology::build(&mesh).expect("valid");
        let cells = std::mem::take(&mut mesh.cells);
        mesh.cells = cells
            .into_iter()
            .enumerate()
            .map(|(ci, mut c)| {
                c.n = n_man;
                c.h0 = h_ex(topo.centroid[ci][0]);
                c
            })
            .collect();
        for ci in 0..mesh.cells.len() {
            for e in 0..3usize {
                let slot = ci * 3 + e;
                if topo.neighbour[slot].is_some() {
                    continue;
                }
                let x = topo.edge_mid[slot][0];
                if x < 1e-9 {
                    mesh.boundaries.push(crate::overland::BoundaryRow {
                        cell: ci as u32,
                        edge: e as u8,
                        condition: BoundaryCondition::Flow(SeriesOrValue::Value(-q_in)),
                        group: None,
                    });
                } else if x > l - 1e-9 {
                    mesh.boundaries.push(crate::overland::BoundaryRow {
                        cell: ci as u32,
                        edge: e as u8,
                        condition: BoundaryCondition::Stage(SeriesOrValue::Value(h_ex(l))),
                        group: None,
                    });
                }
            }
        }
        let mut m = build(&mesh);
        // Settle, then grade the time mean of the final half.
        m.advance(2000.0);
        let mut mean = vec![0.0; m.depth.len()];
        let samples = 400u32;
        for _ in 0..samples {
            m.advance(5.0);
            for (a, h) in mean.iter_mut().zip(&m.depth) {
                *a += h;
            }
        }
        for a in &mut mean {
            *a /= f64::from(samples);
        }
        let (mut err, mut norm) = (0.0, 0.0);
        for (ci, &h) in mean.iter().enumerate() {
            let h_ref = h_ex(topo.centroid[ci][0]);
            err += (h - h_ref).abs() * topo.area[ci];
            norm += h_ref * topo.area[ci];
        }
        assert!(
            err / norm < 0.025,
            "MacDonald subcritical relative L1 depth error {}",
            err / norm
        );
        assert!(
            m.ledger_error().abs() < 1e-6 * m.storage(),
            "mass error {}",
            m.ledger_error()
        );
    }

    /// §15.5: a series-driven slot is a wall until its first
    /// resolution, and conveys from then on.
    #[test]
    fn a_driven_boundary_is_a_wall_until_resolved() {
        let mut mesh = grid(3, 3, 1.0, |_, _| 10.0);
        fill_to_stage(&mut mesh, 10.4);
        let topo = Topology::build(&mesh).expect("valid");
        let (ci, e) = (0..mesh.cells.len() * 3)
            .find(|slot| topo.neighbour[*slot].is_none())
            .map(|slot| (slot / 3, slot % 3))
            .expect("boundary edge");
        mesh.boundaries.push(crate::overland::BoundaryRow {
            cell: ci as u32,
            edge: e as u8,
            condition: BoundaryCondition::Flow(SeriesOrValue::Series("QB".into())),
            group: None,
        });
        let mut m = build(&mesh);
        assert_eq!(m.driven_boundaries().len(), 1);
        assert_eq!(m.driven_boundaries()[0].series, "QB");
        let v0 = m.storage();
        m.advance(30.0);
        assert!(
            (m.storage() - v0).abs() < 1e-12,
            "unresolved slot must wall"
        );
        let bi = m.driven_boundaries()[0].boundary;
        m.set_boundary_flow(bi, 0.05);
        m.advance(30.0);
        assert!(m.storage() < v0, "resolved slot must convey");
        assert!(m.boundary_out > 0.0);
    }

    /// §15.5: a rating outlet reads its curve of stage above the edge
    /// sill at every firing — linear between points, held at the ends,
    /// dry-gated.
    #[test]
    fn a_rating_outlet_follows_its_curve() {
        let mut mesh = grid(3, 3, 1.0, |_, _| 10.0);
        fill_to_stage(&mut mesh, 10.5);
        let topo = Topology::build(&mesh).expect("valid");
        let (ci, e) = (0..mesh.cells.len() * 3)
            .find(|slot| topo.neighbour[*slot].is_none())
            .map(|slot| (slot / 3, slot % 3))
            .expect("boundary edge");
        mesh.boundaries.push(crate::overland::BoundaryRow {
            cell: ci as u32,
            edge: e as u8,
            condition: BoundaryCondition::RatingCurve { curve: "RC".into() },
            group: None,
        });
        let mut m = build(&mesh);
        assert_eq!(m.rating_curve_names(), ["RC"]);
        // Unsupplied curve: wall.
        let v0 = m.storage();
        m.advance(10.0);
        assert!((m.storage() - v0).abs() < 1e-12);
        // Head 0.5 on a curve rising 0 → 0.5 m²/s over a metre of
        // head: q = 0.25 per metre of edge.
        m.set_rating_curve(0, vec![(0.0, 0.0), (1.0, 0.5)]);
        let xi = 1.0; // grid edges are dx long
        let before = m.boundary_out;
        m.advance(4.0);
        let out = m.boundary_out - before;
        let expect = 0.25 * xi * 4.0;
        assert!(
            (out / expect - 1.0).abs() < 0.10,
            "rated outflow {out} vs {expect}"
        );
        // Held at the top end: far above the last point the discharge
        // is the last point's.
        let mut mesh2 = grid(3, 3, 1.0, |_, _| 10.0);
        fill_to_stage(&mut mesh2, 13.0);
        mesh2.boundaries.push(crate::overland::BoundaryRow {
            cell: ci as u32,
            edge: e as u8,
            condition: BoundaryCondition::RatingCurve { curve: "RC".into() },
            group: None,
        });
        let mut m2 = build(&mesh2);
        m2.set_rating_curve(0, vec![(0.0, 0.0), (1.0, 0.5)]);
        let before = m2.boundary_out;
        m2.advance(2.0);
        let out = m2.boundary_out - before;
        let expect = 0.5 * xi * 2.0;
        assert!(
            (out / expect - 1.0).abs() < 0.10,
            "clamped outflow {out} vs {expect}"
        );
    }

    /// §15.4.5: the same march, serial and across the team, writes
    /// byte-identical state — the §6.4 width contract, held for the
    /// surface. The case is big enough that the team genuinely engages
    /// (faces and cells both above the dispatch grain) and busy enough
    /// that every phase does real work: a dam break over a graded
    /// strip, rain, an evaporating film, and a normal-flow outlet,
    /// marched through tier rebuilds.
    #[cfg(feature = "threads")]
    #[test]
    fn the_march_is_byte_identical_at_width() {
        let build_case = || {
            let mut mesh = graded_strip(100, 12, 1.0, 1.02);
            let topo = Topology::build(&mesh).expect("valid");
            let cells = std::mem::take(&mut mesh.cells);
            mesh.cells = cells
                .into_iter()
                .enumerate()
                .map(|(ci, mut c)| {
                    if topo.centroid[ci][0] < 10.0 {
                        c.h0 = 1.0;
                    }
                    c
                })
                .collect();
            for ci in 0..mesh.cells.len() {
                for e in 0..3usize {
                    let slot = ci * 3 + e;
                    if topo.neighbour[slot].is_none() && topo.edge_mid[slot][0] < 1e-9 {
                        mesh.boundaries.push(crate::overland::BoundaryRow {
                            cell: ci as u32,
                            edge: e as u8,
                            condition: BoundaryCondition::NormalFlow { slope: 0.005 },
                            group: None,
                        });
                    }
                }
            }
            let mut m = build(&mesh);
            for r in &mut m.rain {
                *r = 1e-5;
            }
            for ev in &mut m.evap {
                *ev = 1e-6;
            }
            m
        };
        let mut serial = build_case();
        for _ in 0..12 {
            serial.advance(5.0);
        }
        for width in [3usize, 4] {
            let mut teamed = build_case();
            teamed.set_width(width);
            assert!(
                teamed.faces.len() >= PAR_GRAIN * width,
                "the case must be big enough to engage the team"
            );
            for _ in 0..12 {
                teamed.advance(5.0);
            }
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&serial.vol), bits(&teamed.vol), "vol at width {width}");
            assert_eq!(bits(&serial.eta), bits(&teamed.eta), "eta at width {width}");
            assert_eq!(bits(&serial.q), bits(&teamed.q), "q at width {width}");
            assert_eq!(bits(&serial.qcx), bits(&teamed.qcx), "qcx at width {width}");
            assert_eq!(
                serial.rain_in.to_bits(),
                teamed.rain_in.to_bits(),
                "rain ledger at width {width}"
            );
            assert_eq!(serial.evap_out.to_bits(), teamed.evap_out.to_bits());
            assert_eq!(serial.boundary_out.to_bits(), teamed.boundary_out.to_bits());
        }
    }

    /// §15.6: a vertex row collapses to the lowest-bed incident cell,
    /// two spellings of one vertex are one coupling (last wins), and
    /// points naming one node share a slot.
    #[test]
    fn a_vertex_coupling_collapses_to_the_lowest_bed_cell() {
        let mut mesh = grid(2, 2, 1.0, |x, y| 10.0 - 0.5 * x - 0.1 * y);
        let row = |address: &str, node: &str, cd: f64| crate::overland::CouplingRow {
            address: address.into(),
            node: node.into(),
            cd,
            area: 1.0,
            area_authored: false,
        };
        // Vertex 4 = (1, 1), interior: six incident cells.
        mesh.vertex_couplings.push(row("4", "J1", 0.5));
        mesh.vertex_couplings.push(row("4", "J1", 0.7)); // same vertex: last wins
        mesh.cell_couplings.push(row("0", "J2", 0.65));
        mesh.cell_couplings.push(row("3", "J1", 0.65)); // same node: same slot
        let m = build(&mesh);
        let pts = m.coupling_points();
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].cd, 0.7, "last authored row wins");
        // The collapse cell is the lowest-bed incident cell of vertex 4.
        let topo = Topology::build(&mesh).expect("valid");
        let v = &mesh.verts[4];
        let lowest = (0..mesh.cells.len())
            .filter(|&ci| mesh.cells[ci].v.contains(&4))
            .min_by(|&a, &b| topo.centroid[a][2].total_cmp(&topo.centroid[b][2]))
            .expect("incident");
        assert_eq!(pts[0].cell as usize, lowest);
        assert_eq!(pts[0].vertex, Some((v.x, v.y, v.z)));
        assert_eq!(m.coupling_nodes(), ["J1", "J2"]);
        assert_eq!(pts[0].node_slot, 0);
        assert_eq!(pts[1].node_slot, 1);
        assert_eq!(pts[2].node_slot, 0);
    }

    /// §15.6: a ponded surface drains through a coupling at the orifice
    /// rate, the ledger closes exactly, and the conductance is live.
    #[test]
    fn a_pond_drains_through_a_coupling_at_the_orifice_rate() {
        let mut mesh = grid(4, 4, 1.0, |_, _| 10.0);
        fill_to_stage(&mut mesh, 10.5);
        mesh.cell_couplings.push(crate::overland::CouplingRow {
            address: "0".into(),
            node: "J1".into(),
            cd: 0.65,
            area: 0.05,
            area_authored: true,
        });
        let mut m = build(&mesh);
        let v0 = m.storage();
        m.set_node_drive(0, 8.0, 2.0, 10.0, 0.0);
        assert!(m.coupling_conductance(0) > 0.0);
        m.advance(1.0);
        // First-second drain within 10% of the initial orifice rate
        // (the local drawdown cone lowers the driving head a little).
        let q0 = 0.65 * (2.0 * 0.05) * (2.0 * G).sqrt() * 2.5_f64.sqrt();
        assert!(
            (m.coupling_out / q0 - 1.0).abs() < 0.10,
            "drained {} vs orifice {q0}",
            m.coupling_out
        );
        assert_eq!(m.exchanged().len(), 1);
        assert!((m.exchanged()[0] - m.coupling_out).abs() < 1e-12);
        m.advance(119.0);
        // Ledger identity, exactly; the pond is nearly gone.
        assert!(
            (v0 - m.storage() - m.coupling_out).abs() < 1e-9,
            "ledger: {} drained, {} missing",
            m.coupling_out,
            v0 - m.storage()
        );
        assert_eq!(m.coupling_in, 0.0);
        assert!(m.storage() < 0.05 * v0, "pond still holds {}", m.storage());
    }

    /// §15.6: a spill draws against the node's advance-wide ledger —
    /// the same water cannot spill twice within one routing step.
    #[test]
    fn a_spill_cannot_draw_the_same_water_twice() {
        let mut mesh = grid(4, 4, 1.0, |_, _| 10.0);
        mesh.cell_couplings.push(crate::overland::CouplingRow {
            address: "0".into(),
            node: "J1".into(),
            cd: 0.65,
            area: 0.05,
            area_authored: true,
        });
        let mut m = build(&mesh);
        m.set_node_drive(0, 11.0, 1.0, 10.0, 0.05);
        m.advance(10.0);
        assert!(
            (m.coupling_in - 0.05).abs() < 1e-12,
            "spilled {}",
            m.coupling_in
        );
        assert!((m.storage() - 0.05).abs() < 1e-12);
        // A further advance re-arms the budget.
        m.set_node_drive(0, 11.0, 1.0, 10.0, 0.05);
        m.advance(10.0);
        assert!((m.coupling_in - 0.10).abs() < 1e-12);
    }

    /// §15.6: equal heads exchange nothing and leave the basin at rest.
    #[test]
    fn equal_heads_exchange_nothing() {
        let mut mesh = grid(3, 3, 1.0, |_, _| 10.0);
        fill_to_stage(&mut mesh, 10.5);
        mesh.cell_couplings.push(crate::overland::CouplingRow {
            address: "0".into(),
            node: "J1".into(),
            cd: 0.65,
            area: 1.0,
            area_authored: true,
        });
        let mut m = build(&mesh);
        let v0 = m.storage();
        m.set_node_drive(0, 10.5, 0.5, 10.0, 1.0);
        m.advance(30.0);
        assert_eq!(m.coupling_out, 0.0);
        assert_eq!(m.coupling_in, 0.0);
        assert!((m.storage() - v0).abs() < 1e-12);
    }

    /// §15.6: outfall injection scatters down the surface slope from
    /// the vertex, and falls back to area weights on a flat surface.
    #[test]
    fn injection_scatters_downhill_from_the_vertex() {
        let row = |address: &str| crate::overland::CouplingRow {
            address: address.into(),
            node: "OUT".into(),
            cd: 0.65,
            area: 1.0,
            area_authored: true,
        };
        // Sloped and dry: η_v is the vertex ground elevation, cells
        // downhill of it weight positive, uphill cells get nothing.
        let mut mesh = grid(2, 2, 1.0, |x, _| 10.0 - 0.5 * x);
        mesh.vertex_couplings.push(row("4")); // (1, 1), z = 9.5
        let mut m = build(&mesh);
        m.inject(0, 0.1);
        let total: f64 = m.coupling.iter().zip(&m.area).map(|(r, a)| r * a).sum();
        assert!((total - 0.1).abs() < 1e-12, "scatter conserves the rate");
        let topo = Topology::build(&mesh).expect("valid");
        for (ci, r) in m.coupling.iter().enumerate() {
            if *r > 0.0 {
                assert!(
                    topo.centroid[ci][2] < 9.5,
                    "cell {ci} is not downhill of the vertex"
                );
            }
        }
        // Flat: area weights over the whole stencil.
        let mut mesh = grid(2, 2, 1.0, |_, _| 10.0);
        mesh.vertex_couplings.push(row("4"));
        let mut m = build(&mesh);
        m.inject(0, 0.1);
        let stencil: Vec<usize> = (0..mesh.cells.len())
            .filter(|&ci| mesh.cells[ci].v.contains(&4))
            .collect();
        let asum: f64 = stencil.iter().map(|&ci| m.area[ci]).sum();
        for &ci in &stencil {
            assert!((m.coupling[ci] - 0.1 / asum).abs() < 1e-12);
        }
        m.clear_injection();
        assert!(m.coupling.iter().all(|&r| r == 0.0));
    }

    /// §15.5: a stage boundary fills a dry basin toward its stage and
    /// never overshoots the equilibrium; the ledger matches the water.
    #[test]
    fn a_stage_boundary_fills_to_equilibrium() {
        let mut mesh = strip(10, 1.0, |_| 10.0);
        mesh.boundaries.push(crate::overland::BoundaryRow {
            cell: 0,
            edge: 2,
            condition: BoundaryCondition::Stage(SeriesOrValue::Value(10.3)),
            group: None,
        });
        let mut m = build(&mesh);
        // The filling front carries momentum, so cells overshoot the
        // stage transiently; the settled state is the claim. The fed
        // cell's equilibrium clamp bounds what the boundary admits.
        for _ in 0..100 {
            m.advance(30.0);
        }
        for (ci, &eta) in m.eta.iter().enumerate() {
            assert!(
                (eta - 10.3).abs() < 5e-3,
                "cell {ci} settled off-stage: {eta}"
            );
        }
        let expected = m.boundary_in - m.boundary_out;
        assert!(
            ((m.storage() - expected) / m.storage()).abs() < 1e-10,
            "ledger closes"
        );
    }
}
