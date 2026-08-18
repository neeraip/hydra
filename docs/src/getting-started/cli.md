# CLI

The `hydra` binary runs models through Hydra's [engines](../engines.md) and builds report documents from the results.

```
hydra run <MODEL> [--engine KEY] [--results PATH] [--summary PATH]
hydra report --model <PATH> --results <PATH> [-o PATH]
hydra engines
```

**The engine is detected from the model, never from its extension.** `.inp`
belongs to both EPANET and SWMM, so the filename cannot decide. There is no
default engine: if the model does not identify one, Hydra stops and asks you to
name it with `--engine` rather than guessing. See
[Engine selection](#engine-selection).

The water distribution (`wds`, EPANET models) and urban drainage (`uds`, SWMM
models) engines are implemented today; `hydra engines` lists what this build
provides.

> **Upgrading from 2.x:** `hydra <model> <report> <output>` is gone. Use
> `hydra run <model> --summary <report> --results <output>`. Running the old
> form prints a hint naming the replacement. See
> [Migrating from EPANET](../reference/migrating-from-epanet.md) for the full
> mapping.

## Install

For most users, **Cargo install is the recommended path** on macOS, Linux, and Windows.

**Option 1: Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest) and place it on your `PATH`.

> **macOS:** Pre-built CLI binaries are currently not notarised. If Gatekeeper blocks the binary, remove the quarantine flag:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2: Cargo (recommended)**

```sh
cargo install hydra-cli
```

Verify the installation:

```sh
hydra -V
```

## Basic Usage

```sh
# Run a simulation — summary goes to stdout
hydra run network.inp

# Save the summary to a file
hydra run network.inp --summary report.rpt

# Save the summary and the binary time-series results
hydra run network.inp --summary report.rpt --results output.out
```

`hydra run` takes exactly one positional argument: the model. Everything the
run writes is named by a flag, so nothing depends on argument order.
`hydra -V` prints the Hydra engine version and the CLI version on separate
lines.

## Output Formats

The `--summary` path controls the summary format:

```sh
# Plain-text summary (EPANET-style .rpt)
hydra run network.inp --summary report.rpt

# JSON summary (useful for scripts and data pipelines)
hydra run network.inp --summary report.json

# Binary results (.out) — EPANET-compatible, readable by post-processing tools
hydra run network.inp --summary report.rpt --results output.out
```

For configurable report documents (txt, csv, html, pdf) built from a saved
`.out`, see [Generating a report](#generating-a-report).

## Running from a URL

The model may be fetched over HTTP or HTTPS:

```sh
hydra run https://example.com/network.inp
hydra run https://example.com/network.inp --summary report.rpt --results output.out
```

Both `http://` and `https://` are accepted. The fetch follows up to 10 redirects, uses a 10-second connect timeout and a 300-second overall timeout, and accepts response bodies up to 1 GiB. An HTTP 4xx response is treated as an input error (exit `1`); a 5xx or network failure is an I/O error (exit `3`).

## Flags

### `hydra run`

| Flag | Description |
|---|---|
| `<MODEL>` | Path or `http(s)://` URL of the model to run. The only positional |
| `--engine <KEY>` | Run with a named engine (`wds`, `uds`). Omit to detect it from the model |
| `--results <PATH>` | Binary time-series results (`.out`). Omitted, none is written |
| `--summary <PATH>` | Run summary in the engine's native format (`.rpt`; the water engine also writes `.json` when the path ends in `.json`, the drainage engine refuses it for now). Omitted, it goes to stdout |

### Global

| Flag | Description |
|---|---|
| `-q`, `--quiet` | Suppress progress output. Progress is also suppressed when stderr is not a terminal. Errors and diagnostics are never suppressed |
| `-v`, `-vv` | Increase detail. `-v` names the engine and adds per-stage notes; `-vv` adds timing and internals. Conflicts with `--quiet` |
| `-V`, `--version` | Print Hydra and CLI version information |
| `-h`, `--help` | Print usage |

Global flags may appear before or after the subcommand.

## Engine selection

A model's engine is decided by its **contents**, not its filename. Each engine
is asked whether the model is one of its own, and exactly one positive
identification is required to proceed.

| Situation | Result |
|---|---|
| One engine identifies the model | It runs |
| Nothing identifies it, but it is shaped like some engine's format | Error: name the engine with `--engine` |
| No engine recognises the format | Error |
| The owning engine is registered but not implemented | Error naming it |

An EPANET model routes to the water distribution engine and a SWMM model to
the urban drainage engine, each identified by the sections only its format
declares. A sparse `.inp` built solely from sections both formats share is
genuinely ambiguous, and Hydra says so rather than picking.

There is deliberately **no fallback**. Handing a stormwater model to a
pressurised-pipe solver would produce a confident, wrong answer rather than a
failure, so Hydra stops instead of guessing.

`--engine <KEY>` names the engine explicitly. That is more information than
detection has, so it is also the escape hatch for a sparse model that carries
nothing identifying: the named engine parses it under its normal rules.

```bash
hydra run net.inp --engine wds     # skip detection: parse as EPANET
hydra run net.inp --engine uds     # skip detection: parse as SWMM
hydra engines                      # what this build provides
```

An urban drainage run differs from a water distribution run in a few
CLI-visible ways: progress is a single phase (hydrology, routing, and water
quality advance together); `--summary` writes the engine's text report only
(no `.json` yet); and auxiliary files the model declares (daily climate
records, hotstart state, routing interface files) are read and written
relative to the model file's directory, which is why a model fetched over
HTTP cannot declare them.

## Generating a report

`hydra report` builds a report document from a completed run's results. It is a
separate step from the simulation: you point it at the model and the `.out` file
the run produced.

This subcommand covers water distribution models today. A drainage model is
refused by name; its report blocks are available through the GUI and the SDK.

```bash
hydra report --model network.inp --results output.out -o report.html
```

| Flag | Description |
|---|---|
| `--model <PATH>` | The `.inp` file the results were produced from |
| `--results <PATH>` | The `.out` binary from a completed run |
| `--template <PATH>` | Report template JSON: which blocks, in what order. Omit to cover every available block |
| `--format <FORMAT>` | `txt`, `csv`, `html`, or `pdf`. Inferred from the `--out` extension when omitted; defaults to `txt` |
| `-o`, `--out <PATH>` | Output path; omit to write to stdout |
| `--no-timestamp` | Omit the generation timestamp so output is byte-reproducible |

The content comes from the engine's **report blocks**: named, self-contained
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
| `1` | Input error: bad `.inp` file, missing file, HTTP 4xx |
| `2` | Solver error: hydraulics did not converge |
| `3` | I/O error: write failed, permission denied, HTTP 5xx |
| `4` | Internal error: unexpected engine state; please report a bug |

> **Breaking change:** internal errors previously exited with code `2` (the
> solver-error code). They now exit with the dedicated code `4`; codes
> `0`–`3` are unchanged.

## Reading the Report

Both report formats are **summary-level**. Per-node and per-link time series are written only to the binary `.out` file (`--results`).

The text report (`.rpt`) contains:

- **Header**: a Hydra version banner and the network title
- **Input summary**: element counts, head-loss formula, demand model, timesteps, and simulation duration
- **Warnings**: non-fatal diagnostics raised during the run, covering unbalanced hydraulics, negative pressures, and pump-head warnings
- **Analysis timestamps**: "Analysis begun" / "Analysis ended" markers

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
