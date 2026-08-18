# hydra-engine-wds — Quality Sub-Specification

## 1. Overview

This document is the quality sub-specification for `hydra-engine-wds`. It defines the transport, mixing, source-injection, and reaction algorithms used by Hydra's extended-period simulation.

The network data model consumed by this subsystem is defined in [model spec](../model/spec.md). The orchestration contract that invokes this engine is defined in [simulation spec](../simulation/spec.md).

---

## 6. Quality Engine

The quality engine simulates the transport and transformation of a dissolved constituent (or water age, or source tracer) through the network. It operates on the hydraulic flow field produced by the hydraulic engine and advances in quality time steps that sub-divide each hydraulic period.

### 6.1 Simulation Modes

Four quality modes exist — `NONE`, `CHEMICAL`, `AGE`, and `TRACE` ([model spec](../model/spec.md) §2.1). In `AGE` mode the
"concentration" is incremented by $\delta t_q / 3600$ at every sub-step; in
`TRACE` mode the trace node holds 100 % and all other fixed-grade inflows hold 0 %.

**Initial state**: in `CHEMICAL` and `AGE` modes every node quality, pipe segment, and tank compartment is seeded from the nodes' initial quality $C_0$ — each pipe is filled with one full-volume segment carrying the initial quality of its nominal downstream endpoint (the link's `to` node, regardless of the initial flow direction), and each tank's compartments carry the tank node's $C_0$. In `TRACE` mode the **entire** quality state — node values, pipe segments, and tank compartments — is initialised to zero before the trace node is set to 100 %: initial-quality values are chemical-mode data and are not reinterpreted as tracer percentages.

> **DEVIATION from EPANET:** in `TRACE` mode EPANET zeroes the node qualities but still seeds each tank's stored segments from the node's raw $C_0$ — a chemical-units value — so a tank with nonzero initial quality begins a trace run holding phantom tracer at $C_0$ percent. Hydra zeroes tank contents along with the rest of the quality state, resolving this internal inconsistency. This is an intentional correction, not an oversight.

### 6.2 Quality Sub-Step Loop

Within each hydraulic period of duration $\Delta t_h$, quality advances through sub-steps of size $\delta t_q$ (user parameter; must satisfy $\delta t_q \leq \Delta t_h$):

```text
t_q ← 0
while t_q < Δt_h:
δt ← min(δt_q, Δt_h - t_q)
react_in_segments(δt) // §6.5 — bulk + wall reactions in all pipe segments
react_in_tanks(δt) // §6.5 — bulk reactions in CSTR / 2-comp tanks
update_age(δt) // §6.8 — AGE mode: add δt/3600 before transport
transport_mix_push(δt) // §6.3–§6.4 — single-pass node processing
update_sources() // §6.6 — apply source injection overrides
t_q ← t_q + δt
```

Flow directions do not change within the hydraulic period. If any pipe flow direction changed from the previous hydraulic period, the topological sort (§6.3) is recomputed before the first sub-step.

### 6.3 Lagrangian Segment Transport

Each pipe is represented as an ordered list of **segments**, each with a volume $v_s$ and a uniform constituent concentration $c_s$. Segments are ordered from upstream to downstream in the direction of positive flow (or reversed for negative flow).

#### 6.3.1 Topological Sort

Nodes must be processed in topological (upstream-first) order so that a node's outflow concentration can be computed from all its upstream inputs in a single pass. The sort is performed using Kahn's algorithm on the current flow-direction graph:

1. Assign each node an in-degree equal to the number of links with positive flow arriving at it.
2. Enqueue all nodes with in-degree 0 (sources and nodes with all flow leaving).
3. Repeatedly dequeue a node, emit it, and decrement the in-degree of its downstream neighbours; enqueue any that reach 0.
4. Any nodes not emitted (part of a cycle under the current flow directions) are appended in an arbitrary fixed order.

Links with $|Q_k| < Q_{\text{stag}}$ are treated as stagnant — they are excluded from the topological sort and carry no advected mass. $Q_{\text{stag}} = 3.154 \times 10^{-7}$ m³/s (= EPANET's `Q_STAGNANT` = $1.114 \times 10^{-5}$ ft³/s = 0.005 gpm, converted to SI; distinct from EPANET's hydraulic `QZERO` constant of $10^{-6}$ ft³/s, which plays no role in the quality engine). This is a purely numerical guard; it does not scale with unit system.

