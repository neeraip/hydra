# Urban Drainage — Overland Flow Specification

This document holds §15 of the urban drainage specification: two-dimensional
overland flow on a surface mesh, and its couplings to the §6 network, the
§3 meteorology, and the §11 conservation ledger.

---

## 15. Overland Flow

### 15.1 Scope and Stance

The engine simulates free-surface flow over a two-dimensional ground
surface: rainfall and network surcharge spreading across terrain, ponding
in depressions, conveying along streets and floodplains, and returning to
the network or leaving the domain across its boundary. The governing
physics is the two-dimensional shallow-water system in its **local-inertial
approximation**: continuity is kept exactly, and the momentum equation
keeps its local acceleration, pressure-gradient, and friction terms —
the retained inertia is what names the scheme — while the convective
acceleration is omitted by default. The approximation is standard for
flood-spreading problems (Bates, Horritt & Fewtrell 2010; de Almeida &
Bates 2013), where depths are shallow, slopes are mild, and friction
dominates momentum; it is *not* a model of momentum-dominated flow, and
§15.4.6 states the consequences plainly.

**Correspondence.** The predecessor for this section is the community
continuation of SWMM — `openswmm.engine`, whose 6.x line grows
two-dimensional simulation on the SWMM input format. Its 2D subsystem is
pre-release: this specification is written against its explicit
local-inertial marcher (the sole 2D integrator since its implicit
integrators were retired, 2026-07-29), and correspondence notes cite that
implementation. The stance follows §14.1: the *input format* is adopted
absolutely, section for section and default for default, so a model
authored for the predecessor means the same thing here; the *internal
laws* are specified on their own terms, and every difference a user could
observe in results is recorded where it arises. Because the predecessor
is an alpha, §15.10 records the format-stability boundary: the mesh,
boundary-condition, and coupling-map sections are treated as stable; the
options vocabulary is expected to churn, and unknown or retired option
keys warn and are ignored rather than refusing the file.

**Placement in the cascade.** Overland flow joins §1.1's per-step
influence cascade as a peer of the network: within a routing step the
network solves with the overland surface's state frozen, then the surface
advances with the network's state frozen, and the volumes they exchange
are booked exactly once (§15.6). The influence graph within a step
remains loop-free.

**Units.** All overland quantities are SI internally, as everywhere in
this engine: metres, square metres, cubic metres per second, and
$g = 9.80665\ \text{m/s}^2$. Input-side unit handling is §14.15's.

