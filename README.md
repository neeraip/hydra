# Hydra

[![Cargo CI](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml/badge.svg)](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml)
[![GitHub release](https://img.shields.io/github/v/release/neeraip/hydra)](https://github.com/neeraip/hydra/releases/latest)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

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

## Getting Started

### Prerequisites

- **Rust** (stable, ≥ 1.95) — install via [rustup](https://rustup.rs/)
- **[just](https://just.systems)** — cross-platform task runner (`cargo install just` or `brew install just`)
- **For the GUI only:** Node.js 22, [pnpm](https://pnpm.io) 10, [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli`), and the [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your platform

### Build

```sh
just build          # debug
just release        # release (fat LTO)
```

### Test

```sh
just test
```

### CLI

```sh
# Run a simulation — writes report to stdout, no binary output
cargo run --bin hydra -- network.inp

# With explicit output paths (EPANET-style positional convention)
cargo run --bin hydra -- network.inp report.rpt output.out

# Or using named flags
cargo run --bin hydra -- --input network.inp --report report.rpt --output output.out

# JSON report
cargo run --bin hydra -- network.inp report.json

# Install the binary locally
cargo install --path crates/cli
hydra network.inp
```

### GUI

```sh
cd crates/gui/frontend && pnpm install && cd ..
cargo tauri dev    # development mode with hot-reload frontend
cargo tauri build  # production build
```

## Library Usage

Add `hydra` to your `Cargo.toml`:

```toml
[dependencies]
hydra = { git = "https://github.com/neeraip/hydra" }
```

### Parse an INP file and run a full simulation

```rust
use hydra::{io, Simulation, NodeQuantity, LinkQuantity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Read and parse an EPANET .inp file.
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    // 2. Create a simulation and load the network.
    let mut sim = Simulation::create();
    sim.load(network)?;

    // 3. Run hydraulics + quality to completion.
    sim.run()?;

    // 4. Query results at each reporting time step.
    for t in sim.snapshot_times() {
        let head = sim.get_node_result("J1", NodeQuantity::Head, t)?;
        let pressure = sim.get_node_result("J1", NodeQuantity::GaugePressure, t)?;
        let flow = sim.get_link_result("P1", LinkQuantity::Flow, t)?;
        println!("t={t:.0}s  head={head:.3}  pressure={pressure:.3}  flow={flow:.6}");
    }

    // 5. Print warnings (if any).
    for w in sim.warnings() {
        println!("[t={:.0}s] {:?}", w.t, w.kind);
    }

    Ok(())
}
```

### Create a simulation from a parsed network directly

```rust
use hydra::{io, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    // Convenience constructor — shorthand for create() + load().
    let mut sim = Simulation::from_network(network)?;
    sim.run()?;

    Ok(())
}
```

### Step through hydraulics manually

```rust
use hydra::{io, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    let mut sim = Simulation::create();
    sim.load(network)?;

    // Step one hydraulic period at a time.
    loop {
        let dt = sim.step_hydraulics()?;
        if dt == 0.0 { break; }
        // ... inspect or modify state between steps ...
    }

    Ok(())
}
```

## Architecture

Hydra is a multi-crate Rust workspace:

| Crate | Role |
|---|---|
| `hydra` | Umbrella facade — re-exports the complete user-facing API with all dependency versions pre-pinned |
| `hydra-common` | Thin shared infrastructure — engine-agnostic types (`Coordinate`, `Crs`) |
| `hydra-engine` | Complete simulation engine — data model, parsers, unit conversion, GGA hydraulic solver, Lagrangian quality engine, session API, analytics |
| `hydra-cli` | Command-line interface — resolves input, writes output files; no simulation logic |
| `hydra-gui` | Desktop application — Tauri shell with deck.gl canvas, timeline playback, network editor |

`hydra-cli` and `hydra-gui` are downstream consumers of Hydra in exactly the same way a third-party integrator would be — they depend on the umbrella crate and never import from `hydra-engine` or `hydra-common` directly. Anyone who wants a different interface (HTTP, gRPC, Python bindings, etc.) follows the same pattern.

## Specifications

| Document | Scope |
|---|---|
| [`crates/engine/src/model/spec.md`](crates/engine/src/model/spec.md) | Data model, unit system, model file formats |
| [`crates/engine/src/hydraulics/spec.md`](crates/engine/src/hydraulics/spec.md) | Hydraulic engine: GGA solver, sparse Cholesky, valves, demands |
| [`crates/engine/src/quality/spec.md`](crates/engine/src/quality/spec.md) | Quality engine: transport, mixing, reactions, source injection |
| [`crates/engine/src/simulation/spec.md`](crates/engine/src/simulation/spec.md) | Simulation orchestrator: controls, timestep, accounting, session API |
| [`crates/engine/src/analysis/spec.md`](crates/engine/src/analysis/spec.md) | Post-simulation analytics: demand reliability, service compliance |

## Performance

Benchmarked on real-world networks from the [KIOS-Research/EPANET-Benchmarks](https://github.com/KIOS-Research/EPANET-Benchmarks) collection. Hydra is compiled with `lto = "fat"` and `codegen-units = 1`. Times are the minimum of 3 wall-clock runs.

| Network | Nodes | Links | Steps | EPANET | Hydra | Ratio |
|---|---|---|---|---|---|---|
| Balerma | 447 | 454 | 3 | 7 ms | 5 ms | 0.78× |
| KY 8 | 2,432 | 2,823 | 289 | 118 ms | 94 ms | 0.80× |
| KY 9 | 2,650 | 3,042 | 289 | 120 ms | 76 ms | 0.63× |
| KY 10 | 3,211 | 4,528 | 289 | 318 ms | 222 ms | 0.70× |
| Richmond | 872 | 958 | 289 | 26 ms | 31 ms | 1.19× |
| D-Town | 407 | 459 | 1,441 | 137 ms | 120 ms | 0.88× |
| L-TOWN | 785 | 909 | 2,035 | 207 ms | 209 ms | 1.01× |
| BWSN2 | 12,523 | 14,822 | 97 | 598 ms | 731 ms | 1.22× |

Values < 1.0× mean Hydra is faster. Hydra matches or outperforms EPANET on most networks. The remaining gap on control-heavy networks (Richmond, BWSN2) is due to per-iteration overhead in Rust's safe indexing model versus C pointer arithmetic.

For maximum local performance, build with native CPU target features:

```sh
just release-native
```

## Relationship to EPANET

Hydra's hydraulic and quality engines were derived by studying EPANET's mathematical foundations. Hydra is **not** an EPANET clone or compatibility layer — it is a distinct solver that models the same physics. Where the two diverge, Hydra's result is authoritative. Users migrating from EPANET should expect small numerical differences; these reflect Hydra solving the system more accurately, not incorrectly.

## Testing

Hydra's integration strategy is Hydra-native and fixture-driven:
- Purpose-built fixture networks validate control, timestep, quality, reaction, and tank behaviour through physics/behaviour invariants.
- Large real-world networks are exercised with deterministic regression checks by comparing repeated Hydra runs section-by-section.

Correctness is established by physics/behaviour invariants and deterministic Hydra-vs-Hydra regression checks — not by agreement with any external tool's output.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](.github/CONTRIBUTING.md) before opening a pull request — in particular the **Spec First** workflow, which requires spec changes to land before implementation changes for any solver, model, or analytics work.

## License

[AGPL v3](LICENSE) — free to use and modify. Commercial products built on Hydra must either release their source under AGPL v3 or obtain a separate commercial license. See [COMMERCIAL_LICENSE.md](.github/COMMERCIAL_LICENSE.md) for details.
