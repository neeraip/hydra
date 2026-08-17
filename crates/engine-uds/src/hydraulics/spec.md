# hydra-engine-uds — Hydraulics Specification

This document holds §5–§7 of the urban drainage specification: cross-section
geometry, network flow, and structures.

---

## 5. Cross-Section Geometry

### 5.1 The Section-Property Contract

Every channel cross-section supplies four properties as functions of flow
depth $y$: area $A(y)$, top width $W(y)$, hydraulic radius $R(y)$, and the
section factor $\Psi(y) = A R^{2/3}$ used by Manning relations. The
properties are mutually consistent — $W = \mathrm{d}A/\mathrm{d}y$ — and are
the *true* geometry; the slot modification of §6.2 is applied on top of them
by the solver, never baked into them.

For every closed section, $\Psi(y)$ **peaks below full depth** and declines
toward the crown (for a circle, near $y = 0.94\,y_{full}$): the wetted
perimeter grows faster than the area near the top. The specification treats
this honestly rather than clipping it: $\Psi_{max}$ and its depth are
properties of the section, and the normal-depth inversion of §5.7 operates on
the monotone branch below the peak. No property is artificially capped or
frozen short of the crown — the predecessor's 96 % cutoffs existed for its
surcharge branch, which §6.2 retired.

### 5.2 Analytic Families

Where the geometry has a closed form, the engine evaluates it — no tables,
no fitted seeds, no iteration caps. Rectangular, triangular, trapezoidal,
parabolic, and power sections are elementary. The circle is evaluated through
the filled angle $\theta$:

$$y = \frac{D}{2}\left(1 - \cos\frac{\theta}{2}\right), \qquad
A = \frac{D^2}{8}(\theta - \sin\theta), \qquad
R = \frac{D}{4}\left(1 - \frac{\sin\theta}{\theta}\right), \qquad
W = D\sin\frac{\theta}{2},$$

where $D$ is the diameter, with $\theta(y)$ obtained in closed form and the
inverse maps of §5.7 solved on these exact relations. The filled ellipse is
evaluated analytically in the same manner. Closed rectangular, modified
basket-handle, and the rectangular-triangular and rectangular-round compounds
compose from the elementary pieces.

> **CORRESPONDENCE:** the predecessor evaluates circular geometry through
> 51-entry normalised tables with quadratic interpolation over the two lowest
> depth segments, a fitted-polynomial seed for the near-empty inverse, a
> 40-pass iteration returning its seed on non-convergence, and a correction
> clamp whose sign-transfer idiom is inert as written (it bounds the
> correction in one direction only). All of that is table-era machinery for a
> function that has a closed form, and none of it is carried. Differences from
> the tabulated values are bounded by the tables' own interpolation error and
> are largest in the near-empty regime the quadratic patch existed to serve.
>
> *Source: `xsect.dat:31–58`, four 51-entry circular tables; `xsect.c`
> `lookup()`, quadratic only where `i < 2`; `xsect.c:2582–2590`, whose
> 40-pass loop ends `return theta1` — the seed it started from — and whose
> clamp is guarded by `d > 1.0` alone, which is what leaves the `SIGN`
> sign-transfer inert.*

### 5.3 Tabulated Families

The legacy masonry profiles — egg, horseshoe, gothic, catenary,
semi-elliptical, semi-circular, basket-handle — have no defining equation:
**their tables are the shape**. These sections are adopted as tabulated
definitions, transcribed from the predecessor at its stated resolution
together with its interpolation rule (linear, with the quadratic refinement
over the two lowest depth segments), because a "more accurate" evaluation of
a shape that exists only as a table is not a meaningful claim. Each family
records which properties it tabulates and which it derives — area by inverse
lookup where no area table exists, hydraulic radius from the section factor
as $R = (\Psi/A)^{3/2}$ — exactly as the predecessor's four provision groups
do.

### 5.4 Standard-Size Catalogues

The horizontal and vertical ellipse and the arch admit selection by standard
size code — published catalogues of manufactured sections (23 ellipse codes,
102 arch codes) whose full-flow area and hydraulic radius are engineering
data, not computable quantities. The catalogues are adopted verbatim, always
in their published US customary dimensions regardless of the file's unit
system, exactly as the predecessor reads them.

