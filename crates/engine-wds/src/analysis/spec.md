# Water Distribution — Analysis Sub-Specification

This document is the analysis sub-specification of the water-distribution
engine.

## 1. Overview

The analysis module owns post-simulation analytics that are more expensive than
interactive UI transformations. Its results are published through the foundation
layer's reportable-output contract as **report blocks** (§4), produced on demand
from a completed run.

The analysis module does not run hydraulics or quality simulation. It consumes
completed simulation outputs and publishes derived statistics.

Analysis owns no persisted format of its own. Derived results are not written to a
side-car file: they are computed from the results file when a consumer asks for them.
Whether a consumer keeps what it received is that consumer's decision, and a cache it
may invalidate on its own terms is strictly cheaper to hold correct than a schema this
layer would have to version, publish, and prove fresh on every read.

Unlike every other sub-specification in this engine, **this module has no predecessor
lineage**. The data model, solver, and file formats exist to be compatible with an
established engine; post-simulation analysis is original to Hydra. There is consequently
no external behaviour to match and no compatibility commandment to honour — the only
compatibility surface is the block catalog's identifiers (§4.1), and every rule below
is a Hydra design decision rather than a reproduction of prior art.

---

## 2. Design Goals

1. Derived results reach an interface through **one** contract — the foundation
   layer's reportable-output contract (foundation contract §3). A second, analysis-owned
   delivery path would have to be re-specified for every future engine, while the
   block catalog is already engine-neutral at the contract level.
2. Analysis compute must be deterministic for a given simulation output.
3. Analysis must be computable by streaming the results file, so that cost scales
   with the reporting horizon rather than with available memory.

---

## 3. Computation Ownership

The analysis module owns:

1. Full-run aggregation across reporting periods — global extremes, and per-element
   minima and maxima.
2. Distribution binning over an observed value range.
3. Threshold exceedance counts and band distributions.
4. Service-compliance analytics over junction pressure time series (§3.1).
5. Demand-delivery reliability analytics over junction demand time series (§3.2).
6. Volumetric mass-balance closure over the reporting horizon.

**Streaming.** For persisted `.out` inputs, every analysis above is computed by
scanning reporting periods one at a time. Implementations must not require
materialising all per-period values in memory: cost is bounded by one period's
data and by the per-element accumulators, never by the reporting horizon. A
network simulated for a year must analyse in the same memory as one simulated for
a day.

**Aggregation belongs here, not at the interface.** Interface crates may perform
lightweight transformations on values already returned to them — formatting
labels, sorting a table, filtering a population — but must not perform bulk
aggregation over reporting periods at render time. The cost of a scan is the
reason this module exists.

**Caching is a consumer's concern.** Analysis is a pure function of the results
file, so a consumer may cache what it receives for as long as that file is
unchanged. This layer neither performs nor specifies that caching, and publishes
no format for it: an in-memory cache keyed on file identity is invalidated by
construction, whereas a persisted one must additionally be proven fresh on every
read against a file it cannot see change.

### 3.1 Service Compliance Module

Service compliance is computed over **junction nodes only**. Reservoirs and
tanks are fixed- or storage-grade nodes whose gauge pressure is not a
service-delivery metric — a reservoir sits at ≈ 0 gauge pressure in every
period, so including it would count a permanent violation and deflate the
network compliance ratio on every model. Junction membership is derived from
the persisted output's tank/reservoir node index list (model spec §4.4.2), so
no separate network load is required. Per-node results, node counts, and all
summary sample totals therefore cover junctions exclusively, matching the
demand-reliability module's junction-only scope (§3.2).

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

### 3.2 Demand Reliability Module

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

**Scope.** Junctions only, matching §3.1. The summary also records which demand model
the simulation ran under, since reliability is interpreted differently between them —
under a demand-driven model an unmet volume indicates a modelling inconsistency, while
under a pressure-dependent model it is the physical result being sought.

---

## 4. Report Blocks

The analysis module implements the foundation layer's reportable-output
contract (foundation contract §3) for the water-distribution engine: a
statically-queryable catalog of blocks, and deterministic production of
neutral content fragments for one completed simulation.

