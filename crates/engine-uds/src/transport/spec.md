# hydra-engine-uds — Transport Specification

This document holds §8 of the urban drainage specification: constituent
accumulation, mobilisation, network transport, and treatment.

---

## 8. Constituent Transport

### 8.1 Constituents and Mass Sources

A constituent is any substance expressible as an additive concentration —
mass or organism counts per volume — which deliberately excludes pH,
conductivity, turbidity, and colour. Each carries the attributes of §2.8,
including a first-order decay coefficient active in the network (negative
coefficients modelling growth) and the co-pollutant potency relation
$C_{total,i} = C_i + f_{ij} C_j$, applying to accumulation and mobilisation
loads only, $f_{ij}$ free to exceed 1 since it bridges units.

Mass enters the network at a vertex through eight paths, assembled in order
at each routing step's start: surface runoff at the parcel's mobilisation
concentration; direct wet deposition, a constant rain concentration on the
precipitation volume mixed into the ponded store; control-measure drain
flow at the parent parcel's concentration less any drain removal;
subsurface inflow, sewer inflow, and sanitary flow at their constant
concentrations; external inflow as a concentration on its flow or as a
flow-free mass load; and routing-interface-file series. Two further paths
move mass within the system: an inflowing link delivers its previous-step
concentration to its downstream vertex, and street inlets transfer captured
mass to the sewer — returning as backflow under surcharge — at the donating
vertex's previous-step concentration.

### 8.2 Accumulation and Street Cleaning

Land uses partition each parcel for quality alone; each (constituent, land
use) pair owns one accumulation and one mobilisation relation. Accumulation
$b$ — mass per area or per curb length — grows with dry time by one of
three forms:

$$b_{pow} = \min\!\big(B_{max},\,K_B\,t^{N_B}\big), \qquad
b_{exp} = B_{max}\big(1 - e^{-K_B t}\big), \qquad
b_{sat} = \frac{B_{max}\,t}{K_B + t},$$

with the predecessor's column conventions (the saturation form reading its
half-saturation time from the third coefficient column) and admissible
exponent set $\{0\} \cup [0.01, 10]$ adopted as file semantics. A fourth,
external form — a scaled user loading series capped at a maximum — bypasses
the mechanism below.

**The state is mass, not time.** Each dry step inverts the chosen form to
recover the equivalent time for the mass on hand, advances it, and
re-evaluates — so cleaning and mobilisation rewind the clock rather than
resetting it, and accumulation resumes along the same curve. Each form
carries its time-to-maximum, beyond which buildup pins at $B_{max}$, with
the predecessor's blow-up guard on the power form.

> A power form with a zero rate constant or zero exponent has a
> time-to-maximum of zero and **jumps to $B_{max}$ after its first dry
> step** — a line written with zeroed coefficients to mean "no buildup"
> produces the opposite. Reproduced as file semantics; validation flags it,
> since the predecessor never does.

Initial buildup comes from a user loading or from evaluating the form over
the antecedent dry days; accumulation pauses during wet steps, and
snow-only constituents accumulate only under snow cover. **Street
cleaning** runs per land use on an interval within a seasonal window,
each pass removing availability × efficiency of the current mass,
suppressed during rain, under snow on the plowable area, or with a zero
interval.

### 8.3 Mobilisation

Three per-(constituent, land-use) relations, all cut off below the
predecessor's minimum runoff intensity:

$$w_{exp} = K_W\,q^{N_W} m_B, \qquad
w_{rat} = K_W\,(f\,Q)^{N_W}, \qquad
w_{emc} = C_{emc}\,f\,Q,$$

with $q$ the runoff intensity over the parcel, $m_B$ the remaining
accumulated mass, $Q$ the parcel runoff rate, and $f$ the land use's area
fraction. The exponential form is source-limited by construction, each step
depleting the accumulated mass before removal credits apply; the rating
form is evaluated on the **land-use share of flow**, not prorated
afterwards — the two differ whenever $N_W \neq 1$; the event-mean form is
the rating form at unit exponent. A rating or event-mean relation with no
paired accumulation has no mass to draw down; the surface-loading ledger of
§11 stays closed by booking each such load as an equal, simultaneous
accumulation input — an accounting entry, named as such.

