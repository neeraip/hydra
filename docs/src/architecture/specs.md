# Specifications

Every engine is specified subsystem by subsystem, alongside the cross-cutting
specs for the shared foundation and the report layer. These documents are the
authoritative definitions of Hydra's behaviour. Where a spec and the
implementation disagree, the spec wins.

## Shared

| Document | Scope |
|---|---|
| [`crates/common/src/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/common/src/spec.md) | Foundation contracts: engine identity and registry, the reportable-output contract, and the element, quantity, result-variable, criteria and editing contracts |
| [`crates/report/src/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/report/src/spec.md) | Report templates, document model, and the txt/csv/html renderer formats |

## Water Distribution (`wds`)

| Document | Scope |
|---|---|
| [`crates/engine-wds/src/model/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/model/spec.md) | Data model, unit system, model file formats |
| [`crates/engine-wds/src/hydraulics/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/hydraulics/spec.md) | Hydraulic engine: GGA solver, sparse Cholesky, valves, demands |
| [`crates/engine-wds/src/quality/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/quality/spec.md) | Quality engine: transport, mixing, reactions, source injection |
| [`crates/engine-wds/src/simulation/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/simulation/spec.md) | Simulation orchestrator: controls, timestep, accounting, session API |
| [`crates/engine-wds/src/analysis/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/analysis/spec.md) | Post-simulation analytics: demand reliability, service compliance, distributions, and the report-block catalog |

## Urban Drainage (`uds`)

| Document | Scope |
|---|---|
| [`crates/engine-uds/src/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/spec.md) | Charter: scope, principles, correspondence to the predecessor, status |
| [`crates/engine-uds/src/model/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/model/spec.md) | Data model and unit system |
| [`crates/engine-uds/src/hydrology/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/hydrology/spec.md) | Rainfall-runoff, infiltration, LID controls, snowmelt, groundwater, RDII, climate |
| [`crates/engine-uds/src/hydraulics/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/hydraulics/spec.md) | Section geometry, dynamic-wave routing, structures, street inlets |
| [`crates/engine-uds/src/transport/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/transport/spec.md) | Pollutant buildup, washoff, treatment, network transport |
| [`crates/engine-uds/src/simulation/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/simulation/spec.md) | Controls, orchestration, accounting, statistics, session API |
| [`crates/engine-uds/src/report_blocks/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/report_blocks/spec.md) | Post-simulation analytics: report-block catalog, derivations, options |
| [`crates/engine-uds/src/interop/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/interop/spec.md) | Predecessor file formats: INP import, interface files, OUT/RPT output, recognition |
