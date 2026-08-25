#![doc = include_str!("spec.md")]

//! The overland surface: the §15.2 mesh model, its derived topology, and
//! its validation. The marcher (§15.4) and couplings (§15.6) build on
//! these in later phases; until they land, §1.8 governs what a model
//! carrying a mesh receives.

pub mod closure;
pub mod coupling;
pub mod marcher;
pub mod meteorology;

/// The authored overland surface (§15.2, §14.15), in SI after import.
///
/// Everything here is what the file said, converted and index-resolved,
/// before any topology is derived: derivation and validation live in
/// [`Topology::build`], so a session can report authoring defects with
/// the author's own vocabulary and derived defects with the mesh's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverlandMesh {
    /// Mesh vertices in authoring order: position and ground elevation
    /// (m), and the optional identifying tag.
    pub verts: Vec<MeshVertex>,
    /// Cells in authoring order.
    pub cells: Vec<MeshCell>,
    /// §14.15 solver options, defaults applied.
    pub options: OverlandOptions,
    /// Vertex-coupling rows (§15.6): one per vertex, later rows replace.
    pub vertex_couplings: Vec<CouplingRow>,
    /// Cell-coupling rows (§15.6): rows accumulate, several nodes may
    /// couple to one cell.
    pub cell_couplings: Vec<CouplingRow>,
    /// Boundary-condition rows (§15.5), in file order.
    pub boundaries: Vec<BoundaryRow>,
    /// §15.7 per-cell losses; unlisted cells lose nothing.
    pub infiltration: Vec<InfiltrationRow>,
    /// Edge-conveyance rows (§15.2): undirected vertex pair and ψ.
    pub conveyance: Vec<ConveyanceRow>,
    /// Initial cell velocities (§14.15): cell index and (u, v) in m/s.
    pub init_velocity: Vec<InitVelocityRow>,
    /// Whether the mesh declared itself SI (`;; UNITS: SI (m)`), which
    /// skips display-unit conversion at import (§14.15).
    pub units_si: bool,
    /// The external mesh file's name, when `[2D_MESH_FILE]` declared one
    /// (§14.15). The caller supplies its text; the name is what a
    /// refusal or survey reports.
    pub mesh_file: Option<String>,
}

impl OverlandMesh {
    /// Resolve a §14.15 address against the vertices: an index where
    /// numeric and in range, else a tag.
    pub fn resolve_vertex(&self, address: &str) -> Option<u32> {
        resolve(address, self.verts.len(), |i| self.verts[i].tag.as_deref())
    }

    /// Resolve a §14.15 address against the cells, the same rule.
    pub fn resolve_cell(&self, address: &str) -> Option<u32> {
        resolve(address, self.cells.len(), |i| self.cells[i].tag.as_deref())
    }
}

fn resolve<'a>(
    address: &str,
    count: usize,
    tag_of: impl Fn(usize) -> Option<&'a str>,
) -> Option<u32> {
    if let Ok(i) = address.parse::<u32>() {
        if (i as usize) < count {
            return Some(i);
        }
    }
    (0..count)
        .position(|i| tag_of(i) == Some(address))
        .map(|i| i as u32)
}

/// One §14.15 initial-velocity row.
#[derive(Debug, Clone, PartialEq)]
pub struct InitVelocityRow {
    pub cell: u32,
    /// (u, v) in m/s, as authored — velocities are SI in every unit
    /// system, as the predecessor reads them.
    pub u: f64,
    pub v: f64,
}

/// One mesh vertex (§15.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshVertex {
    pub x: f64,
    pub y: f64,
    /// Ground elevation (m).
    pub z: f64,
    pub tag: Option<String>,
}

/// One mesh cell (§15.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshCell {
    /// Vertex indices; winding order free.
    pub v: [u32; 3],
    /// Manning roughness (> 0).
    pub n: f64,
    /// Initial depth (m, ≥ 0).
    pub h0: f64,
    pub tag: Option<String>,
}

