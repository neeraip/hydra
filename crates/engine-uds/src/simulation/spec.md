# hydra-engine-uds — Simulation Specification

This document holds §9–§12 of the urban drainage specification: operational
control, time integration and coupling, conservation, and the session
interface.

---

## 9. Operational Control

### 9.1 Rules

A rule is a prioritised conditional — `IF` premises, `THEN` actions, optional
`ELSE` actions — evaluated against simulation state and acting on link
controls only: channel open/closed, pump on/off or speed, orifice, weir, and
outlet settings in $[0,\,1]$.

**Premises** take the form *object attribute relation value-or-reference*.
The observable vocabulary is model semantics and is adopted exactly: gage
current or past-$n$-hours precipitation (up to 48 h, summed over completed
hourly buckets, and always zero for a gage no parcel references); vertex
depth, maximum depth, head, volume, and *lateral* inflow (not total); link
flow, depth, velocity, status, setting, and time open or closed, with
channels adding full-flow, full-depth, length, and slope; and simulation
time, date, clock time, day, month, and day of year. Every time-valued
comparison carries a half-step tolerance window for both equality and
inequality. An attribute inapplicable to its object evaluates the premise
false rather than raising an error — file semantics, adopted — but
validation warns on a premise that can never hold, rather than leaving it
silently inert.

Premise comparisons — and the constant values in actions — read in the
**file's unit system**, the same boundary rule as §14.6's expressions: the
observed quantity is presented in the units its author wrote the rule for,
which also keeps the last-compared value pair that curve lookups and PID
set-points consume in those units. Time-valued quantities compare in days,
matching the predecessor's clock.

**Boolean structure**: `AND` and `OR` combine premises with conventional
precedence — `A AND B OR C` means `(A AND B) OR C`.

> **CORRESPONDENCE:** the predecessor evaluates premises sequentially with
> short-circuiting through a running accumulator, so `A AND B OR C` binds as
> `A AND (B OR C)` — an artifact of the accumulator, not a documented
> grammar. This engine uses conventional precedence, consistent with its
> water-distribution sibling, where the identical artifact was ratified away.
> Rules mixing `AND` and `OR` without intent-revealing structure may fire
> differently; import (§14) flags them.
>
> *Source: `controls.c:517–535`.*