> **CORRESPONDENCE:** the predecessor's 1D engine computes internally in
> feet while its 2D subsystem computes in SI, so every coupling quantity
> crosses a hard-coded ft↔m boundary (a historical defect had tied those
> factors to the project's flow units, corrupting SI-project coupling).
> This engine is SI throughout, so no such boundary exists to get wrong.

### 15.2 The Mesh

The surface is an **unstructured triangular mesh**: vertices carry a
position and ground elevation $(x, y, z)$; cells are triangles over three
vertex indices, each carrying a Manning roughness $n > 0$, an optional
initial depth $h_0 \geq 0$ (default 0, dry), and an optional identifying
tag. The mesh is planimetric: areas and lengths are measured in plan, and
no terrain steeper than its planimetric projection is represented.

**Local edge convention.** Cell edge $e \in \{0,1,2\}$ is the edge
*opposite* local vertex $e$: for a cell $(v_0, v_1, v_2)$, edge 0 joins
$(v_1, v_2)$, edge 1 joins $(v_2, v_0)$, edge 2 joins $(v_0, v_1)$.

**Topology.** Adjacency is discovered, not declared: two cells sharing a
vertex pair are neighbours across that edge; an edge no second cell
claims is a **boundary edge**, and the set of boundary edges is the whole
of the domain-boundary declaration. Boundary conditions (§15.5) attach to
specific (cell, edge) slots; every boundary edge not so addressed is a
wall.

**Derived geometry.** Cell centroid $\mathbf{c} = $ the vertex mean (in
$x$, $y$, and $z$; the elevation mean $\bar z$ is the cell's bed datum for
the flat closure); cell area $A = \tfrac12 \lvert (\mathbf{v}_1 -
\mathbf{v}_0) \times (\mathbf{v}_2 - \mathbf{v}_0) \rvert$ (orientation
insensitive; winding order is not required of the input). Each edge
carries its planimetric length $\xi$, midpoint, outward unit normal
$\hat{\mathbf{n}}$ (perpendicular to the edge, oriented away from the
centroid), a face bed $z_f = \max(\bar z_L, \bar z_R)$ over the two
incident cells, the sorted endpoint beds $z_{lo} \leq z_{hi}$, and the
inverse normal distance

$$\frac{1}{d_n} = \frac{1}{\max\!\big(\lvert(\mathbf{c}_R -
\mathbf{c}_L)\cdot\hat{\mathbf{n}}\rvert,\ 0.3\,\xi\big)},$$

the floor guarding slivers whose centroids nearly share the edge line.

**Edge conveyance.** Each interior edge carries a conveyance factor
$\psi \in \lbrack 0, 1\rbrack$ (default 1), a sub-grid representation of walls, fences
and berms: it multiplies the edge's flux (§15.4.2). The factor is a
property of the undirected edge — both incident cells see the same
$\psi$ — so antisymmetry of the flux, and with it conservation, is
unaffected.

> **CORRESPONDENCE:** the predecessor parses, stores, and documents this
> factor as complete, but its current marcher never reads it — the only
> consumer was its retired implicit solver, so the factor is inert
> there. This engine applies it as documented. A model relying on
> conveyance barriers computes differently here, and correctly.

**Validation.** A mesh is refused (typed, per §1.8) unless: at least
three vertices and one cell; all vertex indices in range; no repeated
vertex within a cell; every cell area strictly positive; every Manning
$n$ strictly positive; every initial depth non-negative; every $\psi$
within $\lbrack 0, 1\rbrack$.

### 15.3 Stage–Storage Closures

The **conserved variable is the cell's water volume** $V$ (m³). Free
surface elevation $\eta$ and mean depth $\bar h = V / A$ are derived from
$V$ through a closure, re-evaluated whenever $V$ changes, and never
integrated themselves.

**FLAT** (default): the cell floor is flat at the centroid bed,

$$\eta = \bar z + \bar h.$$

**VFR** (volume–free-surface relation): the exact stage–storage relation
of a planar bed through the cell's three vertex elevations (Begnudelli &
Sanders 2006, 2007). With sorted elevations $z_1 \leq z_2 \leq z_3$ and
$\bar z = (z_1{+}z_2{+}z_3)/3$:

$$\bar h(\eta) = \begin{cases}
\dfrac{(\eta - z_1)^3}{3\,(z_2 - z_1)(z_3 - z_1)} & z_1 < \eta \leq z_2\\
(\eta - \bar z) + \dfrac{(z_3 - \eta)^3}{3\,(z_3 - z_1)(z_3 - z_2)} & z_2 < \eta \leq z_3\\
\eta - \bar z & \eta \geq z_3
\end{cases}$$

with the **wet-area fraction** $\mathrm{d}\bar h/\mathrm{d}\eta$ equal to
$(\eta - z_1)^2 / \big((z_2 - z_1)(z_3 - z_1)\big)$ on the lower branch and
$1 - (z_3 - \eta)^2 / \big((z_3 - z_1)(z_3 - z_2)\big)$ on the middle one.
A fully wet cell reduces exactly to the flat closure. The inverse
$\eta(\bar h)$ is closed-form (a cube root) on the lower branch and a
safeguarded bracketed iteration on the middle one, and the pair are exact
inverses of each other so state may round-trip without drift. The
relation is **regularised** by a wet-fraction floor $\varepsilon$
(`VFR_MIN_WET_FRAC`, default 0.01): below the stage where the wet
fraction reaches $\varepsilon$ the relation continues as its tangent with
slope $\varepsilon$, keeping the closure $C^1$ and monotone as a cell
dries. A near-flat cell (relief below $10^{-9}$ m) uses the flat closure
outright.

The flat closure overstates a partially wet cell's surface by up to two
thirds of the cell's relief — water appears to climb the cell's high
corner — and with it the well-balancing of §15.4.6 at shorelines is
approximate rather than exact. VFR is the physically correct closure and
is deliberately *not* the default: it steepens the shoreline dynamics and
costs a multiple of the substeps, and the default follows the
predecessor so that results and costs correspond. The choice is the
model's, per §14.15.

**Face depth.** The depth that conveys across an edge, under
`FACE_RECONSTRUCTION`:

