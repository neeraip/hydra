# Specifications

The engine is specified by subsystem. These documents are the authoritative definitions of Hydra's behaviour:

| Document | Scope |
|---|---|
| [`crates/engine-wds/src/model/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/model/spec.md) | Data model, unit system, model file formats |
| [`crates/engine-wds/src/hydraulics/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/hydraulics/spec.md) | Hydraulic engine: GGA solver, sparse Cholesky, valves, demands |
| [`crates/engine-wds/src/quality/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/quality/spec.md) | Quality engine: transport, mixing, reactions, source injection |
| [`crates/engine-wds/src/simulation/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/simulation/spec.md) | Simulation orchestrator: controls, timestep, accounting, session API |
| [`crates/engine-wds/src/analysis/spec.md`](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/analysis/spec.md) | Post-simulation analytics: demand reliability, service compliance |