**Actions and conflicts**: a setting may be a constant, a curve lookup
(evaluated at the last-compared premise's left-hand value), a time-series
lookup, or a PID controller. Conflicts resolve through a per-link pending
slot: a strictly higher priority replaces, ties keep the earlier rule; an
action fires only if it changes the link's target, and modulated actions are
excluded from the action log. Rules evaluate every routing step, or on the
fixed clock of the rule-step option, whose boundaries the time stepper lands
on (§10); pump startup and shutoff depths apply every step regardless.

### 9.2 PID Controllers

The PID action interprets its parameters as gain $K_p$, integral time $K_i$
(minutes), and derivative time $K_d$ (minutes), applying the velocity-form
update on the normalised error $e = (x_{sp} - x)/x_{sp}$ (normalised by the
controlled value when the set-point is zero; integral term dropped when
$K_i = 0$):

$$\Delta u = K_p\left[(e_0 - e_1) + \frac{e_0\,\Delta t}{K_i}
  + K_d\,\frac{e_0 - 2e_1 + e_2}{\Delta t}\right]$$

added to the link's target setting each step, floored at 0, capped at 1 for
non-pumps, with small-error dead-banding and an error-history reset when
successive errors differ by less than $10^{-4}$. The set-point is the rule's
last-compared premise — the premise both triggers the rule and defines what
the controller regulates toward. Adopted exactly; these are file semantics a
calibrated model depends on.

### 9.3 The Expression Language

Named variables and expressions, treatment relations (§8), and custom
groundwater relations (§3) share one expression language: three-level
precedence (addition, multiplication, exponentiation binding tightest and
associating rightward, so `a^b^c` is `a^(b^c)`), unary minus where no
operand precedes — negating the whole multiplicative term it opens, so
`-a·b^2` is `-(a·b^2)` — scientific-notation literals, case-insensitive
names resolved through the consumer's vocabulary, and nineteen functions
(`sin cos tan cot asin acos atan acot sinh cosh tanh coth abs sgn sqrt log
log10 exp step`).

Evaluation is **total**: roots and logarithms of non-positive arguments,
powers of non-positive bases, division by zero, and any NaN result evaluate
to zero. This is file semantics — calibrated models lean on it — and is
adopted; but because it makes an ill-posed relation read as "no flux" rather
than an error, the engine reports the first domain-guarded evaluation of
each expression as a warning, once, so a mistyped relation announces itself
without changing any result.

## 10. Time Integration and Coupling

### 10.1 The Cascade and Its Clocks

Per §1.1 the coupling is a cascade, one step at a time: the surface balance
supplies the network as a source term; the network supplies transport as a
velocity field; the two backward influences — subsurface discharge reading a
routed stage, inlet backflow — are lagged one step, so within a step the
influence graph is loop-free. This is what permits three clocks:

- the **hydrology clock**, split into a *wet* step (while precipitation, snow
  cover, surface runoff, or a draining control measure exists anywhere) and a
  longer *dry* step otherwise;
- the **routing clock**, governed by §6.5's error-controlled stepping,
  typically far shorter; and
- the **reporting clock**, at which results are recorded. Its first
  boundary is one report step after the simulation start, or the report
  start date where that falls later: a reporting instant closes an interval,
  and at the start no interval has elapsed. The earliest record therefore
  describes the state one step in, and no record describes the initial
  condition.

> **CORRESPONDENCE:** this matches the predecessor, and differs from the
> water-distribution engine, whose quasi-steady solution *is* defined at the
> start instant and is reported there. A reader comparing the two sees a
> drainage run's first record at $t = \Delta t_r$ and a distribution run's
> at $t = 0$; both are the earliest instant at which their engine has an
> answer.


Per routing period: hydrology advances by whole hydrology steps until it
covers the routing period's end; routing advances one trial step under §6.5's
transaction rules, taking hydrology's per-parcel outputs — **linearly
interpolated** to routing times — as lateral vertex inflows; every reporting
boundary passed is then serviced.

Hydrology steps truncate to end exactly at any gage's recording-interval
boundary and at evaporation-change dates, so forcing is constant within a
step; a wet step longer than a series-fed gage's recording interval is
reduced to it, with a warning. Interpolation of hydrology outputs admits the
predecessor's stated exceptions, adopted as the reporting contract:
infiltration and evaporation rates hold piecewise-constant within a
hydrology step; subsurface elevation and moisture report end-of-step values.

**Reported precipitation is the precipitation that drove the computation.**

> **CORRESPONDENCE:** the predecessor re-derives reported rainfall at each
> report time by a one-second-advanced comparison against the gage's interval
> boundaries, so a report time in a recording gap reports zero and one at an
> interval boundary reports the *next* interval's intensity — a value the
> runoff computation has not yet seen. A reported rainfall series can thus be
> offset by an interval from the series that produced the reported runoff.
> This engine reports the forcing actually applied.
>
> *Source: `gage.c:31` (`OneSecond`) and `:561`.*

**Series extension** — delivering §2.9's per-consumer contract: outfall
stage, temperature, and rule-driven series actions **hold** their first or
last value outside the series' range; external inflows and external
accumulation loads read **zero** beyond it. An inflow series that ends
before the simulation therefore falls silently to nothing in the
predecessor; here the engine warns, once per series, when a consumed series
is exhausted before the run ends — a modelling error that produces a
plausible result should announce itself.

**Lateral inflows** are assembled at each routing step's start — hydrology
terms interpolated to the step time; external, sanitary, and RDII inflows
evaluated at the step-start date — with near-zero inflows truncated, and a
negative external inflow legal, booked as an outflow removing mass at the
vertex's concentration.

**Partial models** (§2.1) re-time the loop rather than emptying it: with no
conveyance compartment the clock advances on the smaller of the wet
hydrology and reporting steps; with no surface compartment, climate state
still advances so evaporation continues to apply.

### 10.2 Transactions

Every routing trial step of §6.5 is a **transaction**: rejection restores
vertex, channel, structure, rule, and accounting state completely, per §1.5. The
snapshot capability this requires is the same capability the checkpoint
contract of §12.3 persists; they are one design.

### 10.3 Event Windows

An event list restricts routing to date windows: between events the routing
step stretches to the next hydrology or reporting time, no lateral inflows
apply, and no flow or constituent routing occurs — hydrology continues,
network state freezes. Rules, however, are **operator forcing, not routed
state**: they evaluate on their §9.1 clock through the gap, their actions
landing on the frozen settings, so the network resumes in the operating
state the schedule demands — a time-triggered pump command inside a gap
fires at its appointed time, never late. Overlapping events clip to the
next event's start.
These are user-declared semantics ("only route when it matters") and are
adopted.

The predecessor's **steady-state skip** — bypassing flow routing when no
control fired, the previous step's flow error was within 5 %, and no lateral
inflow moved by more than 5 % — is not carried.

> **CORRESPONDENCE:** the skip freezes the network state through periods it
> judges quiet by fixed tolerances, producing accepted states that are
> approximations by fiat — the class of result §6.4 exists to eliminate. Its
> purpose is wall-clock economy on long dry-weather stretches; this engine's
> sanctioned mechanism for the same economy is step growth under quiescence,
> where the §6.5 error estimate is what certifies the quiet. Continuous-run
> wall-clock differs; if dry-weather cost proves material on real corpora,
> the remedy is revisiting the growth policy, not reintroducing frozen state.
>
> *Source: `routing.c:383–386` (`isInSteadyState`), reached from `:240`.*

## 11. Conservation

### 11.1 The Ledgers

Five balances are tallied over the run, each reporting the error statistic

$$\varepsilon = 100\left(1 - \frac{\mathcal{O}}{\mathcal{I}}\right)$$

(sign-mirrored when a ledger has outflow but no inflow, and zero within an
agreement threshold: 0.0283 m³ — the predecessor's 1 ft³ — for the
volumetric ledgers, 0.001 mass units for the constituent pair), with $\mathcal{I}$ the accumulated inflow side and
$\mathcal{O}$ the outflow side:

- **Surface**: precipitation, run-on, and initial ponded and snow storage,
  against evaporation, infiltration, runoff, underdrain discharge, ploughed
  snow, and final ponded and snow storage.
- **Subsurface**: infiltration and initial storage against upper- and
  lower-zone evapotranspiration, deep percolation, lateral flow, and final
  storage.
- **Network flow** — the one signed ledger: wet-weather and RDII inflow and
  initial storage always on the inflow side; final storage, flooding,
  evaporation, and seepage always on the outflow side; sanitary,
  subsurface, and external inflows and the system outflow each placed by
  their sign. There is no reaction term: constituent decay is mass, not
  volume, and appears in the constituent ledger alone.
- **Constituent**, per pollutant, worst error reported: initial mass and all
  inflow loads against flooding, outflow, reaction, seepage, and final mass —
  signed cases handled at accumulation, count-unit pollutants reported as
  $\log_{10}$.
- **Surface loading**: initial buildup, accumulation, and deposition against
  sweeping, infiltration, BMP removal, wash-off, and final buildup.

These ledgers are definitions, not diagnostics: they state what the engine
means by conservation, and the implementation is judged against them — never
the reverse. The predecessor's unaccumulated flow-balance reaction slot and
its steady-flow initial/final storage asymmetry are bookkeeping defects of
its implementation and have no counterpart here.

### 11.2 Statistics

The engine accumulates per-object and numerical-performance statistics on
every accepted step. The catalogue is adopted from the predecessor and
enumerated here, because §14.9's report is defined against it: a statistic
absent from this list is a column that cannot be printed.

Time-weighted means are accumulated as $\sum x\,\Delta t$ against
$\sum \Delta t$ over the same steps, never as unweighted step means — the
step size varies, so the two differ.

**Surface.** Per parcel: precipitation, run-on, evaporation, infiltration
and runoff depths over the run; runoff separated into its impervious and
pervious shares; peak runoff rate; and the runoff coefficient, the ratio of
total runoff to total supply (precipitation plus run-on), zero when supply
is zero. Wash-off load per parcel and constituent.

**Subsurface.** Per aquifer: infiltration, evapotranspiration, deep
percolation and lateral-flow volumes, and time-weighted mean zone moisture
and water-table elevation.

**Vertices.** Time-weighted mean depth; maximum depth and its instant;
maximum hydraulic grade, the maximum depth referred to the invert; and the
maximum depth observed *at reporting instants*, which is not the maximum
over computational steps and is reported separately because a reader
comparing the report against the results file sees the latter.

Flooding: total flooded time, peak flooding rate and its instant, flooded
volume, and maximum ponded volume.

Surcharge, defined as depth above the highest connecting crown: total
surcharged time, the maximum height above that crown, and the minimum depth
below the rim reached while surcharged — zero when the vertex floods.

Inflow: maximum lateral inflow; maximum total inflow and its instant;
lateral and total inflow volumes; and the vertex flow-balance error, the
§11.1 error statistic applied to that vertex alone, with its inflow volume
against its outflow volume and storage change.

Storage vertices additionally: time-weighted mean volume, mean and maximum
percent full, maximum volume and its instant, evaporation and exfiltration
losses as percentages of the mean volume, and maximum outflow.

Outfalls additionally: the fraction of observed time discharging, the
time-weighted mean and maximum discharge, discharge volume, and discharged
load per constituent.

**Links.** Maximum $|flow|$ and its instant; maximum $|velocity|$; maximum
depth; the maximum flow as a fraction of the section's full-flow capacity
and the maximum depth as a fraction of its full depth; and time flowing
full.

Time-in-class, as fractions of observed time, over the §6.3 classification:
dry, dry at the upstream end, dry at the downstream end, subcritical,
supercritical, critical at the upstream end, critical at the downstream
end — subcritical and supercritical separated by the Froude number, which
the classification already forms. Two further fractions are tallied
independently, because a step is in exactly one flow class but may be in
neither, either, or both of these: the time the §6.3 normal-flow limiter
bound the flow, and the time §7.6 culvert inlet control capped it.

Conduit surcharge times: full at both ends, full at the upstream end alone,
full at the downstream end alone, above normal flow, and capacity-limited.

The **flow instability index**: the count of accepted steps at which a
link's flow reversed the sign of its change while both neighbouring changes
exceeded the flow tolerance — a step-to-step oscillation that a converged
solution should not show — as a fraction of accepted steps.

Pumps additionally: utilisation time, startups, minimum, time-weighted mean
and maximum flow, pumped volume, energy, and time off each end of the
characteristic.

**Constituents.** The §11.1 constituent ledger's inflow side is
accumulated *by origin* — dry weather, wet weather, subsurface, sewer, and
external — summing exactly to the total admitted load, so the constituent
balance is reportable by source in the same partition the volumetric ledger
uses. Per-link cumulative transported load is tallied alongside.

**Numerical performance.** Accepted steps; rejected trials; the
degraded-accuracy tally of §6.5; minimum, maximum, and time-weighted mean
step size; mean iterations per step; the fraction of accepted steps that
reached the trial limit without converging; and the distribution of step
size over five bands spanning the step floor to the routing step, spaced
logarithmically. The top-five diagnostic lists — governing vertices,
least-stable links, most frequently non-converging vertices — accompany
them.

Three changes from the predecessor:

- **Off-curve time is booked to the correct end for every pump type.** The
  predecessor's summary prints low-end and high-end columns for all pumps,
  but only its Type 4 sets distinguishable markers; the other types set a
  flag colliding with the low-end marker, so their off-curve time all books
  high. The evident two-column intent is implemented.
- Steady-skip time vanishes with the skip (§10.3); step-rejection counts and
  the degraded-accuracy tallies of §6.5 take its place among the
  numerical-performance statistics.
- Per-object statistics are gated on the report start date; numerical-
  performance statistics span the whole run — the predecessor's split,
  adopted and stated rather than discovered.

Energy is tallied as $\rho g\,Q\,\Delta H$ per §7.1.

## 12. Session Interface

### 12.1 Lifecycle

The session is phased: **create** → **load** (parse, validate — §5's
mutations applied here) → **run**, stepwise or to completion → **results**.
The load also accepts the contents of the auxiliary records a model may
declare — daily climate records (§3.1) and external rain records (§14.12)
— supplied by the caller, which owns all file I/O; a model declaring an
auxiliary record the caller did not supply refuses the load with the
record named.
Every entry point is guarded by phase, returning a typed error rather than
faulting. Between load and run the whole validated model is readable and
design parameters writable; during the run, current-time results, running
statistics, and to-date balances are readable, while writes are confined to
**boundary forcing and control state** — gage precipitation, vertex lateral
inflow, outfall stage, link target settings, loss coefficients, flow limits,
control-measure drain parameters, concentrations. After the run, results and
run-total balances are readable and nothing is writable.

The asymmetry is the contract, inherited from the predecessor because it is
right: **geometry is frozen once the run starts; forcing is not.**

### 12.2 Results Access

Results are served for every object at every reporting time, interpolated
per §10.1's contract, and identified by object identity — never by position
in an output artifact. What the predecessor's report-selection section does
to its binary file layout is a property of that file, handled at export
(§14); it does not shape this interface.

### 12.3 Checkpointing

A checkpoint captures the **complete** state of §2.10 — surface, subsurface,
snow, and control-measure layer state per parcel; vertex depths; channel
flows and areas; constituent masses and concentrations with removal times;
regulator settings and rule state — such that a run restored from it
continues bit-identically to one never interrupted. It is the transaction
snapshot of §10.2, persisted; one mechanism serves both.

Checkpoints may be written at end of run or on demand mid-run, and loaded in
place of §6.7's initial-condition seeding. The persistence format is this
engine's own; the predecessor's hotstart files are an import and export
concern (§14), where their known omissions — control-measure layer state
lost entirely, surface buildup unreadable past one pollutant — are what the
import can and cannot recover, not limitations of this contract.

### 12.4 Mid-Run Forcing

During a run a caller may inject precipitation per gage (superseding the
gage's data source), lateral inflow per vertex, boundary stage per outfall,
and target settings per controllable link — the same vocabulary rules may
act on, under the same conflict resolution, logged in the action record.
Injection never mutates geometry, and no injection has undocumented side
effects on the stepping policy.
