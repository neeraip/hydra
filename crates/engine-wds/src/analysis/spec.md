# hydra-engine — Analysis Sub-Specification

This document is the analysis sub-specification for `hydra-engine`.

## 1. Overview

The analysis module owns post-simulation analytics that are more expensive than
interactive UI transformations. Its outputs are persisted as an analysis
artifact so interfaces can render rich summaries without heavy on-the-fly
compute.

`hydra-engine`'s analysis module does not run hydraulics or quality simulation. It consumes
completed simulation outputs and publishes derived statistics.

---

## 2. Design Goals

1. GUI and CLI consumers must be able to read analysis results directly from a
   persisted file with no expensive recomputation.
2. Analysis compute must be deterministic for a given simulation output.
3. The artifact must be versioned so schemas can evolve safely.

---

## 3. Artifact Contract

See `encode_analysis_artifact` in `analysis/artifact.rs` for the file schema,
location conventions, and stale-on-edit invalidation semantics.

---

## 4. Computation Ownership

`hydra-engine`'s analysis module owns:

1. Full-run histogram aggregation across time steps.
2. Percentile/quantile computations over simulation outputs.
3. Threshold exceedance counts and ratios.
4. Cross-variable summary statistics intended for dashboards.
5. Service-compliance analytics over node pressure time series.
6. Demand-delivery reliability analytics over junction demand time series.

On-demand analysis may be scoped to a caller-selected metric subset to reduce
compute time on large networks. The current selectable distribution modules are:

1. Pressure distribution
2. Head distribution
3. Flow distribution
4. Velocity distribution
5. Link-status distribution

If no selection is provided, implementations must compute all modules for
backward compatibility.

For persisted `.out` inputs, histogram/distribution analysis is computed using a
streaming two-pass scan over reporting periods:

1. Pass 1 computes exact min/max/mean inputs and status counts.
2. Pass 2 bins values into fixed histograms derived from pass-1 ranges.

To avoid unbounded memory growth on long-duration networks, implementations must
not require materializing all per-period values in memory for persisted-output
analysis. Percentiles may be estimated from histogram bins as long as the
method is deterministic for the same input artifact.

Status distribution values are aggregated over all link-time samples (all links
across all reported periods), not only a single period. For persisted `.out`
analysis, status codes are interpreted using EPANET `StatusType` values and
collapsed as follows:

1. `OPEN`: code 3
2. `ACTIVE`: codes 4 and 6
3. `CLOSED`: codes 0, 1, 2, and 7
4. `OTHER`: any unmapped code

Interface crates may still perform lightweight transformations on already
persisted arrays (for example, formatting labels), but they must not perform
bulk aggregation over all periods at render time.

`hydra-engine`'s analysis module performs analysis computation and byte-level
encode/decode against the analysis artifact types defined in `../model/spec.md` §4.4; it does not
define competing schemas.

Service-compliance and demand-reliability modules may be computed on demand and
returned directly to the caller without extending `analysis.json`. These
modules are still owned by `hydra-engine`'s analysis module and must remain deterministic for a
fixed input.

### 4.1 Service Compliance Module

Service compliance is computed over **junction nodes only**. Reservoirs and
tanks are fixed- or storage-grade nodes whose gauge pressure is not a
service-delivery metric — a reservoir sits at ≈ 0 gauge pressure in every
period, so including it would count a permanent violation and deflate the
network compliance ratio on every model. Junction membership is derived from
the persisted output's tank/reservoir node index list (model spec §4.5.2), so
no separate network load is required. Per-node results, node counts, and all
summary sample totals therefore cover junctions exclusively, matching the
demand-reliability module's junction-only scope (§4.2).

See `compute_service_compliance_from_out` in `analysis/service_compliance.rs`
for inputs, outputs, and the streaming-pass memory contract.

### 4.2 Demand Reliability Module

See `compute_demand_reliability_from_out` in `analysis/demand_reliability.rs`
for inputs, outputs, and the streaming-pass memory contract.

---

## 5. Invalidation

See `encode_analysis_artifact` for the stale-on-edit invalidation rule.

---

## 6. Runtime Estimation API

See `estimate_simulation_runtime` in `simulation/estimator.rs` for the cost
model and determinism guarantee. Inputs: node count, link count, period count,
and selected analysis modules. Output: `RuntimeEstimate` (`Low`/`Medium`/`High`).

---

## 7. Report Blocks

The analysis module implements the foundation layer's reportable-output
contract (hydra-common spec §3) for the water-distribution engine: a
statically-queryable catalog of blocks, and deterministic production of
neutral content fragments for one completed simulation.

### 7.1 Catalog (v1)

