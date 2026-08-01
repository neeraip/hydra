# hydra-engine-uds — Hydraulics Specification

This document holds §5–§7 of the urban drainage specification: cross-section
geometry, network flow, and structures. §6 is given here; §5 and §7 follow.

---

## 6. Network Flow

### 6.1 The Routed System

Flow routing solves the one-dimensional Saint-Venant equations on every
channel of the conveyance graph,

$$\frac{\partial A}{\partial t} + \frac{\partial Q}{\partial x} = q_L,
\qquad
\frac{\partial Q}{\partial t}
+ \frac{\partial}{\partial x}\!\left(\frac{Q^2}{A}\right)
+ gA\,\frac{\partial H}{\partial x}
+ gA\,S_f = 0,$$

where $A$ is flow area, $Q$ discharge, $x$ distance along the channel, $q_L$
lateral inflow per unit length, $H = z + y$ the hydraulic head ($z$ invert
elevation, $y$ flow depth), $g$ gravitational acceleration, and $S_f$ the
friction slope. Friction is Manning's,

$$S_f = \frac{n^2\,Q\,\lvert U \rvert}{A^2 R^{4/3}},$$

with $n$ the Manning coefficient, $U = Q/A$ the mean velocity, and $R$ the
hydraulic radius; the $\lvert U \rvert$ factor makes friction oppose the flow
direction. Force mains substitute a pressurised friction relation while full
(§7). Vertices close the system with continuity between their net inflow and
the rate of change of stored volume. Structures — pumps, orifices, weirs,
outlets — are algebraic head–discharge relations spliced into the graph (§7);
outfalls impose the boundary conditions of §2.6.

There is **one solver**. Per §2.3, the predecessor's reduced routing forms are
not solution methods of this engine; a model file requesting them is handled at
import (§14). Everything below specifies the full dynamic treatment.

> **CORRESPONDENCE:** the predecessor offers steady and kinematic-wave routing
> as user-selectable methods, each with its own topology restrictions, its own
> flooding accounting, and its own special-cased element semantics. This engine
> routes every model with the full equations. A model authored for a reduced
> method produces results at least as physical: backwater, reversal, and
> storage effects the reduced method could not represent are represented.
> Divergences on such models are attributed to the method difference, and the
> import layer reports the substitution.

### 6.2 The Pressurisation Closure

A closed conduit flowing full has no free surface, and the equations of §6.1
presuppose one. This engine closes the gap with a single device, applied
uniformly: every closed cross-section carries a narrow hypothetical **slot**
above its crown, so that depth may exceed the crown height and the
free-surface equations remain valid in every regime. There is no separate
surcharge equation branch, no free-surface/pressurised state machine, and no
transition band: one equation set governs everywhere.

The slot is not an arbitrary widening — it is the engine's model of
**pressure-wave celerity**. A slot of width $w$ atop a full conduit propagates
gravity waves at $c = \sqrt{gA_{full}/w}$, which is precisely the acoustic
celerity of the pressurised pipe it stands in for. The width is therefore
*derived from* a stated celerity rather than posited:

$$w_{slot} = \frac{g\,A_{full}}{c^2}$$

where $A_{full}$ is the section's full-flow area and $c$ is the
**pressurisation celerity**, a session option (`pressure_celerity`, m/s,
default 50, minimum 5). The default is a deliberate compromise, stated rather
than hidden: true waterhammer celerities (order $10^3$ m/s) would make the
system needlessly stiff for sub-minute drainage dynamics, while the default
keeps slot storage negligible — at $c = 50$ m/s the slot stores under 0.4 % of
$A_{full}$ per metre of surcharge — and surge propagation faster than any
process the engine's accuracy definition (§1.2) covers.

**Geometry under the slot.** For $y < y_{full}$ the section's true properties
apply, except that the top width is floored at $w_{slot}$:
$\tilde W(y) = \max\!\big(W(y),\, w_{slot}\big)$. A closed section's width
falls continuously to zero at the crown, so the floor produces a continuous
transition at the depth where $W(y)$ crosses $w_{slot}$, with no additional
shape parameter. For $y \ge y_{full}$:

$$\tilde W = w_{slot}, \qquad
\tilde A(y) = A_{full} + w_{slot}\,(y - y_{full}), \qquad
\tilde R = R_{full}.$$