A catalogue row anchors the section's full-flow values: rise, span, area,
and hydraulic radius. Depth variation follows the shape. A coded ellipse
*is* an ellipse, so its properties are the §5.2 analytic functions evaluated
at the catalogue axes and scaled so the full-flow values land on the
catalogue's — the area by a constant factor, the hydraulic radius likewise.
The arch has no defining equation, so its normalised area, hydraulic-radius,
and width tables are transcribed per §5.3 and scaled to the row's full-flow
values the same way.

Arbitrary user axes fall back to the analytic ellipse (§5.2) — evaluated at
the axes the user wrote — and, for the arch, to the predecessor's
proportionality constants $A_{full} = 0.7879\,y_{full} w_{max}$ and
$R_{full} = 0.2991\,y_{full}$ over the same transcribed tables.

> **CORRESPONDENCE:** two predecessor behaviours are replaced. Its ellipse
> depth variation comes from one normalised 26-point table per orientation,
> computed at a single reference aspect ratio and applied to every size —
> this engine evaluates the true geometry at each section's own axes. And a
> user-dimensioned ellipse's entered width never reaches its hydraulics: full
> area and radius come from fixed-proportion constants
> ($1.2692\,y_{full}^2$, $0.3061\,y_{full}$), so the predecessor solves a
> fixed-ratio ellipse whatever the user drew — this engine solves the ellipse
> the user specified, and import (§14) notices user-dimensioned ellipses,
> whose results differ accordingly.
>
> *Source: `xsect.c:572–573` (horizontal), `600–601` (vertical).*

### 5.5 Custom Shapes

A custom section is defined by a user width-against-depth relation describing
a unit-height section scaled by the channel's full depth: anchored at the
origin, truncated above unit height, extended at its last width if it stops
short, and closed at the top — bottom and top widths both counting as wetted
perimeter. These semantics are the file contract and are adopted exactly.

Evaluation is direct: a piecewise-linear width relation makes area piecewise
quadratic and wetted perimeter a sum of segment lengths, all exact. The
predecessor's resampling of the curve into 51-point normalised tables is not
carried — the shape the user drew is the shape solved.

### 5.6 Transects

A transect represents a natural channel by surveyed station–elevation pairs
(up to 1,500 stations, with the survey rescalable by a multiplier and
offset), with distinct left-overbank, main-channel, and right-overbank
Manning coefficients; an omitted overbank coefficient defaults to the
channel's. Vertical end walls close both ends, contributing wetted perimeter.

**Composite roughness** is handled by conveyance summation: a new conveyance
segment starts at each bank-roughness change and wherever ground re-emerges
above the water line — multi-thread sections summing correctly — each
contributing $K_i = (1/n_i)\,A_i R_i^{2/3}$ in SI form, with the effective
hydraulic radius back-computed from total conveyance,
$R = (n_C K / A)^{3/2}$, through the **same** constant in both directions.

A **meander modifier** substitutes the shorter valley length for the
meandering main-channel length as the channel's effective length, inflating
the main-channel roughness by the modifier's square root so friction loss is
preserved. The adjustment is a property of the one transect that declared it.

Evaluation is direct from the survey geometry, which is piecewise-analytic;
no fixed-resolution resampling intervenes.

> **CORRESPONDENCE:** three predecessor behaviours are not carried. Its
> transect reader treats roughness and the survey buffer as section-scope
> state driven by the `NC` line, so a transect not followed by an `NC` line is
> silently left untabulated, surfacing later as an unrelated link error —
> here a transect is complete when its record ends. Its meander adjustment is
> applied to the shared roughness state in place and never restored, so the
> $\sqrt{L}$ inflation compounds into every subsequent transect that inherits
> the channel coefficient — the code's own saved-and-restored discipline,
> applied to the record but not the live variable, shows the intent this
> engine implements. And its conveyance inversion hard-codes a differently
> rounded Manning constant (1.49) than its forward sum (1.486), leaving every
> tabulated transect and street hydraulic radius $(1.486/1.49)^{3/2} \approx
> 0.40\,\%$ low — an intent visible in the code's single named constant, and
> honoured here by using one constant in both directions.
>
> *Source: `transect.c:463` (inverse) against `consts.h:47` + `transect.c:563` (forward).*