| Block id | Content | Available when |
|---|---|---|
| `wds.run-summary` | Network size (junction / tank-and-reservoir / link / pump counts), reporting window (start, step, final report time, period count), flow and pressure units, quality mode. | always |
| `wds.result-extremes` | Global minimum and maximum of nodal pressure, head, and demand, and of link flow and velocity — plus quality when present — over the reporting horizon. | the file holds ≥ 1 reporting period |
| `wds.pump-energy` | Per-pump table: utilization, average efficiency, average and peak power, average daily cost; plus the network demand charge. | the network has ≥ 1 pump |
| `wds.quality-summary` | Quality mode and global quality extremes with the mode's display unit. | the run produced water-quality results |
| `wds.service-compliance` | Junction-pressure service compliance (§4.1): compliance ratio, violation counts, deficit integral, a worst-junctions table, and a narrative note ("N junctions below the minimum pressure criterion of X"). | the file holds ≥ 1 reporting period |
| `wds.demand-reliability` | Delivered-vs-required demand reliability (§4.2): volumes, reliability ratio, deficit periods, and a worst-junctions table. | the file holds ≥ 1 reporting period |
| `wds.pressure-distribution` | Distribution of per-junction minimum pressure over the run: equal-width bins as a bar chart (counts per bin). | the file holds ≥ 1 reporting period |
| `wds.velocity-distribution` | Distribution of per-link maximum velocity over the run: equal-width bins as a bar chart (counts per bin). | the file holds ≥ 1 reporting period |
| `wds.tank-levels` | Hydraulic head of each tank over the reporting horizon as a line chart (one series per tank, first 8 in node order with a note when more exist). | the network has ≥ 1 tank |

### 7.1.1 Block options

Per the foundation contract (hydra-common spec §3.4) options are opaque
JSON authored per template block; this engine defines:

| Block | Option | Meaning | Default |
|---|---|---|---|
| `wds.service-compliance` | `minPressure` | Minimum acceptable junction pressure, in the results file's pressure display unit | 14 (SI files, m) / 20 (US files, psi) |
| `wds.service-compliance` | `maxPressure` | Optional maximum acceptable pressure, same unit | none |
| `wds.service-compliance` | `worstCount` | Rows in the worst-junctions table | 10 |
| `wds.demand-reliability` | `deficitTolerance` | Deficit flow-rate tolerance (m³/s) below which a per-period shortfall is not counted | 1e-9 |
| `wds.demand-reliability` | `worstCount` | Rows in the worst-junctions table | 10 |

Unknown option fields are ignored; malformed values (wrong type, negative
where a magnitude is required) fail production with the foundation
contract's `failed` error naming the field.

### 7.1.2 Distribution binning

Distribution blocks use **six equal-width bins** spanning the observed
value range, with edges rounded outward to whole display units (pressure:
the file's pressure unit over per-junction minima; velocity: m/s or ft/s
over per-link maxima). A degenerate range (all values equal) yields a
single bin. The bins are emitted as a bar chart (per the foundation
contract's chart item, table-derivable everywhere): category = bin
interval, value = element count. Junction-only for pressure (§4.1
rationale); all links for velocity.

`wds.tank-levels` emits a line chart: x = report time in hours, y = tank
hydraulic head in the file's length display unit, one series per tank
(ids from the network), capped at the first 8 tanks in node order with a
disclosure note when more exist.

Availability is this engine's internal concern — the foundation contract
carries no result-class vocabulary; an inapplicable block fails production
with the neutral "unavailable" error and a human-readable reason.

Block ids are stable per the foundation contract: removing or repurposing an
id is a compatibility break.

### 7.2 Production contract

**Inputs:** the persisted `.out` results file, the corresponding loaded
network, and the optional per-block options value (§7.1.1). Counts and result values come from the `.out` file
(result-authoritative); element identifiers and declared display units come
from the network. Production is read-only and deterministic: identical
inputs always yield identical fragments.

**Extremes sampling:** `wds.result-extremes` and the quality extremes reuse
the sampled range scan (§4-adjacent `scan_ranges`; at most 2048 sampled
periods including the first and last). When the file holds more periods than
the sample budget, the fragment carries a note disclosing sampling; below
the budget, the scan is exhaustive.

**Units:** unit labels are display text per the foundation contract. Flow
and demand carry the network's declared flow unit; pressure, head, and
velocity carry the unit-system-appropriate label (SI: m, m, m/s;
US customary: psi, ft, ft/s). Quality carries mg/L (chemical — the file
default), hours (age), or % (trace). Cost values carry no unit (currency
is not modelled).

**Errors:** unknown ids map to the foundation contract's unknown-block
error; inapplicable blocks (`wds.pump-energy` with zero pumps,
`wds.quality-summary` for a hydraulics-only file) map to its unavailable
error with a reason; read failures map to its failed error. The engine
never renders placeholders — that is the report layer's decision.