- **MEAN** (default): $h_f = \max(\eta_L, \eta_R) - z_f$ — the higher
  water surface over the higher cell bed.
- **VFR_FACE**: the wetted-edge mean depth of $\eta = \max(\eta_L,\eta_R)$
  over the edge's own endpoint beds (Begnudelli & Sanders 2007, eq. 14):
  $0$ for $\eta \leq z_{lo}$; $(\eta - z_{lo})^2 / \big(2(z_{hi} -
  z_{lo})\big)$ for $z_{lo} < \eta \leq z_{hi}$; $\eta - (z_{lo} +
  z_{hi})/2$ above — $C^1$ at both joins.

### 15.4 The Marcher

Overland state advances by an explicit two-phase march: a **∥ face
phase** updates every edge's discharge from the published cell surfaces,
then a **∥ cell phase** applies every cell's accumulated fluxes and
sources and republishes its surface. The phases alternate under the
time-step control of §15.4.4.

#### 15.4.1 State

Each interior edge carries one prognostic unit-width discharge $q$
(m²/s), oriented from the lower-indexed to the higher-indexed incident
cell, and two pending-mass accumulators (m³), one per side. Each cell
carries its volume $V$, its derived $\eta$ and $\bar h$, a reconstructed
velocity proxy $\mathbf{q}_c$ (§15.4.3), and its source rates. Boundary
edges carry their own prognostic discharge (§15.5).

#### 15.4.2 ∥ Face phase

Every due edge computes its new discharge from the last published cell
surfaces. The face depth $h_f$ comes from §15.3; if $h_f \leq h_{dry}$
(`DRY_DEPTH`, default 0.001 m) the edge is a wall this substep: $q
\leftarrow 0$, nothing books. Otherwise, with surface difference $\Delta\eta
= \eta_R - \eta_L$ (a deadband $\lvert\Delta\eta\rvert < 10^{-12}$ m is
taken as zero, so closure round-off cannot masquerade as slope), slope
$S_f = \Delta\eta / d_n$, and the θ-blended semi-implicit update (de
Almeida & Bates 2013; $\theta = 1$ recovers Bates 2010):

$$\hat q = \theta\,q + (1-\theta)\ \tfrac12(\mathbf{q}_{c,L} +
\mathbf{q}_{c,R})\cdot\hat{\mathbf{n}}$$

$$q^{+} = \frac{\hat q - \Delta t\,\big(g\,h_f\,S_f + a\big)}
{1 + g\,\Delta t\, n_f^2\, \lvert\mathbf{q}\rvert_{\max}\ /\ h_f^{7/3}}$$

where $n_f^2 = \big(\tfrac12(n_L + n_R)\big)^2$, and
$\lvert\mathbf{q}\rvert_{\max} = \max\big(\lvert q \rvert,
\lvert\tfrac12(\mathbf{q}_{c,L}+\mathbf{q}_{c,R})\rvert\big)$ is the
**flow-vector magnitude**, floored at the face's own $\lvert q\rvert$.
The friction denominator must use the vector magnitude: using only the
face-normal component under-damps the transverse component and corrugates
steady sheet flow. $a$ is the optional convective term (§15.4.6),
zero by default. The result is clamped by the **Froude cap**,

$$\lvert q^{+} \rvert \leq \mathrm{Fr}_{\max}\, h_f \sqrt{g\,h_f},$$

$\mathrm{Fr}_{\max}$ default 1.5 — a numerical device, not physics
(§15.4.6) — and by **positivity**: the exporting cell grants each of its
three edges at most a $\beta/3$ share of its volume per cell cycle
($\beta = 0.8$), divided by the number of times the face fires per cell
cycle at a tier interface (§15.4.4); a discharge that would overdraw the
share is scaled down. The edge conveyance $\psi$ multiplies the final
flux.

The edge then books the mass it moved, $\Delta M = \psi\, q^{+} \xi\,
\Delta t_f$, into its two side accumulators with opposite signs. Each
edge is the **single writer** of its own discharge and its own two
accumulators, so the phase is order-independent and byte-reproducible
under any §6.4 width; both sides book the *same* floating-point product,
so interior conservation is exact by construction, tier interfaces
included.

#### 15.4.3 ∥ Cell phase

Every due cell drains its own side of each incident edge's accumulator
(in fixed ascending edge order), applies its sources, and clamps at dry:

