# EPANET: A Conceptual and Mathematical Analysis

## Introduction

[OWA-EPANET](https://github.com/OpenWaterAnalytics/EPANET) is a computational engine for simulating the hydraulic and water quality behaviour of pressurised water distribution networks over time. It represents a network as a directed graph of nodes connected by links and advances a time-stepped extended-period simulation, solving at each step for pressures, flows, and constituent concentrations throughout the system. The solver combines rigorous physical models — empirical head-loss formulas, Newton–Raphson linearisation, sparse direct linear algebra, and Lagrangian advection-reaction transport — with flexible engineering constructs such as demand patterns, operational controls, and tank mixing models.

This document provides a self-contained, mathematical and conceptual description of every major subsystem: how the network is represented, how hydraulic equilibrium is computed at each time step, how demands are handled under both fixed and pressure-dependent conditions, how leakage and emitter flows enter the system, how the simulation advances through time, how control logic operates, how water quality is transported and reacted, and how energy and mass balance are tracked. The goal is to give the reader a complete algorithmic and mathematical understanding of the system; implementation-specific details such as memory layout and data-structure internals are omitted, but input/output behaviour and the public API are described at a conceptual level. The analysis describes **EPANET 2.3.5** — tag `v2.3.5` of the [OWA-EPANET](https://github.com/OpenWaterAnalytics/EPANET) repository — and was derived from its C source, which is authoritative throughout.

---

## Table of Contents

- [EPANET: A Conceptual and Mathematical Analysis](#epanet-a-conceptual-and-mathematical-analysis)
  - [Introduction](#introduction)
  - [Table of Contents](#table-of-contents)
  - [1. Network Representation](#1-network-representation)
    - [Node Types](#node-types)
    - [Link Types](#link-types)
    - [Curves and Patterns](#curves-and-patterns)
    - [The State-Vector View](#the-state-vector-view)
  - [2. Hydraulic Simulation](#2-hydraulic-simulation)
    - [2.1 Head Loss in Pipes](#21-head-loss-in-pipes)
      - [Hazen–Williams Formula](#hazenwilliams-formula)
      - [Darcy–Weisbach Formula](#darcyweisbach-formula)
      - [Chezy–Manning Formula](#chezymanning-formula)
      - [Minor Losses and Total Head Loss](#minor-losses-and-total-head-loss)
    - [2.2 Pump Head Gain](#22-pump-head-gain)
    - [2.3 Valve Behaviour](#23-valve-behaviour)
    - [2.4 The Global Gradient Algorithm](#24-the-global-gradient-algorithm)
      - [Governing Equations](#governing-equations)
      - [Linearisation](#linearisation)
      - [Assembly of the Linear System](#assembly-of-the-linear-system)
      - [Flow Update](#flow-update)
      - [Convergence Criterion](#convergence-criterion)
    - [2.5 Sparse Linear Algebra](#25-sparse-linear-algebra)
  - [3. Demand Models](#3-demand-models)
    - [3.1 Demand-Driven Analysis](#31-demand-driven-analysis)
    - [3.2 Pressure-Driven Analysis](#32-pressure-driven-analysis)
  - [4. Emitters](#4-emitters)
  - [5. Pipe Leakage — the FAVAD Model](#5-pipe-leakage--the-favad-model)
  - [6. Time-Stepping and Tank Dynamics](#6-time-stepping-and-tank-dynamics)
    - [6.1 Extended-Period Simulation](#61-extended-period-simulation)
    - [6.2 Adaptive Time Step](#62-adaptive-time-step)
    - [6.3 Tank Level Update](#63-tank-level-update)
  - [7. Control Systems](#7-control-systems)
    - [7.1 Simple Controls](#71-simple-controls)
    - [7.2 Rule-Based Controls](#72-rule-based-controls)
  - [8. Water Quality Simulation](#8-water-quality-simulation)
    - [8.1 Overview and Simulation Modes](#81-overview-and-simulation-modes)
    - [8.2 Lagrangian Segment Transport](#82-lagrangian-segment-transport)
    - [8.3 Source Terms](#83-source-terms)
    - [8.4 Chemical Reactions](#84-chemical-reactions)
      - [Bulk Reactions](#bulk-reactions)
      - [Wall Reactions](#wall-reactions)
      - [Combined Reaction in a Segment](#combined-reaction-in-a-segment)
      - [Roughness–Reaction Correlation](#roughnessreaction-correlation)
  - [9. Tank Mixing Models](#9-tank-mixing-models)
    - [Complete Mix (CSTR)](#complete-mix-cstr)
    - [Two-Compartment Mix](#two-compartment-mix)
    - [FIFO Plug Flow](#fifo-plug-flow)
    - [LIFO (Stacked Layers)](#lifo-stacked-layers)
  - [10. Mass Balance](#10-mass-balance)
  - [11. Energy Tracking](#11-energy-tracking)
  - [12. Flow Balance](#12-flow-balance)
  - [13. Units and Physical Constants](#13-units-and-physical-constants)
  - [14. Input and Output](#14-input-and-output)
    - [Input](#input)
    - [Output](#output)
    - [Binary Output File Format](#binary-output-file-format)
      - [Prolog](#prolog)
      - [Energy](#energy)
      - [Dynamic Results](#dynamic-results)
      - [Network Reactions](#network-reactions)
      - [Epilog](#epilog)
    - [API](#api)
  - [15. Cross-Cutting Engine Contracts](#15-cross-cutting-engine-contracts)

---

## 1. Network Representation

The physical infrastructure is represented as a directed graph. **Nodes** correspond to points in the network — junctions, reservoirs, and tanks — and **links** correspond to the conduits and devices connecting them — pipes, pumps, and valves. The orientation of a link defines a positive flow direction; negative flows simply indicate flow in the reverse direction.

### Node Types

**Junctions** are the ordinary connection points of the network. Each junction has a fixed elevation and one or more demand categories, each associating a base demand rate with a time-varying multiplier pattern. Demand lines in the `[DEMANDS]` section *replace* the demand entered on the junction's own line — the first entry overwrites, later entries for the same node append further categories — and demand entries naming tanks or reservoirs are silently ignored. Junctions may also carry an emitter (representing orifice or sprinkler outflow) and a water quality source. The hydraulic head at a junction is an unknown to be solved at every time step.

**Reservoirs** are fixed-grade nodes whose hydraulic head is always known and equal to the water surface elevation. A reservoir represents an infinitely large storage body that maintains a constant pressure boundary condition. Because its head is known, it does not appear as an unknown in the linear system; instead, it contributes boundary terms to the equations of its neighbouring junctions.

**Tanks** are storage nodes with a variable water level that evolves as water flows in and out. Each tank is characterised by a minimum level, a maximum level, an initial level, and a geometry: either a constant cross-sectional area or a user-defined volume-versus-elevation curve (an optional explicit minimum volume overrides the cylindrical estimate $A \times h_{\min}$; the tank's levels must lie within a volume curve's elevation range). Its hydraulic head at any instant equals the elevation of its water surface, which is updated after each hydraulic time step based on the net flow. A tank also carries a bulk reaction coefficient governing water quality transformations in its stored volume.

### Link Types

**Pipes** are the primary conduits. Each pipe is characterised by its length, internal diameter, a roughness coefficient (whose interpretation depends on the chosen head-loss formula), a minor loss coefficient representing localised losses at fittings, and bulk and wall reaction coefficients for quality simulation. A pipe may also be designated as a check valve, in which case flow is permitted in only one direction; the link is treated as closed whenever the computed flow or pressure gradient would drive flow in the reverse direction.

**Pumps** add hydraulic head to the flow passing through them. The head added is described by a head-versus-flow curve, one of the most important user-supplied relationships in the model. Alternatively, a pump may be defined by a constant power output. A variable-speed setting scales both the head and flow axes of the pump curve according to the affinity laws.

**Valves** regulate flow or pressure and come in seven varieties:

- **Pressure Reducing Valve (PRV)**: limits the hydraulic head on its downstream side to a specified setpoint when the upstream head exceeds it.
- **Pressure Sustaining Valve (PSV)**: maintains the hydraulic head on its upstream side above a specified setpoint when the downstream head would otherwise pull it below.
- **Flow Control Valve (FCV)**: restricts the volumetric flow through it to a specified setpoint.
- **Throttle Control Valve (TCV)**: applies a specified head loss coefficient; it behaves as a pipe with adjustable resistance.
- **General Purpose Valve (GPV)**: head loss as a function of flow is entirely described by a user-supplied piece-wise linear curve.
- **Positional Control Valve (PCV)**: The loss coefficient varies with a percent-open setting, optionally governed by a user-supplied curve relating the valve opening (percent open) to the ratio of its **flow coefficient** to its fully-open flow coefficient, $K_v/K_{v0}$. The internal minor-loss coefficient is then *derived* from this ratio as $K_{m0}/(K_v/K_{v0})^2$ (see §2.3); absent a curve, the ratio is simply linear in percent open.
- **Pressure Breaker Valve (PBV)**: imposes a fixed head-loss setpoint. When the setting exceeds the natural minor-loss head drop at the current flow, the solver forces the exact head loss; otherwise the PBV falls back to ordinary pipe resistance. PBVs have no control states — they always contribute a resistance.

Valve placement is validated at parse time: a PRV, PSV, or FCV may not connect to a tank or reservoir, and specific adjacent combinations are prohibited — two PRVs sharing a downstream node or in series, two PSVs sharing an upstream node or in series, and certain PRV/PSV/FCV adjacencies.

### Curves and Patterns

User-defined **curves** are piece-wise linear relationships used throughout the model: pump head versus flow, pump efficiency versus flow, tank volume versus elevation, general purpose valve head loss versus flow, and positional valve opening versus flow-coefficient ratio ($K_v/K_{v0}$). Intermediate values are obtained by linear interpolation between the two bracketing data points. Behaviour outside a curve's x-range depends on the evaluation path: the solver's segment evaluation for custom pump head curves and general purpose valve curves extends the first or last segment as a straight line — extrapolating linearly beyond the end points — whereas the general lookup used for tank volume, pump efficiency, and similar curves clamps to the end-point values and never extrapolates. A curve's type may be declared with an explicit tag or is otherwise inferred from its first use.

**Patterns** are repeating sequences of dimensionless multipliers indexed by time. They modulate base demands at junctions, pump speed settings, and constituent source concentrations over the course of the simulation. At each hydraulic time step the pattern multiplier is determined by the **pattern period index**: given a global pattern start offset $t_{\text{start}}$, pattern time step $\Delta t_p$, and current simulation time $t$, the number of elapsed periods is $p = \lfloor (t + t_{\text{start}}) / \Delta t_p \rfloor$. For a demand category assigned to pattern $j$ of length $L_j$, the applicable multiplier is $F_j[p \bmod L_j]$, giving each pattern an independently repeating cycle. A **default pattern** is available and is applied to any demand category that has no explicit pattern assigned; if no default pattern exists either, a multiplier of 1.0 is used.

**Reservoir head patterns**: while reservoirs are nominally fixed-grade nodes, each reservoir may optionally be assigned a time pattern. When assigned, the head at that reservoir at each hydraulic time step equals its base elevation multiplied by the current pattern multiplier. This allows time-varying source heads to represent, for example, tidal fluctuations or varying water tower levels.

**Pump utilisation patterns**: each pump may have a separate utilisation pattern (distinct from any energy cost pattern) that controls the pump's speed setting at each hydraulic time step. The current pattern multiplier is applied directly as the normalised speed $\omega$; a multiplier of zero closes the pump, while a multiplier of 1.0 sets it to its rated speed. This allows pump schedules to be encoded as a time series without requiring explicit control rules.

### The State-Vector View

Because each hydraulic time step is solved as a steady state (§6.1), the quantities that genuinely evolve from one step to the next form a small state vector. On the hydraulic side it comprises: the tank water levels — equivalently the stored volumes — integrated forward after each solve (§6.3), the only continuously-evolving physical state; every link's discrete status and numeric setting, as modified by controls and rules (§7) or by pattern-driven pump speed settings (see above); and the link flows themselves, which carry over as the estimates around which the next solve's linearisation begins — §2.2 gives the cold-start seeding used before the first solve, and on re-initialisation existing link flows are preserved as a warm start unless re-initialisation is explicitly requested or the flow is near zero (§6.1). Everything else the hydraulic engine reports — junction heads, pressures, equilibrium link flows, velocities — is *derived*: recomputed at every step by the Global Gradient Algorithm (§2.4) from the boundary conditions this state defines, namely the known heads of reservoirs (possibly pattern-scaled) and tanks at their current levels, together with the pattern-scaled demands.

The water-quality engine carries its own state between quality sub-steps: the ordered segment lists of each pipe, each segment holding a volume and a uniform concentration (§8.2); the mixing state of each tank as prescribed by its mixing model — a single uniform concentration for a complete-mix tank, the volumes and concentrations of the mixing and stagnant zones for a two-compartment tank, and the segment stacks of FIFO and LIFO tanks (§9); and the running mass-balance ledger that accounts for source mass added, mass removed as outflow, and mass reacted (§10).

The seam between the two engines is explicit: at each hydraulic step the solver writes nodal demands, heads, link flows, statuses, and settings to a hydraulics binary file, and the quality simulation *replays* this file — velocities are reconstructed from the saved flows and pipe geometry, and the flow field is held constant across the quality sub-steps within each hydraulic period (§8.1, §14). The saved record is therefore the complete hydraulic input the quality engine ever sees, which is why a saved hydraulics file can be reused across runs without recomputing the hydraulics (§14).

---

## 2. Hydraulic Simulation

### 2.1 Head Loss in Pipes

**Hydraulic head** at any node is the mechanical energy per unit weight of water:

$$H = \frac{P}{\rho g} + z$$

where $P$ is the gauge pressure, $\rho$ is the water density, $g$ is gravitational acceleration, and $z$ is the elevation above datum. Flow from node $i$ to node $j$ is driven by the head difference $H_i - H_j$.

Three empirical head-loss formulas are available, and one is selected uniformly for the entire network.

#### Hazen–Williams Formula

$$h_f = \frac{4.727 \, L}{C^{1.852} \, D^{4.871}} \, Q^{1.852}$$

Here $L$ is the pipe length, $D$ is the internal diameter, $C$ is the Hazen–Williams roughness coefficient (higher values indicate smoother pipes), and $Q$ is the volumetric flow rate. The flow exponent is $n = 1.852$. This formula is empirical and strictly valid only for turbulent flow of water at ordinary temperatures.

#### Darcy–Weisbach Formula

$$h_f = f \cdot \frac{L}{D} \cdot \frac{V^2}{2g} = f \cdot \frac{8 L}{\pi^2 g D^5} \, Q^2$$

where $V = Q / (\pi D^2 / 4)$ is the mean flow velocity and $f$ is the dimensionless Darcy friction factor, which depends on the Reynolds number $Re = VD/\nu$ and the relative roughness $\varepsilon/D$.

For **laminar flow** ($Re \leq 2000$) the Hagen–Poiseuille result applies:

$$f = \frac{64}{Re}$$

yielding a head loss proportional to $Q$ (linear regime).

For **turbulent flow** ($Re \geq 4000$) the friction factor is computed from the Swamee–Jain approximation to the Colebrook-White implicit equation:

$$f = \left[ -2 \log\!\left( \frac{\varepsilon}{3.7 D} + \frac{5.74}{Re^{0.9}} \right) \right]^{-2}$$

where $\varepsilon$ is the absolute roughness. The quantity $f$ and its derivative with respect to $Q$ are evaluated simultaneously at each Newton iteration so that the linearisation of the solver (§2.4) remains consistent.

For **transitional flow** ($2000 < Re < 4000$), a cubic polynomial ensures continuity in $f$ and $df/dQ$ across the transition. The polynomial is anchored at both ends: $f = 64/Re$ at $Re = 2000$ (exact laminar value), and the Swamee–Jain value and its derivative at $Re = 4000$ (turbulent end). The laminar Hagen–Poiseuille branch handles flows below a pipe-geometry-dependent low-flow threshold independently and does not call this cubic.

#### Chezy–Manning Formula

$$h_f = \left( \frac{4 n_M}{1.49 \, \pi \, D^2} \right)^2 \left( \frac{D}{4} \right)^{-1.333} L \, Q^2$$

where $n_M$ is the Manning roughness coefficient. (The source hardcodes the empirical constant as $1.49$, a two-figure rounding of the textbook Manning-Strickler value $1.486$, and the exponent as $-1.333$, a truncation of $-4/3$ — both roundings preserved here as the code's actual behaviour.) The exponent on $Q$ is 2 in this formulation (as used for full circular pipes). This formula is less common for pressurised systems but is supported for completeness.

#### Minor Losses and Total Head Loss

Minor (local) losses due to fittings, bends, and contractions are modelled as:

$$h_{\text{minor}} = K_m \, Q \, |Q|$$

where $K_m$ is the minor loss coefficient (head loss per unit of $Q^2$). The sign convention ensures the loss opposes flow in either direction.

The **total head loss** across a pipe combining friction and minor losses is:

$$h = R \, Q^n \cdot \mathrm{sign}(Q) + K_m \, Q \, |Q|$$

where $R$ is the friction resistance coefficient derived from whichever formula is in use and $n$ is the corresponding flow exponent (1.852 for Hazen–Williams, 2 for Darcy–Weisbach and Chezy–Manning in their simplified forms). The sign convention ensures that the expression is an odd function of $Q$: head loss is always in the direction opposing flow.

To keep the Jacobian non-singular near zero flow, whenever an element's head-loss gradient falls below the `RQTOL` option (default $10^{-7}$), the gradient is floored and the element's head loss becomes a *linear* function of flow — applied to Hazen–Williams and Chezy–Manning pipes, nonlinear pump curves, valve minor losses (floored at $RQtol/2$), and emitters; Darcy–Weisbach pipes rely on their laminar branch instead.

### 2.2 Pump Head Gain

A pump adds head to the flow. Three types of pump curves are supported.

**Power-function curve**: the head gain follows

$$\Delta H = h_0 - r \, Q^N$$

where $h_0$ is the shutoff head (head at zero flow), $r$ is a resistance-like coefficient, and $N$ is the curve exponent. This three-parameter form fits most centrifugal pump characteristics well.

**Constant-power pump**: the head gain is determined by maintaining a fixed power output regardless of flow:

$$\Delta H = \frac{\text{Power}}{\gamma \, Q}$$

where $\gamma = \rho g$ is the specific weight of water. As flow decreases toward zero, the head gain grows without bound. The solver handles this by monitoring the head-loss gradient $|\partial h / \partial Q| = r / Q^2$: when this gradient exceeds $C_\infty \approx 10^8$ (near-zero flow), the pump is treated as a closed link ($P_k = 1/C_\infty$, $Y_k = Q_k$); when the gradient falls below $10^{-6}$ (extremely high flow), the pump is treated as a fully open link with minimal resistance. Between these extremes, the standard linearisation $h = r/Q$ applies. Independently, a constant-power pump whose flow falls below $10^{-6}$ ft³/s is set to **TEMPCLOSED** status (see pump status below), because the power formula is undefined at zero flow. A constant-power pump's nominal design flow is fixed at 1 ft³/s and is restored whenever the pump is re-opened by a status change, setting change, or control, so the $r/Q$ linearisation restarts from a sane point; and during the flow update, a Newton correction that would drive its flow strictly negative is replaced by halving the current flow instead (a correction landing exactly at zero flow is allowed through).

**Custom curve**: a user-defined piece-wise linear head-versus-flow curve. At any operating point the solver identifies the two adjacent data points that bracket the current flow and interpolates linearly to obtain the head gain and its derivative. For initial flow conditions before the first solve, the design flow $Q_0$ for a custom-curve pump is taken as the midpoint between the first and last flow data points on the curve (rather than a named design point).

**Speed scaling via affinity laws**: when a pump operates at a relative speed $\omega$ (with $\omega = 1$ being its rated speed), the affinity laws relate the scaled curve to the rated curve:

$$\Delta H(\omega, Q) = \omega^2 \cdot \Delta H_1\!\left(\frac{Q}{\omega}\right)$$

where $\Delta H_1$ is the head gain at rated speed. Equivalently, the shutoff head scales as $\omega^2$ and the flow axis scales as $\omega$. In the Newton–Raphson solver, a pump is treated as a link with a **negative** head-loss value (a gain), and its linearised resistance coefficient $P_k$ and offset $Y_k$ are derived from the pump curve in the same algebraic framework as pipe head losses.

**Three-point pump curve fitting**: when a pump is specified by three operating points — the shutoff head $h_0$ (head at zero flow), the design point $(q_1, h_1)$, and the maximum-flow point $(q_2, h_2)$ — the power-function parameters are determined analytically:

$$c = \frac{\ln\!\left(\dfrac{h_0 - h_2}{h_0 - h_1}\right)}{\ln\!\left(\dfrac{q_2}{q_1}\right)}, \qquad b = \frac{h_0 - h_1}{q_1^{\,c}}, \qquad a = h_0$$

yielding the curve $\Delta H = a - b \, Q^c$. The curve is validated: it must be strictly decreasing in head ($h_0 > h_1 > h_2$), the exponent $c$ must be positive, and additionally $c \leq 20$ (an upper-bound sanity check enforced by the validator). A **one-point** curve $(q_1, h_1)$ is expanded to this form using $h_0 = 1.33334\,h_1$, $q_2 = 2q_1$, $h_2 = 0$; the power-function fit applies only to curves of exactly one or exactly three points (the latter with their first point at zero flow) — any other curve, whether by point count or by a nonzero first flow, is treated as a custom piece-wise curve instead. Validation also derives each pump's limits: power curves take $Q_{\max}$ as the flow at which the fitted head reaches zero and $H_{\max} = h_0$, while custom curves take their last flow and first head points — this $Q_{\max}$ is the one referenced by the XFLOW warning (§2.4).

**Pump status — XHEAD**: a pump in the OPEN state transitions to the **XHEAD** (excess head) state when the head gain required to maintain the computed flow exceeds the speed-adjusted head limit $\omega^2 H_{\max}$ — the shutoff head $h_0$ for power-function curves, the first head point for custom curves (see the curve-limit derivation above) — by more than the head tolerance $\text{Htol}$ (§2.3). In this state the pump is treated as a closed link for that iteration. The status reverts to OPEN at the start of each periodic status check, and is re-tested against the new computed operating point. For constant-power pumps, XHEAD cannot occur; instead, the pump is flagged TEMPCLOSED when the flow falls below $10^{-6}$ ft³/s (a strictly positive cutoff, since the power formula is undefined at zero flow).

**Initial flow conditions**: before the first Newton–Raphson solve, link flows are initialised as follows — closed links receive a negligible flow $Q_0 \approx 10^{-6}$ ft³/s; pumps receive the product of their speed setting and their design flow $Q_{\text{design}}$; all other links (pipes and valves) receive the flow corresponding to a nominal velocity of 1 ft/s through the full pipe cross-section: $Q = \pi D^2 / 4$. These initial values need not be physically consistent; the Newton–Raphson iteration converges from them to the true solution.

### 2.3 Valve Behaviour

The three control valves — PRV, PSV, and FCV — can inhabit one of three principal discrete states at any iteration — **active**, **open**, or **closed** — plus the exception states XFCV and XPRESSURE described below.

- **Active**: the valve enforces its design constraint. A PRV fixes the downstream head equal to its setpoint; a PSV fixes the upstream head equal to its setpoint; an FCV fixes the flow through it equal to its setpoint. When active, these valves introduce a **head constraint** rather than a resistance relationship, and the corresponding row of the linear system is modified accordingly.
- **Open**: the valve is behaving as a short section of pipe with negligible resistance; no constraint is enforced.
- **Closed**: the valve passes no flow.

After each Newton–Raphson iteration the hydraulic state of each control valve is examined:

- A PRV transitions from ACTIVE to OPEN if the upstream head has fallen to or below the setpoint (no pressure reduction needed), or to CLOSED if the flow through it reverses (maintaining the setpoint would require reverse flow).
- A PSV transitions from ACTIVE to OPEN if the downstream head has risen to or above the setpoint, or to CLOSED if the flow through it reverses.
- An FCV transitions to the **XFCV** state (it cannot enforce its setpoint) if the available head difference is insufficient to sustain the target flow, or if the flow through it turns negative.

TCV, GPV, and PCV valves do not have control states; they always contribute a resistance (head loss as a function of flow) determined by their current setting.

**Precise valve status transitions**: PRV and PSV states are re-evaluated after each Newton–Raphson iteration (governed by the DampLimit parameter; see §2.4); **FCV transitions instead belong to the periodic `linkstatus` schedule** (CheckFreq/MaxCheck, plus the convergence-time pass). The transition rules are:

- *PRV*: in the ACTIVE and OPEN states the reverse-flow test is evaluated first and takes precedence — ACTIVE → CLOSED and OPEN → CLOSED if $Q < -\varepsilon_Q$ (reverse flow). Only otherwise: ACTIVE → OPEN if $H_1 - K_m Q^2 < H_\text{set} - \varepsilon_H$ (upstream pressure insufficient to need reduction); OPEN → ACTIVE if $H_2 \geq H_\text{set} + \varepsilon_H$ (downstream pressure reaches setpoint). CLOSED → ACTIVE if $H_1 \geq H_\text{set} + \varepsilon_H$ and $H_2 < H_\text{set} - \varepsilon_H$; CLOSED → OPEN if $H_1 < H_\text{set} - \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$. The special XPRESSURE state (entered only when an active valve renders the solution matrix ill-conditioned; reported as open but unable to deliver pressure) transitions to CLOSED on reverse flow.
- *PSV*: symmetric to PRV — the reverse-flow test again runs first: ACTIVE → CLOSED and OPEN → CLOSED if $Q < -\varepsilon_Q$; only otherwise ACTIVE → OPEN if $H_2 + K_m Q^2 > H_\text{set} + \varepsilon_H$, and OPEN → ACTIVE if $H_1 < H_\text{set} - \varepsilon_H$. From CLOSED, the **OPEN test runs first**: CLOSED → OPEN if $H_2 > H_\text{set} + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$ — when both this and the ACTIVE condition hold, the valve opens; only otherwise CLOSED → ACTIVE if $H_1 \geq H_\text{set} + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$. The XPRESSURE state transitions to CLOSED on reverse flow.
- *FCV*: transitions to XFCV (cannot enforce set point) if the head difference across the valve is negative or flow is negative. A third ACTIVE→XFCV condition also exists: when the valve is active but the pressure drop across it implies a head-loss coefficient smaller than its fully-open minor-loss coefficient (i.e., the network cannot maintain even a friction-free connection without violating the setpoint), the valve also reverts to XFCV. Transitions back to ACTIVE from XFCV once the flow meets or exceeds the setting.

A PRV/PSV/FCV whose status has been fixed OPEN or CLOSED by a control (its setting voided, §7.1) bypasses all of this logic: it is coefficiented as a plain open or closed link and undergoes no status transitions until a new setting is assigned.

Here $H_\text{set}$ is the absolute head setpoint (elevation of the controlled node plus the setting in pressure-head units), $K_m Q^2$ is the minor-loss head drop at the current flow, and $\varepsilon_H$, $\varepsilon_Q$ are user-configured head and flow tolerances. **Important distinction**: $\varepsilon_H = \text{Htol}$ (default 0.0005 ft) and $\varepsilon_Q = \text{Qtol}$ (default 0.0001 ft³/s) are tolerances used in link status transition tests; $\varepsilon_Q$ appears nowhere else, while $\varepsilon_H$ additionally supplies the dead-band for pressure-based simple controls (§7.1). They are entirely separate from the convergence tolerance $\text{Hacc}$ (default 0.001), which governs solver termination via the relative flow-change criterion (§2.4). Confusing $\text{Qtol}$ with $\text{Hacc}$ leads to incorrect valve and check-valve status behaviour.

**Linearisation coefficients for the resistance-type valve types**:

*Throttle Control Valve (TCV)*: the minor-loss coefficient is computed from the valve setting $s$ (a dimensionless loss coefficient value) and pipe diameter $D$ (in feet):

$$K_m = \frac{0.02517 \, s}{D^4}$$

This factor converts from the user-supplied dimensionless loss coefficient into the internal US customary unit system (flow in ft³/s, head in ft). The coefficient enters the standard minor-loss formula $h = K_m Q |Q|$.

*Pressure Breaker Valve (PBV)*: a PBV imposes a fixed head loss equal to its setting $h_\text{set}$ (in feet) when the setting exceeds the current minor-loss head drop $K_m Q^2$. In this active regime the solver enforces the exact head drop by assigning very large linearisation coefficients: $P_k = C_\infty$ and $Y_k = h_\text{set} \cdot C_\infty$, where $C_\infty$ is a large constant ($\approx 10^8$). Because the GGA flow update is $\Delta Q_k = P_k (H_i - H_j) - Y_k$, this drives $H_i - H_j \to Y_k / P_k = h_\text{set}$ extremely strongly on every iteration. If the current minor-loss head drop already exceeds the setting (the valve is overmatched), the PBV instead falls back to ordinary pipe treatment.

*General Purpose Valve (GPV)*: the solver evaluates the user-supplied head-loss-vs-flow curve at the current absolute flow $|Q|$ to extract the local slope $r$ (ft per ft³/s) and zero-intercept $h_0$ (ft) of the bracketing linear segment. The linearisation coefficients are:

$$P_k = \frac{1}{r}, \qquad Y_k = \left(\frac{h_0}{r} + |Q|\right) \mathrm{sign}(Q)$$

*Positional Control Valve (PCV)*: the percent-open setting $s$ is mapped through a user-supplied opening-to-flow-coefficient ratio curve — below the curve's first point the ratio interpolates from the *origin*; above its last point $(x_n, y_n)$ it interpolates along the straight line joining that point to the literal coordinate pair $(1, 1)$ — an anchor evidently intended as $(100\%, 100\%)$ but written as unity on the curve's percent-scaled axes, so for $x_n > 1$ the slope is negative and the computed ratio *decreases* beyond the last point (the actual arithmetic, preserved here as the code's behaviour); the ratio is clamped to $[10^{-6}, 1]$ and the resulting $K_m$ capped at $10^8$ — to obtain the dimensionless ratio $k_{vr} = K_v / K_{v0}$, where $K_{v0}$ is the flow coefficient at full open. The curve's $x$-axis is percent open and its $y$-axis is $K_v / K_{v0}$ as a percentage. The effective minor-loss coefficient is then:

$$K_m = \frac{K_{m0}}{k_{vr}^2}$$

where $K_{m0}$ is the fully-open minor-loss coefficient. This reflects the relationship that for a given flow, halving the effective orifice area quadruples the head loss.

**Active-state matrix modifications for PRV, PSV, and FCV**:

When these valves are in the ACTIVE state, $P_k$ is set to zero and the link is excluded from the standard off-diagonal assembly (which skips links with $P_k = 0$). Instead, the linear system is augmented directly to enforce the valve's constraint:

*PRV active*: the downstream node head is pinned to the absolute setpoint $H_\text{set} = z_{n_2} + s_k$ by injecting a large conductance into the diagonal and a correspondingly large forcing term into the RHS:

$$A_{jj} \mathrel{+}= C_\infty, \qquad F_j \mathrel{+}= H_\text{set} \cdot C_\infty$$

The link's $Y_k$ is set to the current flow plus the downstream node's flow excess, maintaining approximate flow balance. When the downstream node shows a flow *deficit* (negative excess), that deficit is added to the upstream node's RHS to preserve global mass conservation; a positive excess is not transferred.

*PSV active*: identical treatment applied to the upstream node $i$ with $H_\text{set} = z_{n_1} + s_k$. A small residual conductance ($1/C_\infty$) is also added through the off-diagonal entry to preserve matrix connectivity and avoid numerical singularity.

*FCV active*: the link is rendered nearly disconnected by setting $P_k = 1/C_\infty \approx 0$. The setpoint flow $Q_\text{set}$ is injected as an external demand at the upstream node and as an external supply at the downstream node, both in the flow-excess array and in the RHS vector:

$$F_i \mathrel{-}= Q_\text{set}, \qquad F_j \mathrel{+}= Q_\text{set}$$

The two sides of the FCV are effectively decoupled; the network is solved as if $Q_\text{set}$ flows through the valve as a prescribed boundary condition, and the resulting head difference across the valve is whatever the network produces with that imposed flow.

**Check valve (CVPIPE) status transitions**: a check valve is treated as a pipe that is permitted to carry flow only in its positive direction. After each Newton–Raphson iteration, its status is re-evaluated. Let $\Delta h = H_i - H_j$ be the head difference and $Q$ the current flow:

- If $|\Delta h| > H_{\text{tol}}$: CLOSED if $\Delta h < -H_{\text{tol}}$ (reverse head gradient); CLOSED if $Q < -Q_{\text{tol}}$ (reverse flow); otherwise OPEN.
- If $|\Delta h| \leq H_{\text{tol}}$: CLOSED if $Q < -Q_{\text{tol}}$; otherwise the current status is preserved.

This hysteresis prevents rapid cycling near the zero-flow condition.

### 2.4 The Global Gradient Algorithm

The hydraulic solver at each time step employs the **Todini–Pilati Global Gradient Algorithm (GGA)**, a variant of Newton–Raphson that solves simultaneously for all unknown junction heads and then derives all link flows in a single update.

#### Governing Equations

Two sets of equations must be satisfied simultaneously.

**Flow conservation at each junction** $i$:

$$\sum_{k \in \text{in}(i)} Q_k \;-\; \sum_{k \in \text{out}(i)} Q_k \;=\; D_i$$

where the sums are over all links $k$ whose flow enters or leaves junction $i$, and $D_i$ is the total demand withdrawn at node $i$. The demand includes consumer base demand (scaled by patterns), emitter outflow, leakage, and — in pressure-driven mode — the pressure-dependent portion of consumer demand.

**Head-loss equation for each link** $k$ connecting nodes $i$ and $j$:

$$H_i - H_j = h_k(Q_k)$$

where $h_k$ is a nonlinear function of $Q_k$ (pipe friction, pump curve, or valve characteristic). For a pump, $h_k < 0$ (head gain).

The network has $n_j$ unknown junction heads and $n_l$ unknown link flows, giving $n_j + n_l$ unknowns and the same number of equations. The GGA exploits the specific algebraic structure to reduce this to a system of size $n_j$ for the heads alone.

#### Linearisation

At iteration $m$, the head-loss function of link $k$ is linearised around the current flow estimate $Q_k^{(m)}$ by a first-order Taylor expansion:

$$h_k(Q_k) \;\approx\; h_k(Q_k^{(m)}) \;+\; \frac{\partial h_k}{\partial Q_k}\bigg|_{Q^{(m)}} \left(Q_k - Q_k^{(m)}\right)$$

Two derived quantities characterise every link:

$$P_k = \frac{1}{\displaystyle\frac{\partial h_k}{\partial Q_k}\bigg|_{Q^{(m)}}}, \qquad Y_k = P_k \cdot h_k(Q_k^{(m)})$$

$P_k$ is the inverse of the head-loss gradient and has the dimensions of flow per unit head; it plays the role of a hydraulic conductance. $Y_k$ is the normalised head-loss term, representing the flow contribution from the current head-loss value.

#### Assembly of the Linear System

Substituting the linearised head-loss relationships into the flow-conservation equations yields a symmetric sparse linear system for the unknown heads:

$$\mathbf{A} \, \mathbf{H} = \mathbf{F}$$

The coefficient matrix $\mathbf{A}$ has the structure of a weighted graph Laplacian:

$$A_{ii} = \sum_{k \ni i} P_k \qquad \text{(diagonal: sum over all links incident to node } i\text{)}$$

$$A_{ij} = -P_k \qquad \text{(off-diagonal: } k \text{ is the link connecting } i \text{ and } j\text{)}$$

The right-hand side vector $\mathbf{F}$ at junction $i$ accumulates:

$$F_i = \left(\sum_{k \in \text{out}(i)} Y_k - \sum_{k \in \text{in}(i)} Y_k\right) + \Delta_i$$

where $\Delta_i$ includes the fixed-head boundary contributions from any reservoir or tank directly connected to junction $i$ (their known heads multiply the corresponding $P_k$ and are moved to the right-hand side), plus the demand imbalance at the current iteration. Emitters, leakage, and pressure-dependent demands each add their own linearised conductance to the diagonal of $\mathbf{A}$ and their corresponding $Y$-terms to $\mathbf{F}$.

#### Flow Update

Once the linear system is solved for the new heads $\mathbf{H}^{(m+1)}$, the flow in each link is updated as:

$$Q_k^{(m+1)} = Q_k^{(m)} - Y_k + P_k \left( H_i^{(m+1)} - H_j^{(m+1)} \right)$$

This is exactly the Newton correction obtained by inverting the linearised head-loss equation $H_i - H_j = h_k(Q_k)$ for $Q_k$: the current-flow term $Q_k^{(m)}$ and the offset $-Y_k = -P_k h_k(Q_k^{(m)})$ carry the old operating point, while $P_k(H_i - H_j)$ applies the head-driven correction. Here $i$ is the upstream node and $j$ the downstream node of link $k$ according to the assumed positive direction. For pumps, $H_j - H_i = \Delta H_k > 0$, so the sign convention is consistent.

Emitter flows, leakage flows, and pressure-dependent demand flows are similarly updated using their own linearised head-flow relationships.

#### Convergence Criterion

The primary convergence criterion is **flow accuracy**: the sum of absolute flow changes relative to total absolute flow — both sums accumulated over all links *and* all nodal elements (emitter, pressure-dependent-demand, and leakage flows) — must fall below the user-specified tolerance, and no link's hydraulic status may have changed during the most recent iteration:

$$\epsilon = \frac{\displaystyle\sum_k \left| Q_k^{(m+1)} - Q_k^{(m)} \right|}{\displaystyle\sum_k \left| Q_k^{(m+1)} \right|} \leq \epsilon_{\text{tol}}$$

When the total absolute flow $\sum_k |Q_k^{(m+1)}|$ falls at or below $\epsilon_{\text{tol}}$ (near-stagnant network), the relative formula cannot be used; in that case the solver returns the absolute flow change $\sum_k |\Delta Q_k|$ directly rather than the ratio.

Two kinds of status check run during iteration, with different scheduling:

- **Valve status checks** (`valvestatus`, governing PRV and PSV transitions): when `DampLimit = 0` (the default), these run after every flow update. When `DampLimit > 0`, they are deferred until the relative flow error at or below `DampLimit`; at that point damping (relaxation factor 0.6) is also activated — the check occurs after the current iteration's flow update, so the damping takes effect on the *next* iteration's update.
- **Link status checks** (`linkstatus`, governing pumps, check valves, FCVs, and pipes adjacent to tanks): these run periodically. The first check occurs at iteration *CheckFreq* and repeats every *CheckFreq* iterations thereafter, but stops once *MaxCheck* iterations have been reached. This staging prevents premature status oscillation during early iterations when flows are far from convergence.

When `hasconverged` returns true, `linkstatus` and `pswitch` are called unconditionally (regardless of the CheckFreq schedule), alongside the `valvestatus` result already computed for that iteration under the DampLimit schedule. If any of them changes a link's status the periodic-check schedule (`nextcheck`) is advanced and the solve continues — the iteration counter itself is never reset; it only increments toward the iteration cap. Only a convergence pass that produces no status changes terminates the loop.

The **ExtraIter** parameter handles networks that fail to converge because of status cycling — a cycle in which links repeatedly toggle between open and closed states without settling to a consistent configuration. When convergence is not achieved within MaxIter iterations and ExtraIter > 0, an additional ExtraIter iterations are performed during which a convergence pass exits the loop immediately, skipping the convergence-time `linkstatus` and `pswitch` calls — pumps, check valves, FCVs, and tank-adjacent pipes no longer change state at convergence. The periodic `linkstatus` checks are also silent during the extra iterations, but only because *MaxCheck* (default 10) lies far below *MaxIter* (default 200) — an emergent consequence of the defaults rather than an explicit suspension. `valvestatus` (PRV/PSV) continues to run every iteration. This allows the linear system to converge to a solution consistent with the current link configuration, even if that configuration is not the true steady state. A warning is issued that the system is unbalanced. If ExtraIter = −1, the time-step routine wrapping the solver sets a halt flag (`Haltflag`) as soon as the unbalanced solve returns — before the step's results are saved; the results are then saved as usual, and the simulation terminates at the start of the next step rather than stopping mid-step.

Two supplementary convergence criteria may also be applied; rather than terminating the loop early, they make convergence *stricter* — each is tested only after the flow-accuracy criterion has already passed, and can withhold convergence for another iteration. **FlowChangeLimit** requires the maximum absolute flow change over all links *and* nodal elements (emitters, pressure-dependent demands, leakage) during the most recent iteration to be at or below its threshold. **HeadErrorLimit** requires the largest discrepancy, over all open links, between the head difference across the link and the head loss implied by its current flow to be at or below its threshold — a per-link head-loss residual, not a nodal quantity. Both default to zero, which disables them. Two further gates are always active when their models are in use: convergence additionally requires the leakage flows and — in pressure-driven mode — the pressure-dependent demands to have themselves converged. In the default configuration (fixed demands, no leakage) the sole termination criterion is $\epsilon \leq \epsilon_{\text{tol}}$ with no status change.

**Damping (RelaxFactor)**: the Newton flow update $\Delta Q_k$ may be scaled by a relaxation factor to improve convergence stability. By default `RelaxFactor = 1.0` (full Newton step). When `DampLimit > 0` and the relative flow error falls at or below `DampLimit`, `RelaxFactor` is set to 0.6; because the factor is set after the current iteration's flow update, it damps the *following* iteration's updates. This under-relaxation is applied uniformly to all link flow updates, emitter flow updates, pressure-dependent demand flow updates, and leakage flow updates:

$$Q_k^{(m+1)} = Q_k^{(m)} - \text{RelaxFactor} \cdot \Delta Q_k$$

The purpose is to stabilise convergence in networks with highly nonlinear elements (e.g., active control valves) by reducing the step size when the solver is close to convergence but oscillating.

**Matrix recovery (`badvalve`)**: if the Cholesky factorisation of $\mathbf{A}$ fails at a diagonal entry corresponding to node $n$, the solver checks whether an active PRV, PSV, or FCV has that node as one of its endpoints. If found, the valve's status is forced to XPRESSURE (for PRV/PSV) or XFCV (for FCV), breaking the singularity that the active-state matrix modification introduced. The solver then retries the factorisation and solve. This recovery mechanism ensures that ill-conditioned valve configurations do not crash the solver. The recovery examines only the *first* valve found at the offending node; if that one is not active, the solve fails with an ill-conditioning error even when another valve there is active.

The **XFLOW** status ("pump exceeds maximum flow") is not produced by the solver's iterative status logic — the pump status evaluation checks only whether the head gain exceeds the speed-adjusted shutoff head (yielding XHEAD), and does not compare flow against the maximum-flow point of the pump curve. It is instead raised diagnostically *after* a step has been solved: the warning routine (`writehydwarn`) flags a pump as XFLOW when its flow exceeds $\text{setting} \cdot Q_{\max}$ (emitting warning WARN04), and the same status is returned through the public link-status query. XFLOW therefore never alters the solve; it is a reported warning state only.

### 2.5 Sparse Linear Algebra

The matrix $\mathbf{A}$ is symmetric and positive semi-definite (it is a Laplacian, plus small positive diagonal contributions from emitters and demands). Its sparsity pattern corresponds to the adjacency structure of the junction subgraph. Efficient solution is essential because this system must be solved at every Newton iteration of every hydraulic time step.

Three phases are performed:

**Phase 1 — Node reordering (performed once before simulation begins)**: the Multiple Minimum Degree (MMD) algorithm reorders the junction indices to minimise the fill-in that occurs during Cholesky factorisation. Fill-in arises when a non-zero appears in the factor $\mathbf{L}$ at a position that was zero in $\mathbf{A}$; reordering the rows and columns can dramatically reduce the number of such positions. Parallel links (multiple links connecting the same pair of nodes, regardless of their orientation) are condensed into a single equivalent link before reordering to avoid redundancy. (The reordering routine is invoked with its multiple-elimination tolerance set to −1, so in practice one minimum-external-degree node is eliminated per update cycle — plain minimum external degree rather than true multiple elimination.)

**Phase 2 — Symbolic factorisation (performed once before simulation begins)**: using the reordered sparsity pattern, the algorithm predetermines the exact set of non-zero positions that will appear in the lower Cholesky factor $\mathbf{L}$ (satisfying $\mathbf{A} = \mathbf{L} \mathbf{L}^\top$). These positions are stored in a compressed sparse form. From this point forward, only numerical values need to change; the structure is fixed.

**Phase 3 — Numerical factorisation and solution (performed at every Newton iteration)**: the current values of $P_k$ are inserted into the pre-allocated arrays, the Cholesky factorisation is carried out in the stored non-zero positions, and forward and backward substitution yields the updated head vector $\mathbf{H}$.

Because the sparsity structure does not change between iterations (only the values change), Phases 1 and 2 are not repeated, making per-iteration cost proportional only to the number of non-zeros in $\mathbf{L}$.

---

## 3. Demand Models

### 3.1 Demand-Driven Analysis

In the default **Demand-Driven Analysis (DDA)** mode, all demands are treated as fixed withdrawals regardless of the pressure at the node. Each junction has one or more demand categories; within each category a base demand rate is multiplied by the current value of its associated time pattern to give the instantaneous withdrawal rate. Multiple categories are summed. A global **demand multiplier** ($D_\text{mult}$) is also applied uniformly to all base demands at all junctions, allowing the overall demand level to be scaled up or down without modifying individual data. If the net demand at a node is negative, the node acts as an inflow point (external source).

This model is simple and numerically robust, but it can produce physically unrealistic results for heavily stressed systems: a node with insufficient pressure will still show its full demand satisfied, possibly at a negative computed pressure.

### 3.2 Pressure-Driven Analysis

In **Pressure-Driven Analysis (PDA)** mode, the demand actually delivered depends on the available pressure at each node. The governing relationship is:

$$D(P) = \begin{cases} 0 & P \leq P_{\min} \\ D_{\text{full}} \left( \dfrac{P - P_{\min}}{P_{\text{req}} - P_{\min}} \right)^{n_P} & P_{\min} < P < P_{\text{req}} \\ D_{\text{full}} & P \geq P_{\text{req}} \end{cases}$$

where $P$ is the gauge pressure at the node, $P_{\min}$ is the pressure below which no demand is delivered, $P_{\text{req}}$ is the pressure at which full demand is delivered, $D_{\text{full}}$ is the requested demand, and $n_P$ is the pressure exponent. The default value of $n_P = 0.5$ corresponds to the Wagner formula, which models demand as proportional to the square root of the available pressure head above the minimum threshold. The three parameters are **global** — one set for the whole network — defaulting to $P_{\min} = 0$, $P_{\text{req}} = 0.1$, $n_P = 0.5$, with $P_{\text{req}} - P_{\min} \geq 0.1$ psi (or m) enforced. Pressure-dependent treatment applies only to junctions whose pattern-adjusted total demand is positive: zero- and negative-demand junctions (external inflow points) keep their fixed demand-driven values even in PDA mode.

The PDA model is incorporated into the GGA by treating the pressure-dependent component of demand as a pressure-dependent emitter at each junction (cf. §4). The demand-pressure function is inverted to express pressure as a function of demand:

$$P = P_{\min} + (P_{\text{req}} - P_{\min}) \left( \frac{D}{D_{\text{full}}} \right)^{1/n_P}$$

This is linearised and added to the diagonal of $\mathbf{A}$ and the right-hand side $\mathbf{F}$. Barrier terms prevent the numerical demand from drifting below zero or above $D_{\text{full}}$, maintaining the physical bounds throughout the iteration. The barrier is implemented as a smooth differentiable approximation (not a hard constraint) to avoid discontinuities that would break the Newton–Raphson iteration. Specifically, the signed head-loss and gradient increments from a lower barrier at $Q = 0$ take the form:

$$\Delta h = \frac{a - \sqrt{a^2 + 10^{-6}}}{2}, \qquad \Delta(\partial h/\partial Q) = \frac{10^9}{2}\left(1 - \frac{a}{\sqrt{a^2 + 10^{-6}}}\right), \qquad a = 10^9 \, Q$$

which approaches a large one-sided penalty as $Q \to 0^-$ while remaining smooth and differentiable throughout. An analogous upper barrier is applied at $Q = D_{\text{full}}$. Each iteration's change in a node's pressure-dependent demand is additionally capped at 40% of its full demand, preventing overshoot of the bounded demand function. PDA convergence is tested by re-evaluating the demand function at the newly computed pressures and requiring agreement with the solved demand flows within $10^{-4}$ ft³/s at every node; the same pass counts pressure-deficient junctions and the total percent demand shortfall reported to the user.

---

## 4. Emitters

An emitter represents a device — a sprinkler head, orifice, or nozzle — that discharges water from a junction at a rate governed by the local pressure. The emitter flow is:

$$Q_e = K_e \, P^{n_e}$$

where $K_e$ is the per-junction emitter discharge coefficient, $P$ is the gauge pressure at the junction, and $n_e$ is the pressure exponent — a single **network-wide** option (default 0.5, the orifice value); only the coefficient varies per junction. In US units the coefficient is defined per psi$^{n_e}$ (adjusted for specific gravity), in SI per metre of head. Emitters can also represent aggregate leakage if high spatial resolution is not required.

Within the GGA, an emitter is treated as an additional element at its junction. The head-flow relationship is inverted:

$$H - z = C_e \, Q_e^{1/n_e}$$

where $H - z$ is the pressure head and $C_e = K_e^{-1/n_e}$. This is linearised at the current flow estimate and added to the matrix: the linearised conductance $P_e = 1 / (\partial h_e / \partial Q_e)$ is added to the diagonal of $\mathbf{A}$ at the relevant junction, and the corresponding $Y_e$ term is added to the right-hand side. When the head-loss gradient falls below a resistance tolerance ($10^{-7}$), the emitter relation is replaced by a linear one to keep the Jacobian bounded near zero flow.

By default emitters can admit reverse flow (suction) if the junction pressure falls below the emitter's reference elevation. An **emitter backflow** option can disable this: when backflow is forbidden, a lower barrier function is applied to the head-loss gradient whenever the emitter flow would go negative. The barrier takes the same smooth differentiable form used for PDA demand bounds (§3.2):

$$\Delta h = \frac{a - \sqrt{a^2 + 10^{-6}}}{2}, \qquad \Delta(\partial h/\partial Q) = \frac{10^9}{2}\left(1 - \frac{a}{\sqrt{a^2 + 10^{-6}}}\right), \qquad a = 10^9 \, Q_e$$

This adds a large one-sided penalty as $Q_e \to 0^-$ while remaining smooth and differentiable, strongly driving $Q_e \geq 0$ without creating a hard discontinuity that would break convergence.

---

## 5. Pipe Leakage — the FAVAD Model

Background leakage from deteriorated pipes — through corroded joints, stress cracks, and micro-fractures — is modelled using the **FAVAD** (Fixed And Variable Area Discharge) framework. Unlike a simple orifice, pipe cracks may dilate under pressure, making the effective discharge area itself pressure-dependent. The FAVAD model captures this through:

$$Q_{\text{leak}} = C_o \, \frac{L}{100} \left( A_o + m H \right) \sqrt{H}$$

where $H$ is the pressure head driving leakage from the pipe, $L$ is the pipe length, $A_o$ is the fixed (zero-pressure) crack area **per 100 units of pipe length**, $m$ is the rate of increase of crack area with pressure head (same per-100-length basis), and $C_o = 0.6\sqrt{2g} \approx 4.815$ in pure ft units — leakage scales linearly with pipe length. The crack parameters are supplied in mm² (and mm² per metre of head): internally the coefficient absorbs the mm²→m² factor $10^{-6}$ and a further division by $0.3048^2$ (fixed-area term) or $0.3048$ (variable-area term), so with areas in their input units the effective fixed-area coefficient is $\approx 5.18 \times 10^{-5}$. Expanding this:

$$Q_{\text{leak}} = C_o \tfrac{L}{100} A_o H^{1/2} + C_o \tfrac{L}{100} m H^{3/2}$$

The two terms have different pressure exponents: the fixed-area term behaves like a standard orifice (exponent $1/2$), while the variable-area term has exponent $3/2$.

These are decomposed into **two equivalent emitters** at each node, one for each component:

$$H = C_{\text{fa}} \, Q_{\text{fa}}^{2} \qquad \text{(fixed-area component, orifice-type, exponent } 1/2 \text{ on } H\text{)}$$

$$H = C_{\text{va}} \, Q_{\text{va}}^{2/3} \qquad \text{(variable-area component, exponent } 3/2 \text{ on } H\text{)}$$

The resistance coefficients $C_{\text{fa}}$ and $C_{\text{va}}$ are determined from the FAVAD parameters. For each pipe whose **both** end nodes are junctions, the pipe's leakage contribution is split equally: half is attributed to each end node (the pipe is split conceptually at its midpoint, with each half's leakage then driven by that end node's own pressure head rather than a single midpoint value). When one end of a pipe is a fixed-grade node (reservoir or tank), that fixed-grade end cannot accumulate leakage in the nodal model; the junction at the other end therefore receives the **full** pipe-length contribution rather than half. A pipe whose *both* end nodes are fixed-grade contributes no leakage at all — leakage is realised as a nodal demand, which fixed-grade nodes carry none of. The contributions of all pipes meeting at a given junction are aggregated: the total fixed-area conductance and variable-area conductance at the node are the sums over all incident (half- or full-) pipe contributions. The resulting nodal coefficients are then inverted to form $C_{\text{fa}}$ and $C_{\text{va}}$.

**Derivation of $C_{\text{fa}}$ and $C_{\text{va}}$**: let $\text{LeakCoeff1}_p = C_o A_o L_p / 100$ and $\text{LeakCoeff2}_p = C_o\, m\, L_p / 100$ be the full-pipe FAVAD discharge coefficients for pipe $p$ (unit conversions absorbed as above). The per-end contribution for a junction endpoint $v$ of pipe $p$ is:

$$k_{1,p,v} = \begin{cases} \tfrac{1}{2}\,\text{LeakCoeff1}_p & \text{both end nodes of pipe } p \text{ are junctions} \\ \text{LeakCoeff1}_p & \text{exactly one end node of pipe } p \text{ is a fixed-grade node} \end{cases}$$

(with the same rule applied to $\text{LeakCoeff2}_p$ to give $k_{2,p,v}$). For junction $i$, the total discharge conductances are $K_{\text{fa},i} = \sum_{p \ni i} k_{1,p,i}$ and $K_{\text{va},i} = \sum_{p \ni i} k_{2,p,i}$. The resistance coefficients follow by inverting the discharge relations $Q = K H^{1/2}$ (fixed-area) and $Q = K H^{3/2}$ (variable-area):

$$C_{\text{fa},i} = 1/K_{\text{fa},i}^{2}, \qquad C_{\text{va},i} = 1/K_{\text{va},i}^{2/3}$$

(with the respective term omitted when $K = 0$).

These two emitter-like terms are linearised and incorporated into the GGA matrix assembly in exactly the same way as ordinary emitters (§4): a conductance term is added to the diagonal of $\mathbf{A}$ and a flow offset term is added to the right-hand side $\mathbf{F}$.

Leakage is **non-negative by construction**: the smooth lower barrier of §3.2 is applied *unconditionally* to both leakage components — unlike emitters, where it is conditional on the backflow option — so a node at sub-atmospheric pressure simply leaks nothing; the post-solution per-pipe leakage report likewise clamps negative pressure heads to zero. Leakage carries its own **convergence gate**: each node's solved leakage must match a direct FAVAD evaluation at the current pressure, $\sqrt{H/C_{\text{fa}}} + (H/C_{\text{va}})^{3/2}$, within $10^{-4}$ ft³/s; leakage flow corrections are damped by the same relaxation factor as the flow update and enter the global relative-flow-change measure (§2.4). Before iteration begins, the nodal elements are seeded with nonzero flows — 1.0 ft³/s per emitter, 0.001 ft³/s per leakage component, and the full demand for pressure-dependent demands — and at the converged solution the **reported junction demand aggregates all three outflow elements**: consumer demand plus emitter flow plus leakage flow.

---

## 6. Time-Stepping and Tank Dynamics

### 6.1 Extended-Period Simulation

The extended-period simulation (EPS) advances the network state through a sequence of discrete hydraulic time steps of duration $\Delta t$. Within each hydraulic step the network configuration (demands, pump settings, valve statuses) is assumed constant, and a steady-state hydraulic solution is computed. The procedure at each step is:

1. **Apply patterns** (`demands`): evaluate all time patterns at the current clock time and apply the resulting multipliers to demands at junctions, reservoir heads, and pump speed settings.
2. **Simple controls** (`controls`): evaluate and apply all simple controls that trigger at this time (§7.1). May change link status or settings.
3. **Solve** (`hydsolve`): solve for hydraulic equilibrium via the Global Gradient Algorithm (§2.4).
4. **Compute pump power** (`getallpumpsenergy`): determine the current power draw and efficiency at each pump using the just-solved flow field.
5. **Determine time step** (`timestep`): compute the next time-step duration as the minimum of the nominal step, reporting interval, pattern change, tank fill/drain time, and rule evaluation (§6.2). Rule-based controls (§7.2) are evaluated within this step at intermediate rule-step intervals; if a rule fires, the time step is shortened to the firing time. Tank levels are updated within this computation (not after it).
6. **Accumulate energy** (`addenergy`): accumulate pump energy consumption (kWh) and cost over the time step. Track peak demand.
7. **Update flow balance** (`updateflowbalance`): accumulate volumetric flow balance ledger entries for the step.
8. **Advance clock**: advance the simulation time by the computed time-step duration.

Steps 1–8 repeat until the specified simulation duration is reached.

**Initial hydraulic state**: PRV/PSV/FCV valves carrying a numeric setting begin the simulation ACTIVE; any non-GPV valve initialised to a fixed OPEN/CLOSED status has its setting wiped (no setpoint is enforced until one is assigned); junctions with emitters start from an emitter-flow guess of 1.0 ft³/s; tank pseudo-links start TEMPCLOSED; and on re-initialisation existing link flows are preserved as a warm start unless re-initialisation is explicitly requested or the flow is near zero.

**Important**: rule-based controls are evaluated **after** the hydraulic solve, within the time-step computation (step 5), not before or during the solve. If a rule fires, the hydraulic step is shortened so that the next solve will reflect the new configuration — the current step's solution is **not** re-computed. Tank levels are updated by `ruletimestep()` at each rule sub-step interval; when no rules exist, `tanklevels()` is called directly during `timestep()`.

### 6.2 Adaptive Time Step

The hydraulic time step is not fixed; it adapts so that no physical limit is overshot. The actual step duration used is the minimum of the following quantities:

- The user-specified nominal hydraulic time step.
- The time remaining until the next reporting interval (so that results are recorded at exactly the right moments).
- For each tank, the time at which the tank would reach its minimum level (if the net outflow continues at the current rate) or its maximum level (if the net inflow continues): $\Delta t_{\text{tank}} = \Delta V_{\text{available}} / |Q_{\text{net}}|$.
- The time until the next scheduled change in any pattern (so that demand or pump multipliers change at exactly the right instant).
- The time until the next simple control activates (the earliest instant at which a timer control fires or a tank-level control crosses its threshold at the current fill/drain rate) — considering only controls whose action would actually change the link's current status or setting; level controls on junction pressures never participate, and timer controls fire only on exact clock coincidence, so a timer the clock steps past is skipped entirely.
- When rule-based controls are present, the time at which the next rule fires — determined by sub-stepping the rule evaluation across the hydraulic period (see §6.1 and §7.2).

The end of the simulation is *not* one of these minimised quantities; it is enforced separately, by the main loop simply ceasing to issue steps once the clock reaches the specified duration. This adaptive strategy avoids the need for post-hoc correction of tank levels and ensures pattern changes and control actions are applied at their intended times.

**Default time step derivation**: if the user does not specify either the quality time step or the rule evaluation time step, both default to $\Delta t_h / 10$, capped at $\Delta t_h$. The rule time step is additionally aligned so that evaluations fall on even multiples of the rule step within each hydraulic period — the first evaluation within a period may therefore be shorter than one full rule step to achieve this alignment. The quality time step is further constrained so it never exceeds the hydraulic time step. The hydraulic time step itself is clamped at the minimum of the user-specified nominal step, the pattern time step, and the reporting time step.

### 6.3 Tank Level Update

After each hydraulic solution, the change in stored volume during the time step is:

$$\Delta V = Q_{\text{net}} \cdot \Delta t$$

where $Q_{\text{net}} = Q_{\text{in}} - Q_{\text{out}}$ is the net volumetric flow rate into the tank. For a **constant cross-section** tank with cross-sectional area $A$, the level change is:

$$\Delta h = \frac{\Delta V}{A}$$

For a tank described by a **volume-elevation curve**, the new volume $V_{\text{new}} = V_{\text{old}} + \Delta V$ is looked up in the curve to find the corresponding new water surface elevation.

The post-step limit handling is a predictive one-second **snap**, compensating for the fill/drain time being rounded to the nearest whole second: a tank within one second's net inflow of its maximum volume is set to exactly $V_{\max}$ (the level need never actually cross the limit), and one that has fallen a second's net outflow past its minimum is set to $V_{\min}$; the whole update, head recomputation included, is skipped when $|Q_{\text{net}}| \leq 10^{-6}$ ft³/s. A tank at its minimum is treated as a fixed-grade node (like a small reservoir at its minimum head) for the next time step. The behavioural difference between the two overflow modes is enforced *during the hydraulic solve* rather than by this snap. When overflow is allowed, an over-full tank keeps accepting inflow and the surplus exits freely as overflow while the level is held at the maximum; when overflow is not permitted, the inflow links are held closed (see TEMPCLOSED below) so the tank cannot exceed its maximum, and it is treated as fixed-grade at that level.

**TEMPCLOSED for links at tank limits**: independently of the level-clamping applied after the hydraulic step, whenever the solver performs a link-status check (at the periodic `CheckFreq` interval while the iteration count remains within `MaxCheck`, and whenever a trial solution first satisfies the convergence criteria) the status of every link adjacent to a tank is examined. If a link is carrying flow *into* a tank whose current head equals or exceeds the maximum level and overflow is not permitted, that link is set to TEMPCLOSED — it retains only the vanishingly small conductance assigned to closed links, and the coefficient matrix is assembled as if the link were closed. Similarly, a link carrying flow *out of* a tank at its minimum level is TEMPCLOSED. At the start of each such status check, all TEMPCLOSED and XHEAD links are first re-opened before the new operating point is evaluated, so the status is re-tested rather than being locked in.

---

## 7. Control Systems

### 7.1 Simple Controls

A simple control consists of a single condition and a single action. The condition is either a **level control** — a node's pressure or hydraulic grade exceeds or falls below a specified threshold — or a **timer control** — the simulation clock reaches a specified time or the current time of day reaches a specified hour.

When a simple control fires, its action is applied immediately: it may open or close a link, change a pump's speed setting, or change a valve's setting. Status and setting actions on valves are mutually destructive: opening or closing a valve *discards* its setting (an "open" action converts an active PRV/PSV/FCV into a plain open link), assigning a setting to an FCV forces it ACTIVE, and assigning a setting to a closed valve whose setting was wiped re-opens it; a pump opened by status takes speed 1.0 and closed takes 0.0, and a PCV's loss coefficient is recomputed the moment its setting changes. Tank-level and time-based controls are evaluated once at the start of each hydraulic time step; if such a control fires and changes the network configuration, the subsequent hydraulic solution reflects the new state for the entire duration of that time step. Simple controls conditioned on a **junction** pressure or grade are handled differently: because the controlling pressure is itself an output of the solve, they are re-checked *after* the hydraulic system has converged, by the same `pswitch` routine that runs inside the solver loop. If the converged pressure has crossed the control threshold, the link status is switched and the system is re-solved, iterating until no such control changes state.

For **level controls** on tanks, a small hysteresis margin is applied to prevent chattering when the tank is exactly at the control threshold under non-zero flow. The trigger condition is checked against the tank volume corresponding to the control's grade level, with a margin equal to the current absolute value of the tank's net demand flow rate (`|NodeDemand|`, in internal flow units). A low-level control fires when the current tank volume falls at or below the threshold volume plus the margin; a high-level control fires when the current tank volume reaches or exceeds the threshold volume minus the margin.

Actions are normalised at parse time: pump OPEN/CLOSED map to speeds 1/0, a numeric pump or pipe setting maps to open/closed status, GPVs accept only OPEN/CLOSED, check-valve pipes cannot be controlled at all, and clock-time triggers wrap modulo 24 hours. Both simple controls and rules may carry a `DISABLED` tag removing them from evaluation — though the post-convergence junction-pressure pass does not honour the flag, so a disabled pressure-conditioned control still fires there.

### 7.2 Rule-Based Controls

Rule-based controls support arbitrarily complex conditional logic and are well suited to representing operational strategies such as "turn on pump A if tank X level falls below 5 m AND time of day is between 22:00 and 06:00."

A rule has the structure:

> **IF** (premise₁) **AND/OR** (premise₂) **…** **THEN** (action₁, action₂, …) **ELSE** (action₃, action₄, …)

Premises may test:

- Node pressure, hydraulic grade, **level** (head above the node's elevation), or demand
- Total **system demand**
- Link flow rate, status, or setting (a `POWER` keyword exists in the grammar's vocabulary, but no premise using it can actually be created)
- Tank fill time (time for the tank to reach its maximum at the current inflow rate) or drain time — each false unless the tank is actually filling (respectively draining)
- Simulation time or time of day

Premises evaluate **left-to-right**: an `OR` clause is consulted only when the accumulated result is false, and subsequent `AND` clauses must still hold — approximating AND binding tighter than OR, except that a false result reaching an `AND` clause rejects the rule outright, so any later `OR` alternatives are never consulted; no parentheses are available. Numeric comparisons use a fixed tolerance of 0.001 in user units (with ≤/≥ shifted by it); flow premises test the *absolute value* of flow; status premises map internal XHEAD/TEMPCLOSED states to CLOSED and admit only IS/NOT; and a setting premise on a valve whose setting has been voided is always false. Clock-time equality premises are satisfied if the stated instant fell anywhere within the elapsed rule interval — including across midnight — while inequalities compare only the interval's end.

Rules are evaluated not just at the start of a hydraulic time step but at intermediate **rule time steps** that subdivide each hydraulic step. When a rule fires and changes the network state, the current hydraulic step is shortened to end at the firing time; consistent with §6.1, the shortened step's solution is not recomputed — the next hydraulic step is solved afresh from the new configuration. This allows the simulation to capture, for example, a pump switching on mid-step in response to a falling tank level.

When multiple rules fire simultaneously and their THEN actions conflict (e.g., two rules disagree on the status of the same pump), **priority levels** resolve the conflict: the rule with the *strictly* higher priority value wins — on a tie, the rule evaluated first keeps its action. Rule actions are normalised like simple-control actions (check-valve pipes cannot be acted on, GPVs take no numeric setting, a numeric pipe setting converts to open/closed status), and a setting action is a no-op unless it changes the setting by more than 0.001 — which also determines whether it counts as a state change that shortens the hydraulic step.

---

## 8. Water Quality Simulation

### 8.1 Overview and Simulation Modes

Water quality simulation is layered on top of the hydraulic solution. The flows and velocities computed during the hydraulic phase are stored and replayed during quality simulation, which advances in **quality time steps** that are no longer than hydraulic time steps, and are typically shorter, in order to resolve the advection of concentration fronts through pipes. The quality time step $\delta t$ is a user-specified parameter (defaulting to one-tenth of the hydraulic step), typically a small fraction of the hydraulic time step to satisfy the Courant stability condition for the advection scheme. Within each hydraulic period of duration $\Delta t_h$, the quality simulation executes multiple transport sub-steps of size $\min(\delta t, \text{remaining time})$, iterating until the full hydraulic period is consumed; within each sub-step reactions are applied *first* and advection second — an operator split whose ordering matters at coarse $\delta t$ — with the reaction pass skipped entirely when no pipe carries a nonzero bulk or wall coefficient and no tank a nonzero bulk coefficient. The hydraulic flow field (velocities, directions) is held constant throughout the sub-cycles. Any change of a link's direction *class* (positive, negative, or negligible — flow below `Q_STAGNANT` classes as zero) between consecutive hydraulic periods triggers a re-sort of the nodes into topological order, and a link whose flow reverses sign additionally has its segment list reversed in place so the leading end stays downstream.

Three simulation modes are available:

- **Chemical concentration**: the transport and reaction of a dissolved constituent (e.g., residual chlorine) through the network.
- **Water age**: the average time water has been resident in the distribution system, measured from the sources. No external source or reaction is needed; the age field is initialised from each node's initial quality $C_0$ (zero by default — nonzero initial ages are legal) and increases at a uniform rate as time passes.
- **Source tracing**: the percentage of water at any point in the network that originated from a designated source node. The tracer is injected at 100% at the source; all other inflowing water carries 0%.

### 8.2 Lagrangian Segment Transport

Advective transport of water quality through pipes is modelled with a **Lagrangian moving-segment** scheme. Each pipe is represented as an ordered sequence of segments. Each segment has a volume and a uniform concentration. Segments are created when new water of a different concentration enters a pipe and destroyed when they are fully flushed out the other end. At startup each pipe holds one full-volume segment at its *downstream* node's initial quality; pumps and valves start with no segments; each tank starts as one segment at $(C_0, V_0)$ — split for two-compartment tanks so the mixing zone fills first.

**Advection step**: over a quality time step of duration $\delta t$, a volume $\mathcal{V}_k = Q_k \, \delta t$ of water is swept through pipe $k$. Consumption proceeds from the **downstream (leading) end** of the pipe — the segments nearest the receiving node — inward, one segment at a time. The mass and volume of each consumed fraction are tracked; when the cumulative volume consumed equals $\mathcal{V}_k$, the remaining portion of the last partially-consumed segment is retained as the new leading (downstream-end) segment. The total mass and volume that exited the pipe's downstream end are accumulated at the downstream node.

**Nodal mixing**: transport is organised as a single sweep over the nodes in topological order, not as separate all-pipes phases. When a node's turn comes, the advection step above is applied to each pipe carrying flow *into* it, and its outflow concentration is computed as:

$$c_{\text{out},i} = \frac{\displaystyle\sum_{k \in \text{in}(i)} m_k^{\text{out}}}{\displaystyle\sum_{k \in \text{in}(i)} \mathcal{V}_k^{\text{out}}}$$

where $m_k^{\text{out}}$ and $\mathcal{V}_k^{\text{out}}$ are the mass and volume that flowed out of (i.e., into node $i$ from) pipe $k$ during the quality step. This is the **complete instantaneous mixing** assumption: water entering a junction from all sources is instantly and uniformly blended. A junction with net negative demand (an external inflow point) adds that inflow volume at zero concentration to the denominator, diluting the blend unless a concentration source supplies its mass. The resulting concentration $c_{\text{out},i}$ — after tank mixing and any source contribution — is then pushed immediately, before the next node is processed, as a new upstream (trailing) segment into each pipe carrying flow away from node $i$. Because each pipe's upstream node is swept before its downstream node, this push occurs *before* that pipe's own consumption: when $Q_k\,\delta t$ exceeds the pipe's stored volume, the downstream node draws water that entered the pipe during the same sub-step.

**Topological ordering**: for the advection step to be computed correctly — that is, for each node's outflow concentration to reflect all its upstream contributions — the nodes must be processed in topological order (upstream nodes before downstream nodes). The network is sorted into such an order prior to the quality simulation. For looped networks where no pure topological sort exists, nodes that share mutual dependencies are processed with a fallback ordering strategy.

**Stagnant junctions**: when a junction receives no inflow at all during a quality time step — neither from links nor as external inflow (a dead-end or temporarily stagnant condition) — and the constituent is reactive, the junction concentration is updated to the arithmetic mean of the concentrations in the segment **nearest to the junction** on each incident link. For a pipe whose flow runs into the node (the node is the pipe's downstream end), this is the pipe's downstream-end segment (`FirstSeg`); for a pipe whose flow runs away from the node (the node is the pipe's upstream end), this is the pipe's upstream-end segment (`LastSeg`). In both cases the chosen segment is the one physically adjacent to the stagnant junction, regardless of flow direction. This heuristic prevents the unphysical drift in concentration that would otherwise occur at stagnant junctions driven purely by reaction kinetics without advective renewal.

**Segment merging**: after the outflow concentration of a node is computed, a new segment is pushed into each pipe carrying flow away from the node. Before a new segment is created, the node's outflow concentration $c_\text{node}$ is compared with the concentration of the segment already sitting at the upstream end of that outflow pipe (the pipe end adjacent to the node). If the difference is less than the threshold $C_\text{tol}$ (default 0.01 — mg/L for chemicals, hours for age), the existing segment's volume and mass are merged with the new contribution rather than creating a separate segment; the same tolerance governs inflow-segment coalescing in FIFO and LIFO tanks. Without this merging step, each transport sub-step could create a new segment boundary, causing the total segment count to grow without bound in steady flow. Merging keeps the count manageable at the cost of negligible concentration smoothing.

**Segment memory management**: pipe segments are allocated from an in-memory pool that grows on demand in large blocks, backed by a free-list of recycled records. When a segment is fully flushed out of a pipe it is returned to the free list rather than released to the operating system. New segments are drawn from the free list first; fresh pool entries are used only when the free list is empty. If the pool cannot be extended, an out-of-memory flag is raised and quality simulation is terminated gracefully with an error.

### 8.3 Source Terms

**Water quality sources** inject constituent into the network at designated nodes. Four injection types are available:

- **Concentration source**: the concentration of all water leaving the node is fixed at the specified value. At **reservoirs** the source *replaces* the outflow quality; at **tanks** it is *added on top of* the tank's mixed outflow quality, leaving the stored quality unmodified. At **junctions** this type is effective only when the node has a net negative demand (i.e., the junction is itself a local inflow point, `NodeDemand < 0`); if the junction demand is non-negative the source contributes nothing. Inflows from other links are not overridden at junctions.
- **Mass inflow booster**: a fixed mass rate is added to the node continuously, regardless of flow conditions. The resulting concentration increment depends on the total flow through the node.
- **Setpoint booster**: if the naturally mixed concentration at the node falls below the specified setpoint, it is raised to the setpoint. If it already exceeds the setpoint, no adjustment is made.
- **Flow-paced booster**: a fixed concentration increment is added to the natural concentration of all water leaving the node, in proportion to the flow.

Source concentrations may vary over time via a multiplier pattern. Mass-source strengths are interpreted as mass per **minute** (converted internally to per-second); the other source types are concentrations in user units. A source whose baseline strength is zero is inert regardless of its pattern.

For all source types, a **stagnation guard** suppresses injection when the total volumetric outflow *rate* from the node during a quality sub-step falls at or below a small threshold `Q_STAGNANT` $= 1.114 \times 10^{-5}$ ft³/s (0.005 gpm). This avoids division by zero — and unstable large increments — when computing concentration increments at a near-stagnant node. The *same* `Q_STAGNANT` threshold also governs whether a link's flow is treated as negligible for the topological-sort and transport steps (§8.1); it is one constant serving both purposes, not two. (It is distinct from the hydraulic constant `QZERO` $= 10^{-6}$ ft³/s, which seeds the flow of closed links and serves as the hydraulic engine's own negligible-flow threshold — in the tank level update of §6.3 and the tank and control time-step checks of §6.2 — but plays no role in the quality engine.)

**Water age** requires no source. The "concentration" is initialised from each node's initial quality $C_0$ (zero by default, as noted in §8.1) and incremented by $\delta t$ (in hours) at every quality time step, representing the elapsed time since the water entered the system from a source (reservoir or tank).

**Source tracing** assigns a "concentration" of 100 (representing 100%) to all water leaving the designated trace node. All water entering the network from other *reservoirs* carries a concentration of zero permanently; a tank's reported node quality starts at zero, though its stored volume segments are seeded from its initial quality $C_0$ (per §8.2), and tanks **store and re-release** whatever tracer reaches them through their mixing models. The trace value at any pipe segment or junction then represents the fraction of that water — expressed as a percentage — that originated from the traced source.

### 8.4 Chemical Reactions

Chemical decay or growth of the constituent occurs simultaneously with transport. Reactions are applied independently in each pipe segment and in each tank (as a whole, subject to its mixing model).

#### Bulk Reactions

Bulk reactions occur within the water volume and are governed by:

$$r_{\text{bulk}} = k_b \cdot f(c)$$

where $k_b$ is the bulk reaction rate coefficient and $f(c)$ is the concentration potential, which depends on the reaction order:

- **Zero order** ($n = 0$): $f(c) = 1$ — the reaction proceeds at a constant rate independent of concentration.
- **First order** ($n = 1$): $f(c) = c$ — the reaction rate is proportional to concentration; this covers simple first-order decay (e.g., chlorine demand).
- **Second order** ($n = 2$): $f(c) = c^2$.
- **$n$-th order with limiting concentration $C_L$**: $f(c) = c^{n-1} \cdot c_{\text{potential}}$, where the potential accounts for the approach to a limiting residual (decay toward a non-zero floor, or growth toward a ceiling).
- **Michaelis-Menten kinetics** (negative order): $f(c) = c / (C_L + \mathrm{sign}(k_b)\,c)$ — the denominator is $C_L + c$ for growth ($k_b > 0$) and $C_L - c$ for decay ($k_b < 0$), so the sign is governed by the sign of the rate coefficient. This models saturation kinetics — the reaction rate is approximately first-order at low concentrations and approximately zero-order at high concentrations relative to the half-saturation constant $C_L$.

#### Wall Reactions

Wall reactions represent the interaction of the constituent with the pipe wall material (e.g., disinfectant demand from biofilm or iron corrosion products). The mass transfer process has two stages in series: molecular diffusion from the bulk water to the wall, and the chemical reaction at the wall surface. Both stages must be overcome for mass to be transferred.

The analysis proceeds as follows:

1. Compute the **Reynolds number** $Re = VD/\nu$ and the **Schmidt number** $Sc = \nu / \mathcal{D}$, where $\nu$ is the kinematic viscosity of water and $\mathcal{D}$ is the molecular diffusivity of the constituent.

2. Compute the **Sherwood number** $Sh$, which characterises the ratio of convective to diffusive mass transfer:
   - Stagnant ($Re < 1$): $Sh = 2$ (pure diffusion limit)
   - Laminar ($1 \leq Re < 2300$): Graetz–Lévêque solution for developing concentration profiles in a tube:

   $$Sh = 3.65 + \frac{0.0668 \,(D/L)\,Re\,Sc}{1 + 0.04\,[(D/L)\,Re\,Sc]^{2/3}}$$

   - Turbulent ($Re \geq 2300$): Notter–Sleicher correlation:

   $$Sh = 0.0149 \, Re^{0.88} \, Sc^{1/3}$$

   (The fractional exponents $2/3$ and $1/3$ are evaluated as the truncated constants 0.667 and 0.333.)

3. The **mass transfer coefficient** is:

   $$k_f = \frac{Sh \cdot \mathcal{D}}{D}$$

4. For **first-order** wall reactions ($n_w = 1$), the wall reaction rate $k_w$ (with units of velocity $[\text{m/s}]$) and the mass-transfer coefficient $k_f$ combine in series to give an effective first-order wall decay coefficient:

   $$k_{\text{eff}} = \frac{4}{D} \cdot \frac{k_w \, k_f}{k_f + |k_w|}$$

   Here $4/D$ converts from a surface-area basis to a volume basis for a circular pipe. The series combination ensures that if either the diffusion step ($k_f$) or the wall reaction step ($k_w$) is slow, it dominates the overall rate.

5. For **zero-order** wall reactions ($n_w = 0$), the wall demand rate $k_w$ (converted to internal units as $k_w \cdot f_u^2$ where $f_u$ is the elevation unit conversion factor) and the concentration-dependent diffusive supply rate $c \cdot k_f$ are compared independently. The effective volumetric wall rate is $\mathrm{sgn}(k_w) \cdot \min(|k_w \cdot f_u^2|,\; c \cdot k_f) \cdot 4/D$. When the diffusion boundary layer cannot supply mass as fast as the wall consumes it ($c \cdot k_f < |k_w|$), the reaction becomes mass-transfer-limited and concentration-dependent despite the nominally zero-order kinetics.

Setting the molecular diffusivity to **zero disables the mass-transfer stage entirely**: the Sherwood analysis is skipped, first-order wall kinetics act at their intrinsic rate ($k_{\text{eff}} = 4k_w/D$ with no $k_f$ limitation), and zero-order wall demand is never transfer-limited.

#### Combined Reaction in a Segment

The net concentration change in a pipe segment over a quality time step $\delta t$ is:

$$\Delta c = \left( r_{\text{bulk}} + r_{\text{wall}} \right) \delta t$$

This forward-Euler update is applied uniformly for all reaction orders. After each step the updated concentration is floored at zero ($c \leftarrow \max(0,\, c + \Delta c)$) so that a decay reaction can never drive a constituent negative. The quality time step is kept short relative to the reaction time scale to keep truncation error acceptably small.

#### Roughness–Reaction Correlation

As an alternative to specifying a wall reaction coefficient $k_w$ for each pipe individually, EPANET supports a global **roughness–reaction correlation factor** $R_f$. When $R_f \neq 0$, the wall coefficient for each pipe is derived automatically from its roughness parameter. The correlation formula depends on the head-loss formula in use:

- **Hazen–Williams** ($C$ is the HW roughness coefficient, smoother pipes have higher $C$):
$$k_w = \frac{R_f}{C}$$

- **Darcy–Weisbach** ($\varepsilon$ is the absolute roughness, $D$ the diameter):
$$k_w = \frac{R_f}{|\ln(\varepsilon/D)|}$$

- **Chezy–Manning** ($n_M$ is Manning's roughness, rougher pipes have higher $n_M$):
$$k_w = R_f \cdot n_M$$

In all three cases, $R_f$ has units of $[k_w \cdot \text{roughness parameter}]$, chosen so that the resulting $k_w$ is in the wall-rate units expected by the wall reaction formulas — velocity (m/s or ft/s) for first-order wall kinetics, mass per unit area per unit time for zero-order. The physical motivation is that rougher pipe surfaces tend to harbour more biofilm or corrosion products and hence exhibit higher wall demand. Any pipe whose $k_w$ is set explicitly in the input takes precedence over the correlation.

---

## 9. Tank Mixing Models

Tanks use one of four models to govern how incoming water mixes with water already in storage. The choice of mixing model can significantly affect the predicted concentration of dissolved constituents leaving the tank.

### Complete Mix (CSTR)

The tank is modelled as a **Continuously Stirred Tank Reactor (CSTR)**. All water entering the tank is assumed to mix instantly and uniformly with the existing contents. The concentration at any instant is therefore uniform throughout the tank volume. The mixing update at each quality sub-step is:

$$c_{\text{new}} = \frac{c \cdot V + c_{\text{in}} \cdot V_{\text{in}}}{V + V_{\text{in}}}$$

where $V$ is the current stored volume, $c$ is the (uniform) tank concentration, $c_{\text{in}}$ is the volume-weighted inflow concentration, and $V_{\text{in}}$ is the inflow volume during the sub-step. Bulk reactions are applied separately (not during the mixing step) in the same phase as pipe reactions. If the tank is at capacity, the stored volume is clamped at $V_{\max}$ and the excess mass is booked as overflow outflow in the mass balance.

### Two-Compartment Mix

The tank volume is divided into two compartments represented as two segments: an **inlet mixing zone** with a maximum capacity of $V_{\text{mz}} = f \cdot V_{\max}$ (where $f$ is a user-specified fraction, typically 0.1–0.3, and $V_{\max}$ is the *maximum* tank volume) and a **stagnant zone** comprising the remaining capacity $V_{\text{sz}} = V_{\max} - V_{\text{mz}}$. All inflow enters the mixing zone; all outflow also exits from the mixing zone. Transfers between zones are **directional and discrete**, not bidirectional or continuous:

- **Filling** ($v_{\text{net}} > 0$): inflow mass mixes into the mixing zone (weighted average). If the mixing zone volume would exceed $V_{\text{mz}}$, the excess volume $v_t = \max(0,\; V_{\text{mix}} + v_{\text{net}} - V_{\text{mz}})$ is transferred from the mixing zone to the stagnant zone, carrying the mixing zone's post-mix concentration. The stagnant zone concentration is updated as a volume-weighted average of its current contents and the transferred mass. If the stagnant zone's volume would exceed $V_{\text{sz}}$, the surplus exits as overflow (counted as outflow in the mass balance) and the stagnant zone is clamped to $V_{\text{sz}}$.

- **Emptying** ($v_{\text{net}} < 0$): water is drawn back from the stagnant zone into the mixing zone to compensate for the net deficit: $v_t = \min(V_{\text{stag}},\; |v_{\text{net}}|)$. The mixing zone concentration is updated as a volume-weighted average of its current contents, the inflow mass, and the transferred stagnant zone water.

- **No net flow** ($v_{\text{net}} = 0$): no volume transfer occurs between the zones, and — because there is no net volume change over the sub-step — the mixing-zone concentration is left unchanged; any inflow mass is not blended in during this case.

The outflow concentration is always the mixing zone concentration. This model captures the behaviour of elongated tanks where short-circuiting occurs — inflow water can exit before it fully mixes with the bulk stored water.

### FIFO Plug Flow

The tank is treated as a perfectly ordered pipe with no axial mixing. Water enters from one end and exits from the other in strict **first-in, first-out** order. The segment representation used for pipes (§8.2) is applied directly to the tank. New inflow creates a new segment at the inlet end; outflow consumes segments from the outlet end. Reactions occur within each segment. This model is appropriate for narrow, tall standpipes or tanks with well-separated inlet and outlet ports. When the tank is full, the full inflow volume is withdrawn from the outlet end and the net inflow's mass is booked as overflow.

### LIFO (Stacked Layers)

Water enters and exits from the **same end** of the tank, as in a stratified system. Unlike FIFO, only the *net* flow moves segments — simultaneous inflow and outflow are netted against each other: net inflow forms a new segment at the top (or inlet side), while net outflow removes segments from that same end in **last-in, first-out** order. This model approximates thermal stratification in tanks where buoyancy prevents vertical mixing, so that recently added water leaves first. When the tank is full, the net inflow volume is removed from the *opposite* (first) end and that mass is booked as overflow.

---

## 10. Mass Balance

The simulator maintains a **running mass balance** for the quality constituent throughout the simulation. At each quality time step, the following quantities are accumulated:

- **Initial mass stored**: the total constituent mass in all pipes and tanks at the start of the simulation.
- **Mass added from sources**: constituent injected at network sources — including, implicitly, every reservoir's outflow at its own fixed quality, source or not; conversely, all mass flowing *into* a reservoir counts as network outflow.
- **Mass removed as outflow**: constituent carried out of the network — consumer withdrawals, water leaving through reservoirs, and tank overflow are all accumulated in this term (not consumer demand alone).
- **Mass reacted**: constituent lost (or gained) through bulk and wall reactions; computed as the integral of reaction rates over all pipe segments and tanks.
- **Final mass stored**: the total mass remaining in the network at the end of the simulation.

The overall mass balance ratio is computed as follows. Let $m_\text{reacted}$ be the signed total mass change due to reactions (negative for growth, positive for decay). If $m_\text{reacted} > 0$ (net decay), it is added to the output side of the ledger. If $m_\text{reacted} < 0$ (net growth), its absolute value is added to the input side:

$$\text{ratio} = \frac{\text{mass outflow} + \max(m_\text{reacted}, 0) + \text{final mass stored}}{\text{initial mass stored} + \text{mass added by sources} + \max(-m_\text{reacted}, 0)}$$

A value close to 1.0 confirms that constituent mass is being conserved to within numerical precision. A significant deviation from 1.0 indicates either a numerical error or an inconsistency in the reaction parameterisation. This diagnostic is reported at the end of the simulation.

---

## 11. Energy Tracking

For each pump in the network, the hydraulic power consumed during each time step is:

$$P_{\text{hydraulic}} = \rho g Q \, \Delta H$$

where $Q$ is the flow through the pump and $\Delta H$ is the head added. The actual power drawn from the electrical supply is:

$$P_{\text{electrical}} = \frac{\rho g Q \, \Delta H}{\eta}$$

where $\eta$ is the pump efficiency. If an efficiency curve (efficiency versus flow) is provided, $\eta$ is read from the curve at the current operating point; otherwise a default efficiency is assumed. When a pump operates at a speed setting $\omega \neq 1.0$ and an efficiency curve is supplied, the efficiency is further adjusted using the **Sarbu–Borza** speed-correction formula:

$$\eta_{\omega} = 100 - \frac{100 - \eta_1}{\omega^{0.1}}$$

where $\eta_1$ is the efficiency read from the curve at the speed-adjusted operating point and $\eta_{\omega}$ is the corrected efficiency. For $\omega < 1$ the correction *enlarges* the efficiency gap $(100 - \eta_1)$, so the corrected efficiency is **lower** than the curve value (and slightly higher for $\omega > 1$) — the formula models the penalty of off-nominal operation. The result is clamped to the range 1–100%; the global default efficiency, used when no curve applies, is 75%.

The following energy statistics are accumulated over the simulation period for each pump:

- **Kilowatt-hours consumed**: the time integral of $P_{\text{electrical}}$ over the simulation.
- **Time-weighted average efficiency**: the average of $\eta$ weighted by the fraction of time spent at each operating point.
- **Maximum demand**: the peak value of $P_{\text{electrical}}$ observed at any time step.
- **Cost**: the product of energy consumed and a unit energy price, which may itself vary over time via a cost pattern.

All of these accumulate **only while a pump is open** — a closed pump contributes zero power and is skipped — so the averages are weighted over online time, not total duration. The raw accumulators are normalised at report time: time online becomes percent utilisation of the duration, the efficiency sum becomes its average over online hours, kilowatt-hours divided by online hours become average kW, and total cost is expressed per day (a single-period, zero-duration analysis accounts energy as one hour of operation). These energy diagnostics are essential for assessing the operating cost of different pumping schedules or for optimising pump dispatch.

**Energy cost model**: the unit energy cost $c$ (cost per kWh) used to accumulate `TotalCost` is determined at each time step as follows. A global base cost $c_0$ is multiplied by the current value of a global energy price pattern (if one is assigned), giving a time-varying rate $c_0 f(t)$. Each pump may also carry its own cost override $c_p$ and/or its own cost pattern, which are resolved **independently** of the global values: if a pump's `Ecost` is positive it replaces $c_0$; otherwise the global $c_0$ is used. If a pump's `Epat` is assigned it replaces the global pattern multiplier; otherwise the global pattern multiplier is applied even when the pump has its own cost override. The energy cost accumulated for pump $j$ over time step $\Delta t$ (hours) is therefore:

$$\text{Cost}_j \mathrel{+}= c_j(t) \cdot P_j \cdot \Delta t$$

where $c_j(t)$ is the applicable unit rate at the current time. Cost patterns advance on the same pattern start/step clock as demand patterns.

In addition to energy cost, a global **peak demand charge** parameter $D_c$ (cost per peak kW) is supported. A running maximum of the simultaneous power draw across all pumps $P_{\text{max}} = \max_t \sum_j P_j(t)$ is tracked throughout the simulation. At report time the total peak demand cost $D_c \cdot P_{\text{max}}$ is added to the energy cost summary. The **KwHrsPerFlow** statistic is accumulated at each time step as the time *integral* $\sum_i (P_i / Q_i) \cdot \Delta t_i$ of the instantaneous energy intensity $P/Q$, with the flow floored at $10^{-6}$ cfs; at report time it is divided by the pump's online hours and unit-converted — kWh per million gallons (US) or kWh per m³ (SI) — as the measure of pumping energy per unit throughput.

---

## 12. Flow Balance

In addition to the local hydraulic solution at each time step, the simulation accumulates a **global volumetric flow balance** over the entire simulation period. Each quantity is computed by time-integrating the corresponding flow rate and dividing by total elapsed time at the end of the run — so every component, storage change included, is reported as a period-average flow rate:

- **Total inflow**: water entering the network from reservoirs (fixed-grade sources).
- **Consumer demand delivered**: water withdrawn at junctions as consumer demand.
- **Emitter outflow**: water discharged through emitters.
- **Leakage outflow**: water lost through pipe leakage modelled by the FAVAD equations.
- **Demand deficit**: the volume of consumer demand that was not delivered owing to insufficient pressure (relevant only in PDA mode; zero in DDA mode).
- **Storage change**: the net change in volume stored in all tanks over the simulation period (positive if tanks filled overall, negative if they drained).

The **flow balance ratio** is defined as:

$$\text{balance ratio} = \frac{q_\text{out}}{q_\text{in}}$$

where the ledger is built from the time-averaged flows as follows. When the net tank storage flow $q_\text{stor}$ is positive (tanks filling on average), it is added to the output side: $q_\text{out} = \text{total outflow} + q_\text{stor}$. When $q_\text{stor}$ is negative (tanks draining on average), its absolute value is added to the input side: $q_\text{in} = \text{total inflow} + |q_\text{stor}|$. Here `total outflow` comprises consumer demand, emitter flows, leakage, **and water leaving the network into reservoirs** (a reservoir with non-negative net demand accrues to the outflow side); `total inflow` is the supply from reservoirs plus any junction node with net negative demand. The demand deficit (undelivered demand in PDA mode) is tracked as a separate component and reported alongside the ratio but is not incorporated into the ratio calculation itself; it accumulates only at junctions with positive full demand and only when positive — over-delivery elsewhere never offsets it. A ratio close to 1.0 indicates that the simulation is globally mass-conserving; an exactly balanced (including empty) system reports exactly 1.0, and zero inflow with nonzero outflow reports 0.0. The instantaneous **leakage fraction** — leakage as a percentage of the current period's supply, meaning reservoir and junction inflow plus net tank drainage, and zero when that supply is zero — is also tracked and can be reported at each reporting period to identify periods of high loss.

---

## 13. Units and Physical Constants

All hydraulic computations are performed internally in a fixed set of US customary units regardless of what units the user specifies in the input file: lengths in feet (ft), diameters in feet, flows in ft³/s (cfs), heads in feet, and power in horsepower (hp). Unit conversion factors are applied once during input parsing to translate user-supplied values into internal units, and again at output time to translate results back into user-facing units.

**Unit system selection**: the unit system (US or SI) is inferred automatically from the chosen flow unit:

| Flow units | System | Pressure default |
|------------|--------|------------------|
| CFS, GPM, MGD, IMGD, AFD | US | psi |
| LPS, LPM, MLD, CMH, CMD, CMS | SI | metres |

Pressure units may be overridden independently to psi, kPa, metres, bar, or feet, regardless of the primary unit system. Reported pressures in psi, kPa, and bar scale with the specific gravity (the conversion factor embeds it); metres and feet do not.

**Physical constants**: the default values used in the absence of user overrides are:

| Constant | Default | Notes |
|----------|---------|-------|
| Kinematic viscosity $\nu$ | $1.1 \times 10^{-5}$ ft²/s | Water at 20 °C |
| Molecular diffusivity $\mathcal{D}$ | $1.3 \times 10^{-8}$ ft²/s | Chlorine at 20 °C |
| Specific gravity | 1.0 | Water |

For **kinematic viscosity**, the user may supply either a multiplier (value $> 10^{-3}$, interpreted as a scale factor on the default) or an actual value in ft²/s. For **molecular diffusivity**, the multiplier threshold is $10^{-4}$ rather than $10^{-3}$: values greater than $10^{-4}$ are treated as scale factors on the default diffusivity, while smaller values are taken as the actual diffusivity. When the SI unit system is active, supplied actual values for both quantities are converted from m²/s to ft²/s before storage. (The API's viscosity/diffusivity options always interpret the supplied value as a multiplier of the default — the threshold rule applies to the input file only.)

**Hydraulic solver defaults**: the default values for convergence parameters that apply in the absence of user specification are summarised below.

| Parameter | Default | Meaning |
|-----------|---------|--------|
| MaxIter | 200 | Maximum Newton–Raphson iterations |
| Hacc | 0.001 | Flow accuracy tolerance $\epsilon_{\text{tol}}$ |
| Htol | 0.0005 ft | Head tolerance for status checks |
| Qtol | 0.0001 cfs | Flow tolerance for status checks |
| CheckFreq | 2 | Status check start interval (iterations) |
| MaxCheck | 10 | Maximum iterations with status checks |
| DampLimit | 0 | Flow-error threshold at or below which damping (0.6 relaxation) and deferred PRV/PSV checks begin; **0 = damping never applied**, PRV/PSV checks every iteration |
| ExtraIter | −1 | Halt on non-convergence (0 = no extra; >0 = extra frozen trials) |
| RQtol | $10^{-7}$ | Head-loss-gradient floor for linearisation (§2.1) |
| HeadErrorLimit | 0 | Supplementary per-link head-residual gate (0 = off) |
| FlowChangeLimit | 0 | Supplementary max-flow-change gate (0 = off) |

The flow accuracy (Hacc) is bounded differently depending on how it is supplied, and the two paths disagree in both range and severity. Read from `[OPTIONS] ACCURACY` it is **silently clamped** to $[10^{-5}, 0.1]$, so an out-of-range value in a file loads and runs at the nearest bound. Set through the API it is **rejected** outside $[10^{-8}, 0.1]$, raising the invalid-option-value error. The shared ceiling of $0.1$ is common to both; the floor is three orders of magnitude tighter on the API path, so the engine's own bounds place the limit of what its solver is expected to reach at $10^{-8}$ while the file path stops well short of it.

**Time parameter defaults**: the time settings default and interlock as follows, all resolved once at the end of input processing.

| Parameter | Default | Notes |
|-----------|---------|--------|
| Duration | 0 s | A zero duration is a single-period steady-state run, not an error |
| Hydraulic time step | 3600 s | Also substituted whenever a non-positive value is supplied |
| Pattern time step | 3600 s | Substituted whenever non-positive |
| Reporting time step | 3600 s | Falls back to the pattern time step if given as zero |
| Quality time step | Hydraulic step ÷ 10 | Derived whenever unspecified |
| Rule time step | Hydraulic step ÷ 10 | Derived whenever unspecified, then capped at the hydraulic step |
| Start time of day, pattern start, report start | 0 | |
| Quality tolerance | 0.01 | mg/L for chemicals, hours for age (§8.2) |

The hydraulic step is then lowered to the pattern step and to the reporting step if it exceeds either, and the quality step lowered to the hydraulic step, as §6 describes. One further adjustment is a silent substitution rather than a clamp, and its direction is the opposite of what one would expect: a **report start time later than the simulation duration is reset to zero**, not to the duration — so a run configured to begin reporting after it ends reports everything instead of nothing.

---

## 14. Input and Output

### Input

The network is described in a structured plain-text input file organised into labelled sections. Each section corresponds to a class of network object or a simulation parameter group. The parser makes **two passes** through the file. A first pass does two things: it counts all objects of each type so that memory can be allocated in one contiguous block, and it also captures the `UNITS` and `HEADLOSS` options from the `[OPTIONS]` section — so that second-pass interpretation is independent of where `[OPTIONS]` appears in the file; the object counting itself depends on neither option. A second pass reads and interprets all remaining data. The sections handled include: junctions, reservoirs, tanks, pipes, pumps, valves, demand categories, time patterns, head-loss and efficiency curves, simple controls, rule-based controls, water quality sources, emitter coefficients, leakage parameters, options (head-loss formula selection, flow units, demand model, tolerances), energy pricing, reaction coefficients, tank mixing model assignments, reporting options, initial status overrides, and simulation time parameters — plus initial water quality (`[QUALITY]`), the map/meta sections (`[COORDINATES]`, `[VERTICES]`, `[LABELS]`, `[BACKDROP]`, `[TAGS]`) that are round-tripped but never simulated, and a legacy `[ROUGHNESS]` section that is accepted and silently ignored.

**Keyword matching is by prefix**, and this is a compatibility fact rather than a parsing detail. One routine serves every keyword lookup in the file — section headers, option names, and option *values* alike — walking a keyword table in order and returning the first entry that is a **prefix of the supplied token**, compared without regard to case after leading blanks are skipped. Three consequences follow. Trailing characters are ignored, so `[JUNCTIONS]`, `[JUNC]` and `[JUNCTIONSXYZ]` all select the same section; truncations are not, since the comparison runs to the end of the *keyword*. The keyword table exploits this deliberately, storing stems rather than words — `MESS`, `VERI`, `Junc`, `Reser`, `Tank` — which only resolve because the file's full spellings extend them. And table *order* becomes load-bearing wherever one keyword prefixes another: the ordering is part of the format's meaning, not an incidental property of the table.

The engine hit that last hazard once and patched around it by hand. In the report-field table `Head` precedes `Headloss` and is a prefix of it, so a `[REPORT] HEADLOSS` line would select head reporting instead. The parser therefore tests the token against the literal `Headloss` with a whole-string comparison *before* consulting the prefix matcher at all — a special case existing solely to defeat the general rule. It is the only such guard in the parser.

**Initial link status overrides (`[STATUS]` section)**: the `[STATUS]` section allows the user to set the initial operational status of any link before simulation begins. For pipes, valid entries are `OPEN` or `CLOSED`. For pumps, an entry may be `OPEN`, `CLOSED`, or a numeric relative-speed setting (a value of 0 closes the pump). For valves, an entry may be `OPEN`, `CLOSED`, or a numeric setting that overrides the value from the `[VALVES]` section. Because `[STATUS]` is parsed after the link-definition sections, its entries take precedence over inline status values. During simulation, controls and rules may subsequently change these statuses.

Before simulation begins, the project undergoes a **validation pass** that checks: each tank satisfies $H_{\min} \leq H_{\text{init}} \leq H_{\max}$, with levels inside any volume curve's elevation range; all patterns are non-empty; all curves are non-empty with strictly increasing $x$-values; pump curves follow the §2.2 admissible forms (one point, three points starting at zero flow, or a strictly-decreasing custom curve, with $0 < c \leq 20$); and the network holds at least two nodes and at least one tank or reservoir. Validation runs when the hydraulic solver *opens*, not at file load. (Valve topology, by contrast, is enforced against the §1 placement rules as each valve is defined — at input parse time and again on any API edit that changes a valve's type or end nodes — not by this validation pass.) The unconnected-node check covers **junctions only** — an unlinked tank is harmless and silently permitted — and reports up to ten offenders before failing. If any check fails, the simulation does not start.

One step between parsing and validation *mutates* the network rather than checking it. Node indices are assigned junctions first, then tanks and reservoirs, with the junction block sized from the count taken in the first pass. When fewer junctions are created than `[JUNCTIONS]` lines were counted — which happens when a line's identifier fails to register, most obviously a duplicate — the junction block is left with a gap, and every tank is shifted down to close it. That shift rewrites all references: link end nodes, simple-control node references, rule references, the quality trace node, and the identifier hash table. Because node index order *is* the order in which the output file's identifier table and every per-period result vector are written (§14), this shift propagates into the output file's layout. The path is reachable only through input that already raised a non-fatal error, so for a well-formed file the resulting order is simply junctions in file order followed by tanks and reservoirs in file order.

Alternatively, networks may be constructed entirely through the project API without reference to an input file, by calling the object-creation and property-setting operations in programmatic sequence; the in-memory network can also be regenerated as an input file, with the `[LABELS]` and `[BACKDROP]` sections copied verbatim from the original while `[COORDINATES]`, `[VERTICES]`, and `[TAGS]` are rewritten from the in-memory data. Deleting a pattern or curve renumbers all higher references — references to the deleted pattern are cleared, which for demand patterns means falling back to the default pattern at run time (source, energy, and tank-head references simply become pattern-less) — and a PCV losing its curve has its loss coefficient zeroed.

### Output

**Hydraulic binary file**: at each hydraulic time step the solver writes nodal demands, nodal heads, link flows, link status flags, and link settings to a temporary binary file. This file is then replayed during the water quality simulation, supplying the flow field from which velocities are reconstructed for advection, without requiring the hydraulics to be recomputed. (Velocities themselves are not stored; they are derived from the saved flows and pipe geometry.) The file opens with a 2+6 INT4 header — magic number 516114521, engine version, then node/link/tank/pump/valve counts and duration — validated field-for-field when a saved file is reused, a mismatch being fatal; each snapshot holds an INT4 time, REAL4 demands, heads, flows, status codes and settings, and the next-step interval. The API can save and reuse this file across runs, and can populate the results file from saved hydraulics without a water-quality run.

**Results binary file**: at each reporting time step (which may be less frequent than the hydraulic time step), all computed quantities for every node and link are saved to a separate binary output file. Node quantities include hydraulic head, pressure, demand, and constituent concentration. Link quantities include flow rate, velocity, unit head loss, friction factor, and quality. This file may subsequently be post-processed by external programs (a bundled reader library exposes a standalone API over the same format). When the report-statistic option is anything other than time-series, results accumulate in a temporary file and the dynamic section holds exactly *one* pseudo-period of averaged/minimum/maximum/range values — flows as magnitudes, statuses collapsed to open/closed — with the period count written as 1. The text report is generated by replaying this file, so results must be saved before reporting.

### Binary Output File Format

The binary output file (`.out`) is written in native byte order (little-endian on x86) using `float` (4-byte IEEE 754 single-precision, hereafter REAL4) for all floating-point values and `int` (4-byte signed, hereafter INT4) for integers. String fields are fixed-width arrays: IDs are 32 bytes (MAXID+1 = 32, null-terminated), title lines are 80 bytes (TITLELEN+1 = 80), and filenames are 260 bytes (MAXFNAME+1 = 260). No padding or alignment bytes exist between sections.

The file has five sections written sequentially:

#### Prolog

15 × INT4 header (60 bytes), then strings and arrays:

| Offset (bytes) | Type | Field |
|---|---|---|
| 0 | INT4 | Magic number = $516114521$ |
| 4 | INT4 | Version = $20012$ |
| 8 | INT4 | $N_{\text{nodes}}$ (total junctions + reservoirs + tanks) |
| 12 | INT4 | $N_{\text{tanks}}$ (reservoirs + tanks only) |
| 16 | INT4 | $N_{\text{links}}$ (total pipes + pumps + valves) |
| 20 | INT4 | $N_{\text{pumps}}$ |
| 24 | INT4 | $N_{\text{valves}}$ |
| 28 | INT4 | Quality flag: 0=None, 1=Chemical, 2=Age, 3=Trace |
| 32 | INT4 | Trace node index (1-based; 0 if not trace mode) |
| 36 | INT4 | Flow units enum: 0=CFS, 1=GPM, 2=MGD, 3=IMGD, 4=AFD, 5=LPS, 6=LPM, 7=MLD, 8=CMH, 9=CMD, 10=CMS |
| 40 | INT4 | Pressure units: 0=PSI, 1=kPa, 2=metres, 3=bar, 4=feet |
| 44 | INT4 | Report statistic: 0=Series, 1=Average, 2=Minimum, 3=Maximum, 4=Range |
| 48 | INT4 | Report start time (seconds) |
| 52 | INT4 | Report time step (seconds) |
| 56 | INT4 | Simulation duration (seconds) |
| 60 | char[80] × 3 | Three title lines (240 bytes) |
| 300 | char[260] × 2 | Input filename, *secondary* report filename (usually empty — not the primary report file) (520 bytes) |
| 820 | char[32] × 2 | Chemical name, quality units — the chemical's units in CHEM mode, "hrs" for AGE, "% from" for TRACE (64 bytes) |
| 884 | char[32] × $N_n$ | Node IDs |
| $884 + 32 N_n$ | char[32] × $N_l$ | Link IDs |

Following the ID strings:

| Type | Count | Field |
|---|---|---|
| INT4 | $N_l$ | Link from-node indices (1-based) |
| INT4 | $N_l$ | Link to-node indices (1-based) |
| INT4 | $N_l$ | Link type codes (0=CV, 1=Pipe, 2=Pump, 3=PRV, 4=PSV, 5=PBV, 6=FCV, 7=TCV, 8=GPV, 9=PCV) |
| INT4 | $N_t$ | Tank-to-node index mapping (1-based node index for each tank/reservoir) |
| REAL4 | $N_t$ | Tank cross-section areas (sq ft, internal units — not unit-converted) |
| REAL4 | $N_n$ | Node elevations (converted to output length units) |
| REAL4 | $N_l$ | Link lengths (converted to output length units) |
| REAL4 | $N_l$ | Link diameters (converted to output diameter units; 0.0 for pumps) |

#### Energy

Written immediately after the prolog, once per simulation:

Per pump ($N_p$ records of 28 bytes each):

| Type | Field |
|---|---|
| INT4 | 1-based link index of the pump |
| REAL4 | Percentage of time online (0–100) |
| REAL4 | Average efficiency (%) |
| REAL4 | Average kWh per unit of flow (kWh/Mgal for US, kWh/m³ for SI) |
| REAL4 | Average power consumption (kW) |
| REAL4 | Peak power consumption (kW) |
| REAL4 | Average daily cost |

Followed by one trailing REAL4: demand charge (peak demand × demand cost rate).

#### Dynamic Results

Written once per reporting period. Each period contains the following arrays, all column-major (one variable across all objects, then the next variable):

**Node variables** — 4 arrays of $N_n$ × REAL4, in output units:

| Order | Variable | Notes |
|---|---|---|
| 1 | Demand | Converted to output flow units |
| 2 | Head | Converted to output length units |
| 3 | Pressure | $(H_i - z_i)$ converted to output pressure units |
| 4 | Quality | Converted to output quality units |

**Link variables** — 8 arrays of $N_l$ × REAL4:

| Order | Variable | Notes |
|---|---|---|
| 1 | Flow | Output flow units; signed (negative = reverse) |
| 2 | Velocity | $Q / A_{\text{pipe}}$ converted; 0 for pumps |
| 3 | Headloss | Pipes: $1000 \lvert\Delta h\rvert / L$ (per 1000 length units). Valves: $\lvert\Delta h\rvert$ in output length units. Pumps: $\Delta h$ (signed; negative = head gain). 0 for closed links. |
| 4 | Quality | Average quality across link segments |
| 5 | Status | Cast to REAL4: 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active, 5=XFlow, 6=XFCV, 7=XPressure, 8=Filling, 9=Emptying, 10=Overflowing |
| 6 | Setting | Pipes: roughness. Pumps: speed. PRV/PSV/PBV: setting in pressure units. FCV: setting in flow units. TCV: raw setting. |
| 7 | Reaction rate | Mass/L/day, converted to output quality units |
| 8 | Friction factor | Darcy–Weisbach $f$; dimensionless; 0 for non-pipes or negligible flow |

Bytes per period: $(4 N_n + 8 N_l) \times 4$.

#### Network Reactions

4 × REAL4 (16 bytes):

| Field | Content |
|---|---|
| Avg. bulk reaction rate | Total bulk mass reacted / duration (mass/hr) |
| Avg. wall reaction rate | Total wall mass reacted / duration (mass/hr) |
| Avg. tank reaction rate | Total tank mass reacted / duration (mass/hr) |
| Avg. source input rate | Total source mass input / duration (mass/hr) |

#### Epilog

3 × INT4 (12 bytes):

| Field | Content |
|---|---|
| $N_{\text{periods}}$ | Number of reporting periods written |
| Warning flag | 0 = no warnings |
| Magic number | $516114521$ |

The total file size is:
$$884 + 36 N_n + 52 N_l + 8 N_t + (28 N_p + 4) + N_{\text{periods}} \cdot 4(4 N_n + 8 N_l) + 16 + 12$$

**Text status report**: an optional text-format status report records, at user-specified verbosity, the convergence history of every hydraulic time step (number of iterations, peak head error, flow accuracy achieved), any link status changes during the simulation, the energy consumption and cost summary for all pumps, the final mass balance ratio for water quality, the final flow balance statistics, and — if requested — tabular node and link results at every reporting period.

### API

The system exposes a complete project-handle–based API. The workflow is as follows:

1. Create a project object, which encapsulates all state for a single simulation instance.
2. Open a network description (from file or by programmatic construction).
3. Optionally, open a pre-existing hydraulics results file; if none exists, run the full hydraulic simulation first.
4. Run the hydraulic simulation either in full (computing all time steps internally) or step-by-step; in step-by-step mode the caller advances the clock one hydraulic time step at a time and may modify *scalar* properties and settings between steps — **structural** changes (adding or deleting nodes and links; deleting curves or patterns; changing a link's type or end nodes) are refused while either solver is open — though *adding* a pattern or curve is permitted — and the head-loss formula cannot change while the hydraulic solver is open, nor the quality model while the quality solver is open.
5. Run the water quality simulation, either in full or step-by-step, in a similar fashion.
6. Retrieve any computed result (pressure, flow, concentration, energy, etc.) at any time step.
7. Set any scalar network property (demand, pipe roughness, pump speed, valve setting, control threshold, reaction coefficient, etc.) and re-run as desired; structural edits require the solvers to be closed first.
8. Delete the project to release all resources.

Beyond this workflow, the API surface also covers: full network editing (creating, deleting, and renaming every object class, including demands, patterns, curves, controls, and rules, plus vertex/coordinate and comment/tag metadata); per-control and per-rule enable/disable; solver introspection (iteration statistics, result indices, time to the next event); report control (generation, custom lines, copying and resetting); one-shot run drivers; and regeneration of the input file from memory. A legacy function family wraps a single global default project — the multi-instance concurrency described below applies only to the handle-based API.

Multiple project instances may coexist in the same process, enabling Monte Carlo analysis, parallel scenario evaluation, or re-entrant simulation from multiple threads (provided each thread operates on a distinct project handle and any shared file system resources are managed appropriately).

---

## 15. Cross-Cutting Engine Contracts

The preceding sections follow EPANET's physical subsystems; this one collects the conceptual conventions that span them, each referenced back to the sections carrying its details.

**The internal-units contract.** All hydraulic computation runs in a fixed US customary system — lengths and heads in feet, flows in ft³/s, power in horsepower — with conversion applied at the boundaries: once at input parsing and again at output time (§13). One interior exception exists and it is easy to miss because it converts *back*: **user-defined curves are stored untransformed**, so extracting a linearised segment from a custom pump curve (§2.2) or a GPV head-loss curve (§2.3) converts the current flow into the curve's own units, fits the bracketing segment there, and converts the resulting slope and intercept back to internal units. The arithmetic is the same either way, but a re-implementation that normalises curve points at parse time and then reuses this segment-fitting logic unchanged will double-convert. Interior constants are therefore expressed in these units: the TCV loss-coefficient factor $0.02517\,s/D^4$ (§2.3), the FAVAD orifice coefficient $C_o$ (§5), the constant-power pump's nominal design flow of 1 ft³/s (§2.2), and the quality thresholds `Q_STAGNANT` and `QZERO` (§8.3). Specific gravity couples into the pressure boundary alone: reported pressures in psi, kPa, and bar embed it in their conversion factors, while metres and feet do not (§13), and in US units the emitter coefficient is defined per psi$^{n_e}$ adjusted for specific gravity (§4).

**The status-machine convention.** Every link carries a discrete status re-evaluated during the solve: check-valve pipes toggle OPEN/CLOSED (§2.3), pumps pass through OPEN, XHEAD, and TEMPCLOSED (§2.2), PRVs and PSVs inhabit ACTIVE/OPEN/CLOSED plus XPRESSURE and FCVs ACTIVE/XFCV (§2.3), and tank-adjacent links are forced TEMPCLOSED at tank limits (§6.3). All transition tests share the two tolerances Htol and Qtol, which serve status logic *only* and are entirely separate from the convergence tolerance Hacc governing solver termination (§2.3, §13). The checks follow a re-open-then-re-test pattern — TEMPCLOSED and XHEAD links are first re-opened at the start of each status check and the new operating point re-evaluated, so no status is locked in (§2.2, §6.3) — and convergence itself requires a pass that produces no status change (§2.4).

**The smooth-barrier idiom.** One differentiable one-sided penalty, $\Delta h = (a - \sqrt{a^2 + 10^{-6}})/2$ with $a = 10^9 Q$ and its matching gradient increment, is the engine's device for enforcing inequality bounds without breaking the Newton–Raphson iteration: it holds pressure-driven demands within $[0, D_{\text{full}}]$ (§3.2, the only site using both the lower and the upper barrier), drives emitter flow non-negative when backflow is forbidden (§4), and holds **leakage flow** non-negative with no equivalent opt-out (§5) — the same closed form at all three, approaching a hard constraint as the bound is violated while remaining smooth throughout. The upper barrier is the lower one's mirror, $\Delta h = (a + \sqrt{a^2 + 10^{-6}})/2$ with gradient $\tfrac{1}{2}10^9(1 + a/\sqrt{a^2+10^{-6}})$. The leakage site reaches it through a private duplicate of the routine — same body, different name — rather than a shared call, so a reader tracing callers of the shared helper alone will find two sites and miss the third.

**The $(P, Y)$ linearisation contract.** Every head–flow element reduces at each iteration to the same pair — a conductance $P_k$ (inverse head-loss gradient) and an offset $Y_k$ — consumed identically by the GGA assembly: $P$ into the Laplacian diagonal and off-diagonals, $Y$ into the right-hand side (§2.4). Pipes under all three friction formulas with minor losses (§2.1), pumps as negative head loss (§2.2), the resistance-type valves TCV, PBV, GPV, and PCV (§2.3), emitters (§4), pressure-dependent demands (§3.2), and the two FAVAD leakage components as equivalent emitters (§5) all enter through this one algebra. Its edge cases are handled inside the same coefficients: the RQtol gradient floor keeps $P$ bounded near zero flow (§2.1), and the large constant $C_\infty \approx 10^8$ turns the pair into a near-exact constraint — the PBV's fixed head drop, the active PRV/PSV head pinning, and the constant-power pump's near-zero-flow closure (§2.2, §2.3). Only the ACTIVE control valves step outside it, zeroing $P_k$ and augmenting the matrix directly (§2.3).

**Prefix keyword matching.** Every keyword in the input file — section header, option name, option value — is resolved by one routine that returns the first table entry forming a *prefix* of the supplied token, case-insensitively (§14). The keyword table is written to suit it, holding stems rather than words, and the rule's one harmful collision (`Head` shadowing `Headloss`) is defeated by a hand-written whole-string test rather than by reordering. Matching is therefore strictly looser than equality, and table order is part of the file format's meaning.

**The curve-clamping rule.** User-defined curves are piece-wise linear: intermediate values interpolate linearly between the two bracketing data points, and values outside a curve's x-range are clamped to its end-point values — curves never extrapolate (§1). The PCV opening curve is the one documented departure, interpolating from the origin below its first point and toward the $(100\%, 100\%)$ anchor above its last, with the resulting ratio clamped to $[10^{-6}, 1]$ (§2.3). Validation enforces that every curve is non-empty with strictly increasing $x$-values and that pump curves take one of the admissible forms of §2.2 (§14).

**The pattern-clock convention.** All patterns advance on one clock: the period index $p = \lfloor (t + t_{\text{start}}) / \Delta t_p \rfloor$ selects multiplier $F_j[p \bmod L_j]$, giving each pattern an independently repeating cycle (§1). The same convention drives junction demands, reservoir heads, and pump speed settings (§1), source-concentration multipliers (§8.3), and energy cost patterns (§11). The fallback for an unassigned demand category is not a special case in the arithmetic: pattern index 0 is a synthetic single-entry pattern holding the multiplier 1.0, installed before parsing begins, so a category with no pattern resolves to the default pattern and a project with no default resolves to index 0 — the unit multiplier arrives through the same lookup as every other (§1).

"One clock" is true of the result but not quite of the mechanism. Four of the five consumers read the *hydraulic* clock; source-concentration multipliers read the **quality** clock, which advances on its own finer step within each hydraulic period (§8.1). The two agree on the period index only because the hydraulic step is trimmed so that no pattern boundary falls strictly inside it — the step limit below is therefore not merely a convenience for accuracy but the thing that keeps the two clocks reporting the same pattern period. The adaptive time step conspires with this clock — the next pattern change is one of the minimised step limits, so multipliers change exactly at step boundaries, never mid-step (§6.2).