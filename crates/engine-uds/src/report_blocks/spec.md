# hydra-engine-uds — Analysis Specification

This document owns §13 of the urban drainage specification (§1.6):
post-simulation analytics, published as report blocks under the
foundation reportable-output contract.

---

## 13. Analysis

### 13.1 Stance and Provenance

The engine describes and produces **report blocks**: self-contained units
of reportable content derived from a completed simulation, under the
foundation contract's catalog, fragment, and production rules. Each block
is produced from exactly two inputs — a persisted results file (§14.9)
and the model that produced it — and production is read-only and
deterministic: producing a block computes over what the file already
stores and never re-simulates.

This provenance bounds what a block may claim. The results file holds
period records at the reporting resolution, so every figure a block
reports is a **reporting-resolution figure over the reporting horizon**.
The engine's own per-step accounting (§11) is not reconstructible from
period records; no block reports a §11 quantity, and no block calls any
figure a "continuity error". Where a block reports a residual (§13.4.2),
it is defined and labelled as its own reporting-resolution quantity.

Quantity-bearing values are tagged with the engine's quantity catalog
keys and expressed in each quantity's SI display unit, per the foundation
contract's fragment rules; the results file stores the model's declared
units, so production converts. Values with no catalog quantity (counts,
ratios, clock values) are untagged.

Blocks report **reported elements only**: an element excluded from the
results file by the report selection (§14.9) does not appear, and a block
whose subject has no reported elements refuses production with a reason
rather than producing an empty shell.

### 13.2 Catalog

The catalog is static and model-free. Each block carries an engine-authored
category; blocks sharing a category are adjacent in the catalog:

| Id | Category | Content |
|---|---|---|
| `uds.run-summary` | Summary | Reporting horizon, reported element counts, and system-wide peak rates. |
| `uds.system-balance` | Summary | Whole-network inflow, outflow, flooding, and storage volumes over the reporting horizon, with the inflow/outflow time series. |
| `uds.subcatchment-peaks` | Hydrology | Subcatchments ranked by peak runoff, with peak rainfall and infiltration rates. |
| `uds.runoff-summary` | Hydrology | Per-subcatchment precipitation and infiltration depths, runoff volume, and runoff coefficient. |
| `uds.node-extremes` | Network | Nodes ranked by maximum depth, with peak inflow and flooding. |
| `uds.link-extremes` | Network | Links ranked by peak flow, with peak velocity and capacity used. |
| `uds.flooding-summary` | Network | Every node that floods: peak overflow rate, periods flooded, first occurrence. |
| `uds.outfall-summary` | Network | Per-outfall discharge frequency, rates, and total volume. |
| `uds.surcharge-summary` | Network | Nodes that come within the freeboard of their rim: maximum depth, minimum clearance, time above. |
| `uds.capacity-summary` | Network | Conduits that reach the capacity threshold: maximum capacity fraction and time at or above. |
| `uds.velocity-thresholds` | Network | Conduits counted into self-cleansing/erosive velocity bands by peak velocity. |
| `uds.storage-summary` | Assets | Per storage node: depth utilisation, mean volume, peak inflow vs peak outflow, and attenuation. |

### 13.3 Common Derivations

**Integration.** A rate series sampled at the reporting instants
integrates by the rectangle rule at the report step $\Delta t$:

$$V = \sum_{p} q_p \, \Delta t$$

where $q_p$ is the stored rate at period $p$. This is the definition of
every "volume" and "depth" total in this section; its error against the
engine's per-step accumulation is a consequence of reporting resolution
and is accepted, per §13.1.

**Depths from intensity.** Precipitation and infiltration are stored as
intensities; their totals integrate to depths by the same rule, with
$\Delta t$ in the intensity's time base.

**Discharge counting.** A period counts as discharging when its stored
rate is greater than zero.

### 13.4 The Blocks

#### 13.4.1 Run summary

Key figures of the run: the reporting clock (period count, step, span),
reported element and pollutant counts, and the peaks of the system
rainfall, runoff, total lateral inflow, flooding, and outflow series.

#### 13.4.2 System balance

Totals over the reporting horizon, each integrated from a system series
(§14.9): the five inflow components — runoff, dry-weather, groundwater,
RDII, and external inflow — their sum as total inflow, and on the other
side outflow and flooding. Storage change is the last stored system
storage volume minus the first. An inflow component that is zero at every
period is omitted; total inflow, outflow, flooding, and storage change
always appear.

The block reports a **residual**:

$$R = V_{\text{in}} - V_{\text{out}} - V_{\text{flood}} - \Delta S$$

labelled as the reporting-resolution remainder. It absorbs both
integration error and processes the system series do not itemise
(routing evaporation and seepage); it is not the §11 ledger error and is
not presented as one.

The block carries a line chart of the total lateral inflow, outflow,
and — when any period floods — flooding series, at reporting resolution,
in the flow quantity, over elapsed hours.

Unavailable when the file stores no periods.

#### 13.4.3 Runoff summary

One row per reported subcatchment: total precipitation depth, total
infiltration depth, runoff volume, and the **runoff coefficient**

$$C = \frac{V_{\text{runoff}}}{d_{\text{precip}} \cdot A}$$

