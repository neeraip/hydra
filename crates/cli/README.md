# hydra-cli

[![Crates.io](https://img.shields.io/crates/v/hydra-cli)](https://crates.io/crates/hydra-cli)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)
[![Cargo CI](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml/badge.svg)](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml)

Command-line interface for [Hydra](https://github.com/neeraip/hydra) — reads EPANET `.inp` network descriptions from files or HTTP URLs, runs extended-period hydraulic and water quality simulation, and writes results to `.rpt` and `.out` files.

**[→ Full documentation](https://neeraip.github.io/hydra/getting-started/cli.html)**

## Install

**Option 1 — Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

> **macOS** — After downloading, remove the quarantine flag before running:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2 — Cargo**

```sh
cargo install hydra-cli
```

## Usage

```sh
# Run a simulation — report goes to stdout
hydra network.inp

# Write report and binary output to files
hydra network.inp report.rpt output.out

# Named flags (equivalent)
hydra --input network.inp --report report.rpt --output output.out

# Accept an HTTP URL as input
hydra https://example.com/network.inp

# JSON report
hydra network.inp --report report.json

# Suppress progress output
hydra -q network.inp

# Print version
hydra -v
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Simulation completed (warnings may appear in the report) |
| `1` | Input validation error (bad INP, HTTP 4xx, missing file) |
| `2` | Solver error (non-convergence or singularity) |
| `3` | I/O error (file not found, permission denied, HTTP 5xx, network) |

## License

[AGPL v3](https://github.com/neeraip/hydra/blob/main/LICENSE) — see [COMMERCIAL_LICENSE.md](https://github.com/neeraip/hydra/blob/main/.github/COMMERCIAL_LICENSE.md) for commercial licensing options.
