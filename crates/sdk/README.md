# hydra-sdk

[![Crates.io](https://img.shields.io/crates/v/hydra-sdk)](https://crates.io/crates/hydra-sdk)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)

Water infrastructure simulation — EPANET-compatible extended-period water distribution simulation.

`hydra-sdk` is the user-facing library crate for [Hydra](https://github.com/neeraip/hydra). It is the single dependency you add to build on Hydra: it re-exports the water-distribution engine (`hydra-engine-wds`), the shared foundation contracts (`hydra-common` — engine identity and the reportable-output contract), and report generation (`hydra-report` — templates, document assembly, and the txt/csv/html/pdf renderers), with all internal crate versions pre-pinned.

Hydra is built as a suite of domain engines. Water distribution (`wds`) is the one implemented today; urban drainage (`uds`) and open channel (`och`) are registered in the engine registry as **planned** — reserved and presentable, but with no implementation behind them. As each lands, it joins this same umbrella crate.

**[→ Full documentation](https://neeraip.github.io/hydra/sdk/overview.html)**

## Install

```toml
[dependencies]
hydra-sdk = "5"
```

## Quick start

```rust
use hydra_sdk::{io, Simulation, NodeQuantity, LinkQuantity};

let bytes = std::fs::read("network.inp").unwrap();
let network = io::parse(&bytes).unwrap();

let mut sim = Simulation::create();
sim.load(network).unwrap();
sim.run().unwrap();

for t in sim.snapshot_times() {
    let head = sim.get_node_result("J1", NodeQuantity::Head, t).unwrap();
    let flow = sim.get_link_result("P1", LinkQuantity::Flow, t).unwrap();
    println!("t={t:.0}s  head={head:.3}  flow={flow:.6}");
}
```

## What the water distribution engine models

- Extended-period steady-state hydraulics (Global Gradient Algorithm)
- Pressure-driven and demand-driven demand models
- Conservative and reactive constituent transport (water quality, age, source tracing)
- EPANET 2.3 `.inp` format input; binary `.out` and plain-text `.rpt` output
- Report generation from saved templates (txt, csv, html, and optional pdf)

It does **not** model pressure transients, water-hammer, or multi-phase flow.

## License

[AGPL v3](https://github.com/neeraip/hydra/blob/main/LICENSE) — see [COMMERCIAL_LICENSE.md](https://github.com/neeraip/hydra/blob/main/.github/COMMERCIAL_LICENSE.md) for commercial licensing options.