$$V^{+} = \max\Big(0,\ V + \textstyle\sum_e \Delta M_e + \Delta t_c\,
\big(r + c - E\big)\,A\Big)$$

where $r$ is the cell's rainfall rate (§15.7), $c$ its coupling rate
(§15.6), and $E$ its evaporation demand under a $C^1$ ramp
$t^2(3-2t)$, $t = \bar h / h_{dry}$, so evaporation shuts off smoothly as
the cell dries rather than chattering at the threshold. The closure then
republishes $(\eta, \bar h)$, and the cell's velocity proxy is
reconstructed by the Perot formula over its own edges, in fixed order:

$$\mathbf{q}_c = \frac{1}{A} \sum_e s_e\, q_e\, \xi_e\,
(\mathbf{m}_e - \mathbf{c}),$$

$s_e$ the cell's sign for the edge's orientation and $\mathbf{m}_e$ the
edge midpoint. The offset $\mathbf{m}_e - \mathbf{c}$ is the formula:
substituting the face normal — plausible on a near-orthogonal mesh —
was measured to destabilise the march outright, the θ-blend feeding on
its own mis-reconstruction until the basin stood in metre-scale waves. Each cell writes only its own state and clears only its
own accumulator sides: the phase is order-independent and
byte-reproducible at any width.

#### 15.4.4 Time stepping

The stable step of cell $i$ is the CFL bound of the discrete operator,

$$\Delta t_i = \alpha\ \frac{L_i}{\sqrt{g \bar h_i} +
\lvert \mathbf{u}_i \rvert}, \qquad
L_i = \sqrt{\frac{2 A_i}{\sum_e \xi_e / d_{n,e}}},$$

$\alpha$ = `CFL_NUMBER` (default 0.7, with $\alpha = 1$ the linear
stability limit of the scheme on this mesh metric), $\lvert\mathbf{u}\rvert
= \lvert\mathbf{q}_c\rvert / \bar h$, capped by `MAX_TIMESTEP` (default
10 s). The base step $\Delta t_0$ is the minimum over active cells.

**Local time stepping.** Cells are assigned to power-of-two tiers, tier
$k$ firing every $2^k$ base substeps with $\Delta t = 2^k \Delta t_0$, up
to `LTS_TIERS` tiers (default 4; 1 disables the mechanism). A face fires
at the *finer* of its two cells' cadences, and the positivity share of
§15.4.2 divides by the face's firings per cell cycle, so a coarse cell
cannot be overdrawn by a fine face. Boundary-condition and coupling cells
are pinned to tier 0. Tiers and the active set rebuild every fourth macro
cycle; between rebuilds the base step may only tighten (a frozen step has
been observed to let a seiche grow at $\alpha = 0.7$). Re-tiering
invalidates the positivity bookkeeping of any in-flight accumulator (a
face's take count restarts against an exporter volume that has not yet
absorbed the pending takes, and the doubled grant has been measured to
overdraw cells into the positivity floor, leaking volume), so every
pending accumulator side is gathered into its cell, and the cell's
closure re-evaluated, immediately before tiers are reassigned — at every
rebuild and before a tail collapse. A face walled by deactivation
surrenders its discharge at the rebuild: stale momentum must not survive
re-activation. A window tail too
short for a full macro cycle collapses every cell to tier 0 and lands
exactly on the target time: **an advance always reaches its target** —
there is no rejection or retry in this subsystem, and §15.4.6 records why
none is needed.

**Wetting and drying.** A cell participates in the march only while
**active**. Activation carries hysteresis about `H_MOVE` (default
0.003 m) with band $\min(1\ \text{mm}, h_{move}/2)$: a cell activates
above $h_{move} + \text{band}$ and deactivates below $h_{move} -
\text{band}$. Rain alone never activates a cell: inactive cells
integrate their sources lazily as pure storage between rebuilds, and
enter the march only when their depth crosses the threshold. A one-ring
halo of the active set is kept active so an advancing front always has a
receiving cell, and a face conveys only when **both** its cells are
active (one-sided conveyance has been measured to lose basin volume).
Cells with coupling points or non-wall boundary edges are always active.

