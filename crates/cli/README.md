# hydra-cli

[![Crates.io](https://img.shields.io/crates/v/hydra-cli)](https://crates.io/crates/hydra-cli)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://github.com/neeraip/hydra/blob/main/LICENSE)

Command-line interface for [Hydra](https://github.com/neeraip/hydra) — reads EPANET `.inp` network descriptions from files or HTTP URLs, runs extended-period hydraulic and water quality simulation, and writes results to `.rpt` and `.out` files.

**[→ Full documentation](https://neeraip.github.io/hydra/getting-started/cli.html)**

## Breaking changes (next release)

The next published version contains two breaking CLI changes:

- **`-v` no longer means `--version`.** The short version flag is now `-V`
  (GNU/clap convention). `-v` is rejected with exit code `1` and a hint
  suggesting `-V` (version) or `-q`/`--quiet` — it is not silently
  repurposed, so scripts using the old flag fail loudly.
- **Internal errors now exit with code `4`.** They previously reused exit
  code `2`, the solver-error code. Codes `0`–`3` are unchanged.

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
# Run a simulation — report goes to stdout
hydra network.inp

# Write report and binary output to files
hydra network.inp report.rpt output.out

# Named flags (equivalent)
hydra --input network.inp --report report.rpt --output output.out

# Accept an HTTP URL as input (redirects are followed, up to 10; plain
# http:// is accepted; response bodies up to 1 GiB; 10 s connect / 300 s
# overall timeout)
hydra https://example.com/network.inp

# JSON report
hydra network.inp --report report.json

# Suppress progress output
hydra -q network.inp

# Print version
hydra -V
```

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
