# hydra-engine-uds

[![Crates.io](https://img.shields.io/crates/v/hydra-engine-uds)](https://crates.io/crates/hydra-engine-uds)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)

The urban drainage engine (`uds`) of [Hydra](https://github.com/neeraip/hydra), the water infrastructure simulation platform — stormwater and wastewater collection network simulation on the SWMM data model: rainfall-runoff hydrology (Horton / Green-Ampt / Curve Number infiltration, LID controls, snowmelt, groundwater, RDII), Preissmann-slot dynamic-wave routing, pollutant buildup / washoff / treatment / transport, rule-based controls, and SWMM-compatible INP import and OUT/RPT output.

<!-- PLANNED-ENGINE: och — revise this paragraph when the open channel engine ships. -->
It is one of Hydra's domain engines, alongside water distribution (`hydra-engine-wds`); open channel (`hydra-engine-och`) is a published scaffold awaiting development.

> **Most users should depend on [`hydra-sdk`](https://crates.io/crates/hydra-sdk) instead.** `hydra-sdk` re-exports this crate as its `uds` module under a single stable umbrella dependency.

## Scope

This crate is the complete simulation engine and nothing else: data model, SWMM INP import, hydrology, hydraulics, water quality, controls, session API (`simulation::engine::Simulation`), and predecessor-format output writers. Model text is supplied in memory by callers; the crate performs no filesystem or network I/O.

The authoritative behaviour definition lives in the module-level specifications embedded in the rustdoc (model, hydrology, hydraulics, transport, simulation, interop).