**The published picture.** An advance returns with every cell at the
target time, but a prognostic face discharge was last clamped against
the surface it saw at its own firing, and cell firings since have moved
that surface: a draining front would otherwise expose a super-Froude
discharge inconsistent with the published depths. The observable face
discharge is therefore re-limited on reading: the face flow depth is
re-evaluated from the published surfaces (§15.3), a face at or below
the drying depth reads zero, and the Froude cap of §15.4.2 is
re-applied. The prognostic value is untouched; the re-limit is a
reading, not an integration step.

#### 15.4.5 Concurrency

The ∥ marks above are the §1.7 grant: the face phase and the cell phase
may run across the §6.4 worker width, and their writes are per-edge and
per-cell disjoint with fixed-order gathers, so results are
byte-identical at every width — the same contract §6.4 carries.
Boundary-edge evaluation (§15.5) and network exchange (§15.6) are
specified **sequential**, in file order; they are small, and their shared
ledgers make order part of the result.

#### 15.4.6 Validity, stated plainly

The scheme omits convective acceleration by default. Consequences: a
transcritical control has no drawdown (the frictionless steady state of
the truncated momentum equation is a *flat* surface); dam-break fronts
are first-order; results pinned at the Froude cap are the cap's, not
physics. An optional Stelling–Duinmeijer convective term (`ADVECTION`,
default off) adds the upwind momentum flux difference $a = (u_R \hat
q_R - u_L \hat q_L)/d_n$ between wet cells, with $u_c =
(\mathbf{q}_c\cdot\hat{\mathbf{n}})/\bar h_c$ and the upwind flux taken
from the donor cell; it improves transcritical cases and remains
off-by-default until its validation grades are settled (§15.9). Momentum
stored on a face is discarded when the face gates dry — a known,
accepted dissipation at wetting fronts. A model whose answer depends on
momentum-dominated hydraulics is outside this section's validity, and
the specification says so rather than approximating silently.

No error control governs this subsystem — the explicit CFL bound is the
accuracy control, unlike §6.5's error-steered stepping — because the
scheme is everywhere first-order in time at a step the stability bound
already holds near the accuracy scale.

### 15.5 Domain Boundaries

Every boundary edge is a **wall** (zero flux) unless a boundary
condition addresses its (cell, edge) slot. Five types:

| Type | Law |
|---|---|
| `WALL` | $F = 0$ |
| `NORMAL_FLOW` | Manning outfall at the authored bed slope $S$: $F = -\dfrac{h^{5/3}\sqrt{S}}{n}\,\xi$ |
| `SPECIFIED_FLOW` | $F = -q_{bc}\,\xi$, $q_{bc}$ the authored (or time-series) per-metre discharge |
| `RATING_CURVE` | as `SPECIFIED_FLOW` with $q_{bc}$ read from a curve of stage above the edge bed — the edge's sill, its higher endpoint elevation, the §15.3 face datum — interpolated linearly and held at its end values beyond its range, re-read every firing because stage is state, not time |
| `SPECIFIED_STAGE` | the §15.4.2 momentum law against a ghost cell held at the authored (or time-series) stage, slope arm $d_n = 2A/(3\xi)$, the cell's own $n^2$, prognostic discharge |

$h$ is the cell-mean depth under `MEAN` reconstruction and the wetted-edge
depth under `VFR_FACE`. A time-series stage or flow resolves to a value
per advance at the wiring layer; until its first resolution the slot is
a wall, the refusal-safe reading of an unresolved parameter. All boundary applications clamp in volume
space — a cell cannot be driven negative, and one substep of a stage
boundary moves its cell at most **to** the prescribed stage, from
either side — and the flux booked to the §15.8 ledger is re-derived
from the volume actually applied, so booking and application agree
exactly. The prognostic discharge is likewise re-derived from the
applied flux: a clamped exchange that kept its unclamped momentum
would wind up and drive the basin as an oscillator instead of settling
it. A cell fed through a boundary edge also carries that edge's
discharge in its §15.4.3 velocity reconstruction, in the same
outward-flux convention — reconstructed from interior faces alone, a
boundary-fed cell's θ-blend drags every interior exchange by the
reconstruction's missing share. Boundary edges evaluate at tier-0
cadence, sequentially.

> **CORRESPONDENCE:** the predecessor's earlier diffusive stage-boundary
> law left every boundary-driven steady case one velocity head above the
> prescribed stage; its current inertial ghost-cell law, adopted here,
> removed that offset.

### 15.6 Network Coupling

