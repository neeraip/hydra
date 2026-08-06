# Hydra

[![Library](https://img.shields.io/github/v/release/neeraip/hydra?filter=v*&label=Library)](https://github.com/neeraip/hydra/releases?q=Hydra+Library&expanded=true)
[![CLI](https://img.shields.io/github/v/release/neeraip/hydra?filter=cli-v*&label=CLI)](https://github.com/neeraip/hydra/releases?q=Hydra+CLI&expanded=true)
[![GUI](https://img.shields.io/github/v/release/neeraip/hydra?filter=gui-v*&label=GUI)](https://github.com/neeraip/hydra/releases?q=Hydra+GUI&expanded=true)
[![CI](https://img.shields.io/github/actions/workflow/status/neeraip/hydra/cargo-ci.yml?branch=main&label=CI)](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml)
[![License](https://img.shields.io/badge/license-AGPL--3.0_or_commercial-blue)](#license)

Hydra is a water infrastructure simulation platform written in Rust. It is built as a suite of domain engines sharing one toolchain: a desktop GUI, a `hydra` CLI, and a Rust SDK.

Correctness is defined by conservation laws and Hydra's own convergence criteria rather than by agreement with a reference implementation. Where Hydra departs from the code its data model comes from, the departure is deliberate, documented in the owning specification, and explained.

| Engine | Domain | Source model | Status |
|---|---|---|---|
| **Water Distribution** (`wds`) | Pressurised supply networks — hydraulics, water quality, energy | EPANET `.inp` (2.x) | **Available** |
| **Urban Drainage** (`uds`) | Stormwater and wastewater collection — runoff, routing, quality | SWMM `.inp` | **Available** |
| **Open Channel** (`och`) | Rivers and channels — steady and unsteady flow | HEC-RAS project | Planned |

<!-- PLANNED-ENGINE: och — revise the table's Status column and drop this paragraph as each engine ships. -->
A planned engine is registered in the shared engine registry, so its key and crate name are reserved and the applications can present the full modelling scope — but it carries no implementation, and Hydra refuses to create projects or run simulations for it.

Every available engine runs from all three surfaces. Model *editing* in the desktop app is water distribution only for now: a drainage project is created by importing a SWMM model, and is then browsed, simulated and read like any other.

**[→ Download](https://github.com/neeraip/hydra/releases/latest)** · **[→ Full documentation](https://neeraip.github.io/hydra/)**

## Water distribution engine

Extended-period simulation (EPS) of hydraulic behaviour and water quality dynamics across pressurised pipe networks, computing the full time history of flows, pressures, and constituent concentrations at every node and link.

- **Hydraulics** — GGA solver, Hazen-Williams / Darcy-Weisbach / Chezy-Manning head loss, DDA and PDA demand models, pumps, all EPANET valve types, FAVAD leakage, rule-based controls
- **Water quality** — chemical constituent, water age, source tracing; Lagrangian transport; bulk and wall reactions; all EPANET tank mixing models
- **I/O** — all 11 EPANET flow unit systems; `.out` binary, `.rpt` text, `.json` report output

Inputs are EPANET `.inp` files (local or via HTTP URL) — any 2.x release, since the constructs 2.3 added are optional. Outputs are an EPANET-compatible binary `.out` file and a plain-text or JSON `.rpt` report.

## Urban drainage engine

Continuous and event simulation of stormwater and wastewater collection systems on the SWMM data model: rainfall-runoff with Horton / Green-Ampt / Curve Number infiltration, LID controls, snowmelt, groundwater and RDII; Preissmann-slot dynamic-wave routing through conduits, pumps, orifices, weirs, outlets and street inlets; pollutant buildup, washoff, treatment, and network transport; rule-based controls with PID modulation.

Inputs are SWMM `.inp` files; outputs are a SWMM-compatible binary `.out` file and a text report. Available from the CLI (`hydra run model.inp` — the model's own sections identify the engine), the SDK (the `hydra::uds` module), and the desktop app, where a drainage model can be imported, run and explored but not yet edited.

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
hydra run network.inp                                     # summary to stdout
hydra run network.inp --summary report.rpt --results output.out
hydra run https://example.com/network.inp --summary report.json
hydra engines                                             # engines this build provides
```

See [crates/cli/README.md](crates/cli/README.md) for the full option reference.

### SDK (Rust library)

```toml
[dependencies]
hydra-sdk = "8"
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
| [Engines](https://neeraip.github.io/hydra/engines.html) | The domain engines, what each covers, and what ships today |
| [Getting Started](https://neeraip.github.io/hydra/getting-started/installation.html) | Installation, build, CLI, GUI |
| [SDK](https://neeraip.github.io/hydra/sdk/overview.html) | Library usage and examples |
| [Architecture](https://neeraip.github.io/hydra/architecture/crates.html) | Crate layout and specifications |
| [Reference](https://neeraip.github.io/hydra/reference/inp-format.html) | INP format, performance, EPANET migration |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](.github/CONTRIBUTING.md) before opening a pull request, in particular the **Spec First** workflow, which requires spec changes to land before implementation changes for any solver, model, or analytics work.

## License

Hydra is published under the [GNU Affero General Public License v3.0](LICENSE), with a
[commercial license](COMMERCIAL_LICENSE.md) available for the cases the AGPL does not fit.

**Using Hydra asks nothing of you.** Run the CLI, drive it from a script, model in the desktop app,
check its results against another engine, or change it for your own purposes — none of that carries
an obligation. Your models, your results, and whatever else you run alongside Hydra stay yours, and
you may use them commercially.

**Building Hydra into something you distribute is what the AGPL governs.** Link the crates into your
own application and ship it, or run a modified Hydra as a network service, and that work carries the
same license with its source made available. Calling `hydra` as a separate program — handing it a
file, reading what it writes — is use, not incorporation.

If reciprocity does not suit — a proprietary product, a hosted service you cannot open — the
[commercial license](COMMERCIAL_LICENSE.md) grants those same rights without it.

This is a summary and not legal advice: the [license text](LICENSE) is what governs, and a case near
the line deserves a lawyer rather than a README.
