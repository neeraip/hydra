# CLI

<!-- PLANNED-ENGINE: uds,och — drop the last sentence and document engine selection once a second engine is reachable from the CLI. -->
The `hydra` binary drives Hydra's water distribution [engine](../engines.md): it reads an EPANET `.inp` model, runs an extended-period simulation, and writes `.rpt`, `.out`, and report files. The planned engines are not yet reachable from the CLI.

## Install

For most users, **Cargo install is the recommended path** on macOS, Linux, and Windows.

**Option 1 — Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest) and place it on your `PATH`.

> **macOS** — Pre-built CLI binaries are currently not notarised. If Gatekeeper blocks the binary, remove the quarantine flag:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2 — Cargo (recommended)**

```sh
cargo install hydra-cli
```

Verify the installation:

```sh
hydra -V
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

At most three positional arguments (input, report, output) are accepted; passing more is a usage error (exit `1`). `hydra -V` prints the Hydra engine version and the CLI version on separate lines.

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

Both `http://` and `https://` are accepted, and a URL may also be given via `--input`. The fetch follows up to 10 redirects, uses a 10-second connect timeout and a 300-second overall timeout, and accepts response bodies up to 1 GiB. An HTTP 4xx response is treated as an input error (exit `1`); a 5xx or network failure is an I/O error (exit `3`).

## Flags

| Flag | Description |
|---|---|
| `--input <PATH>` | Path to the `.inp` model file, or an `http(s)://` URL (alternative to positional) |
| `--report <PATH>` | Report output path (`.rpt` or `.json`); defaults to stdout |
| `--output <PATH>` | Binary output path (`.out`); omit to skip |
| `-q`, `--quiet` | Suppress progress output (auto-suppressed when stderr is not a terminal, e.g. when piping or redirecting) |
| `-V`, `--version` | Print version and exit |
| `-h`, `--help` | Print help and exit |

> **Breaking change** — `-v` previously meant `--version`. The short version
> flag is now `-V` (GNU/clap convention). `-v` is no longer accepted: it exits
> with code `1` and a hint suggesting `-V` (version) or `-q`/`--quiet`, rather
> than being silently repurposed, so scripts that relied on the old meaning
> fail loudly.

## Generating a report

`hydra report` builds a report document from a completed run's results. It is a
separate step from the simulation: you point it at the model and the `.out` file
the run produced.

```bash
hydra report --model network.inp --results output.out -o report.html
```

| Flag | Description |
|---|---|
| `--model <PATH>` | The `.inp` file the results were produced from |
| `--results <PATH>` | The `.out` binary from a completed run |
| `--template <PATH>` | Report template JSON — which blocks, in what order. Omit to cover every available block |
| `--format <FORMAT>` | `txt`, `csv`, `html`, or `pdf`. Inferred from the `--out` extension when omitted; defaults to `txt` |
| `-o`, `--out <PATH>` | Output path; omit to write to stdout |
| `--no-timestamp` | Omit the generation timestamp so output is byte-reproducible |

The content comes from the engine's **report blocks** — named, self-contained
sections such as run summary, result extremes, pump energy, service compliance,
and the distribution charts. A template selects and orders them; without one you
get everything that applies to the run. See
[Post-Simulation Analytics](../reference/analytics.md) for what the blocks cover.

`--no-timestamp` exists for diffing and for reproducible builds: with it, the
same inputs produce byte-identical output.

## Exit Codes

| Code | Meaning |
|---|---|
| `0` | Simulation completed (check report for warnings) |
| `1` | Input error — bad `.inp` file, missing file, HTTP 4xx |
| `2` | Solver error — hydraulics did not converge |
| `3` | I/O error — write failed, permission denied, HTTP 5xx |
| `4` | Internal error — unexpected engine state; please report a bug |

> **Breaking change** — internal errors previously exited with code `2` (the
> solver-error code). They now exit with the dedicated code `4`; codes
> `0`–`3` are unchanged.

## Reading the Report

Both report formats are **summary-level**. Per-node and per-link time series are written only to the binary `.out` file (`--output`).

The text report (`.rpt`) contains:

- **Header** — a Hydra version banner and the network title
- **Input summary** — element counts, head-loss formula, demand model, timesteps, and simulation duration
- **Warnings** — non-fatal diagnostics raised during the run: unbalanced hydraulics, negative pressures, and pump-head warnings
- **Analysis timestamps** — "Analysis begun" / "Analysis ended" markers

It does **not** contain per-node/link result tables, a network-status section, or an energy-usage section. Use the `.out` file for full results, or the JSON report's `energy` block for the energy summary.

The JSON report contains the same summary-level data plus energy, flow-balance, and mass-balance blocks, in a structured format:

```json
{
  "input": {
    "junctions": 92, "reservoirs": 1, "tanks": 2,
    "pipes": 117, "pumps": 2, "valves": 0,
    "headloss_formula": "Hazen-Williams", "demand_model": "DDA",
    "hydraulic_timestep_s": 3600.0, "quality_timestep_s": 360.0,
    "duration_s": 86400.0, "report_timestep_s": 3600.0
  },
  "warnings": [...],
  "energy": { "pumps": [...], "peak_demand_kw": 12.3 },
  "flow_balance": { ... },
  "mass_balance": { ... },
  "analysis": { "begun_epoch": "1615687166", "ended_epoch": "1615687167" }
}
```

The `begun_epoch` / `ended_epoch` values are strings holding raw seconds since the Unix epoch (or `null` if unavailable), not formatted datetimes.

For full time-series data across all nodes and links, use the binary `.out` format.

## Diagnostics on stderr

Independently of the report, the simulation command emits warnings and errors to **stderr** as one JSON object per line, suitable for machine parsing in scripts and pipelines:

```json
{"level":"warning","code":"warning/negative_pressure","message":"...","object_id":"J1","time_step":3600.0}
{"level":"error","code":"solver/hydraulic","message":"...","object_id":null,"time_step":null}
```

Each line has `level` (`warning` or `error`), a `code`, a human-readable `message`, and nullable `object_id` and `time_step` fields. The human-readable progress bar and banner (also on stderr) are suppressed by `-q`/`--quiet` and when stderr is not a terminal; the JSON diagnostics are not.

`hydra report` does not emit these. It shares the same [exit codes](#exit-codes) but reports failures as plain `error: …` lines on stderr.