### 5.7 Inversions and Characteristic Depths

Three inverse problems recur, and each is specified with a guaranteed
termination:

**Depth from area** inverts $A(y)$ — monotone by construction — by closed
form where §5.2 provides one, by the tables' own inverse where §5.3 defines
them, and otherwise by bracketed root-finding on the monotone relation to a
stated tolerance. A bracketed solve on a monotone function cannot fail;
there is no iteration cap that silently returns a seed.

**Critical depth**, needed at free outfalls and free-fall ends, solves

$$\frac{A^3}{W} = \frac{Q^2}{g}$$

for $y$, with $Q$ the discharge — by exact formula where the section admits
one (rectangular, triangular, parabolic, power), otherwise by bracketed
root-finding on $[0,\,y_{full}]$.

**Example.** A rectangular channel of width $b = 2$ m carrying
$Q = 3$ m³/s has unit discharge $q = 1.5$ m²/s and critical depth
$y_c = (q^2/g)^{1/3} = (2.25/9.80665)^{1/3} = 0.6122$ m; substituting back,
$A^3/W = (2 \times 0.6122)^3 / 2 = 0.91774 = Q^2/g$.

**Normal depth** inverts the section factor,
$\Psi(y_N) = \dfrac{n\,Q}{\sqrt{S_0}}$ in SI form, on the monotone branch
below the section's $\Psi_{max}$ (§5.1); a demand exceeding $\Psi_{max}$ has
no normal depth in the section and reports the section full.

The stated tolerance for all three bracketed solves is a bracket width of
$10^{-6}$ m: the solve stops there — or at adjacent machine numbers,
whichever comes first — and answers with the bracket midpoint. A micron is
three orders below the tightest head tolerance the acceptance criteria read
(§6.4, default $1.524\times10^{-3}$ m), so no quantity downstream of a
characteristic depth can distinguish the answer from the exact root; halving
past it buys digits nothing reads.

A section whose relations are smooth in a natural parameter may solve there
instead, by derivative (Newton) steps confined to a maintained bracket —
any step leaving the bracket is replaced by its midpoint, so termination
stays guaranteed — stopping when successive iterates agree within the same
stated tolerance in depth. Two sections take this route. The **circle**
solves on the filled angle $\theta$, where both characteristic relations
become logarithmically near-linear ($\ln(A^3/W)$ and $\ln(A^{5/3}/P^{2/3})$
are asymptotically affine in $\ln\theta$ at the dry end), so a handful of
iterations answers where uniform halving of the depth bracket needs twenty
— and the iteration needs no inverse trigonometry at all. The **transect**
(§5.6) solves on depth itself: its survey walk yields the derivatives
beside the values — a partly-submerged segment's width and slant grow at
$dx/\Delta z$ and $\ell/\Delta z$, a submerged one's not at all, and the
conveyance sum differentiates sub-section by sub-section as
$K_i^{\,\prime} = K_i\big(\tfrac{5}{3}A_i'/A_i - \tfrac{2}{3}P_i'/P_i\big)$
— so a Newton step costs one walk, the same as the evaluation it replaces
(its normal-depth relation is the conveyance itself, since §5.6 defines
the effective radius through $K$: $\Psi = A R^{2/3} = n_C K$). Piecewise
kinks where the water line crosses a station are what the bracket
safeguard is for.

> **CORRESPONDENCE:** the predecessor's characteristic-depth searches carry
> fixed iteration budgets whose exhaustion returns the initial estimate — a
> non-converged geometry query silently answering with its seed. Bracketed
> solves terminate; the failure mode is removed rather than tolerated.
>
> *Source: `xsect.c` `getYcritEnum()` (25 steps); `findroot.c:17` (`MAXIT 60`) and `:137`, which answers `-1.e20` on exhaustion.*

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
>
> *Source: `enums.h:337–340` — `SF`, `KW`, `DW` as user-selectable methods.*

### 6.2 The Pressurisation Closure

A closed channel flowing full has no free surface, and the equations of §6.1
presuppose one. This engine closes the gap with a single device, applied
uniformly: every closed cross-section carries a narrow hypothetical **slot**
(the *Preissmann slot* of the open-channel literature) above its crown, so
that depth may exceed the crown height and the free-surface equations remain
valid in every regime. There is no separate
surcharge equation branch, no free-surface/pressurised state machine, and no
transition band: one equation set governs everywhere.