/// §15.7: a cell's authored infiltration losses (§14.15), SI after
/// import — initial loss in metres of depth, continuing loss in metres
/// per second. Resolution is derivation, so it happens at
/// [`Topology::build`] and the marcher, never at parse.
#[derive(Debug, Clone, PartialEq)]
pub struct InfiltrationRow {
    /// The cell as authored: an index where numeric and in range, else
    /// a tag.
    pub address: String,
    /// Initial loss (m of depth), one-time absorbing capacity.
    pub il: f64,
    /// Continuing loss (m/s) while the cell is wet.
    pub cl: f64,
}

/// One §15.6 coupling row: a mesh vertex or cell joined to a network
/// vertex, with its discharge coefficient and exchange area.
#[derive(Debug, Clone, PartialEq)]
pub struct CouplingRow {
    /// The mesh vertex or cell as authored: an index where numeric and
    /// in range, else a tag (§14.15). Resolution is derivation — an
    /// external mesh file may author the vertices a row in the model
    /// addresses — so it happens at [`Topology::build`], never at parse.
    pub address: String,
    /// The coupled network vertex, as the model names it; resolved at
    /// build against §2.6 identifiers.
    pub node: String,
    /// Discharge coefficient (§15.6), default 0.65.
    pub cd: f64,
    /// Exchange area (m²), default 1.0.
    pub area: f64,
    /// Whether the row authored its own area — an unauthored one is
    /// eligible for `COUPLING_AREA AUTO` derivation (§15.6).
    pub area_authored: bool,
}

/// One §15.5 boundary row: the condition attached to a cell's local edge.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryRow {
    pub cell: u32,
    /// Local edge 0..=2, opposite the same-numbered vertex (§15.2).
    pub edge: u8,
    pub condition: BoundaryCondition,
    /// The optional named group, carried but semantically inert (§14.15).
    pub group: Option<String>,
}

/// A §15.5 boundary condition as authored.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundaryCondition {
    Wall,
    /// Manning outfall at the authored bed slope.
    NormalFlow {
        slope: f64,
    },
    /// A held stage (m, elevation datum).
    Stage(SeriesOrValue),
    /// A held per-metre discharge (m³/s per metre of edge, outward
    /// positive as authored; §15.5 applies it as an inflow sign).
    Flow(SeriesOrValue),
    /// Per-metre discharge from a curve of stage above the edge bed.
    RatingCurve {
        curve: String,
    },
}

/// A boundary parameter: a constant, or a named time series resolved at
/// build (§14.15).
#[derive(Debug, Clone, PartialEq)]
pub enum SeriesOrValue {
    Value(f64),
    Series(String),
}

/// One §15.2 edge-conveyance row.
#[derive(Debug, Clone, PartialEq)]
pub struct ConveyanceRow {
    pub from: u32,
    pub to: u32,
    /// ψ ∈ [0, 1].
    pub factor: f64,
}

/// §14.15 `[2D_OPTIONS]`, defaults applied at construction.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlandOptions {
    pub cfl_number: f64,
    pub max_timestep: f64,
    pub theta: f64,
    pub froude_max: f64,
    pub lts_tiers: u32,
    pub h_move: f64,
    pub dry_depth: f64,
    pub cell_closure: CellClosure,
    pub face_reconstruction: FaceReconstruction,
    pub vfr_min_wet_frac: f64,
    pub advection: bool,
    pub rainfall_mode: RainfallMode,
    pub coupling_area_auto: bool,
    pub coupling_cd: f64,
    pub coupling_sync: f64,
    pub report_2d: bool,
    pub output_file: Option<String>,
}

impl Default for OverlandOptions {
    fn default() -> Self {
        OverlandOptions {
            cfl_number: 0.7,
            max_timestep: 10.0,
            theta: 0.8,
            froude_max: 1.5,
            lts_tiers: 4,
            h_move: 0.003,
            dry_depth: 0.001,
            cell_closure: CellClosure::Flat,
            face_reconstruction: FaceReconstruction::Mean,
            vfr_min_wet_frac: 0.01,
            advection: false,
            rainfall_mode: RainfallMode::NaturalNeighbour,
            coupling_area_auto: false,
            coupling_cd: 0.65,
            coupling_sync: 0.0,
            report_2d: true,
            output_file: None,
        }
    }
}