The sort is recomputed only when at least one pipe's flow direction has reversed since the previous hydraulic period.

#### 6.3.2 Advection

For pipe $k$ carrying flow $Q_k > 0$ (positive direction) over sub-step $\delta t$, the volume swept is:

$$\mathcal{V}_k = Q_k \cdot \delta t$$

Starting from the **downstream (outlet) end** of the segment list — the end adjacent to the receiving node:
1. Consume segments one by one, accumulating swept mass $M = \sum c_s v_s$ and swept volume $V_{\text{swept}}$.
2. When $V_{\text{swept}}$ would exceed $\mathcal{V}_k$, partially consume the outlet-end segment: reduce its volume by the remainder needed and stop.
3. The total mass $M_k^{\text{out}}$ and volume $V_{\text{swept}}$ that exited the downstream end of pipe $k$ are accumulated into the downstream node's inflow totals.

For negative flow ($Q_k < 0$), the direction of traversal is reversed — segments are consumed from the other end.

Advection is **not** a separate global phase. Each pipe's consumption happens when its **downstream** node is visited in the single topological sweep (§6.4); because the pipe's upstream node was visited earlier in the same sweep, the fresh segment carrying that node's outflow concentration has already been pushed into the inlet end. This ordering is observable: when $\mathcal{V}_k$ exceeds the pipe's stored volume, the downstream node draws water that entered the pipe during the same sub-step, so a quality front can traverse a chain of short pipes within a single sub-step.

**Sequential by construction**: consumption and pushing interleave node by node, so the transport sweep must **not** be parallelised across pipes or nodes. (The reaction phase of §6.5, which runs before transport, remains the parallelisable step.)

#### 6.3.3 Segment Merging

When a new upstream segment is pushed into a pipe and its concentration $c_{\text{new}}$ is within a tolerance $C_{\text{tol}}$ of the first existing segment's concentration $c_{\text{first}}$, merge them into a single segment rather than prepending a new one:

$$c_{\text{merged}} = \frac{c_{\text{new}} v_{\text{new}} + c_{\text{first}} v_{\text{first}}}{v_{\text{new}} + v_{\text{first}}}, \qquad v_{\text{merged}} = v_{\text{new}} + v_{\text{first}}$$

$C_{\text{tol}}$ is the quality tolerance parameter (`quality_tolerance` option). Default: 0.01 (mg/L in `CHEMICAL` mode; hours in `AGE` mode; percent in `TRACE` mode). If set to 0 in `CHEMICAL` mode, segments are never merged.

This bounds segment growth and prevents unbounded memory use in steady or slowly-varying flows.

#### 6.3.4 Segment Memory

Each pipe and plug-flow tank maintains an ordered collection of segments. Consumed segments are released and new segments are allocated as needed. The choice of backing data structure (pool, free list, per-pipe dynamic array, etc.) is an implementation detail; the only requirement is that segment creation never silently drops a parcel — if memory is exhausted the implementation must report an error rather than corrupt the transport.

### 6.4 Nodal Mixing

Transport, mixing, and segment release form a **single sweep** over the nodes in topological order (§6.3.1). For each node in turn: the inflow-pipe segments are consumed (§6.3.2), the node's outflow concentration is computed (junction mixing below; tank mixing §6.7; reservoir boundary §6.4.3), source injection is applied (§6.6), and new segments carrying the final concentration are pushed into every outflow pipe — all before the next node is processed.

#### 6.4.1 Junctions with Gross Inflow

The outflow concentration is the mass-weighted average of all inflows:

$$c_{\text{out},i} = \frac{\displaystyle\sum_{k \in \text{in}(i)} M_k^{\text{out}}}{\displaystyle\sum_{k \in \text{in}(i)} \mathcal{V}_k^{\text{out}}}$$

where $\mathcal{V}_k^{\text{out}}$ and $M_k^{\text{out}}$ are the volume and mass that exited pipe $k$ into node $i$ during the sub-step. A junction with negative demand ($D_i < 0$) additionally receives an external inflow volume $-D_i\,\delta t$ at zero concentration, which enters the denominator and dilutes the average (a `CONCENTRATION` source can assign this water a quality, §6.6). This is the **instantaneous complete-mixing** assumption.