### 4.1 Catalog (v1)

| Block id | Content | Available when |
|---|---|---|
| `wds.run-summary` | Network size counted **by element kind** — junctions, reservoirs, tanks, pipes, pumps, valves — reporting window (start, step, final report time, period count), flow and pressure units, quality mode. | always |
| `wds.result-extremes` | Global minimum and maximum of nodal pressure, head, and demand, and of link flow and velocity — plus quality when present — over the reporting horizon. | the file holds ≥ 1 reporting period |
| `wds.pump-energy` | Per-pump table: utilization, average efficiency, average and peak power, average daily cost; plus the network demand charge. | the network has ≥ 1 pump |
| `wds.quality-summary` | Quality mode and global quality extremes with the mode's display unit. | the run produced water-quality results |
| `wds.service-compliance` | Junction-pressure service compliance (§3.1): compliance ratio, violation counts, deficit integral, a worst-junctions table, and a narrative note ("N junctions below the minimum pressure criterion of X"). | the file holds ≥ 1 reporting period |
| `wds.demand-reliability` | Delivered-vs-required demand reliability (§3.2): volumes, reliability ratio, deficit periods, and a worst-junctions table. | the file holds ≥ 1 reporting period |
| `wds.pressure-distribution` | Distribution of per-junction minimum pressure over the run: equal-width bins as a bar chart (counts per bin). | the file holds ≥ 1 reporting period |
| `wds.velocity-distribution` | Distribution of per-**pipe** maximum velocity over the run: equal-width bins as a bar chart (counts per bin). Pumps and valves are excluded (§4.1.2). | the file holds ≥ 1 reporting period |
| `wds.tank-levels` | Hydraulic head of each tank over the reporting horizon as a line chart (one series per tank, first 8 in node order with a note when more exist). | the network has ≥ 1 tank |
| `wds.mass-balance` | Network volumetric balance: cumulative inflow and outflow, their difference and closure percentage, plus per-period closure as a line chart. | the file holds ≥ 1 reporting period |
| `wds.pipe-criticality` | Pipes ranked by peak velocity: a table of the top *N* with peak velocity and the period it occurred in. | the network has ≥ 1 pipe |
| `wds.unit-headloss` | Pipes ranked by peak unit headloss: a table of the top *N* with peak unit headloss and pipe diameter — the undersized-main finder. Unit headloss is the length-normalised ratio the results file stores for pipes, numerically identical in both display systems (m/km ≡ ft/kft). | the network has ≥ 1 pipe |
| `wds.quality-compliance` | Junction water-quality compliance: in chemical mode each junction's **minimum residual** over the horizon is judged against `minResidual`; in age mode each junction's **maximum age** is judged against `maxAge`. Compliant/total counts, the compliance ratio, and a worst-junctions table (ranked by the judged value, worst first). Trace runs have no compliance criterion and are unavailable, distinctly from no-quality runs. | the run produced chemical or age quality results |
| `wds.pressure-thresholds` | Junction minimum pressure counted into **caller-supplied threshold bands** rather than observed-range bins (§4.1.2), as a bar chart. | the file holds ≥ 1 reporting period |
| `wds.velocity-thresholds` | Pipe maximum velocity counted into caller-supplied threshold bands, as a bar chart. | the file holds ≥ 1 reporting period |

### 4.1.1 Block options

Per the foundation contract (§3.4) options are opaque
JSON authored per template block; this engine defines:

| Block | Option | Meaning | Default |
|---|---|---|---|
| `wds.service-compliance` | `minPressure` | Minimum acceptable junction pressure, in the results file's pressure display unit | 14 (SI files, m) / 20 (US files, psi) |
| `wds.service-compliance` | `maxPressure` | Optional maximum acceptable pressure, same unit | none |
| `wds.service-compliance` | `worstCount` | Rows in the worst-junctions table | 10 |
| `wds.demand-reliability` | `deficitTolerance` | Deficit flow-rate tolerance (m³/s) below which a per-period shortfall is not counted | 1e-9 |
| `wds.demand-reliability` | `worstCount` | Rows in the worst-junctions table | 10 |
| `wds.pipe-criticality` | `topCount` | Rows in the ranked-pipes table | 5 |
| `wds.unit-headloss` | `topCount` | Rows in the ranked-pipes table | 10 |
| `wds.quality-compliance` | `minResidual` | Minimum acceptable residual (chemical mode), in the file's quality display unit (mg/L) | 0.2 |
| `wds.quality-compliance` | `maxAge` | Maximum acceptable water age (age mode), hours | 24 |
| `wds.quality-compliance` | `worstCount` | Rows in the worst-junctions table | 10 |
| `wds.pressure-thresholds` | `edges` | Ascending band boundaries in the results file's pressure display unit | `[0, 10, 20, 30, 40, 50, 60]` (SI, m) / `[0, 15, 30, 45, 60, 75, 85]` (US, psi) |
| `wds.velocity-thresholds` | `edges` | Ascending band boundaries in the velocity display unit | `[0.1, 0.3, 0.6, 1.0]` (SI, m/s) / `[0.3, 1.0, 2.0, 3.3]` (US, ft/s) |

Unknown option fields are ignored; malformed values (wrong type, negative
where a magnitude is required) fail production with the foundation
contract's `failed` error naming the field.

**Descriptions.** This engine implements the foundation contract's option
description (foundation contract §3.2.1) for the options above, resolved against
a loaded network — with one deliberate omission. `deficitTolerance` is accepted
but never described: it is a floating-point noise floor rather than an
engineering criterion, so its default is imperceptible in any unit
($10^{-9}$ m³/s is $10^{-6}$ L/s), and a control nobody can choose a value for
is worse than no control. It remains settable from a template file. The four unit-dependent options — both pressure criteria and
both `edges` lists — are described with the default and unit label matching that
network's declared unit system, so a template-builder UI offering them never
converts a unit or chooses between the two columns above. Blocks absent from the
table describe no options.

A description is advisory and never the validation authority: production
validates the options value it is given whether or not it was described, so a
hand-authored template behaves identically to one built from a description.

### 4.1.2 Distribution binning