The hydraulic radius holds at its full-pipe value because the slot is
storage, not conveyance: friction in a surcharged pipe is that of the full
pipe.

**Accounting.** Water stored in slots is real stored volume to the continuity
accounts of §11, reported within channel storage; it is bounded by the
celerity choice and vanishes as $c \to \infty$.

> **CORRESPONDENCE:** the predecessor defaults to the EXTRAN treatment — a
> separate algebraic branch for surcharged vertices, Newton-corrected with a
> 0.6/1.0 damping factor, blended into the free-surface update over an
> exponential band reaching 25 % above the crown, using a surface area saved
> from the last unsurcharged state, with closed-conduit top widths frozen at
> 96 % of crown depth (98.53 % under its optional slot) — and offers a slot
> with an empirically-shaped width floored at 1 % of the maximum width. This
> engine retires the entire dual treatment. Surcharged heads differ from
> EXTRAN-mode output; comparison against the predecessor's validation corpus
> is run in its slot mode where surcharge dominates, and residual differences
> are attributable to the width model, which here is a stated celerity rather
> than a fitted curve.

### 6.3 Spatial Discretisation

The scheme is staggered: channels carry the momentum equation for discharge,
vertices carry continuity for head.

**Channel update.** Substituting continuity into momentum and discretising
over a channel of length $L$ — implicit in time for the friction and loss
terms, end-differenced in space, overbars denoting channel-average values —
gives the update

$$Q^{t+\Delta t} =
\frac{Q^{t} + \Delta Q_{inertia} + \Delta Q_{pressure} + \Delta Q_{loss}}
     {1 + \Delta Q_{friction} + \Delta Q_{losses}}$$

with

$$\Delta Q_{inertia} =
  \sigma\left[2\bar U\big(\bar A^{t+\Delta t} - \bar A^{t}\big)
  + \bar U^{2}\,\frac{(A_2 - A_1)\,\Delta t}{L}\right],
\qquad
\Delta Q_{pressure} = -\,gA_w\,\frac{(H_2 - H_1)\,\Delta t}{L},$$

$$\Delta Q_{friction} = \frac{g\,n^2\,\lvert \bar U \rvert\,\Delta t}{R_w^{4/3}},$$

where subscripts 1, 2 denote the upstream and downstream ends, $\bar U$ and
$\bar A$ are mid-channel velocity and area, $\bar A^{t}$ is the mid-channel
area at the previous *time step* (not the previous iterate), $A_w$ and $R_w$
are the upstream-weighted area and hydraulic radius defined below, and
$\Delta Q_{loss}$, $\Delta Q_{losses}$ carry the seepage/evaporation momentum
term and local losses of §7. Friction and local losses sit in the denominator
— an implicit linearisation that keeps the update stable as flow approaches
zero. All geometric properties are the slot-modified $\tilde A, \tilde W,
\tilde R$ of §6.2, so this single update serves every regime.

The velocity used in forming the momentum terms is capped at 15.24 m/s
(the predecessor's 50 ft/s, converted); the cap is a guard against transients
the model does not resolve, applied to the velocity, never to the resulting
flow.

**Inertial damping.** The factor $\sigma$ tapers the inertial terms with the
Froude number: $\sigma = 1$ for $Fr \le 0.5$, $\sigma = 2(1 - Fr)$ for
$0.5 < Fr < 1$, and $\sigma = 0$ for $Fr \ge 1$.

> **Note — a deliberate closure, revisitable.** Near critical flow the
> discretisation cannot resolve the inertial terms, and suppressing them is
> the field-proven stabiliser this scheme's whole lineage carries; the
> alternative — a shock-capturing flux treatment computing bores and jumps
> outright — is a different discretisation, not a parameter change, and is
> recorded here as the known successor should trans-critical accuracy ever
> justify it. The taper is this engine's sole behaviour: the predecessor's
> options forcing $\sigma = 1$ or $\sigma = 0$ are approximation choices, not
> model properties (§2.3), and are handled at import.

**Upstream weighting.** The pressure and friction terms use area and hydraulic
radius weighted toward the upstream end by the same Froude-based factor
(computed before the closed-full override below): no weighting at
$Fr \le 0.5$, fully upstream at $Fr \ge 1$, applied only in positive,
non-full, downstream-sloping flow. A closed conduit flowing full takes
$\sigma = 0$: under the slot's celerity model, inertia of the slot wave is not
a modelled quantity.

**Vertex update.** Each vertex integrates

$$H^{t+\Delta t} = H^{t} + \frac{\Delta V}{A_S},
\qquad
\Delta V = \tfrac{1}{2}\left[\big(\textstyle\sum Q\big)^{t}
          + \big(\textstyle\sum Q\big)^{t+\Delta t}\right]\Delta t,$$

where $\sum Q$ is the net inflow — link flows signed by orientation, plus
lateral inflow, less evaporation and seepage — and $A_S$ is the **assembled**
surface area: the vertex's own storage area (zero for a junction; the ponded
area once a ponding-enabled junction exceeds its full depth) plus a
contribution from each connecting channel — the width-weighted trapezoid of
the adjacent half-length, reapportioned at critical, dry, and offset ends as
§6.6 specifies. Because §6.2 floors every closed section's width at
$w_{slot}$, a surcharged vertex's assembled area is the sum of its slot
widths' contributions: continuity remains an honest ODE in $H$ with no
algebraic special case. The assembled area is floored at a documented minimum
(`min_surface_area`, default 1.167 m², the plan area of a 1.2 m manhole) —
a physical floor, since a real access structure has real plan area, not a
numerical fiction.

