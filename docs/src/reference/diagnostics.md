# Diagnostics & Errors

Hydra reports problems in three ways: **exit codes** (CLI process status), **structured diagnostics** (machine-readable JSON lines on stderr), and **warnings** (non-fatal issues recorded during a run). This page catalogues each, for the water distribution engine.

> For the drainage engine's own import and validation diagnostics see [Diagnostics & Errors (Drainage)](drainage/diagnostics.md).

> **Scope.** The structured diagnostics below are emitted by the simulation
> command (`hydra <input>`). The `hydra report` subcommand shares the same exit
> codes but writes plain `error: …` text to stderr rather than JSON lines, so do
> not parse its stderr as diagnostics.

---

## Exit codes (CLI)

| Code | Meaning |
|---|---|
| `0` | Simulation completed (the report may still contain warnings) |
| `1` | Input error — bad arguments, bad `.inp` file, missing input, HTTP 4xx, or a network validation failure |
| `2` | Solver error — hydraulics or quality did not converge |
| `3` | I/O error — write failed, permission denied, HTTP 5xx, or network failure |
| `4` | Internal error — unexpected engine state; please report a bug |

Codes `0`–`4` are a stable contract. Note that network **validation** failures are input errors (exit `1`), not solver errors.

---

## Structured diagnostics (stderr)

Independently of the report, the CLI writes each warning and error to **stderr** as one compact JSON object per line. This stream is always emitted — it is not suppressed by `-q`/`--quiet` (which only silences the human-readable progress output).

Each line has five keys:

| Key | Type | Notes |
|---|---|---|
| `level` | string | `"warning"` or `"error"` |
| `code` | string | A stable slug, e.g. `warning/negative_pressure` (see tables below) |
| `message` | string | Human-readable description |
| `object_id` | string or `null` | The affected node/link ID, when applicable |
| `time_step` | number or `null` | Simulation time (seconds) of the event; set only on warnings |

Example:

```json
{"level":"warning","code":"warning/negative_pressure","message":"negative pressure at node 'J1'","object_id":"J1","time_step":3600.0}
{"level":"error","code":"solver/hydraulic","message":"...","object_id":null,"time_step":null}
```

### Diagnostic codes

**Warnings** (non-fatal; the run continues):

| Code | Raised when |
|---|---|
| `warning/unbalanced` | A hydraulic step did not converge within the trial limit |
| `warning/negative_pressure` | A node had negative pressure at a reporting step |
| `warning/pump_xhead` | A pump was driven beyond the maximum head on its curve |

**Errors** (fatal; the run stops):

| Code | Meaning | Exit code |
|---|---|---|
| `io/fetch` | Failed to read the input file or URL | `1` (4xx / not found) or `3` (5xx / network) |
| `input/format` | Unrecognised input file format | `1` |
| `input/engine` | The file is a sound `.inp` model, but another engine's — see [Foreign `.inp` dialects](inp-format.md#foreign-inp-dialects) | `1` |
| `input/parse` | `.inp` parse error (bad field, duplicate ID, syntax at a line) | `1` |
| `validation/network` | Network validation failed (one line per violation) | `1` |
| `solver/hydraulic` | The hydraulic solver failed | `2` |
| `solver/quality` | The quality engine failed | `2` |
| `io/output` | Failed to write the `.out` file | `3` |
| `io/report` | Failed to write the report | `3` |
| `session/error` | Other session error (unknown ID, invalid phase, no snapshot at time) | `1` |
| `internal` | Unexpected internal state | `4` |

---

## Warnings

Warnings are recorded during a run and surfaced in three places: the stderr diagnostics above, the `warnings` array of the [JSON report](output-files.md#json--json-report), and the Warnings section of the [text report](output-files.md#rpt--text-report). There are three kinds:

- **Unbalanced hydraulics** — a step did not converge. Often points to an over-constrained or disconnected model; see [Troubleshooting](../getting-started/troubleshooting.md).
- **Negative pressure** — a node's pressure went below zero, indicating demand that the network could not supply at that point (consider [PDA](inp-format.md#options-keywords)).
- **Pump exceeds maximum head** — a pump operated past the top of its curve; check the curve and the operating point.

---

## Validation errors

Before a simulation runs, Hydra validates the network and reports **all** structural problems it finds (not just the first). Each is emitted under the `validation/network` diagnostic code. The checks cover:

| Category | Examples |
|---|---|
| **Connectivity** | Link references an unknown end node; link connects a node to itself; a junction or tank is not reachable from any fixed-grade source; the network has no reservoir or tank |
| **References** | An element references a pattern, curve, or node/link that does not exist, or a curve of the wrong kind; a pump/GPV/PCV is missing its required curve |
| **Curves** | Pump head curve not strictly decreasing; efficiency y-values outside (0, 100]; tank volume curve not strictly increasing; GPV head-loss curve decreasing; curve x-values not strictly increasing; too few points |
| **Tanks** | Initial level outside the `[min, max]` range |
| **Patterns** | Pattern has no factors |
| **Controls & rules** | A simple control or rule action references a link (or a rule premise references a node/link) that does not exist |

The [Issues panel](../getting-started/gui.md#editing-the-network) in the GUI surfaces these findings with links to the affected elements.