Rain and run-on loads mix through the **ponded store** — one extra state
per constituent per parcel: the step's inflow mass over its inflow volume
gives the ponded concentration, infiltration and outflow each remove their
volume's share in order, each clamped to the mass on hand, and the residual
mass is what the new ponded volume carries. Per-land-use removal fractions
discount the mobilised stream and their area-weighted mean the ponded
stream; loads to another parcel become its run-on next step. A step with no
inflow writes residual ponded mass off to final storage — the
predecessor's semantics, adopted: deposited mass does not persist for
resuspension.

**Evaporation leaves mass behind.** The ponded store's mass balance is
closed: evaporated volume concentrates the remainder.

> **CORRESPONDENCE:** the predecessor writes the new ponded mass as
> concentration times a ponded depth from which evaporation has already
> been subtracted, so the evaporated volume's share of mass leaves the
> ledger — neither retained by the store nor booked as a loss anywhere.
> Under §11 a ledger leak is not adoptable semantics; this engine
> conserves, and ponded concentrations after evaporative periods run
> correspondingly higher.

### 8.4 Network Transport

Every channel and storage vertex is a **completely-mixed reactor**; the
concentration field never influences flow. At each routing step, vertex
inflow loads accumulate from §8.1's paths; a vertex holding negligible
volume takes the flow-weighted mixture, one actually holding water updates
as a reactor; channels and storage vertices update by the robust mixing
form

$$c^{t+\Delta t} =
\frac{c^{t}\,V^{t}\,e^{-K_1\Delta t} + C_{in}\,Q_{in}\,\Delta t}
     {V^{t} + Q_{in}\,\Delta t},$$

chosen over the analytical reactor solution because it remains stable as
volumes vanish and never overshoots a step input, with the mixing inflow
volume-adjusted for the channel's storage change and the result clamped at
the larger of the reactor and inflow concentrations.

> **CORRESPONDENCE:** the predecessor evaluates the decay factor as the
> linear truncation $(1 - K_1\Delta t)$ floored at zero, the exponential
> surviving only on a routing path this engine does not have. The
> exponential is exact, costs nothing, and is used here; slowly-decaying
> constituents on long steps differ in the predecessor's disfavour.

Below the dry thresholds — 1 litre of volume or 1 mm of depth — an
element's remaining mass flushes to final storage and its concentration
zeroes, unconditionally for channels and absent inflow for vertices;
initial concentrations seed only elements wet at start. Volume-less links
pass their upstream vertex concentration through. Evaporation concentrates
by $1 + V_{evap}/V$, consistently with §8.3.

### 8.5 Treatment

A treatment expression attaches to any (vertex, constituent), computing
either a resulting concentration or a fractional removal applied to the
influent, over: constituent names (concentration) and their removal
symbols; the hydraulic variables flow, depth, area, step length, and — for
storage vertices — the residence time, updated as
$\theta \leftarrow (\theta + \Delta t)\,V/(V + Q_{in}\Delta t)$ and served
in hours; and the expression language of §9.3, with its total-evaluation
semantics and the unit treatment of §14.6.

Adopted semantics: a referenced constituent denotes the combined-influent
concentration when that constituent's own equation at the vertex is
removal-type — and, by the predecessor's zero-initialised record, also when
it has no equation there at all — and the pre-treatment concentration
otherwise; the two coincide at vertices holding no volume. Guardrails:
treated concentration bounded by the untreated value and zero; removals at
most 1; removal-form yields zero without inflow; a treatment expression
overrides the constituent's global decay at that vertex; co-pollutants
receive no automatic co-treatment. Cyclic removal references are refused at
validation (§14.7).

The mass treatment removes is the concentration drop it applies times the
step's inflow-augmented pool volume, $(c_{mix} - c_{out})(V_{old} +
Q_{in}\Delta t)$, booked to the reaction account of §11.1. The influent's
mass is already inside $c_{mix}$ — mixing precedes treatment — so no
influent term may be added on top: doing so overstates removal by
$(c_{in} - c_{mix})\,Q_{in}\Delta t$ whenever stored water dilutes the
influent. Treatment at an outlet vertex revises its discharge: the
discharged load is the treated mass, and the removed mass moves to the
reaction account — it is never counted in both.