The slot is not an arbitrary widening — it is the engine's model of
**pressure-wave celerity**. A slot of width $w$ atop a full channel propagates
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
shape parameter. The slot-modified area integrates the floored width, so the
consistency $\tilde W = \mathrm{d}\tilde A/\mathrm{d}y$ of §5.1 holds
through the crown band — the addition over the true area is negligible, but
it keeps the vertex update and its error estimate well-posed. For $y \ge y_{full}$:

$$\tilde W = w_{slot}, \qquad
\tilde A(y) = A_{full} + w_{slot}\,(y - y_{full}), \qquad
\tilde R = R_{full}.$$

The hydraulic radius holds at its full-pipe value because the slot is
storage, not conveyance: friction in a surcharged pipe is that of the full
pipe.

**Accounting.** Water stored in slots is real stored volume to the continuity
accounts of §11, reported within channel storage; it is bounded by the
celerity choice and vanishes as $c \to \infty$.

**Example.** A circular channel of diameter 1 m has
$A_{full} = \pi/4 = 0.785398$ m². At the default celerity $c = 50$ m/s the
slot width is $w_{slot} = 9.80665 \times 0.785398 / 50^2 = 3.081$ mm, and
each metre of surcharge stores $w_{slot} \times 1\,\text{m} = 0.3923\,\%$
of the full-flow area — the storage artifact the celerity choice bounds.

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
>
> *Source: `dynwave.c:676–689` — the separate "determine if node is EXTRAN surcharged" branch.*

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
non-full, downstream-sloping flow. A closed channel flowing full takes
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
(`min_surface_area`, default 1.167 m², the plan area of a 1.22 m (4 ft)
manhole) —
a physical floor, since a real access structure has real plan area, not a
numerical fiction.

### 6.4 Iteration

Within a trial step the scheme iterates to self-consistency:

1. **∥ Channel phase**: every channel computes its flow update from the
   last iterate's heads. This phase is order-independent by construction and
   is the specification's parallel region; accumulation of channel flows
   into vertex sums is performed in a fixed order regardless of thread
   count, so results are bit-reproducible under any parallelism.
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
>
> *Source: `dynwave.c:145`, `:284`, `:338` — each phase walks `Nobjects[LINK]` in definition order.*

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
   over vertices, not exceeding the allowance
   $\varepsilon_Q = \texttt{continuity\_tol}\,\sum \lvert Q \rvert
   + (\varepsilon_H/\Delta t) \sum_i A_{S,i}$ —
   the relative term (`continuity_tol` default $10^{-3}$) against the
   network's sum of flow magnitudes, plus the mass-equivalent of the head
   tolerance: the flow rate that would move every vertex's head by exactly
   $\varepsilon_H$ over the step, with $A_{S,i}$ each vertex's current
   free-surface storage area.

The first criterion certifies the iterates have settled; the second certifies
that the state they settled at conserves mass. Iteration runs a minimum of 2
and a maximum of `max_trials` (default 8) passes.

The $\varepsilon_H$ term in $\varepsilon_Q$ is what keeps the two criteria
consistent with each other. Criterion 1 accepts iterates whose heads still
move by up to $\varepsilon_H$, so mass closure finer than
$A_S\,\varepsilon_H/\Delta t$ per vertex is below the resolution the head
gate certifies — a gate demanding it is measuring the settled iterates'
own noise, not conservation. Without the term the allowance is purely
relative and collapses with the flow while the residual's noise floor does
not: a network draining at a dry-weather trickle ($\sum\lvert Q\rvert \sim
10^{-3}$ m³/s) is allowed ~1 µL/s of summed residual, rejects every trial
on criterion 2 alone with heads static to $10^{-7}$ m, runs its full
iteration budget twice per step, and pins the entire run at the step floor
under degraded-accuracy warnings — a 36-hour run taking ~134 000 steps of
0.5 s where the fixed user step would take ~2 000. At high flow the
relative term dominates and the gate is unchanged.

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
>
> *Source: `dynwave.c:232–244` — iteration ends on a budget, and the unconverged iterate is kept.*

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
flow, or negligible area are exempt from the Courant term — and so are
**closed channels flowing full**: their $\sqrt{g\tilde A/\tilde W}$ is the
slot celerity $c$ itself, and resolving the slot wave is not the point of the
closure (§6.2) — an un-exempted slot would pin every surcharged network at
the step floor during exactly the events the engine exists to compute. The
predecessor exempts full conduits under both of its surcharge methods; here
the vertex head-rate constraint and the error test still govern surcharge
accuracy. Steps are
real-valued — no quantisation — floored at $\Delta t_{floor}$ = 0.5 s, and
the run opens at the floor.

