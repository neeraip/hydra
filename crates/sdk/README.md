# hydra-sdk

[![Crates.io](https://img.shields.io/crates/v/hydra-sdk)](https://crates.io/crates/hydra-sdk)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)
[![Cargo CI](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml/badge.svg)](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml)

Water distribution network simulator — EPANET-compatible extended-period simulation.

`hydra-sdk` is the user-facing library crate for [Hydra](https://github.com/neeraip/hydra). It re-exports the complete public API of `hydra-engine-wds` as a single stable dependency, with all internal crate versions pre-pinned.

**[→ Full documentation](https://neeraip.github.io/hydra/sdk/overview.html)**

## Install

```toml
[dependencies]
hydra-sdk = "1"
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

## What Hydra models

- Extended-period steady-state hydraulics (Global Gradient Algorithm)
- Pressure-driven and demand-driven demand models
- Conservative and reactive constituent transport (water quality, age, source tracing)
- EPANET 2.3 `.inp` format input; binary `.out` and plain-text `.rpt` output

Hydra does **not** model pressure transients, water-hammer, or multi-phase flow.

## License

[AGPL v3](https://github.com/neeraip/hydra/blob/main/LICENSE) — see [COMMERCIAL_LICENSE.md](https://github.com/neeraip/hydra/blob/main/.github/COMMERCIAL_LICENSE.md) for commercial licensing options.