/// §15.3 cell stage–storage closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellClosure {
    Flat,
    Vfr,
}

/// §15.3 face-depth reconstruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceReconstruction {
    Mean,
    VfrFace,
}

/// §15.7 rainfall-to-mesh mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RainfallMode {
    NaturalNeighbour,
    System,
    None,
}

/// The derived mesh topology (§15.2): adjacency, geometry, and the
/// per-edge quantities the marcher reads. Built once from a validated
/// [`OverlandMesh`]; building is also where §15.2's validation runs,
/// because most of what it checks is only visible derived.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Topology {
    /// Cell centroids (x, y) and the centroid bed elevation z̄ (m).
    pub centroid: Vec<[f64; 3]>,
    /// Cell planimetric areas (m²), strictly positive.
    pub area: Vec<f64>,
    /// Neighbour cell across local edge `cell*3 + e`; `None` at the
    /// domain boundary.
    pub neighbour: Vec<Option<u32>>,
    /// Edge planimetric lengths (m), flat `cell*3 + e`.
    pub edge_len: Vec<f64>,
    /// Outward unit normals, flat `cell*3 + e`.
    pub edge_normal: Vec<[f64; 2]>,
    /// Edge midpoints (x, y), flat `cell*3 + e`.
    pub edge_mid: Vec<[f64; 2]>,
    /// Sorted endpoint bed elevations (z_lo, z_hi) per edge, flat.
    pub edge_z: Vec<[f64; 2]>,
    /// Inverse centroid-to-centroid normal distance 1/dₙ per interior
    /// edge slot (§15.2's slivered-cell floor applied); 0 at boundary
    /// slots, where §15.5's own arm applies.
    pub inv_dn: Vec<f64>,
    /// Conveyance ψ per edge slot, both slots of an interior edge equal.
    pub conveyance: Vec<f64>,
}

/// A §15.2 mesh defect, named with the author's vocabulary.
#[derive(Debug, Clone, PartialEq)]
pub enum MeshError {
    /// Fewer than three vertices or no cells.
    TooSmall { verts: usize, cells: usize },
    /// A cell names a vertex the mesh does not have.
    VertexOutOfRange { cell: usize, vertex: u32 },
    /// A cell repeats a vertex.
    DegenerateCell { cell: usize },
    /// A cell's planimetric area is not strictly positive.
    ZeroArea { cell: usize },
    /// A cell's Manning roughness is not strictly positive.
    BadRoughness { cell: usize, n: f64 },
    /// A cell's initial depth is negative.
    NegativeDepth { cell: usize },
    /// A conveyance row's factor is outside [0, 1].
    BadConveyance { from: u32, to: u32, factor: f64 },
    /// A conveyance row names an edge no cell pair shares.
    UnknownEdge { from: u32, to: u32 },
    /// A conveyance row joins a vertex to itself.
    SelfEdge { vertex: u32 },
    /// An edge is claimed by more than two cells: the surface is not a
    /// manifold there.
    NonManifoldEdge { from: u32, to: u32 },
    /// A boundary row addresses a cell or edge that does not exist, or
    /// an interior edge.
    BadBoundary { cell: u32, edge: u8 },
    /// A coupling row's address matches no index and no tag.
    UnknownAddress { address: String },
    /// §15.7: infiltration losses must be non-negative and finite.
    BadInfiltration { address: String, il: f64, cl: f64 },
    /// An initial-velocity row names a cell that does not exist.
    BadInitVelocity { cell: u32 },
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeshError::TooSmall { verts, cells } => write!(
                f,
                "the mesh needs at least three vertices and one triangle \
                 (has {verts} and {cells})"
            ),
            MeshError::VertexOutOfRange { cell, vertex } => {
                write!(
                    f,
                    "triangle {cell} names vertex {vertex}, which does not exist"
                )
            }
            MeshError::DegenerateCell { cell } => {
                write!(f, "triangle {cell} repeats a vertex")
            }
            MeshError::ZeroArea { cell } => {
                write!(f, "triangle {cell} has no area")
            }
            MeshError::BadRoughness { cell, n } => {
                write!(f, "triangle {cell}: Manning's n must be positive (is {n})")
            }
            MeshError::NegativeDepth { cell } => {
                write!(f, "triangle {cell}: initial depth cannot be negative")
            }
            MeshError::BadConveyance { from, to, factor } => write!(
                f,
                "edge conveyance {from}-{to}: the factor must lie in [0, 1] (is {factor})"
            ),
            MeshError::UnknownEdge { from, to } => {
                write!(
                    f,
                    "edge conveyance {from}-{to}: no triangle pair shares that edge"
                )
            }
            MeshError::SelfEdge { vertex } => {
                write!(f, "edge conveyance: vertex {vertex} paired with itself")
            }
            MeshError::NonManifoldEdge { from, to } => write!(
                f,
                "edge {from}-{to} is shared by more than two triangles; the \
                 surface folds over itself there"
            ),
            MeshError::BadBoundary { cell, edge } => write!(
                f,
                "boundary condition at triangle {cell} edge {edge}: no such \
                 boundary edge"
            ),
            MeshError::UnknownAddress { address } => write!(
                f,
                "coupling address {address:?} matches no index and no tag"
            ),
            MeshError::BadInfiltration { address, il, cl } => write!(
                f,
                "cell {address:?} authors infiltration losses {il} and {cl}, \
                 which must be non-negative"
            ),
            MeshError::BadInitVelocity { cell } => {
                write!(
                    f,
                    "initial velocity names triangle {cell}, which does not exist"
                )
            }
        }
    }
}