The resulting $c_{\text{out},i}$ — after source injection (§6.6) — is immediately pushed as a new upstream segment into every pipe carrying flow away from node $i$, before the next node in the sweep is processed.

#### 6.4.2 Junctions with Zero Gross Inflow (Stagnant)

When the total **gross** inflow volume — pipe inflows plus any external inflow from a negative demand —

$$\sum_{k \in \text{in}(i)} \mathcal{V}_k^{\text{out}} + \max(0,\, -D_i)\,\delta t = 0$$

at junction $i$ and `quality_mode ∈ {CHEMICAL, AGE}`, the junction concentration is set to the arithmetic mean of the concentrations at the nearest segment boundaries of all adjacent pipes:

$$c_i = \frac{1}{|\mathcal{N}(i)|} \sum_{k \in \mathcal{N}(i)} c_{k,\text{near}}$$

where $c_{k,\text{near}}$ is the concentration of the segment at the end of pipe $k$ facing node $i$ (the front segment for inflow pipes, the back segment for outflow pipes). This prevents unphysical drift at dead-end nodes. The trigger is **gross** inflow, not net inflow: a junction whose inflow and outflow balance is not stagnant.

#### 6.4.3 Reservoirs

A reservoir's outflow concentration is its source concentration (`../model/spec.md` §2.7 / §6.6), or 0 for `AGE` and for non-traced nodes in `TRACE` mode. The source is evaluated at the **current simulation time** (§6.6 pattern modulation). A `CONCENTRATION` source defines the outflow concentration directly (full override, §6.6); `MASS`, `SETPOINT`, and `FLOWPACED` sources instead adjust the reservoir's baseline concentration — its initial quality, since nothing mixes into a fixed-grade node — during source injection (§6.6).

#### 6.4.4 Tanks

Tank mixing is governed by the mixing model assigned in the data model (`../model/spec.md` §2.8). See §6.7.

### 6.5 Reactions

Reactions are applied to each segment and each tank compartment independently, before transport.

#### 6.5.1 Bulk Reactions

The bulk reaction rate for concentration $c$:

$$r_b = k_b \cdot f(c)$$

where $k_b$ is the bulk rate coefficient (positive = growth, negative = decay) and the potential function $f(c)$ by reaction order:

| Order | $f(c)$ | Notes |
|---|---|---|
| 0 | $1$ | Constant rate |
| 1 | $c$ | First-order decay/growth |
| 2 | $c^2$ | Second-order |
| $n$ (general) | $c^{n-1} \cdot \max(0,\, c - C_L)$ if decay ($k_b < 0$) | Positive potential when $c > C_L$; drives $c$ toward $C_L$ from above. When $C_L = 0$: reduces to $c^n$. |
| $n$ (general) | $c^{n-1} \cdot \max(0,\, C_L - c)$ if growth ($k_b > 0$) | Positive potential when $c < C_L$; drives $c$ toward $C_L$ from below. When $C_L = 0$: reduces to $c^n$. |
| Michaelis-Menten | $c / (C_L + c)$ if $k_b > 0$; $c / (C_L - c)$ if $k_b < 0$ | Saturation kinetics; $C_L$ is the half-saturation constant (growth) or limiting concentration (decay). Activated by setting order $< 0$. |

**Note on zero-order**: when `order = 0`, the potential is identically 1 regardless of $C_L$, giving a constant rate $r_b = k_b$.

**Note on the limiting concentration for integer orders**: the `order = 1` and `order = 2` rows above show only the $C_L = 0$ case. When $C_L \neq 0$, a first- or second-order reaction uses the general-$n$ form with the $\max(0, \pm(c - C_L))$ limiting potential — i.e., the reaction drives $c$ toward $C_L$ rather than decaying/growing without bound. This matches EPANET, whose $n$-th-order branch includes $n = 1$ and $n = 2$.

Concentration change over sub-step $\delta t$ (forward Euler):

$$\Delta c_b = r_b \cdot \delta t$$

Clamp updated concentration to $[0, C_{\max}]$ where $C_{\max}$ is a physical ceiling (implementation choice; typically $10^6$ mg/L).

#### 6.5.2 Wall Reactions (Pipes Only)