### 6.4 Iteration

Within a trial step the scheme iterates to self-consistency:

1. **∥ Channel phase**: every channel computes its flow update from the
   last iterate's heads. This phase is order-independent by construction and
   is the specification's parallel region.
2. **Structure phase**: pumps, orifices, weirs, outlets, and zero-length
   connectors compute their flows **against the last iterate's vertex state**.
   The phase is order-independent: no structure sees the running accumulation
   of another's result within the same iterate.
3. **Vertex phase**: heads update per §6.3 from the accumulated flows.

> **CORRESPONDENCE:** the predecessor's structure phase solves serially in
> link-definition order, each structure immediately updating its end vertices,
> so a pump's available-volume clamp observes whichever structures happen to
> precede it in the input file — results are a function of the file's line
> order. That is unattributable divergence by construction, and this engine
> removes it: structure results are a function of the model alone. Where the
> predecessor's order-sensitivity is material, results differ, attributed
> here.

Flows and heads are under-relaxed against the previous iterate with a fixed
factor $\omega = 0.5$; pumps are exempt from flow relaxation and vertices
above their crown from head relaxation, each being governed by mechanisms
carrying their own damping. A relaxed flow whose sign opposes the previous
iterate is replaced by $\pm 2.832\times10^{-5}$ m³/s in the new direction, so
a reversal passes through zero rather than jumping across it. Fixed
relaxation is retained deliberately: it is proven at scale, and acceleration
schemes are a recorded refinement requiring their own specification, not an
implementation liberty.

**Convergence** requires both, for every non-outfall vertex:

1. $\lvert H^{(m)} - H^{(m-1)} \rvert \le \varepsilon_H$, with
   $\varepsilon_H$ = 1.524 mm (the predecessor's 0.005 ft, converted); and
2. the **continuity residual** — the discrepancy between the vertex's net
   inflow and its stored-volume rate,
   $\big\lvert \sum Q - A_S\,(H^{(m)} - H^{t})/\Delta t \big\rvert$ — summed
   over vertices, not exceeding the same relative tolerance against the sum
   of flow magnitudes.

The first criterion certifies the iterates have settled; the second certifies
that the state they settled at conserves mass. Iteration runs a minimum of 2
and a maximum of `max_trials` (default 8) passes.

**A non-converged trial is a rejected trial.** Exhausting the iteration
budget does not produce an accepted state: the trial is discarded under the
transaction rules of §6.5 and retried at half the step. At the step floor the
state is accepted and carries a per-vertex degraded-accuracy warning.

> **CORRESPONDENCE:** the predecessor tallies convergence failures and
> continues with the unconverged iterate; its reported continuity error partly
> measures that acceptance. This engine reports no state that is not a
> solution of its equations, except at the step floor, where it says so. Runs
> that the predecessor completed with unconverged steps differ here — either
> in wall-clock (retries) or in results (the retried steps converge to
> something else); both are the removal of accepted non-solutions.

### 6.5 Time Integration and Error Control

Routing advances by trial steps under the transaction rules of §1.5: a trial
that fails its error test or its convergence budget is discarded — vertex,
channel, structure, and accounting state restored — and retried at half the
interval.

**The attempted step** is seeded by the stability and rate constraints,

$$\Delta t_{try} = \min\!\big(\Delta t_{user},\
  C_f \min_{channels}\frac{L}{\lvert U\rvert + \sqrt{g\tilde A/\tilde W}},\
  \min_{vertices}\ \Delta t_{cr},\ 2\,\Delta t_{prev}\big)$$

where $C_f$ is the Courant factor (default 0.75), $L$ is the channel's **true
length**, $\Delta t_{cr}$ is the time for a vertex's head to change by a
quarter of its crown height at its recent rate (outfalls, near-dry and
above-crown vertices exempt), and the $2\Delta t_{prev}$ term caps growth at
twice the previously accepted step. Channels with $Fr \le 0.01$, negligible
flow, or negligible area are exempt from the Courant term. Steps are
real-valued — no quantisation — floored at $\Delta t_{floor}$ = 0.5 s, and
the run opens at the floor.

**The error test.** These seeds are constraints, not accuracy statements; the
accuracy statement is a per-step local error estimate. For each non-outfall
vertex the estimate is the standard second-difference truncation measure over
the last three accepted times,

$$e_i = \frac{\Delta t^2}{2}\,\lvert \ddot H_i \rvert,$$

with $\ddot H_i$ the three-point divided second difference of the vertex's
head history (the estimate is zero until two steps have been accepted). The
trial is accepted when $\max_i e_i \le$ `routing_err_tol` (metres, default
$10^{-3}$; 0 disables the error test, leaving the constraint-seeded stepping).
A rejected trial halves; a trial at the floor is accepted unconditionally
and, if its estimate or its convergence budget failed, carries the
degraded-accuracy warning naming the worst vertex.

> **CORRESPONDENCE:** the predecessor's variable step is stability-governed
> only — Courant times a factor, plus the quarter-crown rate rule — with no
> measure anywhere of the integration error committed, and is quantised down
> to whole milliseconds. Its optional conduit-lengthening transform buys
> fixed-step stability by fictitiously lengthening short conduits and
> rescaling their slope and roughness — solving a different network than the
> user supplied, and feeding the lengthened length into the Courant condition
> itself. This engine retires the transform entirely: the network solved is
> the network given, short channels cost small steps, and the cost is visible
> in the step diagnostics rather than hidden in falsified geometry. Import
> flags models whose stub channels will Courant-limit the run (§14).

### 6.6 Flow Limits and Special Classes

**Normal-flow limit.** A positive computed flow is limited to Manning normal
flow when the water-surface slope is less than the bed slope, or the upstream
Froude number is at least 1 — the criteria user-selectable as either, both,
or neither, defaulting to both — except that channels adjoining an outfall
always apply the slope test and never the Froude test. The check is skipped
for full upstream ends, critical or dry flow classes, and culvert-coded
channels. This is a physical kinematic limit for steep channels, not a
stability device, and is adopted as model semantics.

**Flow classes.** Channel ends at nearly-dry or critical depth substitute
critical or normal depth for the vertex head on the affected end, with a
linear ramp of the downstream area contribution across the band between
critical and normal depth; a channel dry at both ends carries zero flow for
the trial while retaining a nominal head derivative. The surface-area
assembly of §6.3 reapportions accordingly: a critical (free-fall) end
contributes nothing, the far vertex taking the full-length average; a dry end
contributes only where the channel has no offset there; a channel dry at both
ends contributes a nominal minimum. Flow out of an essentially dry vertex is
clamped to $\pm 3.05\times10^{-5}$ m³/s rather than zeroed, and a
user-supplied flow limit, when given, caps $\lvert Q \rvert$ every iteration.

**Flooding and ponding.** A non-ponded vertex whose head would exceed its
ground elevation (plus any surcharge allowance) is pinned there, the surplus
inflow leaving the system as reported flooding. With ponding enabled the
surplus accumulates over the vertex's ponded area — a virtual store whose
head may rise above ground — and drains back as capacity recovers.

### 6.7 Initial Conditions

Depths and flows default to zero. A user-supplied initial channel flow
implies Manning normal depth. A vertex without a supplied depth is seeded
with the average, over its connecting links, of link end depth plus the
link's upstream offset, at non-outfall, non-storage vertices only; channels
without an initial flow then take the mean of their end-vertex depths. A
checkpoint restore (§12) bypasses the seeding entirely.
