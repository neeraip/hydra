# hydra-cli

[![Crates.io](https://img.shields.io/crates/v/hydra-cli)](https://crates.io/crates/hydra-cli)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)

Command-line interface for [Hydra](https://github.com/neeraip/hydra), the water infrastructure simulation platform. It drives Hydra's water distribution engine: reads EPANET `.inp` network descriptions from files or HTTP URLs, runs extended-period hydraulic and water quality simulation, and writes results to `.rpt` and `.out` files. <!-- PLANNED-ENGINE: uds,och — drop this sentence and document engine selection as each engine becomes reachable from the CLI. -->Hydra's other engines — urban drainage and open channel — are planned and not yet available from the CLI.

**[→ Full documentation](https://neeraip.github.io/hydra/getting-started/cli.html)**

## Breaking changes in 3.0

The argument surface was redesigned so it can carry more than one engine.

- **Subcommands.** `hydra run <model>`, `hydra report`, `hydra engines`.
  A bare `hydra <model>` prints a hint naming the replacement.
- **The EPANET positional triple is gone.** `hydra net.inp net.rpt net.out`
  becomes `hydra run net.inp --summary net.rpt --results net.out`. The old
  order encoded one engine and one pair of artifacts; every added engine
  made it less true.
- **One name per concept.** The model is `<MODEL>`, the binary time series
  is `--results`, the native run log is `--summary`. Previously `--output`
  meant the binary results while `hydra report`'s `--out` meant the report
  document — one letter apart, opposite meanings.
- **`--engine`**, defaulting to detection from the model's contents. There is
  no default engine; an unidentifiable model is an error, never a guess.
- **`-v` is verbosity** (repeatable), reclaimed at this major boundary. `-V`
  remains `--version`.
- **Internal errors exit `4`**, no longer reusing `2` (solver). `0`–`3`
  unchanged.

## Install

For most users, **Cargo install is the recommended path**.

**Option 1 — Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

> **macOS** — Pre-built CLI binaries are currently not notarised. If Gatekeeper blocks the binary, remove the quarantine flag:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2 — Cargo (recommended)**

```sh
cargo install hydra-cli
```

## Usage

```sh
# Run a simulation — summary goes to stdout
hydra run network.inp

# Write the summary and the binary time-series results
hydra run network.inp --summary report.rpt --results output.out

# JSON summary (chosen by the .json suffix)
hydra run network.inp --summary report.json

# Accept an HTTP URL as the model (redirects followed, up to 10; plain
# http:// accepted; bodies up to 1 GiB; 10 s connect / 300 s overall timeout)
hydra run https://example.com/network.inp

# Name the engine instead of detecting it from the model
hydra run network.inp --engine wds

# What engines does this build provide?
hydra engines

# Build a report document from a finished run
hydra report --model network.inp --results output.out -o report.html

# Suppress progress output / add detail
hydra run network.inp -q
hydra run network.inp -v

# Print version
hydra -V
```

### Engine selection

The engine is decided by the model's **contents**, never its extension —
`.inp` belongs to both EPANET and SWMM. Exactly one engine must identify the
model for it to run. If none does, or the file is only shaped like some
engine's format without identifying it, Hydra stops and asks for `--engine`
rather than guessing: routing a stormwater model to a pressurised-pipe solver
would return a confident wrong answer instead of an error.

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Simulation completed (warnings may appear in the report) |
| `1` | Usage/input error (bad arguments, bad INP, HTTP 4xx, missing input file) |
| `2` | Solver error (non-convergence or singularity) |
| `3` | I/O error (permission denied, HTTP 5xx, network failure) |
| `4` | Internal error (unexpected engine state; please report a bug) |

## License

[AGPL v3](https://github.com/neeraip/hydra/blob/main/LICENSE) — see [COMMERCIAL_LICENSE.md](https://github.com/neeraip/hydra/blob/main/.github/COMMERCIAL_LICENSE.md) for commercial licensing options.
