# Introduction

Hydra is a water distribution network simulator written in Rust. It performs extended-period simulation (EPS) of hydraulic behaviour and water quality dynamics across pressurised pipe networks, computing the full time history of flows, pressures, and constituent concentrations at every node and link.

Hydra is a complete evolution of water distribution network simulation — encompassing EPANET-class hydraulics and water quality, with SWMM-class and HEC-RAS-class engines to follow. It is cleanly architected, parallel where mathematically valid, and free of the historical implementation constraints that shaped its predecessors. Correctness is defined by Hydra's own convergence criteria and physical conservation laws — not by agreement with any prior tool's output.

## Features

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

- **Input**: EPANET 2.3 `.inp` format (local files, HTTP URLs)
- **Output**: EPANET-compatible `.out` binary format, `.rpt` text report, `.json` report
- **Unit systems**: all 11 EPANET flow unit variants (CFS, GPM, MGD, IMGD, AFD, LPS, LPM, MLD, CMH, CMD, CMS)

## Relationship to EPANET

Hydra's hydraulic and quality engines were derived by studying EPANET's mathematical foundations. Hydra is **not** an EPANET clone or compatibility layer — it is a distinct solver that models the same physics. Where the two diverge, Hydra's result is authoritative.

For migration guidance, see [Migrating from EPANET](reference/migrating-from-epanet.md). For the preserved derivation work, see [EPANET Analysis](internals/epanet-analysis.md).

## Correctness Policy

- Correctness is established by Hydra-native tests: physics/behaviour invariants and deterministic Hydra-vs-Hydra regression checks.
- EPANET parity data is retained as historical baseline evidence and for deviation characterisation only.
- No active correctness gate may require EPANET executables or EPANET reference output.

## Roadmap

| Engine | Status |
|---|---|
| Hydraulics (WD) | ✅ Complete |
| Water quality | ✅ Complete |
| Stormwater (SWMM-class) | 🔲 Planned |
| 1D river hydraulics (HEC-RAS-class) | 🔲 Planned |

See [INP Format Support](reference/inp-format.md) for current EPANET input coverage.
