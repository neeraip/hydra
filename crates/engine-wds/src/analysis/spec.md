# hydra-engine — Analysis Sub-Specification

This document is the analysis sub-specification for `hydra-engine`.

## 1. Overview

The analysis module owns post-simulation analytics that are more expensive than
interactive UI transformations. Its outputs are persisted as an analysis
artifact so interfaces can render rich summaries without heavy on-the-fly
compute.

`hydra-engine`'s analysis module does not run hydraulics or quality simulation. It consumes
completed simulation outputs and publishes derived statistics.

Unlike every other sub-specification in this engine, **this module has no predecessor
lineage**. The data model, solver, and file formats exist to be compatible with an
established engine; post-simulation analysis is original to Hydra. There is consequently
no external behaviour to match and no compatibility commandment to honour — the only
compatibility surface is the artifact's own versioned schema (§3), and every rule below
is a Hydra design decision rather than a reproduction of prior art.

---

## 2. Design Goals

1. An interface layer must be able to read analysis results directly from a
   persisted file with no expensive recomputation. The graphical interface is the
   intended consumer; the command-line interface does not compute or read analysis.
2. Analysis compute must be deterministic for a given simulation output.
3. The artifact must be versioned so schemas can evolve safely.

---

## 3. Artifact Contract

The artifact is a single JSON document written alongside the results file it
describes, conventionally named `analysis.json` next to `results.out` for the same
run. It is **read-only to consumers**: an interface renders from it and must not
recompute analytics at render time.

**Schema.** The document carries a `schema_version` integer, currently **1**. A
reader must treat a document whose version it does not recognise as unreadable
rather than attempting a partial parse. The top level holds four members:

| Member | Content |
|---|---|
| `source` | Provenance: the results file name, the report file name, and the number of reporting periods analysed |
| `distributions` | The five distribution modules of §4 — pressure, head, flow, velocity (each a continuous distribution) and link status |
| `demand_reliability` | The §4.2 summary, or absent when not computed |
| `service_compliance` | The §4.1 summary, or absent when not computed |

A **continuous distribution** is a list of histogram bins, a summary block, and an
optional threshold breakdown. Each bin carries its half-open interval `[start, end)`
and a sample count. The summary carries minimum, maximum, and mean. The threshold
breakdown, when present, counts samples below, within, and above the configured
limits. The **status distribution** instead counts link-period samples as open,
closed, active, or other.

Two properties are required of the encoding: it must be **canonical**, so that the
same artifact value always produces the same bytes, and it must **round-trip** — a
decode of an encoded artifact reproduces the original value exactly.

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

**Inputs.** The persisted results file, the corresponding loaded network, and a
pressure threshold pair: a required minimum, and an optional maximum. Omitting the
maximum disables the upper-bound test entirely, and every above-maximum count is then
zero by construction. Thresholds are expressed in the results file's pressure unit.

**Sampling.** One sample per (junction, reporting period). Periods are read in a
single streaming pass; no per-period array is ever held for the whole run. Writing
$p$ for a sample's pressure, $p_{\min}$ and $p_{\max}$ for the thresholds, and
$\Delta t_{\text{rep}}$ for the reporting-period duration in seconds, each sample is
classified as **below minimum** ($p < p_{\min}$), **above maximum**
($p > p_{\max}$, only when a maximum is configured), or **within limits** otherwise.

**Per-junction metrics.** For each junction the module accumulates the sample count,
the within-limit count, the below-minimum and above-maximum counts, and the longest
run of *consecutive* out-of-limit samples. It also accumulates two time integrals and
two extrema:

$$I_{\text{deficit}} = \sum \max(p_{\min} - p,\ 0)\,\Delta t_{\text{rep}}, \qquad
  I_{\text{excess}} = \sum \max(p - p_{\max},\ 0)\,\Delta t_{\text{rep}}$$

both in metre-seconds, together with the worst single deficit $\max(p_{\min} - p)$ and
the worst single excess $\max(p - p_{\max})$ in metres. A junction's **violation
ratio** is its out-of-limit sample count over its sample count, and is defined as $0$
when it has no samples.

**Network summary.** Sample counts and both integrals sum across junctions; the two
worst-case extrema and the maximum per-junction violation ratio take the maximum. The
network **compliance ratio** is the within-limit sample count over the total sample
count, and is defined as $1$ when there are no samples — an empty analysis is
vacuously compliant rather than undefined.