impl Topology {
    /// Derive the topology and run §15.2's validation. Every defect is
    /// returned, not just the first: an author fixing a mesh wants the
    /// whole list.
    pub fn build(mesh: &OverlandMesh) -> Result<Topology, Vec<MeshError>> {
        let mut errors = Vec::new();
        let nv = mesh.verts.len();
        let nc = mesh.cells.len();
        if nv < 3 || nc == 0 {
            return Err(vec![MeshError::TooSmall {
                verts: nv,
                cells: nc,
            }]);
        }

        for (ci, c) in mesh.cells.iter().enumerate() {
            if c.v.iter().any(|&v| v as usize >= nv) {
                let vertex = *c.v.iter().find(|&&v| v as usize >= nv).unwrap_or(&c.v[0]);
                errors.push(MeshError::VertexOutOfRange { cell: ci, vertex });
                continue;
            }
            if c.v[0] == c.v[1] || c.v[1] == c.v[2] || c.v[0] == c.v[2] {
                errors.push(MeshError::DegenerateCell { cell: ci });
            }
            // Written as "not strictly positive" so NaN lands in the
            // refusal too.
            if c.n <= 0.0 || c.n.is_nan() {
                errors.push(MeshError::BadRoughness { cell: ci, n: c.n });
            }
            if c.h0 < 0.0 {
                errors.push(MeshError::NegativeDepth { cell: ci });
            }
        }
        if !errors.is_empty() {
            // Geometry below indexes vertices; a mesh that fails the
            // reference checks cannot be measured.
            return Err(errors);
        }

        let mut topo = Topology {
            centroid: Vec::with_capacity(nc),
            area: Vec::with_capacity(nc),
            neighbour: vec![None; nc * 3],
            edge_len: vec![0.0; nc * 3],
            edge_normal: vec![[0.0, 0.0]; nc * 3],
            edge_mid: vec![[0.0, 0.0]; nc * 3],
            edge_z: vec![[0.0, 0.0]; nc * 3],
            inv_dn: vec![0.0; nc * 3],
            conveyance: vec![1.0; nc * 3],
        };

        for (ci, c) in mesh.cells.iter().enumerate() {
            let [a, b, d] = c.v.map(|v| &mesh.verts[v as usize]);
            let cx = (a.x + b.x + d.x) / 3.0;
            let cy = (a.y + b.y + d.y) / 3.0;
            let cz = (a.z + b.z + d.z) / 3.0;
            topo.centroid.push([cx, cy, cz]);
            let area = 0.5 * ((b.x - a.x) * (d.y - a.y) - (d.x - a.x) * (b.y - a.y)).abs();
            if area <= 0.0 || area.is_nan() {
                errors.push(MeshError::ZeroArea { cell: ci });
            }
            topo.area.push(area);
            // Local edge e is opposite vertex e (§15.2).
            for e in 0..3 {
                let (p, q) = match e {
                    0 => (b, d),
                    1 => (d, a),
                    _ => (a, b),
                };
                let slot = ci * 3 + e;
                let (dx, dy) = (q.x - p.x, q.y - p.y);
                let len = (dx * dx + dy * dy).sqrt();
                topo.edge_len[slot] = len;
                topo.edge_mid[slot] = [(p.x + q.x) / 2.0, (p.y + q.y) / 2.0];
                topo.edge_z[slot] = if p.z <= q.z { [p.z, q.z] } else { [q.z, p.z] };
                if len > 0.0 {
                    // Perpendicular, oriented away from the centroid.
                    let (mut nx, mut ny) = (dy / len, -dx / len);
                    let mx = topo.edge_mid[slot][0] - cx;
                    let my = topo.edge_mid[slot][1] - cy;
                    if nx * mx + ny * my < 0.0 {
                        nx = -nx;
                        ny = -ny;
                    }
                    topo.edge_normal[slot] = [nx, ny];
                }
            }
        }

        // Adjacency: first cell claims an edge, the second completes the
        // pair; a third makes the surface non-manifold.
        use std::collections::HashMap;
        let mut claims: HashMap<(u32, u32), (u32, u8)> = HashMap::new();
        for (ci, c) in mesh.cells.iter().enumerate() {
            for e in 0..3u8 {
                let (p, q) = match e {
                    0 => (c.v[1], c.v[2]),
                    1 => (c.v[2], c.v[0]),
                    _ => (c.v[0], c.v[1]),
                };
                let key = if p <= q { (p, q) } else { (q, p) };
                match claims.get(&key) {
                    None => {
                        claims.insert(key, (ci as u32, e));
                    }
                    Some(&(cj, ej)) if topo.neighbour[cj as usize * 3 + ej as usize].is_none() => {
                        topo.neighbour[ci * 3 + e as usize] = Some(cj);
                        topo.neighbour[cj as usize * 3 + ej as usize] = Some(ci as u32);
                    }
                    Some(_) => {
                        errors.push(MeshError::NonManifoldEdge {
                            from: key.0,
                            to: key.1,
                        });
                    }
                }
            }
        }

        // Interior normal distances, with the §15.2 sliver floor.
        for (ci, _) in mesh.cells.iter().enumerate() {
            for e in 0..3 {
                let slot = ci * 3 + e;
                if let Some(nj) = topo.neighbour[slot] {
                    let [nx, ny] = topo.edge_normal[slot];
                    let dx = topo.centroid[nj as usize][0] - topo.centroid[ci][0];
                    let dy = topo.centroid[nj as usize][1] - topo.centroid[ci][1];
                    let dn = (dx * nx + dy * ny).abs().max(0.3 * topo.edge_len[slot]);
                    if dn > 0.0 {
                        topo.inv_dn[slot] = 1.0 / dn;
                    }
                }
            }
        }

        // Conveyance rows resolve to edge slots, mirrored to both sides.
        for row in &mesh.conveyance {
            if row.from == row.to {
                errors.push(MeshError::SelfEdge { vertex: row.from });
                continue;
            }
            if !(0.0..=1.0).contains(&row.factor) {
                errors.push(MeshError::BadConveyance {
                    from: row.from,
                    to: row.to,
                    factor: row.factor,
                });
                continue;
            }
            let key = if row.from <= row.to {
                (row.from, row.to)
            } else {
                (row.to, row.from)
            };
            match claims.get(&key) {
                Some(&(ci, e)) => {
                    let slot = ci as usize * 3 + e as usize;
                    topo.conveyance[slot] = row.factor;
                    if let Some(nj) = topo.neighbour[slot] {
                        // The partner slot on the neighbour: find the
                        // local edge whose neighbour is `ci`.
                        for pe in 0..3 {
                            if topo.neighbour[nj as usize * 3 + pe] == Some(ci) {
                                topo.conveyance[nj as usize * 3 + pe] = row.factor;
                            }
                        }
                    }
                }
                None => errors.push(MeshError::UnknownEdge {
                    from: row.from,
                    to: row.to,
                }),
            }
        }

        // Coupling addresses resolve against the full mesh; §14.15's
        // last-wins rule for vertex rows applies at resolution, so two
        // spellings of one vertex are one coupling.
        for row in &mesh.vertex_couplings {
            if mesh.resolve_vertex(&row.address).is_none() {
                errors.push(MeshError::UnknownAddress {
                    address: row.address.clone(),
                });
            }
        }
        for row in &mesh.cell_couplings {
            if mesh.resolve_cell(&row.address).is_none() {
                errors.push(MeshError::UnknownAddress {
                    address: row.address.clone(),
                });
            }
        }
        for row in &mesh.infiltration {
            if mesh.resolve_cell(&row.address).is_none() {
                errors.push(MeshError::UnknownAddress {
                    address: row.address.clone(),
                });
            }
            // NaN must land in the refusal too.
            if row.il < 0.0 || row.cl < 0.0 || row.il.is_nan() || row.cl.is_nan() {
                errors.push(MeshError::BadInfiltration {
                    address: row.address.clone(),
                    il: row.il,
                    cl: row.cl,
                });
            }
        }
        for row in &mesh.init_velocity {
            if row.cell as usize >= nc {
                errors.push(MeshError::BadInitVelocity { cell: row.cell });
            }
        }

        // Boundary rows must address real boundary edges.
        for row in &mesh.boundaries {
            let ok = (row.cell as usize) < nc
                && row.edge < 3
                && topo.neighbour[row.cell as usize * 3 + row.edge as usize].is_none();
            if !ok {
                errors.push(MeshError::BadBoundary {
                    cell: row.cell,
                    edge: row.edge,
                });
            }
        }

        if errors.is_empty() {
            Ok(topo)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two right triangles closing a unit square: the smallest mesh with
    /// an interior edge. Vertices 0..4 at the corners, diagonal 0-2.
    fn square() -> OverlandMesh {
        OverlandMesh {
            verts: vec![
                MeshVertex {
                    x: 0.0,
                    y: 0.0,
                    z: 10.0,
                    tag: None,
                },
                MeshVertex {
                    x: 1.0,
                    y: 0.0,
                    z: 10.2,
                    tag: None,
                },
                MeshVertex {
                    x: 1.0,
                    y: 1.0,
                    z: 10.4,
                    tag: None,
                },
                MeshVertex {
                    x: 0.0,
                    y: 1.0,
                    z: 10.6,
                    tag: None,
                },
            ],
            cells: vec![
                MeshCell {
                    v: [0, 1, 2],
                    n: 0.02,
                    h0: 0.0,
                    tag: None,
                },
                MeshCell {
                    v: [0, 2, 3],
                    n: 0.03,
                    h0: 0.0,
                    tag: None,
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_square_measures_as_authored() {
        let topo = Topology::build(&square()).expect("valid mesh");
        assert!((topo.area[0] - 0.5).abs() < 1e-12);
        assert!((topo.area[1] - 0.5).abs() < 1e-12);
        // Cell 0 = (v0,v1,v2): local edge 1 joins (v2,v0) — the diagonal.
        assert_eq!(topo.neighbour[1], Some(1));
        // Cell 1 = (v0,v2,v3): local edge 2 joins (v0,v2) — the same edge.
        assert_eq!(topo.neighbour[5], Some(0));
        // Every other edge slot is boundary.
        let boundary = topo.neighbour.iter().filter(|n| n.is_none()).count();
        assert_eq!(boundary, 4);
        // The diagonal's length is √2 on both slots.
        assert!((topo.edge_len[1] - 2f64.sqrt()).abs() < 1e-12);
        assert!((topo.edge_len[5] - 2f64.sqrt()).abs() < 1e-12);
        // Outward normals point away from the owning centroid.
        for ci in 0..2 {
            for e in 0..3 {
                let slot = ci * 3 + e;
                let [nx, ny] = topo.edge_normal[slot];
                let mx = topo.edge_mid[slot][0] - topo.centroid[ci][0];
                let my = topo.edge_mid[slot][1] - topo.centroid[ci][1];
                assert!(nx * mx + ny * my > 0.0, "cell {ci} edge {e}");
            }
        }
        // Sorted endpoint beds on the diagonal: z0=10.0, z2=10.4.
        assert_eq!(topo.edge_z[1], [10.0, 10.4]);
    }

    #[test]
    fn conveyance_lands_on_both_sides_of_the_shared_edge() {
        let mut mesh = square();
        mesh.conveyance.push(ConveyanceRow {
            from: 2,
            to: 0,
            factor: 0.3,
        });
        let topo = Topology::build(&mesh).expect("valid mesh");
        assert_eq!(topo.conveyance[1], 0.3);
        assert_eq!(topo.conveyance[5], 0.3);
        // Untouched slots keep the unrestricted default.
        assert_eq!(topo.conveyance[0], 1.0);
    }

    /// Reference-level defects are all named together, and they gate the
    /// derived checks: rows over an unmeasurable mesh are not judged.
    #[test]
    fn reference_defects_are_named_together_and_gate_the_rest() {
        let mut mesh = square();
        mesh.cells[0].n = 0.0;
        mesh.cells[1].v = [0, 2, 2];
        mesh.conveyance.push(ConveyanceRow {
            from: 1,
            to: 1,
            factor: 0.5,
        });
        let errors = Topology::build(&mesh).expect_err("defective mesh");
        assert!(errors.contains(&MeshError::BadRoughness { cell: 0, n: 0.0 }));
        assert!(errors.contains(&MeshError::DegenerateCell { cell: 1 }));
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, MeshError::SelfEdge { .. })),
            "rows are not judged over an unmeasurable mesh"
        );
    }

    /// Row-level defects over a sound mesh are all named together.
    #[test]
    fn row_defects_are_named_over_a_sound_mesh() {
        let mut mesh = square();
        mesh.conveyance.push(ConveyanceRow {
            from: 1,
            to: 3,
            factor: 0.5,
        });
        mesh.conveyance.push(ConveyanceRow {
            from: 1,
            to: 1,
            factor: 0.5,
        });
        mesh.conveyance.push(ConveyanceRow {
            from: 2,
            to: 0,
            factor: 1.5,
        });
        let errors = Topology::build(&mesh).expect_err("bad rows");
        assert!(errors.contains(&MeshError::UnknownEdge { from: 1, to: 3 }));
        assert!(errors.contains(&MeshError::SelfEdge { vertex: 1 }));
        assert!(errors.contains(&MeshError::BadConveyance {
            from: 2,
            to: 0,
            factor: 1.5
        }));
    }

    #[test]
    fn boundary_rows_must_address_boundary_edges() {
        let mut mesh = square();
        // The diagonal is interior: cell 0 edge 1.
        mesh.boundaries.push(BoundaryRow {
            cell: 0,
            edge: 1,
            condition: BoundaryCondition::Wall,
            group: None,
        });
        let errors = Topology::build(&mesh).expect_err("interior edge");
        assert!(errors.contains(&MeshError::BadBoundary { cell: 0, edge: 1 }));
        // A real boundary edge is accepted.
        let mut mesh = square();
        mesh.boundaries.push(BoundaryRow {
            cell: 0,
            edge: 2,
            condition: BoundaryCondition::NormalFlow { slope: 0.01 },
            group: None,
        });
        assert!(Topology::build(&mesh).is_ok());
    }

    #[test]
    fn a_folded_surface_is_refused() {
        let mut mesh = square();
        // A third cell claiming the diagonal.
        mesh.verts.push(MeshVertex {
            x: 0.5,
            y: -0.5,
            z: 11.0,
            tag: None,
        });
        mesh.cells.push(MeshCell {
            v: [0, 2, 4],
            n: 0.02,
            h0: 0.0,
            tag: None,
        });
        let errors = Topology::build(&mesh).expect_err("non-manifold");
        assert!(errors
            .iter()
            .any(|e| matches!(e, MeshError::NonManifoldEdge { .. })));
    }
}
