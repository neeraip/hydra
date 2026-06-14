# CLI

## Install

**Option 1 — Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest) and place it on your `PATH`.

> **macOS** — After downloading, remove the quarantine flag before running:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2 — Cargo**

```sh
cargo install hydra-cli
```

Verify the installation:

```sh
hydra -v
```

## Basic Usage

```sh
# Run a simulation — report goes to stdout
hydra network.inp

# Save the report to a file
hydra network.inp report.rpt

# Save the report and binary output
hydra network.inp report.rpt output.out

# Same, using named flags (equivalent to the above)
hydra --input network.inp --report report.rpt --output output.out
```

## Output Formats

The report path controls what format is written:

```sh
# Plain-text report (EPANET-style .rpt)
hydra network.inp report.rpt

# JSON report (useful for scripts and data pipelines)
hydra network.inp report.json

# Binary output (.out) — EPANET-compatible, readable by post-processing tools
hydra network.inp report.rpt output.out
```

## Running from a URL

Hydra can fetch a network file directly over HTTP or HTTPS:

```sh
hydra https://example.com/network.inp
hydra https://example.com/network.inp report.rpt output.out
```

## Flags

| Flag | Description |
|---|---|
| `--input <PATH>` | Path to the `.inp` model file (alternative to positional) |
| `--report <PATH>` | Report output path (`.rpt` or `.json`); defaults to stdout |
| `--output <PATH>` | Binary output path (`.out`); omit to skip |
| `-q`, `--quiet` | Suppress progress output (auto-enabled when stdout/stderr are not a terminal) |
| `-v`, `--version` | Print version and exit |
| `-h`, `--help` | Print help and exit |

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Simulation completed (check report for warnings) |
| `1` | Input error — bad `.inp` file, missing file, HTTP 4xx |
| `2` | Solver error — hydraulics did not converge |
| `3` | I/O error — write failed, permission denied, HTTP 5xx |

## Reading the Report

The text report (`.rpt`) follows EPANET conventions. Key sections:

- **Network Status** — link/valve status at each time step
- **Node Results** — demand, head, pressure, quality per node
- **Link Results** — flow, velocity, headloss, quality per link
- **Energy Usage** — pump efficiency and cost summary
- **Warnings** — convergence issues, negative pressures, quality anomalies

The JSON report contains summary-level data (not per-node/link time series) in a structured format:

```json
{
  "input": { "title": "...", "units": "GPM", ... },
  "warnings": [...],
  "energy": { "pumps": [...], "peak_demand_kw": 12.3 },
  "flow_balance": { ... },
  "mass_balance": { ... },
  "analysis": { "begun_epoch": "...", "ended_epoch": "..." }
}
```

For full time-series data across all nodes and links, use the binary `.out` format.