**Scope.** Junctions only, for the reason given above. Because reservoirs and tanks
are skipped, per-node results are keyed by node index and those indices are **not
contiguous**.

### 4.2 Demand Reliability Module

**Inputs.** The persisted results file, the corresponding loaded network, and an
optional deficit tolerance. Delivered demand is read per period from the results file;
**required** demand is recomputed from the network's demand categories and their
patterns at each reporting time, since the results file records what was delivered
rather than what was asked for. Periods are read in a single streaming pass.

**Deficit tolerance.** A per-period shortfall whose *rate* falls below the tolerance
(default $10^{-9}$ m³/s) does not count as a deficit period. This exists solely to stop
floating-point noise registering as service failure. It gates the period *counts* only
— volumes still accumulate sub-threshold shortfalls, so a run of negligible shortfalls
contributes volume without inflating the deficit-period count.

**Per-junction metrics.** Writing $d_{\text{req}}$ and $d_{\text{del}}$ for the
required and delivered rates in a period, the module accumulates required, delivered,
unmet, and surplus volumes over the run, where unmet accrues $\max(d_{\text{req}} -
d_{\text{del}},\ 0)$ and surplus accrues $\max(d_{\text{del}} - d_{\text{req}},\ 0)$,
each multiplied by the reporting-period duration. Surplus is non-zero only under a
pressure-dependent demand model, where delivery may exceed the requested rate once head
passes the required-pressure threshold. It also records the deficit-period count, the
longest run of *consecutive* deficit periods, and the maximum instantaneous deficit rate.

**Served volume and reliability.** Served volume is $\max(V_{\text{req}} -
V_{\text{unmet}},\ 0)$ and the reliability ratio is the served volume over the required
volume, defined as $1$ when required volume is zero or negative. The clamp and the
zero-demand case both exist so the ratio is total on $[0, 1]$ for every network,
including one with no demand at all.

**Network summary.** Volumes and deficit-period counts sum across junctions; the
maximum deficit rate takes the maximum. The network reliability ratio is computed from
the *summed* volumes rather than by averaging per-junction ratios, so it is
demand-weighted: a large junction's shortfall outweighs a small one's.

**Scope.** Junctions only, matching §4.1. The summary also records which demand model
the simulation ran under, since reliability is interpreted differently between them —
under a demand-driven model an unmet volume indicates a modelling inconsistency, while
under a pressure-dependent model it is the physical result being sought.

---

## 5. Invalidation

An artifact describes exactly one results file produced from exactly one model. It
carries no checksum or timestamp of its inputs, so it cannot detect its own staleness:
validity is a **lifecycle obligation on the producer**, not a property a reader can
verify.

The rule is therefore unconditional. When the results file changes, or when the model
that produced it changes, the corresponding artifact is **stale and must be deleted**
before a new one is produced. Deleting is required rather than overwriting, so that a
failure between invalidation and recomputation leaves no artifact at all rather than
one that silently describes a superseded run. A consumer finding no artifact renders
nothing; a consumer finding one is entitled to trust it completely.

The `source` block (§3) records which results file the artifact describes and how many
periods it covers, which is sufficient to detect a *mismatched* pairing but not an
*outdated* one — a results file rewritten with the same name and period count is
indistinguishable to a reader. This is why the obligation rests on the producer.

---

## 6. Runtime Estimation API

The module publishes an advisory estimate of how long an **analysis** will take,
so an interface can warn before starting an expensive one. This is distinct from
the *simulation* runtime estimate, which models a different cost and is specified
in the [simulation spec](../simulation/spec.md) §11. The estimate never
influences what is computed, and is deterministic for identical inputs.

**Inputs** are summary metadata plus the module selection of §4 — node count,
link count, reporting-period count, and which distribution modules were
requested — so an estimate can be produced without reading the results file.

**Cost model.** Analysis cost is dominated by the per-period scan, so the
estimate is a **complexity score** rather than a predicted duration:

$$C = N_p \cdot \Bigl(N\,m_{\text{node}} + L\,(m_{\text{link}} + w_s)\Bigr) \cdot f_{\text{pass}}$$