**Quiescent growth.** Sustained accuracy margin releases the Courant seed:
after three consecutive accepted steps whose error estimates stayed below a
quarter of `routing_err_tol`, $\Delta t_{try}$ may exceed the Courant term —
never $\Delta t_{user}$, $\Delta t_{cr}$, or $2\Delta t_{prev}$ — and the
first rejection, or any estimate above that margin, reinstates it. The
scheme is semi-implicit and iterated, so Courant numbers above 1 are
usable where the dynamics are quiet; this is the mechanism §10.3 offers in
place of the predecessor's steady-state skip, with the error estimate — not
a fixed tolerance on frozen state — certifying the quiet. Long dry-weather
stretches grow to the user step; the first disturbance collapses the step
through the ordinary rejection path.

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
>
> *Source: `link.c:1104–1107` and `:1247–1248` — `LengtheningStep`.*

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
ends contributes a nominal minimum. A channel **closed** by operational
control (§9) is treated exactly as a dry channel: zero flow with the
nominal head derivative retained, while its stored water and surface-area
contributions persist. Flow out of an essentially dry vertex is
clamped to $\pm 2.832\times10^{-6}$ m³/s (the predecessor's $10^{-4}$ ft³/s,
converted) rather than zeroed, and a
user-supplied flow limit, when given, caps $\lvert Q \rvert$ every iteration.

**Flooding and ponding.** A non-ponded vertex whose head would exceed its
ground elevation (plus any surcharge allowance) is pinned there, the surplus
inflow leaving the system as reported flooding. With ponding enabled the
surplus accumulates over the vertex's ponded area — a virtual store whose
head may rise above ground — and drains back as capacity recovers.

### 6.7 Initial Conditions

Depths and flows default to zero. A user-supplied initial channel flow
implies Manning normal depth. A vertex without a supplied depth is seeded
with the average, over connecting links **that carry an initial flow**, of
link end depth plus the link's offset at that end, at non-outfall,
non-storage vertices only — a vertex whose connecting links all start dry
starts dry itself, because an offset alone is geometry, not water; channels
without an initial flow then take the mean of their end-vertex depths.

An **outfall** is seeded from its own boundary condition instead: the depth
that boundary imposes at the initial flows, evaluated exactly as §6.4
evaluates it every step thereafter. A staged boundary is water standing
against the outlet before the run begins, and the channel reaching it holds
that water, so both belong to the opening storage of §11.1. Seeded dry, they
would instead arrive as volume created on the first step, and the flow ledger
would carry the difference for the rest of the run.

A checkpoint restore (§12) bypasses the seeding entirely.

## 7. Structures

Structures are algebraic head–discharge relations spliced into the network
graph (§6.1). Each supplies its flow from the current iterate's vertex state,
its head derivative $\partial Q/\partial H$ to the vertex updates that need
one, and — where stated — an equivalent surface-area contribution to its end
vertices. All relations are evaluated in SI with the exact $g$ of §2.11;
empirical discharge coefficients keep their fitted values. Where a
coefficient is **dimensional** — the weir and outlet coefficients below — the
relation is stated with its SI dimensions here, and the interpretation of the
file's numeric value is the import contract of §14.

> **CORRESPONDENCE:** the predecessor evaluates weir, outlet, and divider
> relations in the user's unit system, so their coefficients silently change
> meaning with the flow-unit selection, and its roadway weir rescales a
> user coefficient by $1/0.552$ under SI — which is nothing but the
> $\sqrt{\text{ft}\to\text{m}}$ dimensional conversion in disguise. This
> engine computes in SI throughout; the same conversions happen once,
> explicitly, at import (§14).
>
> *Source: `link.c:2159` and `:2190`, both commented "for CFS flow units".*