Distribution blocks use **six equal-width bins** spanning the observed
value range, with edges rounded outward to whole display units (pressure:
the file's pressure unit over per-junction minima; velocity: m/s or ft/s
over per-link maxima). A degenerate range (all values equal) yields a
single bin. The bins are emitted as a bar chart (per the foundation
contract's chart item, table-derivable everywhere): category = bin
interval, value = element count. Both populations are restricted, and for the
same reason: an element that cannot exhibit the quantity would otherwise
be counted as a zero. Pressure covers **junctions only** (the §3.1
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
quantity. Populations match §4.1.2 — junctions for pressure, pipes for velocity.
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

### 4.2 Production contract

**Inputs:** the persisted `.out` results file, the corresponding loaded
network, and the optional per-block options value (§4.1.1). Result values and
the reporting window come from the `.out` file (result-authoritative);
element identifiers, declared display units, and counts **by element kind**
come from the network.

Counts are taken from the network rather than the results file because the
results file cannot express them: its node grouping stores reservoirs inside
the tank group, and it records no link-type breakdown at all. Reporting a
combined "tanks and reservoirs" figure would describe the file's storage
layout rather than the network — and the two are not interchangeable, a
reservoir being an infinite-source boundary and a tank being finite storage
on a mass balance. The engine's own run-log report separates all six, so
collapsing them here would also make one engine disagree with itself. Production is read-only and deterministic: identical
inputs always yield identical fragments.

**Extremes sampling:** `wds.result-extremes` and the quality extremes reuse
the sampled range scan (§3-adjacent `scan_ranges`; at most 2048 sampled
periods including the first and last). When the file holds more periods than
the sample budget, the fragment carries a note disclosing sampling; below
the budget, the scan is exhaustive.

**Units (revised with foundation contract v1.7):** quantity-bearing numbers are
**quantity-tagged** (foundation contract §3.3): produced in the referenced
quantity's SI display unit, wearing its SI label, and re-expressed in a
display family by whichever consumer presents them. The results file
carries the model's declared display units, so production converts:

- **Linear quantities** — pressure, head, velocity, volume — convert
  through the engine's own §5 quantity descriptor (its US→SI inverse).
  The descriptor is the presentation authority, and inverting it at
  production makes US-family re-display reproduce the file's value
  exactly. This deliberately ignores specific gravity: the descriptor's
  m↔psi relation is the water mapping, and display conversion is
  presentation, not hydraulics — the solver's own sg-aware conversions
  are untouched. Tank-level series and the demand-reliability and
  mass-balance volumes (already computed in m³) tag the same way.
- **Flow and demand** convert by the declared spelling's §3.1 factor to
  L/s, the flow quantity's SI display unit. Eleven declared spellings map
  onto one SI display unit, so a US-family reader sees the canonical US
  label (gpm) rather than the file's spelling; the run summary still
  records the declared spelling as text.
- **Option echoes** (the compliance criteria) are tagged too: an option
  arrives in file display units (§4.1.1, unchanged) and its echo converts
  like any measured value, so one block never mixes families.

**What stays file-flavored, deliberately:** engine-authored *text* that
embeds numbers — threshold and distribution band labels, narrative notes,
composite units (the pressure-deficit integral's `unit·h`) — and the
untagged quantities whose units are identical in both families (percent,
kW, mg/L, hours, counts, cost). Tags reach numbers, never prose.

Quality carries mg/L (chemical — the file default), hours (age), or %
(trace), untagged. Cost values carry no unit (currency is not modelled).

**Errors:** unknown ids map to the foundation contract's unknown-block
error; inapplicable blocks (`wds.pump-energy` with zero pumps,
`wds.quality-summary` for a hydraulics-only file) map to its unavailable
error with a reason; read failures map to its failed error. The engine
never renders placeholders — that is the report layer's decision.

## 5. Criteria

The engine publishes a criteria catalog under the foundation criteria
contract (foundation contract §7) — the standard a water-distribution network is
assessed against:

| Key | Kind | Quantity | Defaults (SI display units) |
|---|---|---|---|
| `minPressure` | value | pressure | 14 m (the conventional ~20 psi service minimum) |
| `pressure` | band `low`/`required`/`high` | pressure | 24 / 35 / 45 m |
| `velocity` | band `low`/`target`/`high` | velocity | 0.1 / 0.5 / 1.5 m/s |
| `minResidual` | value | concentration | 0.2 mg/L (a conventional disinfectant-residual floor) |
| `maxAge` | value | age | 24 h |

The quality criteria brought two quantities into the engine's §5 catalog —
`concentration` (mg/L) and `age` (h) — both identical in the two display
systems, so their conversions are the identity.

**Consumption (foundation contract §7.4).** A valuation derives options for the
criteria-shaped blocks: `minPressure` becomes `wds.service-compliance`'s
`minPressure` option; the `pressure` and `velocity` bands become the
`edges` of `wds.pressure-thresholds` and `wds.velocity-thresholds`; and
`minResidual` and `maxAge` become `wds.quality-compliance`'s options of
the same names — identity conversions, since their quantities read the
same in both display systems, and both are always sent because which one
applies is the run's quality mode, which the block judges. Block
options are file-display-unit inputs (§4.1.1), so consumption converts
each SI value with the engine's own unit-conversion factors — the model's
specific gravity included, because an option participates in computation
against file values and is not presentation. A band that is not strictly
ascending after conversion cannot make threshold edges; its block is
omitted from the answer and runs on its documented defaults — a
degenerate band is an editing state, not an error (foundation contract §7.3).
Absent keys take the catalog defaults; a malformed valuation (wrong
shape, non-finite number) is refused with a message naming the
criterion.

A `flow` band was cataloged once, for the map's threshold colour scale
and for nothing else. It is retired: flow's ramp is **diverging**
(foundation contract §6.1) because the sign carries direction, so it can never
resolve to a criterion under §6.1's banded rule, and the scale it existed
for cannot be offered for it. A valuation saved while it existed still
parses — §7.3 ignores keys the catalog does not declare.

Every criterion that remains either drives a block or bands a variable,
which is the property worth keeping: a criterion the user can set and
nothing consumes is a control that does nothing.
