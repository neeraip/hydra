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
use super::{
    BoundaryCondition, CellClosure, FaceReconstruction, OverlandMesh, SeriesOrValue, Topology,
};

/// §15.1: standard gravity (m/s²).
const G: f64 = 9.80665;

/// §15.4.2: closure round-off must not masquerade as slope (m).
const ETA_DEADBAND: f64 = 1e-12;

/// §15.4.2: the exporting cell's per-face volume share β.
const BETA: f64 = 0.8;

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
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BoundaryLaw {
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
    area: Vec<f64>,
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
    substep: u64,

    // ── Sources, set by the caller before an advance (m/s) ─────────
    pub rain: Vec<f64>,
    pub evap: Vec<f64>,
    pub coupling: Vec<f64>,

    // ── §15.8 ledger (m³ since construction) ───────────────────────
    pub rain_in: f64,
    pub evap_out: f64,
    pub boundary_in: f64,
    pub boundary_out: f64,
}

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
        for row in &mesh.boundaries {
            let law = match &row.condition {
                BoundaryCondition::Wall => continue,
                BoundaryCondition::NormalFlow { slope } => {
                    BoundaryLaw::NormalFlow { slope: *slope }
                }
                BoundaryCondition::Flow(SeriesOrValue::Value(v)) => {
                    BoundaryLaw::Flow { q_per_m: *v }
                }
                BoundaryCondition::Stage(SeriesOrValue::Value(v)) => BoundaryLaw::Stage { eta: *v },
                // Series and curves resolve to per-advance values at the
                // wiring layer; until then the slot is a wall, which is
                // the refusal-safe reading of an unresolved parameter.
                BoundaryCondition::Flow(SeriesOrValue::Series(_))
                | BoundaryCondition::Stage(SeriesOrValue::Series(_))
                | BoundaryCondition::RatingCurve { .. } => continue,
            };
            let slot = row.cell as usize * 3 + row.edge as usize;
            let xi = topo.edge_len[slot];
            boundaries.push(Boundary {
                cell: row.cell,
                xi,
                law,
                mx: topo.edge_mid[slot][0],
                my: topo.edge_mid[slot][1],
                inv_dn_ghost: 3.0 * xi / (2.0 * topo.area[row.cell as usize]),
                q: 0.0,
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
            substep: 0,
            rain: vec![0.0; nc],
            evap: vec![0.0; nc],
            coupling: vec![0.0; nc],
            rain_in: 0.0,
            evap_out: 0.0,
            boundary_in: 0.0,
            boundary_out: 0.0,
        };
        for ci in 0..nc {
            m.reclose(ci);
        }
        m.refresh_perot();
        m.rebuild_active();
        m
    }

    /// Re-derive a cell's surface from its volume (§15.3).
    fn reclose(&mut self, ci: usize) {
        let h = self.vol[ci].max(0.0) / self.area[ci];
        self.depth[ci] = h;
        self.eta[ci] = match self.closure {
            CellClosure::Flat => flat_eta(&self.bed[ci], h),
            CellClosure::Vfr => vfr_eta(&self.bed[ci], h, self.vfr_eps),
        };
    }

    /// §15.4.3 Perot reconstruction over each cell's faces, fixed order:
    /// $\mathbf{q}_c = \frac1A \sum_e s_e q_e \xi_e (\mathbf{m}_e -
    /// \mathbf{c})$ — the midpoint offset, not the normal; the two differ
    /// on any skewed cell, and the normal form was measured to destabilise
    /// the march (waves growing from the θ-blend feeding on itself).
    fn refresh_perot(&mut self) {
        for ci in 0..self.area.len() {
            let (mut sx, mut sy) = (0.0, 0.0);
            let lo = self.cf_off[ci] as usize;
            let hi = self.cf_off[ci + 1] as usize;
            for &(fi, sign) in &self.cf_face[lo..hi] {
                let f = &self.faces[fi as usize];
                let flux = sign * self.q[fi as usize] * f.xi;
                sx += flux * (f.mx - self.cx[ci]);
                sy += flux * (f.my - self.cy[ci]);
            }
            self.qcx[ci] = sx / self.area[ci];
            self.qcy[ci] = sy / self.area[ci];
        }
        // §15.5 completion: a boundary edge's discharge belongs to its
        // cell's reconstruction too, in the same outward-flux convention
        // (the prognostic is inflow-positive, so outward is its negation).
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
        self.active = next;
    }

    /// §15.4.4: the stable step over active cells.
    fn stable_dt(&self) -> f64 {
        let mut dt = self.max_dt;
        for ci in 0..self.area.len() {
            if !self.active[ci] || self.depth[ci] <= self.dry_depth {
                continue;
            }
            let h = self.depth[ci];
            let speed = (G * h).sqrt() + (self.qcx[ci].hypot(self.qcy[ci])) / h;
            if self.metric[ci] > 0.0 && speed > 0.0 {
                let l = (2.0 * self.area[ci] / self.metric[ci]).sqrt();
                dt = dt.min(self.cfl * l / speed);
            }
        }
        dt
    }

    /// §15.4.2 ∥ face phase (serial in this slice; per-face writes only).
    fn fire_faces(&mut self, dt: f64) {
        for fi in 0..self.faces.len() {
            let f = &self.faces[fi];
            let (cl, cr) = (f.cl as usize, f.cr as usize);
            // Both-active gating: a one-sided face loses basin volume.
            if !self.active[cl] || !self.active[cr] {
                self.q[fi] = 0.0;
                continue;
            }
            let h_f = match self.face_rec {
                FaceReconstruction::Mean => face_depth_mean(self.eta[cl], self.eta[cr], f.z_face),
                FaceReconstruction::VfrFace => {
                    face_depth_vfr(self.eta[cl], self.eta[cr], f.z_lo, f.z_hi)
                }
            };
            if h_f <= self.dry_depth {
                self.q[fi] = 0.0;
                continue;
            }
            let mut d_eta = self.eta[cr] - self.eta[cl];
            if d_eta.abs() < ETA_DEADBAND {
                d_eta = 0.0;
            }
            let slope = d_eta * f.inv_dn;
            let q_perot =
                0.5 * ((self.qcx[cl] + self.qcx[cr]) * f.nx + (self.qcy[cl] + self.qcy[cr]) * f.ny);
            let q_hat = self.theta * self.q[fi] + (1.0 - self.theta) * q_perot;
            let q_mag = self.q[fi]
                .abs()
                .max(0.5 * (self.qcx[cl] + self.qcx[cr]).hypot(self.qcy[cl] + self.qcy[cr]));
            let h73 = h_f * h_f * h_f.cbrt();
            let mut q_new = (q_hat - dt * G * h_f * slope) / (1.0 + G * dt * f.n2 * q_mag / h73);
            // Froude cap.
            let q_cap = self.froude_max * h_f * (G * h_f).sqrt();
            q_new = q_new.clamp(-q_cap, q_cap);
            // Positivity: the exporter grants at most β/3 of its volume
            // per cell cycle.
            let exporter = if q_new > 0.0 { cl } else { cr };
            let budget = BETA / 3.0 * self.vol[exporter].max(0.0);
            let take = q_new.abs() * f.psi * f.xi * dt;
            if take > budget && take > 0.0 {
                q_new *= budget / take;
            }
            self.q[fi] = q_new;
            let dm = f.psi * q_new * f.xi * dt;
            self.facc_l[fi] -= dm;
            self.facc_r[fi] += dm;
        }
    }

    /// §15.4.3 ∥ cell phase: gather own sides in fixed order, apply
    /// sources, clamp, re-close.
    fn fire_cells(&mut self, dt: f64) {
        for ci in 0..self.area.len() {
            let lo = self.cf_off[ci] as usize;
            let hi = self.cf_off[ci + 1] as usize;
            let mut flux = 0.0;
            for &(fi, sign) in &self.cf_face[lo..hi] {
                let fi = fi as usize;
                if sign > 0.0 {
                    flux += self.facc_l[fi];
                    self.facc_l[fi] = 0.0;
                } else {
                    flux += self.facc_r[fi];
                    self.facc_r[fi] = 0.0;
                }
            }
            let a = self.area[ci];
            let rain = self.rain[ci] * a * dt;
            let coup = self.coupling[ci] * a * dt;
            self.rain_in += rain;
            // §15.4.3: evaporation shuts off C¹ as the cell dries, and
            // takes no more than the cell holds.
            let t = (self.depth[ci] / self.dry_depth).clamp(0.0, 1.0);
            let ramp = t * t * (3.0 - 2.0 * t);
            let want = self.evap[ci] * ramp * a * dt;
            let before = self.vol[ci] + flux + rain + coup;
            let take = want.min(before.max(0.0));
            self.evap_out += take;
            self.vol[ci] = (before - take).max(0.0);
            self.reclose(ci);
        }
    }

    /// §15.5: boundary laws, serially, in slot order, clamped in volume
    /// space and booked as applied.
    fn fire_boundaries(&mut self, dt: f64) {
        for bi in 0..self.boundaries.len() {
            let b = &self.boundaries[bi];
            let ci = b.cell as usize;
            let h = self.depth[ci];
            // Inflow-positive volume the law asks to move this substep.
            let asked: f64 = match b.law {
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
        }
    }

    /// Advance to exactly `span` seconds from now (§15.4.4: an advance
    /// always reaches its target).
    pub fn advance(&mut self, span: f64) {
        let mut remaining = span;
        // Rebuild cadence: every fourth macro cycle of the (future) tier
        // ladder — with the single-tier march, every 4·2^(K−1) substeps.
        let rebuild_every = 4u64 << (self.lts_tiers.saturating_sub(1));
        let mut dt0 = self.stable_dt();
        while remaining > 1e-12 {
            if self.substep.is_multiple_of(rebuild_every) {
                self.rebuild_active();
                dt0 = self.stable_dt();
            } else {
                // Between rebuilds the step may only tighten.
                dt0 = dt0.min(self.stable_dt());
            }
            let dt = dt0.min(remaining);
            self.fire_faces(dt);
            self.fire_cells(dt);
            self.fire_boundaries(dt);
            self.refresh_perot();
            self.substep += 1;
            remaining -= dt;
        }
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