### 7.1 Pumps

A pump's flow comes from its characteristic, in five types plus one
degenerate. Writing $\omega$ for the speed setting, $V_1$ and $y_1$ for the
inlet vertex's volume and depth, and $H_1$, $H_2$ for the end heads:

$$Q = \omega\cdot\begin{cases}
\hat{q}(V_1) & \text{Type 1 — stepwise on wet-well volume}\\
\hat{q}(y_1) & \text{Type 2 — stepwise on inlet depth}\\
q(H_2 - H_1) & \text{Type 3 — the centrifugal characteristic}\\
q(y_1) & \text{Type 4 — an in-line depth profile}\\
q\!\big((H_2 - H_1)/\omega^2\big) & \text{Type 5 — variable-speed Type 3}\\
Q_{in} & \text{ideal transfer}
\end{cases}$$

where $\hat q$ is stepwise lookup — the curve's value at the first point
whose abscissa exceeds the argument — and the continuous types interpolate
linearly. Type 5's head division by $\omega^2$ with the flow then scaled by
$\omega$ is the affinity-law scaling of the rated curve. The head argument is
floored at zero; reverse flow is never admitted; an ideal pump must be its
vertex's only outlet. Types 3 and 5 supply $\partial Q/\partial H$ as the
negated curve slope divided by the speed setting, Type 4 by forward
difference; the stepwise types supply none. Pumps contribute no surface
area to their end vertices.

A pump carries one piece of operational state, the speed setting $\omega$
itself. It begins at the model's initial status — zero for a pump declared
off, one for a pump declared on — and operational control (§9.1) writes it
directly. A pump at $\omega = 0$ passes no flow, and nothing but a write to
$\omega$ starts it again.

Startup and shutoff depths, where given, override that setting each step
before the characteristic is evaluated, and the override is written back to
it, so the pair latches: a running pump whose inlet depth falls below the
shutoff depth takes $\omega = 0$, and a stopped pump whose inlet depth rises
above the startup depth takes $\omega = 1$. A variable speed therefore does
not survive a shutoff-and-restart cycle — the pump resumes at full speed
until control sets it again. A depth left unspecified imposes nothing, which
leaves a pump given neither entirely control-driven.

At a storage inlet vertex — and the virtual wet well a Type 1
pump receives elsewhere — flow is clamped so the vertex cannot be drawn below
empty, $Q \le Q_{in} + V/\Delta t$; at non-storage vertices, a depth-driven
pump whose projected end-of-step depth would go negative falls back to
$Q = Q_{in}$. **This clamp applies to every depth- and head-driven type,
Type 5 included.**

> **CORRESPONDENCE:** the predecessor omits Type 5 from the negative-depth
> fallback — Types 2–4 are protected and the variable-speed type is not, an
> evident oversight from the type's later addition rather than a modelled
> distinction. This engine protects all of them; a Type 5 pump drawing a
> shallow non-storage vertex differs accordingly.
>
> *Source: `dynwave.c:485–497`; `enums.h:424`.*

Pump energy is tallied from the physics, $P = \rho g\,Q\,\Delta H$, without
an efficiency factor, replacing the predecessor's chain of US-unit
conversion constants.

### 7.2 Orifices

An orifice — side or bottom, circular or rectangular, coefficient $C_d$,
optional flap gate — discharges by Torricelli:

$$Q = C_d A_O \sqrt{2 g H_e}$$

with $A_O$ the opening area and $H_e$ the effective head, free-discharge or
differential as the tailwater dictates. A partially open setting recomputes
$A_O$ from the §5 geometry of the opening; an optional open/close rate slews
the setting.

An **unsubmerged inlet** degrades smoothly to weir behaviour below a
changeover head, with the weir coefficient *derived* — not user-supplied — by
requiring the two regimes to agree at the changeover: for a bottom orifice
the changeover is $h_c = (C_d/0.414)(A_O/P_O)$ with $P_O$ the opening
perimeter, collapsing to a sharp-crested weir of crest length $P_O$; for a
side orifice the changeover is the opening height with matching against the
centre-line head, carrying the user's $C_d$ across rescaled by $\sqrt{g}$.
Submergence applies the Villemonte factor
$\big[1 - \big((H_2 - Z_O)/(H_1 - Z_O)\big)^{1.5}\big]^{0.385}$ on the heads
above the crest. A flap gate charges the Armco loss
$\Delta H = (4U^2/g)\,e^{-1.15\,U/\sqrt{H_e}}$, subtracted and re-solved.

