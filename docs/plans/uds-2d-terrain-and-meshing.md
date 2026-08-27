# 2D terrain data, meshing, and sub-grid bathymetry

**Status:** tentative. Nothing here is scheduled, committed, or started.

**Parked:** 2026-08-26.

**Revisit when:** SWMM6 leaves alpha. The study below is against
`openswmm.engine` alpha.3.

**Why parked:** two of the three gaps are about matching or beating an
engine whose 2D surface is still moving — its node output field, its mesh
container and its `[2D_OPTIONS]` keys are all alpha-stage. Building
against them now risks building against a shape that changes.

---

## What prompted it

A question about the `.h5` file that appears beside a SWMM6 run, and a
reasonable follow-on: what is the point of a 2D engine that does not read
a DEM?

The short answer is that the DEM *is* used — every vertex elevation comes
from one — but it is consumed when the mesh is built, not when the model
is solved. The longer answer is that the question lands on three real
gaps, one of which is ours alone.

---

## What SWMM6 actually does

Verified against the alpha.3 source, not its manual.

### Mesh input

`[2D_VERTICES]` (x, y, z, optional tag) and `[2D_TRIANGLES]` (three
vertex indices, Manning n, optional initial depth, optional tag), inline
or in an external file named by `[2D_MESH_FILE] FILE <path>`. The
external file is the same section grammar with nesting disabled
(`src/engine/2d/input/SectionHandlers2D.cpp`), which is what Hydra
already reads (interop §14.15).

A GeoPackage container is an alternative to the `.inp` entirely
(`src/engine/input/geopackage/GeoPackageSchema.cpp`). Its schema comment
states the rule: *"Canonical storage is the relational index form: vertex
coordinates plus triangle connectivity. Derived topology — neighbour
adjacency, areas/centroids, edge geometry, vertex stencils — is
intentionally NOT stored."* The `mesh_2d_triangles.bed_elev` and `geom`
columns exist but are *"DERIVED presentation copies… IGNORED on read"*,
written so QGIS can draw the mesh. A reader browsing the `.gpkg` would
reasonably mistake `bed_elev` for authoritative per-cell terrain. It is
not.

**One elevation per vertex, and nothing else.** `MeshData.hpp` carries
`vx/vy/vz` per vertex; per cell it carries three vertex indices, Manning
n, initial depth and velocity, tags and coupling. Area, centroid and
`tri_cz` (*"Centroid Z (avg of vertex elevations)"*) are derived and
rebuilt at initialise. There is no DEM, raster, GeoTIFF or
elevation-volume table anywhere in the engine; the only `.tif` in the
tree is an example string in the plugin SDK's docs.

### Results output

With `OUTPUT_FILE` set in `[2D_OPTIONS]`, results go to a CF-1.11 /
UGRID-1.0 HDF5 file (`src/engine/2d/output/Default2DOutputPlugin.cpp`) —
output only, opening directly in ParaView or QGIS. It carries:

- **face (cell) series:** depth, head, head gradients limited and
  unlimited, RT0 cell-centred velocity, plus fixed envelope datasets;
- **node (vertex) series:** `Mesh2_node_head` (the solver field, dry-cell
  head = bed elevation) and `Mesh2_node_depth`, whose own comment reads
  *"signed vertex water depth (wet-masked render reconstruction) … this
  is the field renderers/profilers should interpolate."*

That last one is the important find. See gap 1.

### Closures

`CELL_CLOSURE FLAT` (default) and `VFR`, the Begnudelli and Sanders
(2006, 2007) stage–storage relation of the plane through a cell's three
vertex elevations; `FACE_RECONSTRUCTION MEAN` / `VFR_FACE`. Hydra
implements both, with the same default and for the same stated reason.
Their manual's heading is *"VFR is correct and is not the default."*

---

## What HEC-RAS does differently

Its geometry `.h5` is a genuine input sidecar carrying sub-cell terrain:
per-cell elevation-volume curves and per-face elevation-conveyance
curves, built from a fine DEM, so a large cell holds a terrain
distribution rather than a plane. TUFLOW's SGS is the same idea. This is
the thing people mean when they say a 2D engine "uses the DEM", and
neither Hydra nor SWMM6 does it.

---

## Where Hydra stands: three separable gaps

### 1. The canvas invents the vertex field — ours alone, and the cheapest

`cellValuesAtVertices` in `crates/gui/frontend/src/canvas/surfaceMesh.ts`
builds corner values as an **unweighted mean of every incident cell**,
dry ones included, with a separate mean-depth alpha mask. SWMM6 computes
its render field in the engine
(`src/engine/2d/mesh/VertexReconstruction.cpp`):

- depth-weighted mean of incident **wet** cells' free surfaces, dry cells
  excluded, so a dry cell's bed elevation can never lift the water
  surface up an adverse slope or bed step (no-new-maxima);
- a **wetted-contact gate**, `η_i > z_v`: a cell votes at a corner only
  where its water reaches that corner. Their comment names what it
  prevents — a thin film at the base of a steep cell stamping its low
  level onto the cell's high vertex, *"the wall-base notch in profile
  plots"*;
- stored as **signed** depth `η_v − z_v`, 0 being the no-data sentinel.

