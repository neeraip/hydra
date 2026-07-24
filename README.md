# Hydra

[![Library](https://img.shields.io/github/v/release/neeraip/hydra?filter=v*&label=Library)](https://github.com/neeraip/hydra/releases?q=Hydra+Library&expanded=true)
[![CLI](https://img.shields.io/github/v/release/neeraip/hydra?filter=cli-v*&label=CLI)](https://github.com/neeraip/hydra/releases?q=Hydra+CLI&expanded=true)
[![GUI](https://img.shields.io/github/v/release/neeraip/hydra?filter=gui-v*&label=GUI)](https://github.com/neeraip/hydra/releases?q=Hydra+GUI&expanded=true)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

Hydra is a water distribution network simulator written in Rust. It performs extended-period simulation (EPS) of hydraulic behaviour and water quality dynamics across pressurised pipe networks, computing the full time history of flows, pressures, and constituent concentrations at every node and link.

Inputs are EPANET 2.3 `.inp` files (local or via HTTP URL). Outputs are an EPANET-compatible binary `.out` file and a plain-text or JSON `.rpt` report.

**[→ Full documentation](https://neeraip.github.io/hydra/)**

## Features

- **Hydraulics** — GGA solver, Hazen-Williams / Darcy-Weisbach / Chezy-Manning head loss, DDA and PDA demand models, pumps, all EPANET valve types, FAVAD leakage, rule-based controls
- **Water quality** — chemical constituent, water age, source tracing; Lagrangian transport; bulk and wall reactions; all EPANET tank mixing models
- **I/O** — all 11 EPANET flow unit systems; `.out` binary, `.rpt` text, `.json` report output
- **Interfaces** — desktop GUI (Windows, macOS, Linux) and a `hydra` CLI binary

## Install

### GUI

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

### CLI

**Pre-built binary** (no Rust required) — download from the [releases page](https://github.com/neeraip/hydra/releases/latest).

**Cargo:**

```sh
cargo install hydra-cli
```

**Basic usage:**

```sh
hydra network.inp                                         # report to stdout
hydra network.inp report.rpt output.out                   # write report + binary output
hydra https://example.com/network.inp --report report.json  # HTTP input, JSON report
```

See [crates/cli/README.md](crates/cli/README.md) for the full option reference.

### SDK (Rust library)

```toml
[dependencies]
hydra-sdk = "1"
```

```rust
use hydra_sdk::{io, Simulation, NodeQuantity};

let network = io::parse(&std::fs::read("network.inp")?)?;
let mut sim = Simulation::create();
sim.load(network)?;
sim.run()?;
```

See the [SDK documentation](https://neeraip.github.io/hydra/sdk/overview.html) for a full usage guide.

## Build from source

Prerequisites: Rust ≥ 1.95, [`just`](https://just.systems/).  
GUI only: Node.js 24, pnpm 11, Tauri CLI.

```sh
git clone https://github.com/neeraip/hydra.git
cd hydra
just build
just test
```

See [CONTRIBUTING.md](.github/CONTRIBUTING.md) for the full development setup.

## Documentation

| | |
|---|---|
| [Getting Started](https://neeraip.github.io/hydra/getting-started/installation.html) | Installation, build, CLI, GUI |
| [SDK](https://neeraip.github.io/hydra/sdk/overview.html) | Library usage and examples |
| [Architecture](https://neeraip.github.io/hydra/architecture/crates.html) | Crate layout and specifications |
| [Reference](https://neeraip.github.io/hydra/reference/inp-format.html) | INP format, performance, EPANET migration |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](.github/CONTRIBUTING.md) before opening a pull request, in particular the **Spec First** workflow, which requires spec changes to land before implementation changes for any solver, model, or analytics work.

## License

[AGPL v3](LICENSE): free to use and modify. Commercial products built on Hydra must either release their source under AGPL v3 or obtain a separate commercial license. See [COMMERCIAL_LICENSE.md](.github/COMMERCIAL_LICENSE.md) for details.