For vertex-continuity purposes an orifice stands in as an equivalent short
pipe of length $\max(60.96\ \text{m},\ 2\Delta t\sqrt{g\,y_{full}})$,
contributing surface area to its end vertices — a bookkeeping device adopted
because §6.3's assembled area needs a finite contribution from every wet
link — and supplies the analytic derivative $0.5\,Q/H_e$ submerged,
$1.5\,Q/(H_1 - Z_O)$ as a weir.

### 7.3 Weirs

Weir types and their head–discharge relations, with $C_W$ the discharge
coefficient (dimension $\mathrm{m}^{1/2}/\mathrm{s}$ for the transverse
form; per relation otherwise), $L_e$ the effective crest length, $H_e$ the
effective head, and $\theta$ the notch angle:

$$Q = C_W L_e H_e^{3/2} \quad \text{(transverse)}, \qquad
Q = C_W \tan(\theta/2)\,H_e^{5/2} \quad \text{(V-notch)},$$

$$Q = C_W L_e^{0.83} H_e^{1.67} \quad \text{(side-flow, reverting to the
transverse form under reverse flow)}.$$

The trapezoidal weir is the sum of a rectangular centre and triangular ends
with **two independent coefficients**, the second applying to the end
sections alone. Effective crest length subtracts end contractions,
$L_e = L - 0.1\,n_c H_e$ floored at zero — a heavily contracted weir under
high head stops flowing rather than reversing. A partially raised crest turns
a V-notch into a trapezoid. Submergence applies Villemonte with the type's
own head exponent, except that a trapezoidal weir's end sections always take
the V-notch exponent. Each type admits exactly one cross-section shape, per
§2.7; any other is rejected at validation.

Weirs are **surchargeable** by default (the roadway weir excepted): above the
opening they switch to an equivalent-orifice form $Q = C_O\sqrt{H_e}$, the
coefficient fixed by evaluating the weir equation at a head equal to the full
opening height and dividing by $\sqrt{h/2}$, and the orifice form driven by
the head measured to the opening's mid-height — the centre-line convention
that makes the regimes agree at the changeover. A weir with surcharge
disabled caps its head at the opening height and continues weir-equation
flow. Weirs contribute an equivalent-pipe surface area exactly as orifices
do, and supply the analytic exponent-scaled derivative per type.

### 7.4 Outlets

An outlet's flow is an arbitrary function of upstream depth or head
difference — a power relation $Q = a H_e^{b}$ or a tabulated rating —
scaled by the setting, with flap-gate reversal blocking. The coefficient $a$
is dimensional whenever $b \neq 1$; its file interpretation is §14's.

### 7.5 Flow Dividers

Under the full dynamic treatment a divider is an ordinary junction: the
momentum equations determine the split, as they do in the predecessor's own
dynamic mode. The divider's prescribed split rules are semantics of the
reduced routing forms and travel with them to the import contract (§14).

### 7.6 Culverts and Roadway Weirs

A channel designated a **culvert** by an FHWA HDS-5 code receives an
inlet-control capacity check layered on the ordinary §6 solution:
unsubmerged flow from the form-1 critical-energy equation or the form-2
power law per the code's published form, submerged flow from the quadratic
HDS-5 relation, a linear transition between, and the smaller of the
inlet-control and dynamic solutions governs. The 57 published inlet
configurations (form, $K$, $M$, $c$, $Y$) are adopted verbatim as FHWA data.
Slope corrections take their published magnitudes: $-0.5\,S_O$ on the
headwater ratio for ordinary inlets and $+0.7\,S_O$ for mitered ones, in
HDS-5's sign convention.