Wall reactions require mass transfer from the bulk to the pipe wall. The two stages — diffusion and surface reaction — operate in series.

**Step 1 — Reynolds and Schmidt numbers**:

$$Re = \frac{4 |Q|}{\pi D \nu}, \qquad Sc = \frac{\nu}{\mathcal{D}}$$

**Step 2 — Sherwood number** (ratio of convective to diffusive mass transfer):

$$Sh = \begin{cases} 2 & Re < 1 \\ 3.65 + \dfrac{0.0668\,(D/L)\,Re\,Sc}{1 + 0.04\,[(D/L)\,Re\,Sc]^{2/3}} & 1 \leq Re < 2300 \quad \text{(Graetz-Lévêque)} \\ 0.0149\,Re^{0.88}\,Sc^{1/3} & Re \geq 2300 \quad \text{(Notter-Sleicher)} \end{cases}$$

> **DEVIATION from EPANET:** EPANET evaluates the fractional exponents in these correlations as the truncated literals `0.667` and `0.333`. Hydra deliberately uses the exact $2/3$ and $1/3$ as part of its numerical modernisation; the resulting difference in $Sh$ is well under 1 % for physically realistic arguments. This is an intentional divergence, not an oversight.

**Step 3 — Mass transfer coefficient**:

$$k_f = \frac{Sh \cdot \mathcal{D}}{D}$$

**Step 4 — Effective first-order wall decay coefficient** (series combination of diffusion and wall reaction, $n_w = 1$ only):

$$k_{\text{eff}} = \frac{4}{D} \cdot \frac{k_w \, k_f}{k_f + |k_w|}$$

where $k_w$ is the first-order wall reaction rate coefficient (units: length/time, m/s after unit conversion). The factor $4/D$ converts from a surface-area basis to a volume basis for a circular cross-section.

**Wall reaction applies only to $n_w \in \{0, 1\}$**. These are the only two supported wall reaction orders. Any other value is a validation error (`../model/spec.md` §2.9). For $n_w = 1$, the series-combination formula above applies. For $n_w = 0$, the wall rate has units of mass per area per time (mg/(m²·s)), and the wall rate and the diffusive mass-transfer rate are each computed independently and the lesser magnitude prevails:

$$r_w^{(0)} = \operatorname{sgn}(k_w) \cdot \min\!\left(\lvert k_w \rvert,\; 10^3 c \cdot k_f\right) \cdot \frac{4}{D} \cdot 10^{-3}$$

where $k_f$ is the same mass-transfer coefficient from step 3, and $c$ is the current segment concentration in mg/L. The factor $10^3$ converts $c$ from mg/L to mg/m³ so that both sides of the $\min$ are in mg/(m²·s); the trailing $10^{-3}$ converts the volumetric rate back from mg/m³/s to mg/L/s. The term $10^3 c \cdot k_f$ is the maximum mass flux the diffusion boundary layer can deliver to the wall; when this flux is smaller than the wall demand $|k_w|$, the reaction becomes mass-transfer-limited and concentration-dependent despite being nominally zero-order. The sign of $k_w$ is preserved so that growth ($k_w > 0$) and decay ($k_w < 0$) are both handled correctly.

**Unit conversion for $k_w$** (applied during INP loading):

| Wall order | User units (SI input) | User units (US input) | Conversion to internal |
|---|---|---|---|
| First-order ($n_w = 1$) | m/day | ft/day | $\div 86400 \div u_\ell$ where $u_\ell$ = user length per m |
| Zero-order ($n_w = 0$) | mg/(m²·day) | mg/(ft²·day) | $\div 86400 \times u_\ell^2$ (area in denominator inverts the factor) |

This conversion applies identically to the per-pipe `wall_coeff`, the global `wall_coeff`, and the roughness–reaction correlation factor `roughness_reaction_factor` (§6.5.4), which shares $k_w$'s dimensions. Bulk coefficients (global, per-pipe, and per-tank `bulk_coeff`) carry no length factor: on-disk units are per day, internal per second ($\div 86400$).

**Serialisation (INP writing)** applies the exact inverse: $\times 86400 \times u_\ell$ (first-order wall), $\times 86400 \div u_\ell^2$ (zero-order wall), $\times 86400$ (bulk) — so coefficient values are stable across repeated load/save cycles.

