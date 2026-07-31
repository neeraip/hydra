# Specifications

Every engine is specified subsystem by subsystem, alongside the cross-cutting
specs for the shared foundation and the report layer. These documents are the
authoritative definitions of Hydra's behaviour — where a spec and the
implementation disagree, the spec wins.

## Shared

| Document | Scope |
|---|---|
| [`crates/common/src/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/common/src/spec.md) | Foundation contracts: engine identity, the reportable-output contract |
| [`crates/report/src/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/report/src/spec.md) | Report templates, document model, and the txt/csv/html renderer formats |

## Water Distribution (`wds`)

| Document | Scope |
|---|---|
| [`crates/engine-wds/src/model/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/model/spec.md) | Data model, unit system, model file formats |
| [`crates/engine-wds/src/hydraulics/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/hydraulics/spec.md) | Hydraulic engine: GGA solver, sparse Cholesky, valves, demands |
| [`crates/engine-wds/src/quality/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/quality/spec.md) | Quality engine: transport, mixing, reactions, source injection |
| [`crates/engine-wds/src/simulation/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/simulation/spec.md) | Simulation orchestrator: controls, timestep, accounting, session API |
| [`crates/engine-wds/src/analysis/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/analysis/spec.md) | Post-simulation analytics: demand reliability, service compliance, distributions, the report-block catalog, and the analysis artifact |

<!-- PLANNED-ENGINE: uds,och — replace this section with each engine's real spec table as it lands. -->

## Urban Drainage (`uds`) and Open Channel (`och`)

Not yet written. Both engines are [registered but planned](../engines.md) — no
behaviour has been specified, so there is nothing yet for an implementation to
conform to. Their specs land before any implementation code does, as the
spec-first workflow requires.