Their manual states the general rule: interpolating the solver field for
display *"would drag water surfaces up dry banks and down into thin
films"*. That describes what our canvas does today.

**Resolved by removal, 2026-08-26.** Smoothing a run's values is gone:
results are drawn one flat colour per cell, which is what the solver
holds, and the toggle is offered only over a field the mesh holds at its
vertices (the ground), where colouring the vertices draws the plane
through three known elevations and invents nothing. The custom shader
layer, the barycentric attribute, the cubic bubble, the cell-to-vertex
averaging and the pointer's cell-interior reading all went with it.

The gap itself stands, and this is what closing it would take: a §15
reconstruction evaluated at reporting instants, a §14.16 version 2
carrying one f32 per vertex per record (about +12% on the record for a
triangulation), and a GUI that reads corner values instead of computing
them. Signed depth would also give the waterline as a zero crossing
inside a cell, which beats masking whole corners. Only then is a
continuous drawing of a result honest.

Open question it would have to settle: depth and water surface both fall
out of one reconstructed `η_v`, but speed has no vertex field in SWMM6's
output either. Either we ship one or the blend keeps averaging for that
variable and says so.

### 2. No sub-grid bathymetry

Recorded as a named absence in overland spec §15.10 alongside this plan.
Without it, mesh resolution and accuracy are coupled: resolving a kerb, a
channel invert or a wall needs cells at that scale, and cell count drives
runtime. It is a real competitive gap against HEC-RAS and TUFLOW for
large urban floodplains, and it is not a gap against SWMM6.

The architecture already has the seam. §15.3 abstracts exactly the right
thing — `h̄(η)`, `η(h̄)`, wet-area fraction, and a face-depth rule — so a
sub-grid closure is a third case beside FLAT and VFR, reading tabulated
per-cell and per-face curves instead of closed forms. The tables are
built at import on the native side and reach the engine as typed data, so
the engine never opens a raster and the browser build survives. The
`.inp` has no vocabulary for the tables, which is the "not expressible in
the legacy format" case the interop rule already anticipates.

### 3. No path from a DEM to a mesh

Hydra has no mesher. The only triangulation in the codebase is
Bowyer–Watson for rain-gauge natural-neighbour weights
(`crates/engine-uds/src/overland/meteorology.rs`). A user needs a mesh
built somewhere else entirely; the models this was tested against were
generated by throwaway Python.

That is defensible for SWMM6, which is an engine. It is not defensible
for a platform shipping a GUI that offers 2D, and it is the gap that most
deserves the "what is the point" question.

---

## The plan, if we do it

### Architecture

A new crate, `hydra-mesh`, named for what it is. It owns triangulation,
refinement, elevation assignment and roughness assignment: pure
computation over typed inputs, wasm-clean, no filesystem. It does not
open a GeoTIFF.

The DEM arrives as a **sampler the caller implements** —
`elevation_at(x, y) -> Option<f64>` — so the CLI can back it with a local
raster and the GUI with a tile or COG service, and the mesher never knows
which. Output is the engine's `OverlandMesh`, data-model-first, which the
interop writer emits as `[2D_VERTICES]` / `[2D_TRIANGLES]` on export.
Acquiring bytes stays in `hydra-cli` / `hydra-gui`, as model files do.

Note for whoever picks this up: the Rust workspace has **no geospatial
dependencies at all** today. `proj4` is JavaScript, in the frontend, for
node coordinates. This introduces a geometry stack where there is none.

### Stages

| Stage | What it does | Rough size |
|---|---|---|
| 0 | Sample a DEM onto an existing mesh's vertices | days |
| 1 | Boundary polygon + target size + DEM → graded mesh with elevations | weeks |
| 2 | Breaklines, refinement regions, buildings, quality guarantee, manhole snapping | months |
| 3 | Sub-grid tables from the same sampler (gap 2) | on top |

Stage 2 is where real meshers live, and the quality guarantee is a
requirement rather than a refinement: §15.2 already carries a sliver
guard on the edge normal distance because degenerate triangles wreck the
timestep, and a mesher without angle control produces them by the
thousand.

### The cheaper fork

Read meshes other tools build — `.2dm`, GMSH, or GeoPackage, where
SWMM6's schema is a target to match. That is interop work, which is this
project's strength, and combined with stage 0 it unblocks anyone with a
GIS team in weeks rather than months. It is a stepping stone rather than
a detour: stage 1 wants the same sampler and the same output type.

---

## Open decisions

1. Where a DEM comes from in practice: local GeoTIFF, COG over HTTP,
   ESRI ASCII, or an in-house service.
2. Import other people's meshes first, or generate our own first.
3. Driven from a `hydra mesh` CLI command, a GUI wizard, or both. CLI
   first is the testable order.
4. Whether to take a dependency for the triangulation (`spade` gives
   constrained Delaunay in pure Rust and builds for wasm) or write it.
   Quality refinement has no crate worth trusting either way, so stage 2
   is ours regardless.

---

## Recorded elsewhere

- Overland spec §15.10 gained sub-grid bathymetry as a named absence, per
  §1.8's rule that a gap is a named absence rather than an approximation.
  That is a statement of what the engine does not do, and commits nothing
  here.