Concentration change over sub-step $\delta t$:

$$\Delta c_w = k_{\text{eff}} \cdot c \cdot \delta t \quad (n_w = 1), \qquad \Delta c_w = r_w^{(0)} \cdot \delta t \quad (n_w = 0)$$

#### 6.5.3 Combined Segment Update

$$c_{\text{new}} = c_{\text{old}} + \Delta c_b + \Delta c_w$$

clamped to $[0, C_{\max}]$. The same forward-Euler step applies to both terms. The quality sub-step $\delta t_q$ must be chosen small enough relative to reaction time scales to keep truncation error acceptable.

∥ **Parallelism**: reactions in each segment are independent of all other segments; all pipe segments and tank compartments may be updated concurrently.

#### 6.5.4 Roughness–Reaction Correlation

When `roughness_reaction_factor` $R_f \neq 0$, the wall reaction coefficient $k_w$ for each pipe is derived automatically from its roughness parameter rather than read from per-pipe data. Per-pipe explicit `wall_coeff` values take precedence and are not overridden.

The formula depends on the active head-loss formula:

| Head-loss formula | Roughness parameter | Wall coefficient derived as |
|---|---|---|
| Hazen-Williams | $C$ (higher = smoother) | $k_w = R_f / C$ |
| Darcy-Weisbach | $\varepsilon$ (absolute roughness) | $k_w = R_f / |{\ln(\varepsilon/D)}|$ |
| Chezy-Manning | $n_M$ (higher = rougher) | $k_w = R_f \cdot n_M$ |

The physical rationale is that rougher pipe surfaces tend to harbour more biofilm or corrosion products and therefore exhibit higher wall demand. The derived $k_w$ is passed to the wall reaction formula (§6.5.2) in place of the per-pipe value.

### 6.6 Source Injection

Source injection is applied within the per-node transport sweep (§6.4): after a node's mixed concentration is computed and **before** its outflow segments are pushed, the source overrides or augments the mixed concentration at designated nodes.

**Stagnation guard**: source injection at node $i$ is suppressed when the total volumetric outflow *rate* from $i$ during the sub-step is at or below the stagnation threshold $Q_{\text{stag}}$ (§6.3.1) — i.e. when $\mathcal{V}_{\text{out},i} \leq Q_{\text{stag}}\,\delta t$. Besides avoiding division by zero, this prevents the large, unstable concentration increments a near-zero outflow volume would otherwise produce for `MASS`/`FLOWPACED` sources. It is the *same* threshold used for link stagnation in transport (§6.3.1) — a single guard, matching EPANET's `Q_STAGNANT`.