with $A$ the subcatchment's area from the model. The coefficient is
absent when the subcatchment received no precipitation, and absent when
the model no longer contains the reported subcatchment. Rows are ranked
by runoff volume, largest first, ties broken by identifier; the table
length is the `rows` option (§13.5).

Unavailable when the run reports no subcatchments.

#### 13.4.4 Subcatchment peaks, node extremes, link extremes

Ranked maxima tables, one row per element, length per the `rows` option:
subcatchments by peak runoff (with peak rainfall and infiltration
intensities), nodes by maximum depth (with maximum head, peak inflow,
peak flooding), links by peak absolute flow (with peak velocity and
maximum capacity fraction). Ties break by identifier.

Unavailable when the run reports no elements of the block's class.

#### 13.4.5 Flooding summary

One row per node that floods (§13.3 discharge counting, applied to the
node flooding series): peak overflow rate, periods flooded, and the
elapsed hours at the end of the first flooded period. Ranked by peak
overflow, largest first. Unavailable when the run reports no nodes, and
unavailable — distinctly — when no node floods.

#### 13.4.6 Outfall summary

One row per reported outfall vertex: **discharge frequency** (the
percentage of periods discharging, judged on the node's total inflow
series), **mean discharge** (the mean stored rate over discharging
periods), peak discharge, and total volume. Ranked by total volume,
largest first. Outfalls are identified from the model; a reported vertex
the model no longer contains is not an outfall.

Unavailable when the run reports no outfall vertices.

#### 13.4.7 Surcharge summary

One row per reported vertex with a positive rim depth in the model (a
junction's or storage's maximum depth) whose stored depth ever exceeds
$d_{\text{rim}} - f$, with $f$ the freeboard option: rim depth, maximum
depth reached, minimum clearance $d_{\text{rim}} - d_{\max}$ (negative
when the rim is exceeded), and the hours with depth above the freeboard
line. Ranked by hours above, largest first.

Unavailable when the run reports no vertex with a rim, and — distinctly —
when no vertex comes within the freeboard of its rim.

#### 13.4.8 Capacity summary

One row per reported conduit whose stored capacity fraction ever reaches
the threshold option: maximum capacity fraction, and the hours at or
above the threshold. Ranked by hours, largest first. Non-conduit links
have no meaningful capacity fraction and are excluded.

Unavailable when the run reports no conduits, and — distinctly — when no
conduit reaches the threshold.

#### 13.4.9 Velocity thresholds

Every reported conduit counted into three bands by its peak absolute
velocity against the two velocity edges (self-cleansing, erosive): below
the self-cleansing velocity, within the band, and above the erosive
velocity. Presented as a bar chart of counts with the two edges as
tagged values beside it.

Unavailable when the run reports no conduits.

#### 13.4.10 Storage summary

One row per reported storage vertex — utilisation and detention
performance as two halves of one question, "is the basin doing its job":

- **maximum depth** reached, and **depth used** — maximum depth as a
  percentage of the model's full depth, the utilisation figure (depth
  rather than volume, so no storage-geometry integration is involved);
- **mean stored volume** over the horizon;
- **peak inflow** (the node's total inflow series) against **peak
  outflow**, where a period's outflow is the water leaving through
  incident links — flow away on links oriented out of the vertex plus
  reverse flow on links oriented into it;
- **attenuation**: $100\,(1 - Q_{\text{out}}^{\text{peak}} /
  Q_{\text{in}}^{\text{peak}})$, absent when the storage saw no inflow.

Ranked by peak inflow, largest first. Unavailable when the run reports
no storage vertices.

### 13.5 Options

Ranked tables (`uds.subcatchment-peaks`, `uds.runoff-summary`,
`uds.node-extremes`, `uds.link-extremes`) accept a `rows` option: a
positive integer bounding the table length, default 10. A non-positive
or non-integer value refuses production. Blocks whose row count is the
subject itself (`uds.flooding-summary`, `uds.outfall-summary`,
`uds.surcharge-summary`, `uds.capacity-summary`) list every qualifying
element and take no row option.

The criteria-shaped options are expressed in **SI display units of their
quantity** — this engine's blocks are specified natively, so no
file-unit convention applies to them: `uds.surcharge-summary` accepts
`freeboard` (metres, default 0.3); `uds.capacity-summary` accepts
`threshold` (a fraction, default 0.8); `uds.velocity-thresholds` accepts
`edges` (two ascending velocities in m/s, default 0.6 and 3.0). A
malformed `edges` value refuses production.

### 13.6 Criteria

The engine publishes a criteria catalog under the foundation criteria
contract (hydra-common §7) — the standard a drainage network is assessed
against:

| Key | Kind | Quantity | Defaults (SI display units) |
|---|---|---|---|
| `freeboard` | value | depth | 0.3 m of clearance kept below a rim |
| `capacity` | value | percent | 80 % of conduit capacity |
| `velocity` | band `selfCleansing`/`erosive` | velocity | 0.6 / 3.0 m/s |

**Consumption (hydra-common §7.4).** `freeboard` becomes
`uds.surcharge-summary`'s `freeboard` option; `capacity`, divided by
one hundred, becomes `uds.capacity-summary`'s `threshold`; the
`velocity` band becomes `uds.velocity-thresholds`' `edges`. Options are
already SI (§13.5), so consumption converts nothing. A degenerate
velocity band omits its block; absent keys take the catalog defaults; a
malformed valuation is refused with a message naming the criterion.
