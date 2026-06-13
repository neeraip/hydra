# EPANET: A Conceptual and Mathematical Analysis

## Introduction

[OWA-EPANET](https://github.com/OpenWaterAnalytics/EPANET) is a computational engine for simulating the hydraulic and water quality behaviour of pressurised water distribution networks over time. It represents a network as a directed graph of nodes connected by links and advances a time-stepped extended-period simulation, solving at each step for pressures, flows, and constituent concentrations throughout the system. The solver combines rigorous physical models — empirical head-loss formulas, Newton-Raphson linearisation, sparse direct linear algebra, and Lagrangian advection-reaction transport — with flexible engineering constructs such as demand patterns, operational controls, and tank mixing models.

This document provides a self-contained, mathematical and conceptual description of every major subsystem: how the network is represented, how hydraulic equilibrium is computed at each time step, how demands are handled under both fixed and pressure-dependent conditions, how leakage and emitter flows enter the system, how the simulation advances through time, how control logic operates, how water quality is transported and reacted, and how energy and mass balance are tracked. The goal is to give the reader a complete algorithmic and mathematical understanding of the system; implementation-specific details such as memory layout and data-structure internals are omitted, but input/output behaviour and the public API are described at a conceptual level.

---

## Table of Contents

- [EPANET: A Conceptual and Mathematical Analysis](#epanet-a-conceptual-and-mathematical-analysis)
  - [Introduction](#introduction)
  - [Table of Contents](#table-of-contents)
  - [1. Network Representation](#1-network-representation)
    - [Node Types](#node-types)
    - [Link Types](#link-types)
    - [Curves and Patterns](#curves-and-patterns)
  - [2. Hydraulic Simulation](#2-hydraulic-simulation)
    - [2.1 Head Loss in Pipes](#21-head-loss-in-pipes)
      - [Hazen-Williams Formula](#hazen-williams-formula)
      - [Darcy-Weisbach Formula](#darcy-weisbach-formula)
      - [Chezy-Manning Formula](#chezy-manning-formula)
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

---

## 1. Network Representation

The physical infrastructure is represented as a directed graph. **Nodes** correspond to points in the network — junctions, reservoirs, and tanks — and **links** correspond to the conduits and devices connecting them — pipes, pumps, and valves. The orientation of a link defines a positive flow direction; negative flows simply indicate flow in the reverse direction.

### Node Types

**Junctions** are the ordinary connection points of the network. Each junction has a fixed elevation and one or more demand categories, each associating a base demand rate with a time-varying multiplier pattern. Junctions may also carry an emitter (representing orifice or sprinkler outflow) and a water quality source. The hydraulic head at a junction is an unknown to be solved at every time step.

**Reservoirs** are fixed-grade nodes whose hydraulic head is always known and equal to the water surface elevation. A reservoir represents an infinitely large storage body that maintains a constant pressure boundary condition. Because its head is known, it does not appear as an unknown in the linear system; instead, it contributes boundary terms to the equations of its neighbouring junctions.

**Tanks** are storage nodes with a variable water level that evolves as water flows in and out. Each tank is characterised by a minimum level, a maximum level, an initial level, and a geometry: either a constant cross-sectional area or a user-defined volume-versus-elevation curve. Its hydraulic head at any instant equals the elevation of its water surface, which is updated after each hydraulic time step based on the net flow. A tank also carries a bulk reaction coefficient governing water quality transformations in its stored volume.

### Link Types

**Pipes** are the primary conduits. Each pipe is characterised by its length, internal diameter, a roughness coefficient (whose interpretation depends on the chosen head-loss formula), a minor loss coefficient representing localised losses at fittings, and bulk and wall reaction coefficients for quality simulation. A pipe may also be designated as a check valve, in which case flow is permitted in only one direction; the link is treated as closed whenever the computed flow or pressure gradient would drive flow in the reverse direction.

**Pumps** add hydraulic head to the flow passing through them. The head added is described by a head-versus-flow curve, one of the most important user-supplied relationships in the model. Alternatively, a pump may be defined by a constant power output. A variable-speed setting scales both the head and flow axes of the pump curve according to the affinity laws.

**Valves** regulate flow or pressure and come in seven varieties:

- **Pressure Reducing Valve (PRV)**: limits the hydraulic head on its downstream side to a specified setpoint when the upstream head exceeds it.
- **Pressure Sustaining Valve (PSV)**: maintains the hydraulic head on its upstream side above a specified setpoint when the downstream head would otherwise pull it below.
- **Flow Control Valve (FCV)**: restricts the volumetric flow through it to a specified setpoint.
- **Throttle Control Valve (TCV)**: applies a specified head loss coefficient; it behaves as a pipe with adjustable resistance.
- **General Purpose Valve (GPV)**: head loss as a function of flow is entirely described by a user-supplied piece-wise linear curve.
- **Positional Control Valve (PCV)**: The loss coefficient varies with a percent-open setting, optionally governed by a user-supplied curve relating the valve opening to the ratio of its loss coefficient to its fully-open loss coefficient.
- **Pressure Breaker Valve (PBV)**: imposes a fixed head-loss setpoint. When the setting exceeds the natural minor-loss head drop at the current flow, the solver forces the exact head loss; otherwise the PBV falls back to ordinary pipe resistance. PBVs have no control states — they always contribute a resistance.

### Curves and Patterns

User-defined **curves** are piece-wise linear relationships used throughout the model: pump head versus flow, pump efficiency versus flow, tank volume versus elevation, general purpose valve head loss versus flow, and positional valve opening versus loss ratio. Intermediate values are obtained by linear interpolation between the two bracketing data points.

**Patterns** are repeating sequences of dimensionless multipliers indexed by time. They modulate base demands at junctions, pump speed settings, and constituent source concentrations over the course of the simulation. At each hydraulic time step the pattern multiplier is determined by the **pattern period index**: given a global pattern start offset $t_{\text{start}}$, pattern time step $\Delta t_p$, and current simulation time $t$, the number of elapsed periods is $p = \lfloor (t + t_{\text{start}}) / \Delta t_p \rfloor$. For a demand category assigned to pattern $j$ of length $L_j$, the applicable multiplier is $F_j[p \bmod L_j]$, giving each pattern an independently repeating cycle. A **default pattern** is available and is applied to any demand category that has no explicit pattern assigned; if no default pattern exists either, a multiplier of 1.0 is used.

**Reservoir head patterns**: while reservoirs are nominally fixed-grade nodes, each reservoir may optionally be assigned a time pattern. When assigned, the head at that reservoir at each hydraulic time step equals its base elevation multiplied by the current pattern multiplier. This allows time-varying source heads to represent, for example, tidal fluctuations or varying water tower levels.

**Pump utilisation patterns**: each pump may have a separate utilisation pattern (distinct from any energy cost pattern) that controls the pump's speed setting at each hydraulic time step. The current pattern multiplier is applied directly as the normalised speed $\omega$; a multiplier of zero closes the pump, while a multiplier of 1.0 sets it to its rated speed. This allows pump schedules to be encoded as a time series without requiring explicit control rules.

---

## 2. Hydraulic Simulation

### 2.1 Head Loss in Pipes

**Hydraulic head** at any node is the mechanical energy per unit weight of water:

$$H = \frac{P}{\rho g} + z$$

where $P$ is the gauge pressure, $\rho$ is the water density, $g$ is gravitational acceleration, and $z$ is the elevation above datum. Flow from node $i$ to node $j$ is driven by the head difference $H_i - H_j$.

Three empirical head-loss formulas are available, and one is selected uniformly for the entire network.

#### Hazen-Williams Formula

$$h_f = \frac{4.727 \, L}{C^{1.852} \, D^{4.871}} \, Q^{1.852}$$

Here $L$ is the pipe length, $D$ is the internal diameter, $C$ is the Hazen-Williams roughness coefficient (higher values indicate smoother pipes), and $Q$ is the volumetric flow rate. The flow exponent is $n = 1.852$. This formula is empirical and strictly valid only for turbulent flow of water at ordinary temperatures.

#### Darcy-Weisbach Formula

$$h_f = f \cdot \frac{L}{D} \cdot \frac{V^2}{2g} = f \cdot \frac{8 L}{\pi^2 g D^5} \, Q^2$$

where $V = Q / (\pi D^2 / 4)$ is the mean flow velocity and $f$ is the dimensionless Darcy friction factor, which depends on the Reynolds number $Re = VD/\nu$ and the relative roughness $\varepsilon/D$.

For **laminar flow** ($Re \leq 2000$) the Hagen-Poiseuille result applies:

$$f = \frac{64}{Re}$$

yielding a head loss proportional to $Q$ (linear regime).

For **turbulent flow** ($Re \geq 4000$) the friction factor is computed from the Swamee-Jain approximation to the Colebrook-White implicit equation:

$$f = \left[ -2 \log\!\left( \frac{\varepsilon}{3.7 D} + \frac{5.74}{Re^{0.9}} \right) \right]^{-2}$$

where $\varepsilon$ is the absolute roughness. The quantity $f$ and its derivative with respect to $Q$ are evaluated simultaneously at each Newton iteration so that the linearisation of the solver (§2.4) remains consistent.

For **transitional flow** ($2000 < Re < 4000$), a cubic polynomial ensures continuity in $f$ and $df/dQ$ across the transition. The polynomial is anchored at both ends: $f = 64/Re$ at $Re = 2000$ (exact laminar value), and the Swamee-Jain value and its derivative at $Re = 4000$ (turbulent end). The laminar Hagen-Poiseuille branch handles flows below a pipe-geometry-dependent low-flow threshold independently and does not call this cubic.

#### Chezy-Manning Formula

$$h_f = \left( \frac{4 n_M}{1.486 \, \pi \, D^2} \right)^2 \left( \frac{D}{4} \right)^{-4/3} L \, Q^2$$

where $n_M$ is the Manning roughness coefficient. The exponent on $Q$ is 2 in this formulation (as used for full circular pipes). This formula is less common for pressurised systems but is supported for completeness.

#### Minor Losses and Total Head Loss

Minor (local) losses due to fittings, bends, and contractions are modelled as:

$$h_{\text{minor}} = K_m \, Q \, |Q|$$

where $K_m$ is the minor loss coefficient (head loss per unit of $Q^2$). The sign convention ensures the loss opposes flow in either direction.

The **total head loss** across a pipe combining friction and minor losses is:

$$h = R \, Q^n \cdot \mathrm{sign}(Q) + K_m \, Q \, |Q|$$

where $R$ is the friction resistance coefficient derived from whichever formula is in use and $n$ is the corresponding flow exponent (1.852 for Hazen-Williams, 2 for Darcy-Weisbach and Chezy-Manning in their simplified forms). The sign convention ensures that the expression is an odd function of $Q$: head loss is always in the direction opposing flow.

### 2.2 Pump Head Gain

A pump adds head to the flow. Three types of pump curves are supported.

**Power-function curve**: the head gain follows

$$\Delta H = h_0 - r \, Q^N$$

where $h_0$ is the shutoff head (head at zero flow), $r$ is a resistance-like coefficient, and $N$ is the curve exponent. This three-parameter form fits most centrifugal pump characteristics well.

**Constant-power pump**: the head gain is determined by maintaining a fixed power output regardless of flow:

$$\Delta H = \frac{\text{Power}}{\gamma \, Q}$$

where $\gamma = \rho g$ is the specific weight of water. As flow decreases toward zero, the head gain grows without bound. The solver handles this by monitoring the head-loss gradient $|\partial h / \partial Q| = r / Q^2$: when this gradient exceeds $C_\infty \approx 10^8$ (near-zero flow), the pump is treated as a closed link ($P_k = 1/C_\infty$, $Y_k = Q_k$); when the gradient falls below $10^{-6}$ (extremely high flow), the pump is treated as a fully open link with minimal resistance. Between these extremes, the standard linearisation $h = r/Q$ applies. Independently, a constant-power pump whose flow falls to zero or below is set to **TEMPCLOSED** status (see pump status below), because the power formula is undefined at zero flow.

**Custom curve**: a user-defined piece-wise linear head-versus-flow curve. At any operating point the solver identifies the two adjacent data points that bracket the current flow and interpolates linearly to obtain the head gain and its derivative. For initial flow conditions before the first solve, the design flow $Q_0$ for a custom-curve pump is taken as the midpoint between the first and last flow data points on the curve (rather than a named design point).

**Speed scaling via affinity laws**: when a pump operates at a relative speed $\omega$ (with $\omega = 1$ being its rated speed), the affinity laws relate the scaled curve to the rated curve:

$$\Delta H(\omega, Q) = \omega^2 \cdot \Delta H_1\!\left(\frac{Q}{\omega}\right)$$

where $\Delta H_1$ is the head gain at rated speed. Equivalently, the shutoff head scales as $\omega^2$ and the flow axis scales as $\omega$. In the Newton-Raphson solver, a pump is treated as a link with a **negative** head-loss value (a gain), and its linearised resistance coefficient $P_k$ and offset $Y_k$ are derived from the pump curve in the same algebraic framework as pipe head losses.

**Three-point pump curve fitting**: when a pump is specified by three operating points — the shutoff head $h_0$ (head at zero flow), the design point $(q_1, h_1)$, and the maximum-flow point $(q_2, h_2)$ — the power-function parameters are determined analytically:

$$c = \frac{\ln\!\left(\dfrac{h_0 - h_2}{h_0 - h_1}\right)}{\ln\!\left(\dfrac{q_2}{q_1}\right)}, \qquad b = \frac{h_0 - h_1}{q_1^{\,c}}, \qquad a = h_0$$

yielding the curve $\Delta H = a - b \, Q^c$. The curve is validated: it must be strictly decreasing in head ($h_0 > h_1 > h_2$), the exponent $c$ must be positive, and additionally $c \leq 20$ (an upper-bound sanity check enforced by the validator).

**Pump status — XHEAD**: a pump in the OPEN state transitions to the **XHEAD** (excess head) state when the head gain required to maintain the computed flow exceeds the speed-adjusted shutoff head $\omega^2 h_0$. In this state the pump is treated as a closed link for that iteration. The status reverts to OPEN at the start of each periodic status check, and is re-tested against the new computed operating point. For constant-power pumps, XHEAD cannot occur; instead, the pump is flagged TEMPCLOSED if the flow falls to zero (since the power formula is undefined at zero flow).

**Initial flow conditions**: before the first Newton-Raphson solve, link flows are initialised as follows — closed links receive a negligible flow $Q_0 \approx 10^{-6}$ ft³/s; pumps receive the product of their speed setting and their design flow $Q_{\text{design}}$; all other links (pipes and valves) receive the flow corresponding to a nominal velocity of 1 ft/s through the full pipe cross-section: $Q = \pi D^2 / 4$. These initial values need not be physically consistent; the Newton-Raphson iteration converges from them to the true solution.

### 2.3 Valve Behaviour

The three control valves — PRV, PSV, and FCV — can inhabit one of three discrete states at any iteration: **active**, **open**, or **closed**.

- **Active**: the valve enforces its design constraint. A PRV fixes the downstream head equal to its setpoint; a PSV fixes the upstream head equal to its setpoint; an FCV fixes the flow through it equal to its setpoint. When active, these valves introduce a **head constraint** rather than a resistance relationship, and the corresponding row of the linear system is modified accordingly.
- **Open**: the valve is behaving as a short section of pipe with negligible resistance; no constraint is enforced.
- **Closed**: the valve passes no flow.

After each Newton-Raphson iteration the hydraulic state of each control valve is examined:

- A PRV transitions from ACTIVE to OPEN if the upstream head has fallen to or below the setpoint (no pressure reduction needed), or to CLOSED if the downstream head exceeds the setpoint (would require reverse flow to maintain it).
- A PSV transitions from ACTIVE to OPEN if the downstream head has risen to or above the setpoint, or to CLOSED if the upstream head falls below the setpoint.
- An FCV transitions to OPEN if the available head difference is insufficient to sustain the target flow, or to CLOSED if the target flow is negative.

TCV, GPV, and PCV valves do not have control states; they always contribute a resistance (head loss as a function of flow) determined by their current setting.

**Precise valve status transitions**: each control valve's state is re-evaluated after each Newton-Raphson iteration (governed by the DampLimit parameter; see §2.4). The transition rules are:

- *PRV*: ACTIVE → OPEN if $H_1 - K_m Q^2 < H_\text{set} - \varepsilon_H$ (upstream pressure insufficient to need reduction); ACTIVE → CLOSED if $Q < -\varepsilon_Q$ (reverse flow). OPEN → ACTIVE if $H_2 \geq H_\text{set} + \varepsilon_H$ (downstream pressure reaches setpoint). CLOSED → ACTIVE if $H_1 \geq H_\text{set} + \varepsilon_H$ and $H_2 < H_\text{set} - \varepsilon_H$; CLOSED → OPEN if $H_1 < H_\text{set} - \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$. The special XPRESSURE state (reverse pressure gradient that would require reverse flow) transitions to CLOSED on reverse flow.
- *PSV*: symmetric to PRV — ACTIVE → OPEN if $H_2 + K_m Q^2 > H_\text{set} + \varepsilon_H$; OPEN → ACTIVE if $H_1 < H_\text{set} - \varepsilon_H$; CLOSED → ACTIVE if $H_1 \geq H_\text{set} + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$ (upstream head at or above setpoint and exceeds downstream); CLOSED → OPEN if $H_2 > H_\text{set} + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$. The XPRESSURE state transitions to CLOSED on reverse flow.
- *FCV*: transitions to XFCV (cannot enforce set point) if the head difference across the valve is negative or flow is negative. A third ACTIVE→XFCV condition also exists: when the valve is active but the pressure drop across it implies a head-loss coefficient smaller than its fully-open minor-loss coefficient (i.e., the network cannot maintain even a friction-free connection without violating the setpoint), the valve also reverts to XFCV. Transitions back to ACTIVE from XFCV once the flow meets or exceeds the setting.

Here $H_\text{set}$ is the absolute head setpoint (elevation of the controlled node plus the setting in pressure-head units), $K_m Q^2$ is the minor-loss head drop at the current flow, and $\varepsilon_H$, $\varepsilon_Q$ are user-configured head and flow tolerances. **Important distinction**: $\varepsilon_H = \text{Htol}$ (default 0.0005 ft) and $\varepsilon_Q = \text{Qtol}$ (default 0.0001 ft³/s) are tolerances used **only** in link status transition tests. They are entirely separate from the convergence tolerance $\text{Hacc}$ (default 0.001), which governs solver termination via the relative flow-change criterion (§2.4). Confusing $\text{Qtol}$ with $\text{Hacc}$ leads to incorrect valve and check-valve status behaviour.

**Linearisation coefficients for the resistance-type valve types**:

*Throttle Control Valve (TCV)*: the minor-loss coefficient is computed from the valve setting $s$ (a dimensionless loss coefficient value) and pipe diameter $D$ (in feet):

$$K_m = \frac{0.02517 \, s}{D^4}$$

This factor converts from the user-supplied dimensionless loss coefficient into the internal US customary unit system (flow in ft³/s, head in ft). The coefficient enters the standard minor-loss formula $h = K_m Q |Q|$.

*Pressure Breaker Valve (PBV)*: a PBV imposes a fixed head loss equal to its setting $h_\text{set}$ (in feet) when the setting exceeds the current minor-loss head drop $K_m Q^2$. In this active regime the solver enforces the exact head drop by assigning very large linearisation coefficients: $P_k = C_\infty$ and $Y_k = h_\text{set} \cdot C_\infty$, where $C_\infty$ is a large constant ($\approx 10^8$). Because the GGA flow update is $\Delta Q_k = P_k (H_i - H_j) - Y_k$, this drives $H_i - H_j \to Y_k / P_k = h_\text{set}$ extremely strongly on every iteration. If the current minor-loss head drop already exceeds the setting (the valve is overmatched), the PBV instead falls back to ordinary pipe treatment.

*General Purpose Valve (GPV)*: the solver evaluates the user-supplied head-loss-vs-flow curve at the current absolute flow $|Q|$ to extract the local slope $r$ (ft per ft³/s) and zero-intercept $h_0$ (ft) of the bracketing linear segment. The linearisation coefficients are:

$$P_k = \frac{1}{r}, \qquad Y_k = \left(\frac{h_0}{r} + |Q|\right) \mathrm{sign}(Q)$$

*Positional Control Valve (PCV)*: the percent-open setting $s$ is mapped through a user-supplied opening-to-flow-coefficient ratio curve (linearly extrapolated beyond its endpoints if necessary) to obtain the dimensionless ratio $k_{vr} = K_v / K_{v0}$, where $K_{v0}$ is the flow coefficient at full open. The curve's $x$-axis is percent open and its $y$-axis is $K_v / K_{v0}$ as a percentage. The effective minor-loss coefficient is then:

$$K_m = \frac{K_{m0}}{k_{vr}^2}$$

where $K_{m0}$ is the fully-open minor-loss coefficient. This reflects the relationship that for a given flow, halving the effective orifice area quadruples the head loss.

**Active-state matrix modifications for PRV, PSV, and FCV**:

When these valves are in the ACTIVE state, $P_k$ is set to zero and the link is excluded from the standard off-diagonal assembly (which skips links with $P_k = 0$). Instead, the linear system is augmented directly to enforce the valve's constraint:

*PRV active*: the downstream node head is pinned to the absolute setpoint $H_\text{set} = z_{n_2} + s_k$ by injecting a large conductance into the diagonal and a correspondingly large forcing term into the RHS:

$$A_{jj} \mathrel{+}= C_\infty, \qquad F_j \mathrel{+}= H_\text{set} \cdot C_\infty$$

The link's $Y_k$ is set to the current flow plus the downstream node's flow excess, maintaining approximate flow balance. Any excess inflow at the downstream node is redistributed to the upstream node's RHS to preserve global mass conservation.

*PSV active*: identical treatment applied to the upstream node $i$ with $H_\text{set} = z_{n_1} + s_k$. A small residual conductance ($1/C_\infty$) is also added through the off-diagonal entry to preserve matrix connectivity and avoid numerical singularity.

*FCV active*: the link is rendered nearly disconnected by setting $P_k = 1/C_\infty \approx 0$. The setpoint flow $Q_\text{set}$ is injected as an external demand at the upstream node and as an external supply at the downstream node, both in the flow-excess array and in the RHS vector:

$$F_i \mathrel{-}= Q_\text{set}, \qquad F_j \mathrel{+}= Q_\text{set}$$

The two sides of the FCV are effectively decoupled; the network is solved as if $Q_\text{set}$ flows through the valve as a prescribed boundary condition, and the resulting head difference across the valve is whatever the network produces with that imposed flow.

**Check valve (CVPIPE) status transitions**: a check valve is treated as a pipe that is permitted to carry flow only in its positive direction. After each Newton-Raphson iteration, its status is re-evaluated. Let $\Delta h = H_i - H_j$ be the head difference and $Q$ the current flow:

- If $|\Delta h| > H_{\text{tol}}$: CLOSED if $\Delta h < -H_{\text{tol}}$ (reverse head gradient); CLOSED if $Q < -Q_{\text{tol}}$ (reverse flow); otherwise OPEN.
- If $|\Delta h| \leq H_{\text{tol}}$: CLOSED if $Q < -Q_{\text{tol}}$; otherwise the current status is preserved.

This hysteresis prevents rapid cycling near the zero-flow condition.

### 2.4 The Global Gradient Algorithm

The hydraulic solver at each time step employs the **Todini-Pilati Global Gradient Algorithm (GGA)**, a variant of Newton-Raphson that solves simultaneously for all unknown junction heads and then derives all link flows in a single update.

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

At iteration $m$, the head-loss function of link $k$ is linearised around the current flow estimate $Q_k^{(m)}$:

$$h_k(Q_k) \;\approx\; \frac{\partial h_k}{\partial Q_k}\bigg|_{Q^{(m)}} Q_k \;-\; Y_k$$

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

$$Q_k^{(m+1)} = P_k \left( H_i^{(m+1)} - H_j^{(m+1)} \right) + Y_k$$

where $i$ is the upstream node and $j$ the downstream node of link $k$ according to the assumed positive direction. For pumps, $H_j - H_i = \Delta H_k > 0$, so the sign convention is consistent.

Emitter flows, leakage flows, and pressure-dependent demand flows are similarly updated using their own linearised head-flow relationships.

#### Convergence Criterion

The primary convergence criterion is **flow accuracy**: the sum of absolute flow changes relative to total absolute flow must fall below the user-specified tolerance and no link's hydraulic status may have changed during the most recent iteration:

$$\epsilon = \frac{\displaystyle\sum_k \left| Q_k^{(m+1)} - Q_k^{(m)} \right|}{\displaystyle\sum_k \left| Q_k^{(m+1)} \right|} \leq \epsilon_{\text{tol}}$$

When the total absolute flow $\sum_k |Q_k^{(m+1)}|$ falls at or below $\epsilon_{\text{tol}}$ (near-stagnant network), the relative formula cannot be used; in that case the solver returns the absolute flow change $\sum_k |\Delta Q_k|$ directly rather than the ratio.

Two kinds of status check run during iteration, with different scheduling:

- **Valve status checks** (`valvestatus`, governing PRV and PSV transitions): when `DampLimit = 0` (the default), these run after every flow update. When `DampLimit > 0`, they are deferred until the relative flow error at or below `DampLimit`; at that point damping (relaxation factor 0.6) is also activated simultaneously.
- **Link status checks** (`linkstatus`, governing pumps, check valves, and pipes adjacent to tanks): these run periodically. The first check occurs at iteration *CheckFreq* and repeats every *CheckFreq* iterations thereafter, but stops once *MaxCheck* iterations have been reached. This staging prevents premature status oscillation during early iterations when flows are far from convergence.

When `hasconverged` returns true, a full status check is performed — all three routines (`valvestatus`, `linkstatus`, and `pswitch`) are called regardless of the CheckFreq/DampLimit schedule. If any of them changes a link's status the iteration counter resets and the solve continues; only a convergence pass that produces no status changes terminates the loop.

The **ExtraIter** parameter handles networks that fail to converge because of status cycling — a cycle in which links repeatedly toggle between open and closed states without settling to a consistent configuration. When convergence is not achieved within MaxIter iterations and ExtraIter > 0, an additional ExtraIter iterations are performed with the periodic link-status checks (`linkstatus`) suspended — pumps, check valves, and tank-adjacent pipes no longer change state. `valvestatus` (PRV/PSV) continues to run every iteration. This allows the linear system to converge to a solution consistent with the current link configuration, even if that configuration is not the true steady state. A warning is issued that the system is unbalanced. If ExtraIter = −1, the solver sets a halt flag (`Haltflag`) after the current time step's results are saved; the simulation then terminates at the start of the next step rather than stopping mid-step.

Two supplementary convergence criteria may also be applied. **FlowChangeLimit** terminates iteration early if the maximum absolute flow change in any link during the most recent iteration falls at or below the specified threshold. **HeadErrorLimit** terminates iteration if the maximum absolute head residual (flow imbalance expressed in head units) at any node is at or below its threshold. Both default to zero, which disables them; in the default configuration the sole termination criterion is $\epsilon \leq \epsilon_{\text{tol}}$ with no status change.

**Damping (RelaxFactor)**: the Newton flow update $\Delta Q_k$ may be scaled by a relaxation factor to improve convergence stability. By default `RelaxFactor = 1.0` (full Newton step). When `DampLimit > 0` and the relative flow error falls at or below `DampLimit`, `RelaxFactor` is set to 0.6 for that iteration. This under-relaxation is applied uniformly to all link flow updates, emitter flow updates, and pressure-dependent demand flow updates:

$$Q_k^{(m+1)} = Q_k^{(m)} - \text{RelaxFactor} \cdot \Delta Q_k$$

The purpose is to stabilise convergence in networks with highly nonlinear elements (e.g., active control valves) by reducing the step size when the solver is close to convergence but oscillating.

**Matrix recovery (`badvalve`)**: if the Cholesky factorisation of $\mathbf{A}$ fails at a diagonal entry corresponding to node $n$, the solver checks whether an active PRV, PSV, or FCV has that node as one of its endpoints. If found, the valve's status is forced to XPRESSURE (for PRV/PSV) or XFCV (for FCV), breaking the singularity that the active-state matrix modification introduced. The solver then retries the factorisation and solve. This recovery mechanism ensures that ill-conditioned valve configurations do not crash the solver.

The **XFLOW** status exists in the link status enumeration ("pump exceeds maximum flow") but is not currently triggered by the steady-state solver — the pump status evaluation checks only whether the head gain exceeds the speed-adjusted shutoff head (yielding XHEAD), and does not compare flow against the maximum-flow point of the pump curve. XFLOW is therefore a reserved diagnostic state rather than one raised during normal simulation.

### 2.5 Sparse Linear Algebra

The matrix $\mathbf{A}$ is symmetric and positive semi-definite (it is a Laplacian, plus small positive diagonal contributions from emitters and demands). Its sparsity pattern corresponds to the adjacency structure of the junction subgraph. Efficient solution is essential because this system must be solved at every Newton iteration of every hydraulic time step.

Three phases are performed:

**Phase 1 — Node reordering (performed once before simulation begins)**: the Multiple Minimum Degree (MMD) algorithm reorders the junction indices to minimise the fill-in that occurs during Cholesky factorisation. Fill-in arises when a non-zero appears in the factor $\mathbf{L}$ at a position that was zero in $\mathbf{A}$; reordering the rows and columns can dramatically reduce the number of such positions. Parallel links (multiple pipes connecting the same pair of nodes in the same direction) are condensed into a single equivalent link before reordering to avoid redundancy.

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

where $P$ is the gauge pressure at the node, $P_{\min}$ is the pressure below which no demand is delivered, $P_{\text{req}}$ is the pressure at which full demand is delivered, $D_{\text{full}}$ is the requested demand, and $n_P$ is the pressure exponent. The default value of $n_P = 0.5$ corresponds to the Wagner formula, which models demand as proportional to the square root of the available pressure head above the minimum threshold.

The PDA model is incorporated into the GGA by treating the pressure-dependent component of demand as a pressure-dependent emitter at each junction (cf. §4). The demand-pressure function is inverted to express pressure as a function of demand:

$$P = P_{\min} + (P_{\text{req}} - P_{\min}) \left( \frac{D}{D_{\text{full}}} \right)^{1/n_P}$$

This is linearised and added to the diagonal of $\mathbf{A}$ and the right-hand side $\mathbf{F}$. Barrier terms prevent the numerical demand from drifting below zero or above $D_{\text{full}}$, maintaining the physical bounds throughout the iteration. The barrier is implemented as a smooth differentiable approximation (not a hard constraint) to avoid discontinuities that would break the Newton-Raphson iteration. Specifically, the signed head-loss and gradient increments from a lower barrier at $Q = 0$ take the form:

$$\Delta h = \frac{a - \sqrt{a^2 + 10^{-6}}}{2}, \qquad \Delta(\partial h/\partial Q) = \frac{10^9}{2}\left(1 - \frac{a}{\sqrt{a^2 + 10^{-6}}}\right), \qquad a = 10^9 \, Q$$

which approaches a large one-sided penalty as $Q \to 0^-$ while remaining smooth and differentiable throughout. An analogous upper barrier is applied at $Q = D_{\text{full}}$.

---

## 4. Emitters

An emitter represents a device — a sprinkler head, orifice, or nozzle — that discharges water from a junction at a rate governed by the local pressure. The emitter flow is:

$$Q_e = K_e \, P^{n_e}$$

where $K_e$ is the emitter discharge coefficient, $P$ is the gauge pressure at the junction, and $n_e$ is the pressure exponent (typically 0.5 for an orifice). Emitters can also represent aggregate leakage if high spatial resolution is not required.

Within the GGA, an emitter is treated as an additional element at its junction. The head-flow relationship is inverted:

$$H - z = C_e \, Q_e^{1/n_e}$$

where $H - z$ is the pressure head and $C_e = K_e^{-1/n_e}$. This is linearised at the current flow estimate and added to the matrix: the linearised conductance $P_e = 1 / (\partial h_e / \partial Q_e)$ is added to the diagonal of $\mathbf{A}$ at the relevant junction, and the corresponding $Y_e$ term is added to the right-hand side.

By default emitters can admit reverse flow (suction) if the junction pressure falls below the emitter's reference elevation. An **emitter backflow** option can disable this: when backflow is forbidden, a lower barrier function is applied to the head-loss gradient whenever the emitter flow would go negative. The barrier takes the same smooth differentiable form used for PDA demand bounds (§3.2):

$$\\Delta h = \\frac{a - \\sqrt{a^2 + 10^{-6}}}{2}, \\qquad \\Delta(\\partial h/\\partial Q) = \\frac{10^9}{2}\\left(1 - \\frac{a}{\\sqrt{a^2 + 10^{-6}}}\\right), \\qquad a = 10^9 \\, Q_e$$

This adds a large one-sided penalty as $Q_e \\to 0^-$ while remaining smooth and differentiable, strongly driving $Q_e \\geq 0$ without creating a hard discontinuity that would break convergence.

---

## 5. Pipe Leakage — the FAVAD Model

Background leakage from deteriorated pipes — through corroded joints, stress cracks, and micro-fractures — is modelled using the **FAVAD** (Fixed And Variable Area Discharge) framework. Unlike a simple orifice, pipe cracks may dilate under pressure, making the effective discharge area itself pressure-dependent. The FAVAD model captures this through:

$$Q_{\text{leak}} = C_o \left( A_o + m H \right) \sqrt{H}$$

where $H$ is the pressure head at the pipe midpoint, $A_o$ is the fixed (zero-pressure) crack area, $m$ is the rate of increase of crack area with pressure, and $C_o = 0.6\\sqrt{2g}$ is the orifice discharge coefficient. In the internal US customary unit system (flow in ft³/s, head in ft, area in ft²), $C_o \\approx 4.815 \\times 10^{-6}$ ft³/(s·ft^{1/2}) when area is expressed in the units used by the FAVAD crack parameters. Expanding this:

$$Q_{\text{leak}} = C_o A_o H^{1/2} + C_o m H^{3/2}$$

The two terms have different pressure exponents: the fixed-area term behaves like a standard orifice (exponent $1/2$), while the variable-area term has exponent $3/2$.

These are decomposed into **two equivalent emitters** at each node, one for each component:

$$H = C_{\text{fa}} \, Q_{\text{fa}}^{2} \qquad \text{(fixed-area component, orifice-type, exponent } 1/2 \text{ on } H\text{)}$$

$$H = C_{\text{va}} \, Q_{\text{va}}^{2/3} \qquad \text{(variable-area component, exponent } 3/2 \text{ on } H\text{)}$$

The resistance coefficients $C_{\text{fa}}$ and $C_{\text{va}}$ are determined from the FAVAD parameters. For each pipe whose **both** end nodes are junctions, the pipe's leakage contribution is split equally: half is attributed to each end node (the pipe is split conceptually at its midpoint). When one end of a pipe is a fixed-grade node (reservoir or tank), that fixed-grade end cannot accumulate leakage in the nodal model; the junction at the other end therefore receives the **full** pipe-length contribution rather than half. The contributions of all pipes meeting at a given junction are aggregated: the total fixed-area conductance and variable-area conductance at the node are the sums over all incident (half- or full-) pipe contributions. The resulting nodal coefficients are then inverted to form $C_{\text{fa}}$ and $C_{\text{va}}$.

**Derivation of $C_{\text{fa}}$ and $C_{\text{va}}$**: let $\text{LeakCoeff1}_p$ and $\text{LeakCoeff2}_p$ be the full-pipe FAVAD discharge coefficients for pipe $p$. The per-end contribution for a junction endpoint $v$ of pipe $p$ is:

$$k_{1,p,v} = \begin{cases} \tfrac{1}{2}\,\text{LeakCoeff1}_p & \text{both end nodes of pipe } p \text{ are junctions} \\ \text{LeakCoeff1}_p & \text{exactly one end node of pipe } p \text{ is a fixed-grade node} \end{cases}$$

(with the same rule applied to $\text{LeakCoeff2}_p$ to give $k_{2,p,v}$). For junction $i$, the total discharge conductances are $K_{\text{fa},i} = \sum_{p \ni i} k_{1,p,i}$ and $K_{\text{va},i} = \sum_{p \ni i} k_{2,p,i}$. The resistance coefficients follow by inverting the discharge relations $Q = K H^{1/2}$ (fixed-area) and $Q = K H^{3/2}$ (variable-area):

$$C_{\text{fa},i} = 1/K_{\text{fa},i}^{2}, \qquad C_{\text{va},i} = 1/K_{\text{va},i}^{2/3}$$

(with the respective term omitted when $K = 0$).

These two emitter-like terms are linearised and incorporated into the GGA matrix assembly in exactly the same way as ordinary emitters (§4): a conductance term is added to the diagonal of $\mathbf{A}$ and a flow offset term is added to the right-hand side $\mathbf{F}$.

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

**Important**: rule-based controls are evaluated **after** the hydraulic solve, within the time-step computation (step 5), not before or during the solve. If a rule fires, the hydraulic step is shortened so that the next solve will reflect the new configuration — the current step's solution is **not** re-computed. Tank levels are updated by `ruletimestep()` at each rule sub-step interval; when no rules exist, `tanklevels()` is called directly during `timestep()`.

### 6.2 Adaptive Time Step

The hydraulic time step is not fixed; it adapts so that no physical limit is overshot. The actual step duration used is the minimum of five quantities:

- The user-specified nominal hydraulic time step.
- The time remaining until the next reporting interval (so that results are recorded at exactly the right moments).
- For each tank, the time at which the tank would reach its minimum level (if the net outflow continues at the current rate) or its maximum level (if the net inflow continues): $\Delta t_{\text{tank}} = \Delta V_{\text{available}} / |Q_{\text{net}}|$.
- The time until the next scheduled change in any pattern (so that demand or pump multipliers change at exactly the right instant).
- The time remaining until the end of the simulation.

This adaptive strategy avoids the need for post-hoc correction of tank levels and ensures pattern changes are applied at their intended times.

**Default time step derivation**: if the user does not specify either the quality time step or the rule evaluation time step, both default to $\Delta t_h / 10$, capped at $\Delta t_h$. The rule time step is additionally aligned so that evaluations fall on even multiples of the rule step within each hydraulic period — the first evaluation within a period may therefore be shorter than one full rule step to achieve this alignment. The quality time step is further constrained so it never exceeds the hydraulic time step. The hydraulic time step itself is clamped at the minimum of the user-specified nominal step, the pattern time step, and the reporting time step.

### 6.3 Tank Level Update

After each hydraulic solution, the change in stored volume during the time step is:

$$\Delta V = Q_{\text{net}} \cdot \Delta t$$

where $Q_{\text{net}} = Q_{\text{in}} - Q_{\text{out}}$ is the net volumetric flow rate into the tank. For a **constant cross-section** tank with cross-sectional area $A$, the level change is:

$$\Delta h = \frac{\Delta V}{A}$$

For a tank described by a **volume-elevation curve**, the new volume $V_{\text{new}} = V_{\text{old}} + \Delta V$ is looked up in the curve to find the corresponding new water surface elevation.

If the new level would fall below the minimum, the level is clamped at the minimum and the tank is treated as a fixed-grade node (like a small reservoir at its minimum head) for the next time step. If the new level would exceed the maximum and overflow is allowed, the surplus volume exits freely and the tank remains at its maximum level; if overflow is not permitted, the tank is clamped at its maximum level and treated as fixed-grade.

**TEMPCLOSED for links at tank limits**: independently of the level-clamping applied after the hydraulic step, within each Newton-Raphson iteration the status of every link adjacent to a tank is examined. If a link is carrying flow *into* a tank whose current head equals or exceeds the maximum level and overflow is not permitted, that link is set to TEMPCLOSED for the current iteration — it contributes no conductance and the coefficient matrix is assembled as if the link were closed. Similarly, a link carrying flow *out of* a tank at its minimum level is TEMPCLOSED. At the start of the next iteration, all TEMPCLOSED and XHEAD links are re-opened before the new operating point is evaluated, so the status is re-tested fresh at each iteration rather than being locked in.

---

## 7. Control Systems

### 7.1 Simple Controls

A simple control consists of a single condition and a single action. The condition is either a **level control** — a node's pressure or hydraulic grade exceeds or falls below a specified threshold — or a **timer control** — the simulation clock reaches a specified time or the current time of day reaches a specified hour.

When a simple control fires, its action is applied immediately: it may open or close a link, change a pump's speed setting, or change a valve's setting. Simple controls are evaluated once at the start of each hydraulic time step. If a control fires and changes the network configuration, the subsequent hydraulic solution reflects the new state for the entire duration of that time step.

For **level controls** on tanks, a small hysteresis margin is applied to prevent chattering when the tank is exactly at the control threshold under non-zero flow. The trigger condition is checked against the tank volume corresponding to the control's grade level, with a margin equal to the current absolute value of the tank's net demand flow rate (`|NodeDemand|`, in internal flow units). A low-level control fires when the current tank volume falls at or below the threshold volume plus the margin; a high-level control fires when the current tank volume reaches or exceeds the threshold volume minus the margin.

### 7.2 Rule-Based Controls

Rule-based controls support arbitrarily complex conditional logic and are well suited to representing operational strategies such as "turn on pump A if tank X level falls below 5 m AND time of day is between 22:00 and 06:00."

A rule has the structure:

> **IF** (premise₁) **AND/OR** (premise₂) **…** **THEN** (action₁, action₂, …) **ELSE** (action₃, action₄, …)

Premises may test:

- Node pressure, hydraulic grade, or demand
- Link flow rate, status, or setting
- Pump power output
- Tank fill time (time for the tank to reach its maximum at the current inflow rate) or drain time
- Simulation time or time of day

Rules are evaluated not just at the start of a hydraulic time step but at intermediate **rule time steps** that subdivide each hydraulic step. When a rule fires and changes the network state, the hydraulics are re-solved for the remaining fraction of the hydraulic step. This allows the simulation to capture, for example, a pump switching on mid-step in response to a falling tank level.

When multiple rules fire simultaneously and their THEN actions conflict (e.g., two rules disagree on the status of the same pump), **priority levels** resolve the conflict: the rule with the numerically higher priority value wins.

---

## 8. Water Quality Simulation

### 8.1 Overview and Simulation Modes

Water quality simulation is layered on top of the hydraulic solution. The flows and velocities computed during the hydraulic phase are stored and replayed during quality simulation, which advances in **quality time steps** that are no longer than hydraulic time steps, and are typically shorter, in order to resolve the advection of concentration fronts through pipes. The quality time step $\delta t$ is a user-specified parameter (independently of the hydraulic time step), typically set to a fraction of the hydraulic time step to satisfy the Courant stability condition for the advection scheme. Within each hydraulic period of duration $\Delta t_h$, the quality simulation executes multiple transport sub-steps of size $\min(\delta t, \text{remaining time})$, iterating until the full hydraulic period is consumed. The hydraulic flow field (velocities, directions) is held constant throughout the sub-cycles. Flow direction changes between consecutive hydraulic periods trigger a re-sort of the nodes into topological order before the next hydraulic period's sub-cycles begin.

Three simulation modes are available:

- **Chemical concentration**: the transport and reaction of a dissolved constituent (e.g., residual chlorine) through the network.
- **Water age**: the average time water has been resident in the distribution system, measured from the sources. No external source or reaction is needed; concentration is initialised to zero and increases at a uniform rate as time passes.
- **Source tracing**: the percentage of water at any point in the network that originated from a designated source node. The tracer is injected at 100% at the source; all other inflowing water carries 0%.

### 8.2 Lagrangian Segment Transport

Advective transport of water quality through pipes is modelled with a **Lagrangian moving-segment** scheme. Each pipe is represented as an ordered sequence of segments. Each segment has a volume and a uniform concentration. Segments are created when new water of a different concentration enters a pipe and destroyed when they are fully flushed out the other end.

**Advection step**: over a quality time step of duration $\delta t$, a volume $\mathcal{V}_k = Q_k \, \delta t$ of water is swept through pipe $k$. Starting from the upstream end of the pipe, segments are consumed one by one. The mass and volume of each consumed fraction are tracked; when the cumulative volume consumed equals $\mathcal{V}_k$, the remaining portion of the last consumed segment is returned to the front of the pipe. The total mass and volume that exited the pipe's downstream end are accumulated at the downstream node.

**Nodal mixing**: once all pipes have been processed, the outflow concentration at each junction is computed as:

$$c_{\text{out},i} = \frac{\displaystyle\sum_{k \in \text{in}(i)} m_k^{\text{out}}}{\displaystyle\sum_{k \in \text{in}(i)} \mathcal{V}_k^{\text{out}}}$$

where $m_k^{\text{out}}$ and $\mathcal{V}_k^{\text{out}}$ are the mass and volume that flowed out of (i.e., into node $i$ from) pipe $k$ during the quality step. This is the **complete instantaneous mixing** assumption: water entering a junction from all sources is instantly and uniformly blended. The resulting concentration $c_{\text{out},i}$ is then pushed as a new upstream segment into each pipe carrying flow away from node $i$.

**Topological ordering**: for the advection step to be computed correctly — that is, for each node's outflow concentration to reflect all its upstream contributions — the nodes must be processed in topological order (upstream nodes before downstream nodes). The network is sorted into such an order prior to the quality simulation. For looped networks where no pure topological sort exists, nodes that share mutual dependencies are processed with a fallback ordering strategy.

**Stagnant junctions**: when a junction has zero net inflow during a quality time step (dead-end or temporarily stagnant condition) and the constituent is reactive, the junction concentration is updated to the arithmetic mean of the concentrations in the segment **nearest to the junction** on each incident link. For a pipe whose flow runs into the node (the node is the pipe's downstream end), this is the pipe's downstream-end segment (`FirstSeg`); for a pipe whose flow runs away from the node (the node is the pipe's upstream end), this is the pipe's upstream-end segment (`LastSeg`). In both cases the chosen segment is the one physically adjacent to the stagnant junction, regardless of flow direction. This heuristic prevents the unphysical drift in concentration that would otherwise occur at stagnant junctions driven purely by reaction kinetics without advective renewal.

**Segment merging**: after the outflow concentration of a node is computed, a new segment is pushed into each pipe carrying flow away from the node. Before a new segment is created, the node's outflow concentration $c_\text{node}$ is compared with the concentration of the segment already sitting at the upstream end of that outflow pipe (the pipe end adjacent to the node). If the difference is less than the threshold $C_\text{tol}$, the existing segment's volume and mass are merged with the new contribution rather than creating a separate segment. Without this merging step, each transport sub-step could create a new segment boundary, causing the total segment count to grow without bound in steady flow. Merging keeps the count manageable at the cost of negligible concentration smoothing.

**Segment memory management**: pipe segments are allocated from a pre-allocated in-memory pool backed by a free-list of recycled records. When a segment is fully flushed out of a pipe it is returned to the free list rather than released to the operating system. New segments are drawn from the free list first; fresh pool entries are used only when the free list is empty. If the pool is exhausted, an out-of-memory flag is raised and quality simulation is terminated gracefully with an error.

### 8.3 Source Terms

**Water quality sources** inject constituent into the network at designated nodes. Four injection types are available:

- **Concentration source**: the concentration of all water leaving the node is fixed at the specified value. At **reservoirs and tanks** this is applied directly. At **junctions** this type is effective only when the node has a net negative demand (i.e., the junction is itself a local inflow point, `NodeDemand < 0`); if the junction demand is non-negative the source contributes nothing. Inflows from other links are not overridden at junctions.
- **Mass inflow booster**: a fixed mass rate is added to the node continuously, regardless of flow conditions. The resulting concentration increment depends on the total flow through the node.
- **Setpoint booster**: if the naturally mixed concentration at the node falls below the specified setpoint, it is raised to the setpoint. If it already exceeds the setpoint, no adjustment is made.
- **Flow-paced booster**: a fixed concentration increment is added to the natural concentration of all water leaving the node, in proportion to the flow.

Source concentrations may vary over time via a multiplier pattern.

For all source types, a **stagnation guard** suppresses injection when the total volumetric outflow from the node during a quality sub-step is exactly zero (i.e. no volume leaves the node). This avoids division by zero when computing concentration increments. The separate QZERO threshold ($1.114 \times 10^{-5}$ ft³/s) governs whether a link's flow is treated as stagnant for topological-sort and transport purposes (§8.1); it does not apply to source injection.

**Water age** requires no source. The "concentration" is initialised to zero everywhere and incremented by $\delta t$ (in hours) at every quality time step, representing the elapsed time since the water entered the system from a source (reservoir or tank).

**Source tracing** assigns a "concentration" of 100 (representing 100%) to all water leaving the designated trace node. All water entering the network from other fixed-grade nodes or from tanks carries a concentration of zero. The trace value at any pipe segment or junction then represents the fraction of that water — expressed as a percentage — that originated from the traced source.

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
- **Michaelis-Menten kinetics** (negative order): $f(c) = c / (C_L \pm c)$, which models saturation kinetics — the reaction rate is approximately first-order at low concentrations and approximately zero-order at high concentrations relative to the half-saturation constant $C_L$.

#### Wall Reactions

Wall reactions represent the interaction of the constituent with the pipe wall material (e.g., disinfectant demand from biofilm or iron corrosion products). The mass transfer process has two stages in series: molecular diffusion from the bulk water to the wall, and the chemical reaction at the wall surface. Both stages must be overcome for mass to be transferred.

The analysis proceeds as follows:

1. Compute the **Reynolds number** $Re = VD/\nu$ and the **Schmidt number** $Sc = \nu / \mathcal{D}$, where $\nu$ is the kinematic viscosity of water and $\mathcal{D}$ is the molecular diffusivity of the constituent.

2. Compute the **Sherwood number** $Sh$, which characterises the ratio of convective to diffusive mass transfer:
   - Stagnant ($Re < 1$): $Sh = 2$ (pure diffusion limit)
   - Laminar ($1 \leq Re < 2300$): Graetz-Lévêque solution for developing concentration profiles in a tube:

   $$Sh = 3.65 + \frac{0.0668 \,(D/L)\,Re\,Sc}{1 + 0.04\,[(D/L)\,Re\,Sc]^{2/3}}$$

   - Turbulent ($Re \geq 2300$): Notter-Sleicher correlation:

   $$Sh = 0.0149 \, Re^{0.88} \, Sc^{1/3}$$

3. The **mass transfer coefficient** is:

   $$k_f = \frac{Sh \cdot \mathcal{D}}{D}$$

4. For **first-order** wall reactions ($n_w = 1$), the wall reaction rate $k_w$ (with units of velocity $[\text{m/s}]$) and the mass-transfer coefficient $k_f$ combine in series to give an effective first-order wall decay coefficient:

   $$k_{\text{eff}} = \frac{4}{D} \cdot \frac{k_w \, k_f}{k_f + |k_w|}$$

   Here $4/D$ converts from a surface-area basis to a volume basis for a circular pipe. The series combination ensures that if either the diffusion step ($k_f$) or the wall reaction step ($k_w$) is slow, it dominates the overall rate.

5. For **zero-order** wall reactions ($n_w = 0$), the wall demand rate $k_w$ (converted to internal units as $k_w \cdot f_u^2$ where $f_u$ is the elevation unit conversion factor) and the concentration-dependent diffusive supply rate $c \cdot k_f$ are compared independently. The effective volumetric wall rate is $\mathrm{sgn}(k_w) \cdot \min(|k_w \cdot f_u^2|,\; c \cdot k_f) \cdot 4/D$. When the diffusion boundary layer cannot supply mass as fast as the wall consumes it ($c \cdot k_f < |k_w|$), the reaction becomes mass-transfer-limited and concentration-dependent despite the nominally zero-order kinetics.

#### Combined Reaction in a Segment

The net concentration change in a pipe segment over a quality time step $\delta t$ is:

$$\Delta c = \left( r_{\text{bulk}} + r_{\text{wall}} \right) \delta t$$

This forward-Euler update is applied uniformly for all reaction orders. The quality time step is kept short relative to the reaction time scale to keep truncation error acceptably small.

#### Roughness–Reaction Correlation

As an alternative to specifying a wall reaction coefficient $k_w$ for each pipe individually, EPANET supports a global **roughness–reaction correlation factor** $R_f$. When $R_f \neq 0$, the wall coefficient for each pipe is derived automatically from its roughness parameter. The correlation formula depends on the head-loss formula in use:

- **Hazen-Williams** ($C$ is the HW roughness coefficient, smoother pipes have higher $C$):
$$k_w = \frac{R_f}{C}$$

- **Darcy-Weisbach** ($\varepsilon$ is the absolute roughness, $D$ the diameter):
$$k_w = \frac{R_f}{|\ln(\varepsilon/D)|}$$

- **Chezy-Manning** ($n_M$ is Manning's roughness, rougher pipes have higher $n_M$):
$$k_w = R_f \cdot n_M$$

In all three cases, $R_f$ has units of $[k_w \cdot \text{roughness parameter}]$, chosen so that the resulting $k_w$ is in the wall-rate units expected by the wall reaction formulas (velocity, m/s or ft/s). The physical motivation is that rougher pipe surfaces tend to harbour more biofilm or corrosion products and hence exhibit higher wall demand. Any pipe whose $k_w$ is set explicitly in the input takes precedence over the correlation.

---

## 9. Tank Mixing Models

Tanks use one of four models to govern how incoming water mixes with water already in storage. The choice of mixing model can significantly affect the predicted concentration of dissolved constituents leaving the tank.

### Complete Mix (CSTR)

The tank is modelled as a **Continuously Stirred Tank Reactor (CSTR)**. All water entering the tank is assumed to mix instantly and uniformly with the existing contents. The concentration at any instant is therefore uniform throughout the tank volume. The mixing update at each quality sub-step is:

$$c_{\text{new}} = \frac{c \cdot V + c_{\text{in}} \cdot V_{\text{in}}}{V + V_{\text{in}}}$$

where $V$ is the current stored volume, $c$ is the (uniform) tank concentration, $c_{\text{in}}$ is the volume-weighted inflow concentration, and $V_{\text{in}}$ is the inflow volume during the sub-step. Bulk reactions are applied separately (not during the mixing step) in the same phase as pipe reactions.

### Two-Compartment Mix

The tank volume is divided into two compartments represented as two segments: an **inlet mixing zone** with a maximum capacity of $V_{\text{mz}} = f \cdot V_{\max}$ (where $f$ is a user-specified fraction, typically 0.1–0.3, and $V_{\max}$ is the *maximum* tank volume) and a **stagnant zone** comprising the remaining capacity $V_{\text{sz}} = V_{\max} - V_{\text{mz}}$. All inflow enters the mixing zone; all outflow also exits from the mixing zone. Transfers between zones are **directional and discrete**, not bidirectional or continuous:

- **Filling** ($v_{\text{net}} > 0$): inflow mass mixes into the mixing zone (weighted average). If the mixing zone volume would exceed $V_{\text{mz}}$, the excess volume $v_t = \max(0,\; V_{\text{mix}} + v_{\text{net}} - V_{\text{mz}})$ is transferred from the mixing zone to the stagnant zone, carrying the mixing zone's post-mix concentration. The stagnant zone concentration is updated as a volume-weighted average of its current contents and the transferred mass. If the stagnant zone's volume would exceed $V_{\text{sz}}$, the surplus exits as overflow (counted as outflow in the mass balance) and the stagnant zone is clamped to $V_{\text{sz}}$.

- **Emptying** ($v_{\text{net}} < 0$): water is drawn back from the stagnant zone into the mixing zone to compensate for the net deficit: $v_t = \min(V_{\text{stag}},\; |v_{\text{net}}|)$. The mixing zone concentration is updated as a volume-weighted average of its current contents, the inflow mass, and the transferred stagnant zone water.

- **No net flow** ($v_{\text{net}} = 0$): no volume transfer occurs; inflow mass still mixes into the mixing zone.

The outflow concentration is always the mixing zone concentration. This model captures the behaviour of elongated tanks where short-circuiting occurs — inflow water can exit before it fully mixes with the bulk stored water.

### FIFO Plug Flow

The tank is treated as a perfectly ordered pipe with no axial mixing. Water enters from one end and exits from the other in strict **first-in, first-out** order. The segment representation used for pipes (§8.2) is applied directly to the tank. New inflow creates a new segment at the inlet end; outflow consumes segments from the outlet end. Reactions occur within each segment. This model is appropriate for narrow, tall standpipes or tanks with well-separated inlet and outlet ports.

### LIFO (Stacked Layers)

Water enters and exits from the **same end** of the tank, as in a stratified system. New inflow creates a new segment at the top (or inlet side); outflow removes segments from the same end in **last-in, first-out** order. This model approximates thermal stratification in tanks where buoyancy prevents vertical mixing, so that recently added water leaves first.

---

## 10. Mass Balance

The simulator maintains a **running mass balance** for the quality constituent throughout the simulation. At each quality time step, the following quantities are accumulated:

- **Initial mass stored**: the total constituent mass in all pipes and tanks at the start of the simulation.
- **Mass added from sources**: constituent injected at network sources.
- **Mass removed as demand**: constituent carried out by consumer withdrawals.
- **Mass reacted**: constituent lost (or gained) through bulk and wall reactions; computed as the integral of reaction rates over all pipe segments and tanks.
- **Final mass stored**: the total mass remaining in the network at the end of the simulation.

The overall mass balance ratio is computed as follows. Let $m_\text{reacted}$ be the signed total mass change due to reactions (negative for growth, positive for decay). If $m_\text{reacted} > 0$ (net decay), it is added to the output side of the ledger. If $m_\text{reacted} < 0$ (net growth), its absolute value is added to the input side:

$$\text{ratio} = \frac{\text{mass demand outflow} + \max(m_\text{reacted}, 0) + \text{final mass stored}}{\text{initial mass stored} + \text{mass added by sources} + \max(-m_\text{reacted}, 0)}$$

A value close to 1.0 confirms that constituent mass is being conserved to within numerical precision. A significant deviation from 1.0 indicates either a numerical error or an inconsistency in the reaction parameterisation. This diagnostic is reported at the end of the simulation.

---

## 11. Energy Tracking

For each pump in the network, the hydraulic power consumed during each time step is:

$$P_{\text{hydraulic}} = \rho g Q \, \Delta H$$

where $Q$ is the flow through the pump and $\Delta H$ is the head added. The actual power drawn from the electrical supply is:

$$P_{\text{electrical}} = \frac{\rho g Q \, \Delta H}{\eta}$$

where $\eta$ is the pump efficiency. If an efficiency curve (efficiency versus flow) is provided, $\eta$ is read from the curve at the current operating point; otherwise a default efficiency is assumed. When a pump operates at a speed setting $\omega \neq 1.0$ and an efficiency curve is supplied, the efficiency is further adjusted using the **Sarbu-Borza** speed-correction formula:

$$\eta_{\omega} = 100 - \frac{100 - \eta_1}{\omega^{0.1}}$$

where $\eta_1$ is the efficiency read from the curve at the speed-adjusted operating point and $\eta_{\omega}$ is the corrected efficiency. This empirical formula accounts for the fact that pump efficiency typically improves at lower speeds.

The following energy statistics are accumulated over the simulation period for each pump:

- **Kilowatt-hours consumed**: the time integral of $P_{\text{electrical}}$ over the simulation.
- **Time-weighted average efficiency**: the average of $\eta$ weighted by the fraction of time spent at each operating point.
- **Maximum demand**: the peak value of $P_{\text{electrical}}$ observed at any time step.
- **Cost**: the product of energy consumed and a unit energy price, which may itself vary over time via a cost pattern.

These energy diagnostics are essential for assessing the operating cost of different pumping schedules or for optimising pump dispatch.

**Energy cost model**: the unit energy cost $c$ (cost per kWh) used to accumulate `TotalCost` is determined at each time step as follows. A global base cost $c_0$ is multiplied by the current value of a global energy price pattern (if one is assigned), giving a time-varying rate $c_0 f(t)$. Each pump may also carry its own cost override $c_p$ and/or its own cost pattern, which are resolved **independently** of the global values: if a pump's `Ecost` is positive it replaces $c_0$; otherwise the global $c_0$ is used. If a pump's `Epat` is assigned it replaces the global pattern multiplier; otherwise the global pattern multiplier is applied even when the pump has its own cost override. The energy cost accumulated for pump $j$ over time step $\Delta t$ (hours) is therefore:

$$\text{Cost}_j \mathrel{+}= c_j(t) \cdot P_j \cdot \Delta t$$

where $c_j(t)$ is the applicable unit rate at the current time.

In addition to energy cost, a global **peak demand charge** parameter $D_c$ (cost per peak kW) is supported. A running maximum of the simultaneous power draw across all pumps $P_{\text{max}} = \max_t \sum_j P_j(t)$ is tracked throughout the simulation. At report time the total peak demand cost $D_c \cdot P_{\text{max}}$ is added to the energy cost summary. The **KwHrsPerFlow** statistic is accumulated at each time step as $\sum_i (P_i / Q_i) \cdot \Delta t_i$ — a time-weighted harmonic-mean energy intensity — and reported as a measure of pumping efficiency per unit throughput.

---

## 12. Flow Balance

In addition to the local hydraulic solution at each time step, the simulation accumulates a **global volumetric flow balance** over the entire simulation period. Each quantity is computed by time-integrating the corresponding flow rate:

- **Total inflow**: water entering the network from reservoirs (fixed-grade sources).
- **Consumer demand delivered**: water withdrawn at junctions as consumer demand.
- **Emitter outflow**: water discharged through emitters.
- **Leakage outflow**: water lost through pipe leakage modelled by the FAVAD equations.
- **Demand deficit**: the volume of consumer demand that was not delivered owing to insufficient pressure (relevant only in PDA mode; zero in DDA mode).
- **Storage change**: the net change in volume stored in all tanks over the simulation period (positive if tanks filled overall, negative if they drained).

The **flow balance ratio** is defined as:

$$\text{balance ratio} = \frac{q_\text{out}}{q_\text{in}}$$

where the ledger is built from the time-averaged flows as follows. When the net tank storage flow $q_\text{stor}$ is positive (tanks filling on average), it is added to the output side: $q_\text{out} = \text{total outflow} + q_\text{stor}$. When $q_\text{stor}$ is negative (tanks draining on average), its absolute value is added to the input side: $q_\text{in} = \text{total inflow} + |q_\text{stor}|$. Here `total outflow` includes consumer demand, emitter flows, and leakage (all three are folded into `totalOutflow`); `total inflow` is the supply from reservoirs plus any junction node with net negative demand. The demand deficit (undelivered demand in PDA mode) is tracked as a separate component and reported alongside the ratio but is not incorporated into the ratio calculation itself. A ratio close to 1.0 indicates that the simulation is globally mass-conserving. The instantaneous **leakage fraction** — leakage as a percentage of total supply at any given time step — is also tracked and can be reported at each reporting period to identify periods of high loss.

---

## 13. Units and Physical Constants

All hydraulic computations are performed internally in a fixed set of US customary units regardless of what units the user specifies in the input file: lengths in feet (ft), diameters in feet, flows in ft³/s (cfs), heads in feet, and power in horsepower (hp). Unit conversion factors are applied once during input parsing to translate user-supplied values into internal units, and again at output time to translate results back into user-facing units.

**Unit system selection**: the unit system (US or SI) is inferred automatically from the chosen flow unit:

| Flow units | System | Pressure default |
|------------|--------|------------------|
| CFS, GPM, MGD, IMGD, AFD | US | psi |
| LPS, LPM, MLD, CMH, CMD, CMS | SI | metres |

Pressure units may be overridden independently to psi, kPa, metres, bar, or feet, regardless of the primary unit system.

**Physical constants**: the default values used in the absence of user overrides are:

| Constant | Default | Notes |
|----------|---------|-------|
| Kinematic viscosity $\nu$ | $1.1 \times 10^{-5}$ ft²/s | Water at 20 °C |
| Molecular diffusivity $\mathcal{D}$ | $1.3 \times 10^{-8}$ ft²/s | Chlorine at 20 °C |
| Specific gravity | 1.0 | Water |

For **kinematic viscosity**, the user may supply either a multiplier (value $> 10^{-3}$, interpreted as a scale factor on the default) or an actual value in ft²/s. For **molecular diffusivity**, the multiplier threshold is $10^{-4}$ rather than $10^{-3}$: values greater than $10^{-4}$ are treated as scale factors on the default diffusivity, while smaller values are taken as the actual diffusivity. When the SI unit system is active, supplied actual values for both quantities are converted from m²/s to ft²/s before storage.

**Hydraulic solver defaults**: the default values for convergence parameters that apply in the absence of user specification are summarised below.

| Parameter | Default | Meaning |
|-----------|---------|--------|
| MaxIter | 200 | Maximum Newton-Raphson iterations |
| Hacc | 0.001 | Flow accuracy tolerance $\epsilon_{\text{tol}}$ |
| Htol | 0.0005 ft | Head tolerance for status checks |
| Qtol | 0.0001 cfs | Flow tolerance for status checks |
| CheckFreq | 2 | Status check start interval (iterations) |
| MaxCheck | 10 | Maximum iterations with status checks |
| DampLimit | 0 | Flow error at which damping activates (0 = always) |
| ExtraIter | −1 | Halt on non-convergence (0 = no extra; >0 = extra frozen trials) |

---

## 14. Input and Output

### Input

The network is described in a structured plain-text input file organised into labelled sections. Each section corresponds to a class of network object or a simulation parameter group. The parser makes **two passes** through the file. A first pass does two things: it counts all objects of each type so that memory can be allocated in one contiguous block, and it also extracts the `UNITS` and `HEADLOSS` options from the `[OPTIONS]` section. These two options must be known before the first pass completes because they determine how patterns, curves, and other objects are sized and interpreted. A second pass reads and interprets all remaining data. The sections handled include: junctions, reservoirs, tanks, pipes, pumps, valves, demand categories, time patterns, head-loss and efficiency curves, simple controls, rule-based controls, water quality sources, emitter coefficients, leakage parameters, options (head-loss formula selection, flow units, demand model, tolerances), energy pricing, reaction coefficients, tank mixing model assignments, reporting options, initial status overrides, and simulation time parameters.

**Initial link status overrides (`[STATUS]` section)**: the `[STATUS]` section allows the user to set the initial operational status of any link before simulation begins. For pipes and pumps, valid entries are `OPEN` or `CLOSED`. For valves, an entry may be `OPEN`, `CLOSED`, or a numeric setting that overrides the value from the `[VALVES]` section. Status overrides are applied after all link definitions have been parsed, so they take precedence over inline status values. During simulation, controls and rules may subsequently change these statuses.

Before simulation begins, the project undergoes a **validation pass** that checks for the following conditions: each tank must satisfy $H_{\min} \leq H_{\text{init}} \leq H_{\max}$; all patterns must have at least one period; all curves must have strictly increasing $x$-values (monotone); pump curves must have strictly decreasing head values; and the number of curve data points must meet a minimum for interpolation to work. Any unconnected node (a junction or tank with no adjacent links) is detected and reported as an error. If any validation check fails, the simulation does not start and the error is reported.

Alternatively, networks may be constructed entirely through the project API without reference to an input file, by calling the object-creation and property-setting operations in programmatic sequence.

### Output

**Hydraulic binary file**: at each hydraulic time step the solver writes all nodal heads, nodal demands, link flows, link velocities, and link status flags to a temporary binary file. This file is then replayed during the water quality simulation, supplying the velocity field needed for advection without requiring the hydraulics to be recomputed.

**Results binary file**: at each reporting time step (which may be less frequent than the hydraulic time step), all computed quantities for every node and link are saved to a separate binary output file. Node quantities include hydraulic head, pressure, demand, and constituent concentration. Link quantities include flow rate, velocity, unit head loss, friction factor, and quality. This file may subsequently be post-processed by external programs.

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
| 40 | INT4 | Pressure units: 0=PSI, 1=kPa, 2=metres |
| 44 | INT4 | Report statistic: 0=Series, 1=Average, 2=Minimum, 3=Maximum, 4=Range |
| 48 | INT4 | Report start time (seconds) |
| 52 | INT4 | Report time step (seconds) |
| 56 | INT4 | Simulation duration (seconds) |
| 60 | char[80] × 3 | Three title lines (240 bytes) |
| 300 | char[260] × 2 | Input filename, report filename (520 bytes) |
| 820 | char[32] × 2 | Chemical name, chemical units (64 bytes) |
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
| 8 | Friction factor | Darcy-Weisbach $f$; dimensionless; 0 for non-pipes or negligible flow |

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
4. Run the hydraulic simulation either in full (computing all time steps internally) or step-by-step; in step-by-step mode the caller advances the clock one hydraulic time step at a time and may modify network properties between steps.
5. Run the water quality simulation, either in full or step-by-step, in a similar fashion.
6. Retrieve any computed result (pressure, flow, concentration, energy, etc.) at any time step.
7. Set any network property (demand, pipe roughness, pump speed, valve setting, control threshold, reaction coefficient, etc.) and re-run as desired.
8. Delete the project to release all resources.

Multiple project instances may coexist in the same process, enabling Monte Carlo analysis, parallel scenario evaluation, or re-entrant simulation from multiple threads (provided each thread operates on a distinct project handle and any shared file system resources are managed appropriately).