| Source type | Effect on node concentration $c_i$ (when $Q_{\text{out},i} > 0$) |
|---|---|
| `CONCENTRATION` | At **reservoirs and tanks**: $c_i \leftarrow c_{\text{src}}$ (full override — the outflow quality *is* the source value; a tank's stored quality is left unmodified). At **junctions**: effective only when the junction has net negative demand ($D_i < 0$, i.e., the node is a local inflow point); otherwise the source contributes nothing regardless of $c_{\text{src}}$. |
| `MASS` | $c_i \leftarrow c_i + (r_s / 60) \cdot \delta t / (V_{\text{out},i} \cdot 10^3)$ where $r_s$ is the source rate in mg/**min** (÷60 → mg/s), $\delta t$ in s, $V_{\text{out},i}$ the outflow volume in m³; the factor $10^3$ converts m³ to L so the result is in mg/L. |
| `SETPOINT` | $c_i \leftarrow \max(c_i, c_{\text{src}})$ (raise to setpoint if below it; no reduction) |
| `FLOWPACED` | $c_i \leftarrow c_i + c_{\text{src}}$ (fixed increment above natural mixed concentration) |

The effective source value at time $t$ is `base_value` × $F_{\text{pattern}}(t)$ (`../model/spec.md` §2.7).

> **DEVIATION from EPANET:** at a tank, EPANET *adds* a `CONCENTRATION` source on top of the tank's mixed outflow quality — the flow-paced booster formula applied to a different source type, contradicting EPANET's own definition of a concentration source ("fixes the quality of water leaving the node"). Hydra implements the defined semantics uniformly: the tank's outflow quality is the source value. Additive boosting at a tank remains expressible with a `FLOWPACED` source. This is an intentional correction of an EPANET inconsistency, not an oversight.

### 6.7 Tank Mixing Models

#### 6.7.1 Complete Mix (CSTR)

The tank is a single well-mixed compartment; concentration is uniform throughout volume $V$. Mixing and reaction are applied as **two separate steps** within each sub-step (matching EPANET's `tankmix1`), not as a single combined ODE update.

**Mixing** — the inflow volume $V_{\text{in}} = Q_{\text{in}}\,\delta t$ at volume-weighted inflow concentration $c_{\text{in}}$ is blended into the current contents, then the net volume change is applied:

$$c_{\text{mixed}} = \frac{c_{\text{old}}\,V + c_{\text{in}}\,V_{\text{in}}}{V + V_{\text{in}}}, \qquad V \leftarrow V + (Q_{\text{in}} - Q_{\text{out}})\,\delta t$$

**Reaction** — the bulk reaction (§6.5.1) is applied to the tank concentration in the tank-reaction phase of the sub-step loop (§6.2), independently of the mixing step:

$$c \leftarrow c + r_b(c)\,\delta t$$

Outflow concentration = the post-mixing, post-reaction concentration.

**Overflow**: when `overflow = true` and the mixing update would push the stored volume above $V_{\max}$, the volume is clamped at $V_{\max}$ and the spilled volume — carrying the mixed concentration — is charged to the outflow mass balance ($M_{\text{demand}}$, §6.9) rather than accumulating in storage.

#### 6.7.2 Two-Compartment Mix

The tank is represented as two segments: a **mixing zone** with maximum capacity $V_{\text{mz}} = f \cdot V_{\max}$ (user fraction $f$, applied to the tank's *maximum* volume) and a **stagnant zone** with maximum capacity $V_{\text{sz}} = V_{\max} - V_{\text{mz}}$. All inflow enters and all outflow exits from the mixing zone. Transfers between zones are **directional and discrete** — there is no continuous bidirectional exchange.

Let $v_{\text{in}}$ = inflow volume during the sub-step, $w_{\text{in}}$ = inflow mass ($v_{\text{in}} \cdot c_{\text{in}}$), $v_{\text{net}}$ = net volume change (inflow − outflow), and $c_m$, $V_m$ = mixing zone concentration and volume, $c_s$, $V_s$ = stagnant zone concentration and volume.

**Filling** ($v_{\text{net}} > 0$):

1. Mix inflow into the mixing zone:

   $$c_m \leftarrow \frac{c_m \cdot V_m + w_{\text{in}}}{V_m + v_{\text{in}}}$$

2. Compute overflow from mixing zone: $v_t = \max(0,\; V_m + v_{\text{net}} - V_{\text{mz}})$.
3. If $v_t > 0$, transfer to stagnant zone:

   $$c_s \leftarrow \frac{c_s \cdot V_s + c_m \cdot v_t}{V_s + v_t}$$

   Then clamp: $V_m \leftarrow V_{\text{mz}}$, $V_s \leftarrow V_s + v_t$. If $V_s > V_{\text{sz}}$, the surplus $(V_s - V_{\text{sz}}) \cdot c_s$ exits as overflow mass and $V_s \leftarrow V_{\text{sz}}$.
4. If $v_t = 0$: $V_m \leftarrow V_m + v_{\text{net}}$, clamped to $[0, V_{\text{mz}}]$. If the updated mixing zone volume is below its maximum capacity ($V_m < V_{\text{mz}}$), clear the stagnant zone ($V_s \leftarrow 0$) — unmixed water is only retained when the mixing zone is at capacity.

**Emptying** ($v_{\text{net}} < 0$):

1. Compute transfer back from stagnant zone: $v_t = \min(V_s,\; |v_{\text{net}}|)$.
2. Mix inflow and transferred water into the mixing zone:

   $$c_m \leftarrow \frac{c_m \cdot V_m + w_{\text{in}} + c_s \cdot v_t}{V_m + v_{\text{in}} + v_t}$$
3. Update volumes: $V_s \leftarrow \max(0,\; V_s - v_t)$, $V_m \leftarrow V_{\text{mz}} + v_t + v_{\text{net}}$.

**No net flow** ($v_{\text{net}} = 0$): inflow mass still mixes into the mixing zone (step 1 of filling); no volume transfer occurs.

> **DEVIATION from EPANET:** at exactly zero net flow, EPANET discards the inflow mass (its two-compartment update only blends inflow inside the strict filling/emptying branches). Hydra instead mixes the inflow into the mixing zone even when $v_{\text{net}} = 0$, which is more physically correct — at zero net flow the inflow and an equal outflow are both passing through the mixing zone, so the incoming constituent should participate in the mix. This is an intentional accuracy improvement, not an oversight.

Outflow concentration = $c_m$ (mixing zone concentration).

Bulk reactions are applied to each zone independently after the mixing/transfer step: $c_m \leftarrow c_m + r_b \cdot \delta t$, $c_s \leftarrow c_s + r_b \cdot \delta t$.

#### 6.7.3 FIFO Plug Flow

The tank is treated as a perfectly ordered pipe. Its contents are represented as the same ordered segment list used for pipes (§6.3). Inflow creates a new segment at the inlet end; outflow consumes segments from the outlet end. Bulk reactions apply segment-by-segment. Segment merging (§6.3.3) applies.

Outflow concentration = the concentration of the oldest (outlet-end) segment, consuming it at rate $Q_{\text{out}}$.

The tank node's **reported** concentration is the same outlet-end value: the water the tank would deliver to the network at that instant.

> **DEVIATION from EPANET:** EPANET reports a net-filling FIFO tank's node quality as approximately the current **inflow** concentration — empirically, a tank full of initial water at one concentration reports the fresh inflow's concentration from the first period of a fill, regardless of its contents. Hydra reports the outlet-end (oldest) segment instead, because "tank quality" should describe the water the tank holds at its outlet and would deliver, not the water that just arrived at its inlet. During a long fill of a tank whose initial water differs from the inflow, the two conventions diverge by the full concentration difference until the old water is flushed through; downstream transport is unaffected in both tools, since actual outflow always draws the oldest segments. This is an intentional reporting improvement, not an oversight.

#### 6.7.4 LIFO Stacked Layers

Inflow and outflow both occur at the **same end** (top). Each sub-step processes the **gross** inflow and outflow volumes, in this order:

1. **Outflow**: pop the gross outflow volume $v_{\text{out}}$ from the top of the stack, consuming segments (the top segment partially, if needed) until $v_{\text{out}}$ is exhausted or the stack is empty.
2. **Inflow**: push the gross inflow volume $v_{\text{in}}$ as a new top segment at the inflow concentration $c_{\text{in}}$, merging with the existing top segment when the concentrations are within $C_{\text{tol}}$ (§6.3.3).

Reactions apply segment-by-segment.

Outflow concentration = the concentration of the topmost segment after the update.

> **DEVIATION from EPANET:** EPANET's LIFO model moves only the **net** flow $v_{\text{net}} = v_{\text{in}} - v_{\text{out}}$: filling pushes a $v_{\text{net}}$-sized top segment, draining pops $|v_{\text{net}}|$, and simultaneous inflow and outflow cancel without touching the stack — the exchanged mass is discarded. Hydra processes the gross volumes (as the FIFO model does), so the water actually passing through the top of the stack participates in transport; this conserves mass and is truer to the stacked-layer premise, in the same family as the zero-net-flow mixing correction in §6.7.2. This is an intentional accuracy improvement, not an oversight.

### 6.8 Water Age

In `AGE` mode, "concentration" is interpreted as residence time (hours). Initial ages are seeded from each node's initial quality $C_0$, interpreted in hours (§6.1) — zero only by default, so nonzero starting ages are legal. At every quality sub-step, after reactions and before transport, add $\delta t / 3600$ to the concentration of every segment in every pipe and every tank compartment. Reservoirs (fixed-grade nodes) hold a constant age of 0 — water entering from a reservoir resets the age to 0.

### 6.9 Mass Balance

The quality engine maintains a running mass balance to verify numerical conservation. Accumulated at each quality sub-step:

| Quantity | Description |
|---|---|
| $M_{\text{init}}$ | Constituent mass in all pipes and tanks at $t = 0$ |
| $M_{\text{added}}$ | Mass injected by all sources |
| $M_{\text{demand}}$ | Mass leaving the network — consumer demand withdrawals **plus** mass carried into fixed-grade reservoir sinks (EPANET's `masslost`) |
| $M_{\text{reacted}}$ | Net mass consumed (or produced) by bulk and wall reactions (signed; positive = decay) |
| $M_{\text{final}}$ | Constituent mass remaining at end of simulation |

**Balance ratio**:

$$\rho_m = \frac{M_{\text{demand}} + \max(M_{\text{reacted}},\, 0) + M_{\text{final}}}{M_{\text{init}} + M_{\text{added}} + \max(-M_{\text{reacted}},\, 0)}$$

where $M_{\text{reacted}} > 0$ represents net decay (mass removed from the water) and $M_{\text{reacted}} < 0$ represents net growth (mass added to the water). Decay contributes to the output side of the ledger; growth contributes to the input side. A value of $\rho_m \approx 1$ confirms conservation. A significant deviation indicates a numerical error or inconsistent reaction parameterisation.

> **Tank overflow**: for **CSTR** and **two-compartment** tanks, water spilled by an overflowing tank is charged to $M_{\text{demand}}$ — the CSTR stored volume is clamped at $V_{\max}$ and its spill priced at the mixed concentration; the two-compartment model charges its stagnant-zone surplus at the stagnant concentration — so the balance closes. For **FIFO** and **LIFO** plug-flow tanks this is not yet done: their overflow is folded into the pass-through outflow, so a network that loses mass through a FIFO/LIFO overflow may still show a small residual in $\rho_m$.

#### 6.9.1 Disaggregated Reaction Rate Accumulators

In addition to the signed $M_{\text{reacted}}$ used for the balance ratio, the quality engine maintains four **unsigned** (absolute-value) running totals for the binary output network-reactions section:

| Accumulator | Description |
|---|---|
| $W_{\text{bulk}}$ | Absolute mass reacted by **pipe bulk** reactions |
| $W_{\text{wall}}$ | Absolute mass reacted by **pipe wall** reactions |
| $W_{\text{tank}}$ | Absolute mass reacted by **tank bulk** reactions |
| $W_{\text{source}}$ | Total mass injected by sources (gated; see below) |

**Accumulation rule — pipe reactions (§6.5):** for each pipe segment with concentration $c_0$, bulk concentration change $\delta c_b$, wall concentration change $\delta c_w$, and segment volume $V_s$:

$$W_{\text{bulk}} \mathrel{+}= \lvert \delta c_b \rvert \cdot V_s, \qquad W_{\text{wall}} \mathrel{+}= \lvert \delta c_w \rvert \cdot V_s$$

**Accumulation rule — tank reactions (§6.5, §6.7.2):** for each tank compartment with concentration $c_0$, bulk concentration change $\delta c$, and compartment volume $V$:

$$W_{\text{tank}} \mathrel{+}= \lvert \delta c \rvert \cdot V$$

**Accumulation rule — sources (§6.6):** after evaluating the effective source concentration and computing the mass added $m_s$ at a node:

$$W_{\text{source}} \mathrel{+}= \max(m_s, 0)$$

**Gating:** all four accumulators are updated only when the current simulation time $t \geq t_{\text{report\_start}}$. This matches the reporting window used for snapshots.

**Output conversion:** each accumulator is in mg/L × m³ (concentration × volume, SI). Since 1 m³ = 10³ L, one unit of accumulation equals 10³ mg. The binary output writes average rates in mg/hr:

$$R_{\text{bulk}} = \frac{W_{\text{bulk}} \cdot 10^3}{T}, \quad R_{\text{wall}} = \frac{W_{\text{wall}} \cdot 10^3}{T}, \quad R_{\text{tank}} = \frac{W_{\text{tank}} \cdot 10^3}{T}, \quad R_{\text{source}} = \frac{W_{\text{source}} \cdot 10^3}{T}$$

where $D$ is the total simulation duration in seconds and $T$ is the averaging window in hours:

$$T = \begin{cases} D / 3600, & D > 0 \\ 1, & D = 0 \end{cases}$$

The one-hour value applies **only** to a single-period run ($D = 0$, [model spec](../model/spec.md) §2.1), where no elapsed time exists to average over. It is not a floor: a sub-hour simulation averages over its own true duration, so a 30-minute run reports rates over 0.5 h rather than over 1 h.

---

