# Introduction

Hydra is a water infrastructure simulation platform written in Rust. It is built as a suite of domain engines sharing one toolchain: a desktop GUI, a `hydra` CLI, and a Rust SDK.

| Engine | Domain | Source model | Status |
|---|---|---|---|
| **Water Distribution** (`wds`) | Pressurised supply networks — hydraulics, water quality, energy | EPANET `.inp` (2.x) | **Available** |
| **Urban Drainage** (`uds`) | Stormwater and wastewater collection — runoff, routing, quality | SWMM `.inp` | **Available** |
| **Open Channel** (`och`) | Rivers and channels — steady and unsteady flow | HEC-RAS project | Planned |

<!-- PLANNED-ENGINE: och — revise the Status column as each engine ships. -->
See [Engines](engines.md) for what each engine covers and what each status means in practice.

## Water Distribution Engine

Extended-period simulation (EPS) of hydraulic behaviour and water quality dynamics across pressurised pipe networks, computing the full time history of flows, pressures, and constituent concentrations at every node and link.

Unless a page says otherwise, the rest of this documentation describes this engine.

### Hydraulics

- **Head-loss formulas**: Hazen-Williams, Darcy-Weisbach, Chezy-Manning (with minor losses)
- **Demand models**: Demand-Driven Analysis (DDA) and Pressure-Dependent Analysis (PDA)
- **Emitters**: pressure-dependent outflow at junctions
- **Leakage**: FAVAD (Fixed and Variable Area Discharge) model
- **Pumps**: head-curve (1/3-point and custom), constant-power, variable-speed patterns
- **Valves**: PRV, PSV, FCV, TCV, GPV, PBV, PCV
- **Tanks**: cylindrical and volume-curve geometries, overflow mode
- **Controls**: simple time/level/pressure controls, rule-based controls with priorities
- **Solver**: Global Gradient Algorithm (GGA) with sparse Cholesky factorisation

### Water Quality

- **Modes**: chemical constituent, water age, source trace
- **Transport**: Lagrangian segment-based advection
- **Reactions**: first-order and zero-order bulk and wall decay, limiting potential, roughness correlation
- **Sources**: concentration, mass booster, flow-paced booster, setpoint booster
- **Tank mixing**: complete (CSTR), two-compartment, FIFO (plug flow), LIFO

### I/O

- **Input**: EPANET `.inp` format, any 2.x release (local files, HTTP URLs)
- **Output**: EPANET-compatible `.out` binary format, `.rpt` text report, `.json` report
- **Unit systems**: all 11 EPANET flow unit variants (CFS, GPM, MGD, IMGD, AFD, LPS, LPM, MLD, CMH, CMD, CMS)

### Relationship to EPANET

The water distribution engine's hydraulic and quality solvers were derived by studying EPANET's mathematical foundations. Hydra is **not** an EPANET clone or compatibility layer; it is a distinct solver that models the same physics. Where the two diverge, Hydra's result is authoritative.

The same principle will apply to each engine Hydra adds: it reads the established source-model format for its domain, and models the physics independently rather than reimplementing the reference tool.

For migration guidance, see [Migrating from EPANET](reference/migrating-from-epanet.md).

See [INP Format Support](reference/inp-format.md) for current EPANET input coverage.

## Urban Drainage Engine

Continuous and event simulation of stormwater and wastewater collection systems on the SWMM data model. Available from the `hydra` CLI, the Rust SDK, and the desktop app, where a drainage model is imported, then edited, run and explored.

- **Hydrology**: rainfall-runoff on subcatchments with Horton, Green-Ampt, and Curve Number infiltration; LID controls; snowmelt; groundwater; RDII unit hydrographs; climate files and the Hargreaves evaporation relation
- **Hydraulics**: Preissmann-slot dynamic-wave routing through conduits, pumps, orifices, weirs, and outlets; storage nodes, flow dividers, and HEC-22 street inlets
- **Water quality**: pollutant buildup and washoff on land uses, node treatment expressions, network transport
- **Controls**: rule-based controls with priorities and PID modulation
- **I/O**: SWMM `.inp` input; SWMM-compatible `.out` binary and text report output

The engine models the same physics as SWMM with its own solver and its own convergence and conservation criteria — the same relationship the water distribution engine has to EPANET. Where the predecessor's behaviour is reproduced, substituted, or corrected, the model import says so through named notices rather than silently rewriting.
