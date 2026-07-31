# Post-Simulation Analytics

After a run, Hydra can derive higher-level metrics from the saved results. There are three surfaces:

- **Programmatic analytics** (SDK) — two on-demand modules, *demand reliability* and *service compliance*, that read a saved `.out` file and return a structured report.
- **Report blocks** (SDK) — a catalog of named, self-contained content blocks that render into txt/csv/html/PDF documents. Several of them are built *on* the two modules above, which is how those metrics reach a generated report. See [SDK Overview](../sdk/overview.md#reports).
- **The GUI Analysis tab** — an interactive dashboard computed separately from the same results.

The CLI exposes none of the three. The GUI uses report blocks for its generated reports, but its Analysis tab computes its own dashboard rather than calling the two modules directly.

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
- **Deficit periods** — the number of periods where the shortfall exceeded the deficit tolerance — plus the longest consecutive deficit streak and the maximum instantaneous deficit rate.

**Reliability ratio** = served ÷ required volume, in `[0, 1]` (reported per node and for the whole network).

**Options** — `DemandReliabilityOptions { deficit_tolerance }`: shortfalls below this rate (m³/s) are not counted as deficit *periods*, though their volume still accumulates. Default `1e-9`.

**Units**: volumes in m³, rates in m³/s, times in seconds (internal SI). The report also records the model's demand model (DDA or PDA) — under DDA delivered ≈ required, so reliability is mainly meaningful under PDA or when checking a deficit scenario.

---

## Service compliance (SDK)

Measures how often junction pressures stayed within acceptable bounds.

Entry point (see [SDK Examples](../sdk/examples.md#pressure-compliance-analysis)):

```rust
compute_service_compliance_from_out(out_path, thresholds) -> ServiceComplianceReport
```

It reads only the `.out` file (junction membership comes from the file's node table). Only **junction** nodes are analysed — reservoirs and tanks are excluded so they don't register as permanent violations.

**Thresholds** — `ServiceComplianceThresholds { min_pressure, max_pressure }`: `min_pressure` is required; `max_pressure` is optional (`None` disables the upper-bound check). Use `ServiceComplianceThresholds::min_only(p)` for a lower bound only. When set, `max_pressure` must be strictly greater than `min_pressure`.

**What it computes**, per period and junction:

- Sample counts: within limits, below minimum, above maximum.
- **Deficit / excess integrals** — pressure shortfall or excess accumulated over time (m·s).
- **Worst** observed deficit and excess (m), and the longest consecutive violation streak.

The summary aggregates these across all junctions and periods and reports a **compliance ratio** (fraction of in-limit samples). Pressures use the units stored in the `.out` file (metres of head for Hydra output).

---

## The GUI Analysis tab

The **Analysis** tab computes its own dashboard from the scenario's results (a histogram/summary pass, independent of the two SDK modules above). It has six panels:

| Panel | Shows |
|---|---|
| **System Summary** | Metric chips: minimum pressure (and where), maximum velocity (and where), a pressure-compliance percentage, total pump energy, and mass-balance closure |
| **Histograms** | Distribution of per-junction minimum pressure and per-**pipe** maximum velocity. Pumps and valves are excluded from the velocity population — they have no pipe velocity, and counting them would bank a spurious zero each |
| **Pipe Criticality** | The top pipes ranked by peak velocity, with diameter and end nodes |
| **Audit Panels** | Mass-balance audit (cumulative inflow/outflow, closure, trend) and energy audit (pump energy, specific energy, peak power) |
| **Tank Levels** | Per-tank head over the simulation horizon |
| **Pump Energy** | Per-pump average power, with total energy and cost |

---

## Availability

| Surface | Reliability / compliance modules | Report blocks | GUI Analysis dashboard |
|---|---|---|---|
| SDK (`hydra-sdk`) | ✅ direct | ✅ | — |
| CLI | ❌ | ❌ | — |
| GUI | ✅ via report blocks | ✅ | ✅ |
