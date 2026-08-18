# Post-Simulation Analytics

After a run, Hydra can derive higher-level metrics from the saved results. There are three surfaces:

- **Programmatic analytics** (SDK): two on-demand modules, *demand reliability* and *service compliance*, that read a saved `.out` file and return a structured report.
- **Report blocks** (SDK): a catalog of named, self-contained content blocks that render into txt/csv/html/PDF documents. Several of them are built *on* the two modules above, which is how those metrics reach a generated report. See [SDK Overview](../sdk/overview.md#reports).
- **The GUI Results tab**: an interactive dashboard computed separately from the same results.

The CLI exposes report blocks through its `report` subcommand ([CLI](../getting-started/cli.md#generating-a-report)), for water distribution models, but not the two modules directly. The GUI uses report blocks for its generated reports, while its Results tab computes its own dashboard rather than calling the two modules.

---

## Report blocks

The water distribution engine publishes fifteen blocks. Each is self-contained:
it carries its own heading and renders identically in every output format.

| Block id | Heading | Contents |
|---|---|---|
| `wds.run-summary` | Run Summary | Network size, reporting window, units, and quality mode |
| `wds.result-extremes` | Result Extremes | Global minimum and maximum pressure, head, demand, flow, and velocity |
| `wds.pump-energy` | Pump Energy | Per-pump utilization, efficiency, power, and cost, plus network totals |
| `wds.quality-summary` | Water Quality Summary | Quality mode and global quality extremes |
| `wds.quality-compliance` | Quality Compliance | Junction compliance against the residual (chemical) or age criterion, with the worst-performing junctions |
| `wds.service-compliance` | Pressure Adequacy | Junction-pressure compliance against a minimum (and optional maximum) |
| `wds.demand-reliability` | Demand Reliability | Delivered-vs-required volumes and the reliability ratio |
| `wds.pressure-distribution` | Pressure Distribution | Distribution of each junction's minimum pressure |
| `wds.velocity-distribution` | Velocity Distribution | Distribution of each pipe's maximum velocity |
| `wds.pressure-thresholds` | Pressure Thresholds | Junction minimum pressure counted into caller-supplied bands |
| `wds.velocity-thresholds` | Velocity Thresholds | Pipe maximum velocity counted into caller-supplied bands |
| `wds.tank-levels` | Tank Levels | Hydraulic head of each tank over the reporting horizon |
| `wds.mass-balance` | Mass Balance | Cumulative inflow and outflow with closure percentage |
| `wds.pipe-criticality` | Pipe Criticality | Pipes ranked by peak velocity |
| `wds.unit-headloss` | Unit Headloss | Pipes ranked by peak headloss per unit length: the undersized-main finder |

A block that does not apply to a run is **not** dropped: it renders as a
placeholder section under its normal heading, carrying the reason. Asking for
`wds.pump-energy` on a network with no pumps yields a Pump Energy section
reading `[not available: the network has no pumps]`. This is deliberate: a
requested section that silently vanished would be indistinguishable from one
that was never requested.

Some blocks accept options. The `*-thresholds` pair takes its band `edges`,
and several take a `worstCount` for their worst-performing tables.
`report_block_options(id, network)` describes what a given block accepts,
including labels, defaults, and bounds, so an interface can build an editor for
it without hardcoding the list.

### Templates

A template is the saved answer to "what goes in my report": a document title
plus an ordered list of block references. The GUI's template builder and the
CLI's `--template` flag read the same JSON.

```json
{
  "version": 1,
  "title": "Quarterly hydraulic report",
  "blocks": [
    { "id": "wds.run-summary" },
    { "id": "wds.pump-energy", "title": "Pumping cost" },
    { "id": "wds.pressure-thresholds", "options": { "edges": [20, 40, 60] } }
  ]
}
```

| Field | Required | Meaning |
|---|---|---|
| `version` | yes | Template format version. Must be `1`; any other value is rejected with a typed error |
| `title` | yes | Document title. Plain text, must not be empty |
| `blocks` | no | Ordered block references. Defaults to empty, which yields a document with no sections |
| `blocks[].id` | yes | A block id from the table above |
| `blocks[].title` | no | Heading override, replacing the block's default heading |
| `blocks[].options` | no | Per-block options, passed to the engine verbatim. This is how the `*-thresholds` blocks receive their band edges |

Unknown fields are ignored on read, so a template written by a newer Hydra still
loads. An id that is not in the catalog is **not** an error: it renders as a
placeholder section headed with the id itself, so a mistyped id is visible in
the output rather than silently dropped.

Omit `--template` entirely to get every block that applies to the run.

### Formats

The same document renders four ways, and the differences are presentational
rather than editorial: every format carries the same sections in the same
order.

| Format | Notes |
|---|---|
| `txt` | Fixed-width columns; reads correctly in a monospace viewer |
| `csv` | RFC 4180 quoting, full numeric precision |
| `html` | One self-contained file: inline CSS, no external resources, no scripts |
| `pdf` | A4, numbered `3 / 12` in the bottom margin, and a running header naming the document and what it was produced from on every page after the first |

Only the PDF is typeset, so only it paginates: a section heading is never left
as the last thing on a page, and a table continuing onto another page repeats
its column header. A long table still breaks where it falls: sections are not
forced onto fresh pages.

---

## Demand reliability (SDK)

Measures how well the network delivered the demand that was asked of it, per junction and network-wide.

Entry points (see [SDK Examples](../sdk/examples.md#demand-reliability-analysis)):

```rust
compute_demand_reliability_from_out(out_path, &network) -> DemandReliabilityReport
compute_demand_reliability_from_out_with_options(out_path, &network, options)
```

It needs both the `.out` file and the loaded `Network` (the network supplies the demand categories and patterns used to recompute *required* demand). Only **junction** nodes are analysed.

**What it computes**, per reporting period and junction:

- **Required** demand (from the model) and **delivered** demand (from the `.out` file), each accumulated into a volume.
- **Unmet** volume (`required − delivered`, clamped at zero) and **surplus** volume (`delivered − required`, non-zero only under PDA when pressure exceeds the required-pressure threshold).
- **Deficit periods**: the number of periods where the shortfall exceeded the deficit tolerance, plus the longest consecutive deficit streak and the maximum instantaneous deficit rate.

**Reliability ratio** = served ÷ required volume, in `[0, 1]` (reported per node and for the whole network).

**Options**: `DemandReliabilityOptions { deficit_tolerance }`. Shortfalls below this rate (m³/s) are not counted as deficit *periods*, though their volume still accumulates. Default `1e-9`.

**Units**: volumes in m³, rates in m³/s, times in seconds (internal SI). The report also records the model's demand model (DDA or PDA). Under DDA delivered ≈ required, so reliability is mainly meaningful under PDA or when checking a deficit scenario.

---

## Service compliance (SDK)

Measures how often junction pressures stayed within acceptable bounds.

Entry point (see [SDK Examples](../sdk/examples.md#pressure-compliance-analysis)):

```rust
compute_service_compliance_from_out(out_path, thresholds) -> ServiceComplianceReport
```

It reads only the `.out` file (junction membership comes from the file's node table). Only **junction** nodes are analysed: reservoirs and tanks are excluded so they don't register as permanent violations.

**Thresholds**: `ServiceComplianceThresholds { min_pressure, max_pressure }`. `min_pressure` is required; `max_pressure` is optional (`None` disables the upper-bound check). Use `ServiceComplianceThresholds::min_only(p)` for a lower bound only. When set, `max_pressure` must be strictly greater than `min_pressure`.

**What it computes**, per period and junction:

- Sample counts: within limits, below minimum, above maximum.
- **Deficit / excess integrals**: pressure shortfall or excess accumulated over time (m·s).
- **Worst** observed deficit and excess (m), and the longest consecutive violation streak.

The summary aggregates these across all junctions and periods and reports a **compliance ratio** (fraction of in-limit samples). Pressures use the units stored in the `.out` file (metres of head for Hydra output).

---

## The GUI Results tab

The **Results** tab computes its own dashboard from the scenario's results (a histogram/summary pass, independent of the two SDK modules above). It has six panels:

| Panel | Shows |
|---|---|
| **System Summary** | Metric chips: minimum pressure (and where), maximum velocity (and where), a pressure-compliance percentage, total pump energy, and mass-balance closure |
| **Histograms** | Distribution of per-junction minimum pressure and per-**pipe** maximum velocity. Pumps and valves are excluded from the velocity population: they have no pipe velocity, and counting them would bank a spurious zero each |
| **Pipe Criticality** | The top pipes ranked by peak velocity, with diameter and end nodes |
| **Audit Panels** | Mass-balance audit (cumulative inflow/outflow, closure, trend) and energy audit (pump energy, specific energy, peak power) |
| **Tank Levels** | Per-tank head over the simulation horizon |
| **Pump Energy** | Per-pump average power, with total energy and cost |

---

## Availability

| Surface | Reliability / compliance modules | Report blocks | GUI Results dashboard |
|---|---|---|---|
| SDK (`hydra-sdk`) | ✅ direct | ✅ | — |
| CLI | ✅ via report blocks | ✅ `hydra report` (water distribution) | — |
| GUI | ✅ via report blocks | ✅ | ✅ |