where $N_p$ is the reporting-period count, $N$ and $L$ the node and link counts
(each floored at 1), $m_{\text{node}}$ the number of selected node modules
(pressure, head) and $m_{\text{link}}$ the number of selected link modules (flow,
velocity). The status module contributes a fractional weight $w_s$ rather than a
whole one, being a counting pass rather than a binning pass. The factor
$f_{\text{pass}}$ accounts for the **two-pass** structure of §4: when any
continuous distribution is selected the file is scanned twice — once for ranges,
once to bin — so the score is scaled accordingly; a selection of only the status
module needs a single pass and is not scaled.

Selecting nothing, or a results file with no reporting periods, short-circuits
to the lowest category without evaluating the model.

The **shape** of this model is normative — the score must rise with period count,
network size, and the number of selected modules, so a larger analysis never
receives a lower estimate than a smaller one. The particular weights and the
category thresholds are fitted and may be re-tuned without a specification
change.

**Output** is the same three-valued ordinal the simulation estimator returns —
`Low`, `Medium`, or `High` — obtained by comparing the score against two
thresholds. Deliberately coarse: the score is a proxy for work, not a
measurement of time.

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
| `wds.velocity-distribution` | Distribution of per-**pipe** maximum velocity over the run: equal-width bins as a bar chart (counts per bin). Pumps and valves are excluded (§7.1.2). | the file holds ≥ 1 reporting period |
| `wds.tank-levels` | Hydraulic head of each tank over the reporting horizon as a line chart (one series per tank, first 8 in node order with a note when more exist). | the network has ≥ 1 tank |
| `wds.mass-balance` | Network volumetric balance: cumulative inflow and outflow, their difference and closure percentage, plus per-period closure as a line chart. | the file holds ≥ 1 reporting period |
| `wds.pipe-criticality` | Pipes ranked by peak velocity: a table of the top *N* with peak velocity and the period it occurred in. | the network has ≥ 1 pipe |
| `wds.pressure-thresholds` | Junction minimum pressure counted into **caller-supplied threshold bands** rather than observed-range bins (§7.1.2), as a bar chart. | the file holds ≥ 1 reporting period |
| `wds.velocity-thresholds` | Pipe maximum velocity counted into caller-supplied threshold bands, as a bar chart. | the file holds ≥ 1 reporting period |

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
| `wds.pipe-criticality` | `topCount` | Rows in the ranked-pipes table | 5 |
| `wds.pressure-thresholds` | `edges` | Ascending band boundaries in the results file's pressure display unit | `[0, 10, 20, 30, 40, 50, 60]` (SI, m) |
| `wds.velocity-thresholds` | `edges` | Ascending band boundaries in the velocity display unit | `[0.1, 0.3, 0.6, 1.0]` (SI, m/s) |

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
interval, value = element count. Both populations are restricted, and for the
same reason: an element that cannot exhibit the quantity would otherwise
be counted as a zero. Pressure covers **junctions only** (the §4.1
rationale — a reservoir sits at ≈ 0 gauge pressure every period).
Velocity covers **pipes only**: a pump or valve has no pipe velocity, so
including them banks one spurious zero per non-pipe link into the lowest
bin, which on a pump-heavy network can dominate the chart.

#### Threshold bands

The two `*-thresholds` blocks bin against **caller-supplied boundaries** instead
of the observed range, because they answer a different question. An
observed-range distribution shows the *shape* of a population; a threshold
distribution shows how much of it sits either side of a criterion that carries
engineering meaning — pressures below zero are junctions in deficit, velocities
above the design limit are pipes running fast. Rebinning those to the observed
range would erase exactly the reading being sought, so the boundaries are an
input rather than a derivation.

Given $n$ ascending edges $e_1 < e_2 < \dots < e_n$, an element is counted into
one of $n+1$ bands: below $e_1$, each half-open $[e_i, e_{i+1})$, and at or above
$e_n$. **The outer two bands are unbounded**, so no element is ever dropped —
a junction at $-5$ m is counted, not discarded, which is the property the
observed-range form cannot offer when the caller has fixed the edges. Band counts
therefore sum to the population size.

Edges are supplied and interpreted in the results file's display unit for the
quantity. Populations match §7.1.2 — junctions for pressure, pipes for velocity.
Edges must be strictly ascending and at least one must be given; otherwise
production fails with the foundation contract's `failed` error naming the field.

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