The overland surface and the §6 network exchange at **coupling points**:
authored mappings from a mesh vertex or cell to a network node (§14.15).
Nothing couples implicitly — an unmapped node never exchanges. A vertex
mapping conveys through the vertex's incident-cell stencil, collapsed
for junction exchange to its lowest-bed cell: that is where water
actually pools, and a single source cell is what the drain cap is
written against — both directions of the orifice exchange apply there.
Outfall injection (network→surface, below) instead spreads across the
stencil, weighted by the surface slope from the vertex down toward each
cell ($w_k = \max(0, (\eta_v - \eta_k)/d_k)$, $d_k$ the distance from
the vertex to the cell centroid, $\eta_v$ the wet-depth-weighted mean
surface of the vertex's wet incident cells, or the vertex ground
elevation when all are dry), falling back to area weights on a flat or
dry surface. Two rows resolving to one vertex are one coupling, the
last authored winning (§14.15).

**Junction exchange** is one orifice law for both directions. With
$\Delta h = \eta_{2D} - h_{1D}$ (the overland surface against the node's
hydraulic grade, both in metres of the shared datum):

$$Q = C_d\, A_{\!e}\ \mathrm{sign}(\Delta h)\, \sqrt{2g}\
\varphi(\lvert \Delta h \rvert) \cdot G_{rim} \cdot R_{wet}$$

- $\varphi(x) = \sqrt{x}$ for $x \geq \varepsilon_h$ ($\varepsilon_h$ =
  0.02 m); below, the $C^1$ quadratic
  $\varphi(x) = \tfrac{3}{2\sqrt{\varepsilon_h}}\,x -
  \tfrac{1}{2\varepsilon_h^{3/2}}\,x^2$ matching value and slope at
  $\varepsilon_h$ — the infinite slope of the bare square root at
  $\Delta h = 0$ is exactly the stiffness that makes near-equal heads
  oscillate, and the regularisation removes it.
- $G_{rim}$: a $C^1$ smoothstep opening over the 50 mm above the node's
  **rim** — the ground elevation, invert plus the node's full depth —
  evaluated at $\max(\eta_{2D}, h_{1D})$: exchange exists only when
  either side reaches the ground. Street drainage into a node whose
  water stands below its rim is **not modelled** (recorded in §15.10).
  The predecessor's source calls this elevation the node's "crown"; the
  value is the rim, and this specification says rim, because §6 already
  uses *crown* for a conduit soffit and the two must not be confused.
- $R_{wet}$: the §15.4.3 cubic ramp on the source side's depth, so a
  draining film shuts off smoothly.
- $A_{\!e}$: the authored exchange area, ramped from $1\times$ to
  $2\times$ over the first 50 mm above ground. Under `COUPLING_AREA
  AUTO`, unauthored areas derive from the node's largest connected
  conduit: $\mathrm{clamp}(1.25\,A_{conduit},\ 0.05,\ 2.0)$ m².

Positive $Q$ drains the surface into the node; negative spills the node
onto the surface. Two hard caps bound the exchange: a drain takes at
most a $\beta$ share of the source cell's volume per substep, and a
spill draws against the node's stored volume through a per-node ledger
spanning the whole advance — the same water cannot spill twice within
one routing step.

