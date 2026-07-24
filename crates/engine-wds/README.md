# hydra-engine-wds

[![Crates.io](https://img.shields.io/crates/v/hydra-engine-wds)](https://crates.io/crates/hydra-engine-wds)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)

Core simulation engine for [Hydra](https://github.com/neeraip/hydra) — water distribution network data model, EPANET INP/OUT/RPT I/O, Global Gradient Algorithm hydraulic solver, Lagrangian water quality engine, simulation session API, and post-simulation analytics.

> **Most users should depend on [`hydra-sdk`](https://crates.io/crates/hydra-sdk) instead.** `hydra-sdk` re-exports the complete public API of this crate as a single stable umbrella dependency.

## Scope

This crate owns:

| Module | Responsibility |
|---|---|
| `model` | Network data model, state types, validation |
| `io` | Unit conversion; INP parser; binary `.out` reader/writer; `.rpt` writer |
| `hydraulics` | GGA Newton-Raphson solver |
| `quality` | Lagrangian transport, mixing, reactions, source tracing |
| `simulation` | Session API, controls, timestep orchestration, accounting |
| `analysis` | Post-simulation analytics |

This crate does **not** own interface logic (CLI, GUI) or network I/O — simulation inputs (INP model bytes) are supplied in memory by callers. The one filesystem carve-out is the explicit path-based streaming of binary `.out` result files and analysis artifacts (`io::out_reader`, `io::analysis_io`), so large results never have to be loaded whole.

## License

[AGPL v3](https://github.com/neeraip/hydra/blob/main/LICENSE) — see [COMMERCIAL_LICENSE.md](https://github.com/neeraip/hydra/blob/main/.github/COMMERCIAL_LICENSE.md) for commercial licensing options.