> **CORRESPONDENCE:** the predecessor codes the mitered slope correction at
> ten times its published magnitude (its convention's $-0.7\,S_O$ entered as
> $-7.0$, in a line whose own comment reads "-7 for mitered inlets", so it is
> written down rather than mistyped), so a mitered culvert on any appreciable
> slope carries an
> order-of-magnitude overcorrection. This engine uses the published value;
> mitered-culvert models differ accordingly, in this engine's favour against
> the standard the feature claims to implement.
>
> *Source: `culvert.c:204–210`, applied at `:218`, `:301`, `:351`.*

A **roadway weir** applies the FHWA head-dependent coefficient when road
width and surface are given (otherwise the user's constant), from the
digitised low-head and submergence tables, with submergence factors floored
at their published minima. It pairs in parallel with a culvert to model
embankment overtopping.

### 7.7 Channel Losses and Force Mains

**Minor losses** (entrance, exit, average — velocities evaluated at their
respective locations) enter the §6.3 denominator as
$\frac{\Delta t}{2L}\sum K_{m,i}\lvert U_i\rvert$.

**Channel evaporation and seepage** are uniformly distributed lateral
losses, $q_E = e_t\,W(\bar y)$ and $q_S = s\,f_c\,W^{*}(\bar y)$ — the
seepage width capped at the depth of maximum width, seepage being vertical —
bounded by the channel's volume per step. The momentum equation gains
Strelkoff's lateral-outflow term, entering the §6.3 numerator as
$+2.5\,\bar U q_L \Delta t/L$ with $L$ the channel's true length; the lost
volume debits the appropriate vertex.

**Storage units** evaporate at the potential rate times a realisation
fraction (default 0 — an unconfigured unit does not evaporate) applied to
the start-of-step surface area, and seep through bottom and sloped-side
areas separately: a saturated conductivity alone gives a constant rate; the
full suction/conductivity/deficit triple invokes the Green–Ampt relation of
§3. The bottom sees the ponded depth, the banks the half-depth convention
above the elevation where the storage geometry begins widening. **Seepage
geometry is defined for every storage shape of §2.6**, the elliptical
paraboloid included.

> **CORRESPONDENCE:** the predecessor's exfiltration initialiser has no case
> for the elliptical paraboloid — added to storage geometry after
> exfiltration was written — and no default over an unzeroed allocation, so
> a paraboloid unit with seepage reads uninitialised geometry. The evident
> intent of covering every shape is implemented here.
>
> *Source: `exfil.c:93–151` and `:234`; `enums.h:402`.*

**Force mains** — circular, pressurised — substitute their friction relation
for Manning's while full: Hazen–Williams, or Darcy–Weisbach with the
Swamee–Jain friction factor (laminar $64/Re$ below 2000, a linear blend to
turbulent between 2000 and 4000, the fully rough form at extreme $Re$).
Partly full, an equivalent Manning coefficient applies, per the
predecessor's published fits converted to SI. A force main's section factor
carries the Hazen–Williams exponent ($A R^{0.63}$), which alters its
normal-flow limit accordingly. The predecessor's force-main lengthening
compensation lapses with the transform it compensated for (§6.5).

### 7.8 Streets and Inlets

Dual drainage pairs street cross-sections (compiled to §5.6 transects, so
street channels route as ordinary channels) with HEC-22 inlet capture. The
HEC-22 relations — gutter spread, frontal-flow ratio with its fixed-point
solve for depressed gutters, grate frontal and side efficiencies, curb
full-capture length, and the on-sag weir/orifice forms with their published
transition depths — are adopted as the published standard defines them, with
their fitted coefficients intact and $g$ exact per §2.11. Inlet families
(seven standard grates plus generic, three curb-throat geometries, slotted
drains, drop inlets, custom-curve inlets, and the implicit combination),
placement shape-checks, replicate/clogging/cap/local-depression modifiers,
`AUTOMATIC` on-grade/on-sag resolution, and the capture-transfer semantics —
captured flow moving from bypass vertex to sewer vertex each routing step
carrying the bypass concentration, surcharge returning as backflow
apportioned by open-area ratio among standard inlets and by count among
custom ones — are model semantics and are adopted
exactly. On-grade capture is computed from the gutter-spread relation at the
channel's longitudinal slope, as HEC-22 defines it; the method is inherently
insensitive to backwater, which is a property of the standard, stated rather
than obscured.