**Damping into the network iteration.** The exchange enters the §6.4
vertex update as a lagged source, and a lagged source with a steep
head-sensitivity destabilises the iteration it feeds. The exchange
conductance $G = -\partial Q / \partial h_{1D} \geq 0$ (the ramp and gate
derivatives dropped, so $G$ only ever adds damping) therefore augments
the vertex's storage term: the §6.4 update at a coupled vertex uses
$A_{\!s} + G\,\Delta t$ in place of its surface area $A_{\!s}$. A coupled
vertex additionally reports the median-dual footprint of its surrounding
cells ($\sum A_i / 3$ over the vertex's incident cells) as its ponded
area, and is ponding-capable **regardless of the model's global ponding
option** — §6.6's gate does not apply at a coupled vertex, because the
pond is not a modelling choice there: it is the overland surface itself,
and the network's grade must track it.

> **CORRESPONDENCE:** the predecessor overrides the node's ponded area
> the same way and documents the consequence: the near-node storage is
> counted on both sides of the coupling. This engine adopts the
> mechanism and the acknowledgement; removing the double count is a
> recorded refinement, not an implementation liberty.

**Outfall coupling** is asymmetric. Surface→network: a coupled outfall's
boundary stage reads the surface (the deepest wet stencil cell's
surface, blended in by a wetness ramp; a dry stencil leaves the authored
stage), evaluated inside each network iteration; a flap gate suppresses
the override when the surface stands above the stage the outfall would
otherwise carry. The ramp is keyed on the depth in excess of the drying
depth, $t = (d - h_{dry})/h_{dry}$ smoothstepped: a draining cell comes
to rest holding a film at about the drying depth, and a ramp keyed on
the raw depth would read that immovable film as wet and deadlock the
outfall at its own bed (a measured predecessor failure).
Network→surface: the outfall's discharge over the step is accumulated in
volume, capped against a per-cell budget frozen at the step's start, and
injected into the surface as a constant rate over the following step.

**Timing.** By default the surface co-advances **once per §10 routing
period** — the model's routing-step clock, not §6.5's adaptive steps,
which subdivide the period on their own error control: the network
advances the period with the surface frozen, then the surface advances
the same interval with the node grades frozen, then the exchanged
volumes queue as network lateral inflow delivered over the next period.
`COUPLING_SYNC` widens the batch in elapsed time (clamped to the band from one routing
period to 60 s) for models where the surface advance dominates; batching
delays delivery by one batch, and the predecessor's measurements record
fill-and-spill ponds turning into overshoot sawtooth at wide batches —
which is why per-period is the default. Exchange and boundary
evaluation happen at tier-0 cadence within the advance, sequentially.

### 15.7 Meteorology on the Mesh

**Rainfall** reaches cells directly, under `RAINFALL_MODE`:

- `NATURAL_NEIGHBOUR` (default): gauge intensities interpolate to cell
  centroids by Laplace (non-Sibsonian) natural-neighbour weights within
  the convex hull of the located gauges, inverse-distance-squared
  outside it. Gauge positions come from the model's display symbols —
  the one place a display section acquires engine semantics, recorded
  as §14.15's amendment to §14.5's rule — and gauges without a position
  are excluded. The weights reproduce linear
  fields exactly inside the hull and are precomputed once. Degenerate
  gauge sets fall back: one gauge serves everywhere; collinear or paired
  gauges serve by inverse distance; no located gauge falls back to
  `SYSTEM`.
- `SYSTEM`: the arithmetic mean of all gauges, uniformly.
- `NONE`: no rain on the mesh — the mode for models whose parcels
  already capture the storm, where rain-on-mesh would count the same
  water twice.

A model carrying both parcels (§3) and rain-on-mesh over the same
footprint double-counts that rainfall; import warns (§14.15), and
nothing subtracts one from the other. Parcel runoff does **not** flow
onto the mesh (§15.10): parcels drain to nodes exactly as without a
mesh, and reach the surface only through a coupled node's exchange.

**Evaporation** applies the §3 potential rate through the §15.4.3 ramp.

**Infiltration** is the initial-loss/continuing-loss model of
established two-dimensional practice, per cell and default zero
(an unlisted cell is impermeable, which is the right reading of an
urban mesh):

- The **initial loss** $IL$ (m of depth) is a one-time absorbing
  capacity: the first $IL \cdot A$ cubic metres of water on the cell are
  taken, whatever their source — rain, inundation, a spill — and the
  remaining capacity is state, drawn down monotonically over the run
  and never restored.
- The **continuing loss** $CL$ (m/s) is a constant rate while the cell
  is wet, applied through the §15.4.3 ramp so a drying film shuts it
  off $C^1$, and capped at what the cell holds.

Losses apply in a fixed order at each cell firing — initial loss, then
continuing loss, then evaporation, each from what remains — and
identically in the lazy path for inactive cells. Both book to the
§15.8 ledger as infiltration out.

> **CORRESPONDENCE:** the predecessor's 2D engine has no infiltration
> at all; the correspondence here is to the wider field's standard
> (TUFLOW's IL/CL soil model), adopted because it is what practising
> modellers calibrate against. A soil-column model (Green–Ampt with
> recovery) remains a recorded absence (§15.10): the initial loss never
> recovers, which is the conservative reading for single-event runs and
> wrong for multi-week ones.

### 15.8 Conservation

The overland ledger carries, in cubic metres over the reporting window:
initial and final surface storage, rainfall in, evaporation out,
infiltration out, exchange in and out with the network (junctions and
outfalls separately), and boundary in and out. Interior conveyance never appears:
each interior edge books one antisymmetric mass pair, so interior
transport conserves exactly, at every tier interface, by construction —
and the continuity report measures only what crosses the subsystem's
boundary. Every exchange term books the volume *actually applied* after
its caps and clamps, never the volume requested.

On the network side, the exchange enters §11's ledger as its own named
pair — surface drainage (in) and surface spill (out) — rather than
folding into existing terms.

> **CORRESPONDENCE:** the predecessor books 1D→2D spill into its node
> flooding total and 2D→1D drainage into external inflow, so a coupled
> run's "flooding" silently includes water the surface later returned.
> This engine gives the exchange its own ledger lines; readers comparing
> continuity reports across engines must add the predecessor's exchange
> shares back out of its flooding and inflow totals.

### 15.9 Verification

Three families gate this section, in the §1.8 sense that an
unimplemented gate is an unserved capability:

**Analytic.** The SWASHES library (Delestre et al. 2013), graded on
relative $L^1$ depth error and mass error against independently
implemented reference solutions: lake at rest over immersed and emerged
bumps (exact to round-off — the well-balancing property, which the
deadband and face-gating exist to protect); subcritical flow over a bump
(≲ 1%, the residual being the omitted velocity-head dip); MacDonald
subcritical profile (≈ 2%, likewise the omitted velocity-head term,
integrated along the channel; the predecessor's marcher measures 2.0%
on the same case). A frictionless case posed between a
specified-flow inlet and a specified-stage outlet holds an undamped
standing wave — both laws reflect, and the scheme adds no dissipation
of its own — so steady analytic cases are graded on the **time-mean**
field over whole periods of the settled oscillation, which is the
steady solution the oscillation rides on. Transcritical, dam-break, and oscillating
(Thacker) cases are graded against recorded baselines rather than
analytic solutions while the convective term is optional, and the
supercritical MacDonald profile is an **expected failure**, recorded as
the boundary of validity.

**Conservation.** A closed basin conserves volume to $10^{-10}$
relative; every ledger term above closes against the state it describes;
tier-interface accounting is exact; the local-time-stepping march agrees
with the single-tier march.

**Determinism.** Byte-identical results at every §6.4 width, results
and report both; SI and US-unit authorings of one physical model agree
to round-trip precision.

### 15.10 Recorded Gaps and Deferrals

Typed, per §1.8 — each is a named absence, not an approximation:

- **Time-series results for the mesh: resolved.** §14.16 specifies the
  sidecar results stream — this engine's own framed layout at the
  §14.9 reporting instants — chosen over the predecessor's CF/UGRID
  HDF5 because a native-library dependency conflicts with the browser
  build's constraints (§1.4).
- **Checkpoint carriage: resolved.** §12.3 carries the overland state
  — mesh runs checkpoint and resume bit-identically, behind their own
  mesh fingerprint. The one deviation is recorded there: the §14.16
  sidecar of a resumed run begins at the resume instant. The
  predecessor's hotstart files carry no 2D state at all.
- **Sub-rim street drainage.** Junction exchange opens only at the
  rim; a surface film over a node whose water stands below ground does
  not drain into it.
- **Runoff-to-mesh.** Parcel runoff routes to nodes only.
- **Mesh infiltration: resolved** as §15.7's initial-loss/continuing-
  loss model. A soil column (Green–Ampt with recovery) remains absent;
  the initial loss never recovers.
- **Overland constituent transport**, **mesh adaptivity**: absent here
  as in the predecessor.
- **Report additions: resolved.** §14.9 specifies the overland flow
  continuity balance, the overland time-step summary, and the flow
  routing balance's §15.8 named pair.
- **Device compute.** The marcher's kernels are specified as pure
  per-element maps over flat state, which is deliberately the shape a
  GPU backend could lift without restructuring. None is planned: the
  engine's double-precision rule has no efficient home on consumer or
  Apple GPUs, cross-device bit-identity is unattainable, and the
  predecessor's own measurements show device launch overhead losing to
  a hot processor team below hundreds of thousands of cells. A device
  backend, if one ever earns its place, would be a separately labelled
  mode for datacenter-class hardware, never the reported result's
  default path.
- **Format churn.** The predecessor is pre-release; §14.15's retired-key
  mechanism absorbs option-vocabulary changes, and structural format
  changes are adopted deliberately, by amending this specification.
