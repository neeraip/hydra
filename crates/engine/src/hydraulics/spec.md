# hydra-engine — Hydraulics Sub-Specification

## 1. Overview

This document is the hydraulics sub-specification for `hydra-engine`. It defines the hydraulic engine algorithm used by Hydra's extended-period simulation.

The network data model consumed by this subsystem is defined in [model spec](../model/spec.md). The orchestration contract that invokes this engine is defined in [simulation spec](../simulation/spec.md).

---

## 3. Hydraulic Engine

The hydraulic engine produces, for each hydraulic time step, the **head at every junction** and the **flow through every link** that jointly satisfy conservation of mass at all nodes and the head-loss relationship across every link. It does so by iterating a Newton-Raphson procedure — the **Global Gradient Algorithm (GGA)** — until convergence.

### 3.1 Governing Equations

Let $n_j$ be the number of junctions (unknown heads) and $n_l$ the number of links (unknown flows).

**Node flow balance** (one equation per junction $i$):

$$\sum_{k : \text{to}(k)=i} Q_k \;-\; \sum_{k : \text{from}(k)=i} Q_k \;=\; D_i$$

where $Q_k$ is the signed flow in link $k$ (positive in the `from→to` direction) and $D_i$ is the total nodal outflow (demand + emitter + leakage).

**Link head-loss equation** (one equation per link $k$ connecting nodes $i$ and $j = \text{to}(k)$):

$$H_{\text{from}(k)} - H_{\text{to}(k)} = h_k(Q_k)$$

where $h_k$ is the signed head-loss function for link $k$ (negative for a pump — a head gain). For fixed-grade nodes the head is known and contributes a boundary term to the adjacent junction equations rather than appearing as an unknown.

The $n_j + n_l$ equations in $n_j + n_l$ unknowns $(\mathbf{H}, \mathbf{Q})$ are nonlinear. The GGA reduces them to a system of size $n_j$ in $\mathbf{H}$ alone by analytically expressing $\mathbf{Q}$ as a linear function of $\mathbf{H}$ at each iteration.

### 3.2 Head-Loss Functions

One formula is selected globally and applies to all pipes.

> **Note on unit-system-dependent constants**: the numeric constants in §3.2.1 and §3.2.3 are unit-system-specific. Since Hydra uses SI internally (see `../model/spec.md` §3), implementations must use the SI column values from the tables below.

#### 3.2.1 Hazen-Williams

$$h_f = R_{\text{HW}} \cdot Q^{1.852} \cdot \operatorname{sign}(Q), \qquad R_{\text{HW}} = \frac{\alpha_{\text{HW}}\, L}{C^{1.852}\, D^{4.871}}$$

where $\alpha_{\text{HW}}$ takes the following values depending on the unit system:

| Unit system | $L$, $D$ | $Q$ | $h_f$ | $\alpha_{\text{HW}}$ |
|---|---|---|---|---|
| US customary | ft | ft³/s | ft | 4.727 |
| SI | m | m³/s | m | 10.67 |

**Example (US customary):** a 1000 ft pipe, 12 in diameter ($D = 1$ ft), $C = 100$, carrying $Q = 1$ ft³/s:

$$R_{\text{HW}} = \frac{4.727 \times 1000}{100^{1.852} \times 1^{4.871}} \approx \frac{4727}{3981} \approx 1.187 \;\text{ft/(ft}^3\text{/s)}^{1.852}$$

$$h_f = 1.187 \times 1^{1.852} \approx 1.19 \;\text{ft}$$

**Example (SI):** same pipe expressed in SI ($L = 304.8$ m, $D = 0.3048$ m, $Q = 0.02832$ m³/s):

$$R_{\text{HW}} = \frac{10.67 \times 304.8}{100^{1.852} \times 0.3048^{4.871}} \approx \frac{3252}{3981 \times 0.00802} \approx 102 \;\text{m/(m}^3\text{/s)}^{1.852}$$

$$h_f = 102 \times 0.02832^{1.852} \approx 0.36 \;\text{m} \;\;(\approx 1.19 \;\text{ft} \checkmark)$$

The identical physical head loss is produced by both formulations; only $\alpha_{\text{HW}}$ changes.

Flow exponent $n = 1.852$.

#### 3.2.2 Darcy-Weisbach

$$h_f = f(Q) \cdot \frac{8L}{\pi^2 g D^5} \cdot Q\,|Q|$$

The friction factor $f$ and its derivative $\partial f / \partial Q$ are computed simultaneously:

- **Laminar** ($Re \leq 2000$, $Re = 4Q/(\pi D \nu)$): $f = 64/Re$, giving $h_f = R_{\text{HP}} \cdot Q$ where $R_{\text{HP}} = 128\nu L / (\pi g D^4)$.
- **Turbulent** ($Re \geq 4000$): Swamee-Jain approximation to Colebrook-White:

$$f = \left[-2\log\!\left(\frac{\varepsilon}{3.7D} + \frac{5.74}{Re^{0.9}}\right)\right]^{-2}$$

- **Transitional** ($2000 < Re < 4000$): Dunlop cubic polynomial that interpolates $f$ and $\partial f / \partial Q$ continuously at both boundaries. Let $r = Re/2000$ (so $r \in (1, 2)$) and define the following roughness-dependent anchor values at $Re = 4000$ (using $w = Q/s = Re \cdot \pi/4$, $A_B = 5.74/4000^{0.9}$, $A_9 = -2/\ln 10$, $A_C = -1.8 \cdot A_9 \cdot A_B$):

$$y_2 = \frac{\varepsilon}{3.7D} + A_B, \qquad y_3 = A_9 \ln y_2$$
$$f_a = \frac{1}{y_3^2} \quad (\text{friction factor at }Re=4000), \qquad f_b = \left(2 + \frac{A_C}{y_2\,y_3}\right)f_a \quad (\text{normalised gradient at }Re=4000)$$

Cubic polynomial coefficients:
$$x_1 = 7f_a - f_b, \quad x_2 = 0.128 - 17f_a + 2.5f_b, \quad x_3 = -0.128 + 13f_a - 2f_b, \quad x_4 = 0.032 - 3f_a + 0.5f_b$$

$$f = x_1 + r\bigl(x_2 + r(x_3 + r\,x_4)\bigr), \qquad \frac{df}{dQ} = \frac{x_2 + r(2x_3 + 3r\,x_4)}{s \cdot 500\pi}$$

Numerically, $A_B \approx 3.289 \times 10^{-3}$, $A_9 \approx -0.8686$, $A_C \approx -5.142 \times 10^{-3}$. These constants encode the turbulent anchor point and ensure $f$ is $C^1$-continuous across both regime boundaries.

#### 3.2.3 Chezy-Manning

$$h_f = R_{\text{CM}} \cdot Q^2 \cdot \operatorname{sign}(Q), \qquad R_{\text{CM}} = \frac{n_M^2\, L}{k_M^2 \cdot (D/4)^{4/3} \cdot (\pi D^2/4)^2}$$

where $k_M$ takes the following values depending on the unit system:

| Unit system | $L$, $D$ | $Q$ | $h_f$ | $k_M$ |
|---|---|---|---|---|
| SI | m | m³/s | m | 1.0 |
| US customary | ft | ft³/s | ft | 1.486 |

The factor $k_M$ arises from the empirical unit conversion embedded in Manning's velocity formula ($V = (k_M/n_M) R_h^{2/3} S^{1/2}$), which was originally formulated in SI.

**Example (SI):** $L = 100$ m, $D = 0.5$ m, $n_M = 0.013$, $Q = 0.25$ m³/s:

$$R_{\text{CM}} = \frac{0.013^2 \times 100}{1.0^2 \times (0.125)^{4/3} \times (0.1963)^2} \approx \frac{0.169}{1.0 \times 0.06300 \times 0.03854} \approx 69.7 \;\text{m/(m}^3\text{/s)}^2$$

$$h_f = 69.7 \times 0.25^2 \approx 4.36 \;\text{m}$$

**Example (US customary):** same pipe ($L = 328.1$ ft, $D = 1.640$ ft, $Q = 8.829$ ft³/s):

$$R_{\text{CM}} = \frac{0.013^2 \times 328.1}{1.486^2 \times (0.4101)^{4/3} \times (2.111)^2} \approx \frac{0.5545}{2.208 \times 0.3239 \times 4.456} \approx 0.174 \;\text{ft/(ft}^3\text{/s)}^2$$

$$h_f = 0.174 \times 8.829^2 \approx 13.56 \;\text{ft} \;\;(\approx 4.13 \;\text{m} \approx 4.36 \;\text{m} \checkmark)$$

Flow exponent $n = 2$.

#### 3.2.4 Minor Losses

For all formula choices, minor losses are added to the friction head loss:

$$h_k = h_f + K_m \cdot Q_k\,|Q_k|$$

This additive form preserves the odd-function property — head loss opposes flow regardless of direction.

**TCV (throttle control valve)** — the setting $s$ is a dimensionless loss coefficient specifying the partial-open resistance. The effective minor-loss coefficient used in the formula above is:

$$K_{m,\text{eff}} = \frac{0.08262 \cdot s}{D^4} \quad (\text{SI: }Q \text{ in m}^3/\text{s}, D \text{ in m})$$

where the constant $0.08262 = 8/(\pi^2 g)$ with $g = 9.81$ m/s². The resulting $K_{m,\text{eff}}$ replaces $K_m$ in the minor-loss term for that step.

**GPV (general-purpose valve)** — head loss is read directly from the `GPV_HEADLOSS` curve at the current absolute flow $|Q|$, using piecewise-linear interpolation. The intercept $h_0$ and slope $r$ of the bracketing curve segment give:

$$P_k = \frac{1}{r}, \qquad Y_k = \frac{h_0}{r} \cdot \text{sgn}(Q) + Q$$

(friction head loss $h_f = 0$; no minor-loss term). When the valve is status-`CLOSED` it is treated as a standard closed pipe.

**PCV (positional control valve)** — the setting $s \in [0, 100]$ is a percent-open position. The effective loss ratio $k_v$ is interpolated from the `PCV_LOSS_RATIO` curve (if assigned) or assumed linear ($k_v = s/100$). The curve's $y$-values are $K_v/K_{v,\text{full}}$ as percentages. Then:

$$k_v = \frac{K_v}{K_{v,\text{full}}} \in (0, 1], \qquad K_{m,\text{eff}} = \frac{K_m}{k_v^{\,2}}$$

At $s = 100$, $K_{m,\text{eff}} = K_m$ (fully open). At $s = 0$, the valve is fully closed ($K_{m,\text{eff}} \to \infty$, treated as `CLOSED`). $K_{m,\text{eff}}$ replaces $K_m$ in the minor-loss formula for that step. The `PCV_LOSS_RATIO` curve uses the general evaluation defined in `../model/spec.md` §2.3, including linear extrapolation beyond its endpoints.

#### 3.2.5 Pump Head Gain

For a pump operating at relative speed $\omega$:

- **Power-function**: $\Delta H = \omega^2 H_0 - r\,\omega^{2-N} Q^N$ (where $H_0$, $r$, $N$ are from the head curve; see `../model/spec.md` §2.6.3)
- **Constant-HP**: $\Delta H = P_{\text{rated}} / (\gamma Q)$ where $\gamma = \rho g$
- **Custom curve**: $\Delta H$ is evaluated as the intercept-plus-slope at the speed-adjusted flow $Q/\omega$, multiplied by $\omega^2$ (affinity law). The linearisation derivative for a custom curve pump is:

$$\frac{\partial \Delta H}{\partial Q} = \omega \cdot \left.\frac{\partial y}{\partial x}\right|_{x = Q/\omega}$$

where $\partial y / \partial x$ is the slope of the bracketing segment of the piecewise-linear head curve evaluated at the adjusted flow point $Q/\omega$.

In all cases the pump contributes a **negative** head-loss ($h_k = -\Delta H$) so that the head-loss framework is uniform.

**Three-point curve fitting**: when a pump head curve is defined by three points $(0, h_0)$, $(q_1, h_1)$, $(q_2, h_2)$ (and no explicit analytic form is provided), the power-function parameters are:

$$N = \frac{\ln\!\left(\dfrac{h_0 - h_2}{h_0 - h_1}\right)}{\ln\!\left(\dfrac{q_2}{q_1}\right)}, \qquad r = \frac{h_0 - h_1}{q_1^N}, \qquad H_0 = h_0$$

Required: $h_0 > h_1 > h_2 \geq 0$, $q_2 > q_1 > 0$, $N > 0$, and $N \leq 20$.

**Single-point curve expansion**: when a pump head curve contains only one point $(q_1, h_1)$, it is expanded to three points before fitting: $(0,\; 1.33334 \cdot h_1)$, $(q_1,\; h_1)$, $(2 q_1,\; 0)$.

### 3.3 Linearisation (P and Y Coefficients)

At Newton iteration $m$, the head-loss function of each link $k$ is linearised around the current estimate $Q_k^{(m)}$:

$$h_k(Q_k) \approx g_k \cdot Q_k - Y_k$$

where $g_k = \partial h_k / \partial Q_k \big|_{Q^{(m)}}$ is the **head-loss gradient**. Two per-link scalars are derived:

$$P_k = \frac{1}{g_k}, \qquad Y_k = P_k \cdot h_k\!\left(Q_k^{(m)}\right)$$

$P_k$ is the **hydraulic conductance** of the linearised link. $Y_k$ is the corresponding normalised head-loss offset.

**Guard conditions** (prevent division by zero and ill-conditioning):

Let $g_{\min}$ be a small positive threshold (units: [head]/[flow] = s/m² in SI) below which the linearised gradient is considered degenerate. The value is $10^{-6}$ s/m², used unchanged regardless of the runtime unit system. This matches EPANET's `CSMALL` constant and serves as a purely numerical guard against division by zero — not a physically meaningful threshold.

- If $g_k < g_{\min}$: set $g_k = g_{\min}/n_f$ and $h_k = g_k \cdot Q_k$ (linear regime), where $n_f$ is the flow exponent of the active head-loss formula (1.852 for Hazen-Williams, 2 for Darcy-Weisbach and Chezy-Manning).
- Closed link: $P_k = 1/C_{\infty}$, $Y_k = Q_k$ (frozen at current flow with near-zero conductance).
- Active PRV/PSV/FCV: $P_k = 0$ (excluded from standard assembly; handled separately in §3.5).

**PBV special case**: A Pressure Breaker Valve with setting $h_s > 0$ forces a fixed head loss. If the minor-loss head drop at the current flow already exceeds the setting ($K_m Q_k^2 > h_s$), the PBV falls back to ordinary pipe/minor-loss treatment. Otherwise:

$$P_k = C_{\infty}, \qquad Y_k = h_s \cdot C_{\infty}$$

resulting in $H_{\text{from}} - H_{\text{to}} \to Y_k / P_k = h_s$ with very high stiffness. If the setting is absent or zero, the PBV is treated as an ordinary pipe.

∥ **Parallelism**: $P_k$ and $Y_k$ for all links are mutually independent and may be computed concurrently.

#### 3.3.1 Emitter Coefficients

For each junction $i$ with `emitter_coeff` $K_e > 0$, the emitter is treated as a fictitious pipe to a fictitious reservoir at elevation $z_i$. Let $\hat{n} = 1/n_e$ where $n_e$ = `emitter_exp` (default 0.5), so the default $\hat{n} = 2$. The head loss through the emitter is:

$$h_e = K_e \cdot |Q_e|^{\hat{n}}$$

where $Q_e$ is the current emitter flow. The linearised gradient:

$$g_e = \hat{n} \cdot K_e \cdot |Q_e|^{\hat{n} - 1}$$

**Guard**: if $g_e < g_{\min}$ (same threshold as the link guard in §3.3; $10^{-6}$ s/m²): set $g_e = g_{\min} / \hat{n}$, $h_e = g_e \cdot Q_e$ (linear regime). Otherwise: $h_e = g_e \cdot Q_e / \hat{n}$ (consistent with the power-law).

**Backflow barrier**: if `emitter_backflow = false`, apply the lower barrier (§3.3.4) to $(Q_e, h_e, g_e)$ to enforce $Q_e \geq 0$.

The resulting $P_e = 1/g_e$, $Y_e = h_e/g_e$ are added to the assembly per §3.4.

#### 3.3.2 PDA Demand Coefficients

Active only when `demand_model = PDA`. For each junction $i$ with `full_demand` $D_{\text{full}} > 0$, model the demanded flow as a fictitious emitter with exponent $n_d = 1/n_P$ ($n_P$ = `pda_pressure_exponent`). Let $P_{\min}$ = `pda_min_pressure` and $P_{\text{req}}$ = `pda_required_pressure` (both from `../model/spec.md` §2.1)

> **Note on $D_{\text{full}}$**: $D_{\text{full},i}$ is the pattern-scaled base demand for junction $i$ at the current time step — exactly the value that would be treated as a fixed constant in DDA mode. In PDA mode it becomes the upper bound on achievable demand rather than a fixed load.

The inverted demand function gives a head vs. flow relationship:

$$h_d = (P_{\text{req}} - P_{\min}) \cdot \left(\frac{D_i}{D_{\text{full}}}\right)^{1/n_P}$$

Linearised gradient:

$$g_d = \frac{n_d \cdot (P_{\text{req}} - P_{\min})}{D_{\text{full}}} \cdot \left(\frac{|D_i|}{D_{\text{full}}}\right)^{n_d - 1}, \qquad n_d = \frac{1}{n_P}$$

$h_d = g_d \cdot D_i / n_d$. Two barriers are applied:

- **Lower barrier** at $D_i = 0$: enforces $D_i \geq 0$ (§3.3.4 with $\delta q = D_i$).
- **Upper barrier** at $D_i = D_{\text{full}}$: enforces $D_i \leq D_{\text{full}}$ (§3.3.4 with $\delta q = D_i - D_{\text{full}}$).

If $g_d \leq 0$ after barrier application, the junction's demand contribution is skipped (gradient is not added to $\mathbf{A}$).

#### 3.3.3 FAVAD Leakage Coefficients

For each junction $i$ with nonzero derived leakage resistance $c_{\text{fa},i}$ and/or $c_{\text{va},i}$ (aggregated at load time from incident pipe FAVAD coefficients per `../model/spec.md` §2.10), the leakage is modelled via the **inverted power-law** (head as function of flow): 

$$h_{\text{fa}} = c_{\text{fa}} \cdot q_{\text{fa}}^{\,2} \quad (\text{fixed area: exponent } 1/n = 2), \qquad h_{\text{va}} = c_{\text{va}} \cdot q_{\text{va}}^{\,2/3} \quad (\text{variable area: exponent } 1/n = 2/3)$$

This is the **inverted** form of $Q = (h/c)^n$ (i.e., $Q_{\text{fa}} = \sqrt{h/c_{\text{fa}}}$, $Q_{\text{va}} = (h/c_{\text{va}})^{1.5}$). Using the general form $h = c \cdot q^{1/n}$:

$$g = \frac{1}{n} \cdot c \cdot |q|^{1/n - 1}, \qquad h = g \cdot q \cdot n$$

- Fixed area: $1/n = 2$, so $g_{\text{fa}} = 2 c_{\text{fa}} |q_{\text{fa}}|$.
- Variable area: $1/n = 2/3$, so $g_{\text{va}} = \tfrac{2}{3} c_{\text{va}} |q_{\text{va}}|^{-1/3}$.

**Lower barrier** (enforces $q \geq 0$): applied after gradient computation (§3.3.4). Leakage is always outflow; the barrier prevents the Newton iteration from driving it negative.

Resulting $P_{\text{fa}} = 1/g_{\text{fa}}$, $Y_{\text{fa}} = h_{\text{fa}}/g_{\text{fa}}$ (and similarly for the variable-area component) are added to the assembly per §3.4.

#### 3.3.4 Barrier Functions

Barrier functions augment the head-loss and gradient of a nonlinear element to approximately enforce a flow bound. They are smooth and differentiable everywhere, ensuring the Newton system remains non-singular.

**Lower barrier** (enforce $q \geq q_0$; applied with $\delta q = q - q_0$):

$$a = 10^9 \cdot \delta q, \quad b = \sqrt{a^2 + 10^{-6}}$$
$$\Delta h = \frac{a - b}{2}, \qquad \Delta g = \frac{10^9}{2}\left(1 - \frac{a}{b}\right)$$

**Upper barrier** (enforce $q \leq q_1$; applied with $\delta q = q - q_1$):

$$\Delta h = \frac{a + b}{2}, \qquad \Delta g = \frac{10^9}{2}\left(1 + \frac{a}{b}\right)$$

$h \mathrel{+}= \Delta h$, $g \mathrel{+}= \Delta g$. When $\delta q \gg 0$, $\Delta h \approx 0$ (feasible region, no correction); when $\delta q \ll 0$, $\Delta h \to -\infty$ with very high stiffness (violation pushed back strongly).

### 3.4 Linear System Assembly

Substituting the linearised link equations into the node flow-balance equations yields:

$$\mathbf{A}\,\mathbf{H} = \mathbf{F}$$

$\mathbf{A}$ is an $n_j \times n_j$ symmetric positive semi-definite matrix with the structure of a **weighted graph Laplacian**:

$$A_{ii} = \sum_{k \ni i} P_k \qquad (\text{sum over all non-zero-}P \text{ links incident to junction } i)$$

$$A_{ij} = -P_k \qquad (k \text{ is the unique link connecting junctions } i \text{ and } j)$$

The right-hand side at junction $i$:

$$F_i = \underbrace{\sum_{k:\text{to}(k)=i} Y_k - \sum_{k:\text{from}(k)=i} Y_k}_{\text{link } Y\text{-terms}} + \underbrace{\sum_{\text{fixed-grade } n \in \mathcal{N}(i)} P_k H_n}_{\text{boundary heads}} + \underbrace{\Delta_i}_{\text{flow imbalance}}$$

where $\Delta_i = \text{(net flow into } i \text{ from all links)} - D_i$ is the current flow-balance residual at junction $i$, and the boundary-head sum accounts for any reservoirs or tanks directly connected to $i$ (their known heads are moved to the RHS).

**Additional diagonal contributions** (emitters, leakage, PDA demands — each adds its own linearised conductance $P_e$ to $A_{ii}$ and its $Y_e$ term to $F_i$):

| Source | $\Delta A_{ii}$ | $\Delta F_i$ |
|---|---|---|
| Emitter | $1/g_e$ | $(h_e + z_i)/g_e$ |
| FAVAD leak (fixed-area) | $1/g_{\text{fa}}$ | $(h_{\text{fa}} + z_i)/g_{\text{fa}}$ |
| FAVAD leak (variable-area) | $1/g_{\text{va}}$ | $(h_{\text{va}} + z_i)/g_{\text{va}}$ |
| PDA demand | $1/g_d$ | $(h_d + z_i + P_{\min})/g_d$ |

∥ **Parallelism**: diagonal and off-diagonal contributions from each link can be accumulated concurrently using atomic adds (or partitioned by node ownership).

> **Simultaneous mechanisms**: any combination of emitter ($K_e > 0$), FAVAD leakage ($c_{\text{fa},i}$ or $c_{\text{va},i} > 0$), and PDA demand (`demand_model = PDA`, $D_{\text{full},i} > 0$) may be active concurrently at the same junction. All active mechanisms contribute cumulatively to $A_{ii}$ and $F_i$; their conductances and offsets are summed independently.

> **DDA vs. PDA demand in $\Delta_i$**: In DDA mode the pattern-scaled base demand $D_{\text{base},i}$ enters the assembly as a fixed constant deducted from $\Delta_i$ (it is not a conductance contribution and has no entry in the table above). In PDA mode, $D_{\text{base},i}$ still defines $D_{\text{full},i}$ (the target demand upper bound) but is **not** separately deducted as a constant from $\Delta_i$; the PDA conductance row in the table above fully represents the demand contribution. The PDA demand flow $D_i^{(m)}$ (the current pressure-dependent achieved demand) replaces the fixed constant in the residual: $\Delta_i = \text{(net inflow from links + emitter + leakage)} - D_i^{(m)}$.

### 3.5 Active Valve Matrix Modifications

PRV, PSV, and FCV in the ACTIVE state are not included in the standard assembly (their $P_k = 0$). Instead they directly modify the assembled $\mathbf{A}$ and $\mathbf{F}$:

**PRV active** (pins downstream node $j$ to absolute head $H_s = z_j + s_k$):

$$A_{jj} \mathrel{+}= C_{\infty}, \qquad F_j \mathrel{+}= H_s \cdot C_{\infty}$$

$$Y_k = Q_k + \Delta_j \quad (\text{flow balance at downstream node})$$

Any negative flow excess at $j$ is redistributed to $F_i$ to maintain global mass balance.

**PSV active** (pins upstream node $i$ to absolute head $H_s = z_i + s_k$):

$$A_{ii} \mathrel{+}= C_{\infty}, \qquad F_i \mathrel{+}= H_s \cdot C_{\infty}$$

$$Y_k = Q_k - \Delta_i$$

A small residual $1/C_{\infty}$ is added to $A_{ij}$ and $A_{jj}$ to preserve matrix connectivity.

**FCV active** (imposes fixed flow $Q_s = s_k$):

$$F_i \mathrel{-}= Q_s, \qquad F_j \mathrel{+}= Q_s, \qquad P_k = 1/C_{\infty}$$

The two sides of the valve are nearly decoupled; the valve's flow appears as a prescribed external demand/supply pair.

$C_{\infty}$ is a large constant (implementation choice; must satisfy $C_{\infty} \gg \max P_k$ so that the pinned head dominates).

### 3.6 Sparse Linear Algebra

$\mathbf{A}$'s sparsity pattern is the **junction adjacency graph** — exactly one non-zero off-diagonal entry per link connecting two junctions. It is fixed for the entire simulation.

Three phases, each performed at a different frequency:

#### Phase 1 — Node Reordering (once, before simulation)

Apply the **Multiple Minimum Degree (MMD)** algorithm to the junction adjacency graph to find a permutation $\sigma$ that minimises fill-in during Cholesky factorisation. Parallel links (multiple links between the same junction pair) are condensed into a single equivalent entry before reordering.

Store the reordering permutation. All subsequent assembly and factorisation operate on the reordered system.

#### Phase 2 — Symbolic Factorisation (once, before simulation)

Using the reordered sparsity pattern, determine the exact set of positions $(i,j)$, $i \geq j$, that will be non-zero in the lower Cholesky factor $\mathbf{L}$ (where $\mathbf{A} = \mathbf{L}\mathbf{L}^\top$). Allocate and index these positions. From this point forward the structure of $\mathbf{L}$ never changes — only its numerical values.

#### Phase 3 — Numerical Factorisation and Solution (every Newton iteration)

1. Insert current $P_k$ values into the pre-allocated $\mathbf{A}$ arrays.
2. Compute the Cholesky factorisation $\mathbf{A} = \mathbf{L}\mathbf{L}^\top$ in the pre-allocated non-zero positions.
3. Solve $\mathbf{L}\mathbf{y} = \mathbf{F}$ by forward substitution.
4. Solve $\mathbf{L}^\top\mathbf{H} = \mathbf{y}$ by backward substitution.

If $\mathbf{A}$ is numerically singular during factorisation and the failing row corresponds to an active control valve node, fix that valve's status to `OPEN` (or `XFCV` for FCV) and restart the current iteration. Otherwise, report an unrecoverable solver error.

### 3.7 Flow Update

After solving for $\mathbf{H}^{(m+1)}$, update all flows using the **correction form**:

**Link flow**:
$$\delta q_k = Y_k - P_k \!\left(H_{\text{from}(k)}^{(m+1)} - H_{\text{to}(k)}^{(m+1)}\right)$$
$$Q_k^{(m+1)} = Q_k^{(m)} - \delta q_k$$

Equivalently: $Q_k^{(m+1)} = Q_k^{(m)} - Y_k + P_k\!\left(H_{\text{from}(k)}^{(m+1)} - H_{\text{to}(k)}^{(m+1)}\right)$.

(For fixed-grade endpoints the known head is used directly.)

**Emitter flow**: compute $(h_e, g_e)$ from the current emitter flow $Q_e^{(m)}$ as in §3.3.1, then:
$$\delta Q_e = \frac{h_e - (H_i^{(m+1)} - z_i)}{g_e}, \qquad Q_e^{(m+1)} = Q_e^{(m)} - \delta Q_e$$

**PDA demand flow**: $D_i^{(m+1)} = P_d \!\left(H_i^{(m+1)} - z_i - P_{\min}\right) + Y_d$ (clamped to $[0, D_{\text{full}}]$ by barrier functions).

**Leakage flows**: for each component (fixed-area, variable-area) independently:
$$q_{\text{fa}}^{(m+1)} = P_{\text{fa}}\!\left(H_i^{(m+1)} - z_i\right) + Y_{\text{fa}}, \qquad q_{\text{va}}^{(m+1)} = P_{\text{va}}\!\left(H_i^{(m+1)} - z_i\right) + Y_{\text{va}}$$
where $P_{\text{fa/va}}$ and $Y_{\text{fa/va}}$ are the barrier-adjusted coefficients from §3.3.3. Clamp to $\geq 0$ after update.

∥ **Parallelism**: all link and nodal flow updates are mutually independent and may be computed concurrently.

### 3.8 Convergence Criteria

The iteration is considered **converged** when all of the following hold simultaneously:

1. **Relative flow accuracy**: let $S_Q = \sum_k |Q_k^{(m+1)}| + \sum_i Q_{e,i}^{(m+1)} + \sum_i D_i^{(m+1)} + \sum_i Q_{\text{leak},i}^{(m+1)}$ (sum of magnitudes over link flows, emitter flows, PDA demand flows, and leakage flows) and $\Delta S_Q$ the corresponding sum of absolute flow changes between iterations. Then:\n\n$$\varepsilon_Q = \begin{cases} \Delta S_Q / S_Q & S_Q > \text{\texttt{flow\_tol}} \\ \Delta S_Q & \text{otherwise (absolute criterion)} \end{cases}$$\n\nConvergence requires $\varepsilon_Q \leq$ `flow_tol`.

2. **Per-link head balance error** (checked only when `head_error_limit > 0`): for each open link $k$ with $P_k > 0$, the head balance residual is the discrepancy between the computed head difference and the linearised head loss:

$$\epsilon_{H,k} = \left|(H_{\text{from}(k)} - H_{\text{to}(k)}) - \frac{Y_k}{P_k}\right|$$

The condition is $\max_k \epsilon_{H,k} \leq$ `head_error_limit`. If `head_error_limit = 0` this criterion is skipped.

3. **Absolute flow change** (checked only when `flow_change_limit > 0`): $\max_k |Q_k^{(m+1)} - Q_k^{(m)}| \leq$ `flow_change_limit`. If `flow_change_limit = 0` this criterion is skipped.

4. **No link status change** during the most recent iteration (after a full status check; see §3.9).

**Leakage secondary convergence check**: after the main convergence criteria above are satisfied, the leakage solution is validated by directly evaluating the leakage at the converged heads. For each junction $i$ with active leakage:

$$q_{\text{ref},i} = \sqrt{\max(0,h_i) / c_{\text{fa},i}} + \max(0,h_i / c_{\text{va},i})^{3/2}$$

(terms for absent components are omitted). If $|q_{\text{ref},i} - (q_{\text{fa},i} + q_{\text{va},i})| > Q_{\text{leak-tol}}$ for any junction, the solution is not yet converged and the Newton loop continues. $Q_{\text{leak-tol}}$ is an absolute tolerance in m³/s; the value is $2.83 \times 10^{-6}$ m³/s (= $10^{-4}$ ft³/s, approximately 0.005 gpm or 0.2 lpm). This check is independent of the relative flow accuracy criterion (criterion 1) and must be satisfied simultaneously with the other criteria.

**Note**: `head_tol` is used as the absolute tolerance $\varepsilon_H$ in link status transition conditions (§3.9), not as a convergence criterion for the solver iteration. 

If convergence is not reached within `max_iter` iterations and `extra_iter > 0`, an additional `extra_iter` iterations are run with all status changes frozen. Results are valid but marked as **unbalanced**. If `extra_iter = −1`, simulation halts on non-convergence.

**Damping**: when `damp_limit > 0` and $\varepsilon_Q \leq \text{damp\_limit}$, a relaxation factor of 0.6 is applied to all flow updates and valve status checks are simultaneously activated.

**Post-convergence control re-evaluation (`pswitch`)**: when the solver reaches convergence (all four criteria above satisfied), a full status check is performed that includes not only `valvestatus` and `linkstatus` (§3.9) but also a re-evaluation of **simple controls** (`../simulation/spec.md` §4.1) whose trigger is a **junction** head (not a tank level or timer, since those do not change during the Newton iteration). If any simple control fires and changes a link's status or setting at this point, the Newton loop resumes from the current iteration count (the counter is **not** reset). Convergence is only accepted when a full status check — including simple controls — produces no changes. During extra iterations (`iter > max_iter`), convergence is accepted immediately without the `pswitch` check.

### 3.9 Link Status Logic

Status checks are triggered periodically (every `check_freq` iterations, up to `max_check`) and always after convergence is first reached.

#### Check Valve (CV pipe)

- OPEN → CLOSED: if $H_{\text{from}} - H_{\text{to}} < -\varepsilon_H$ or $Q < -\varepsilon_Q$.
- CLOSED → OPEN: if $H_{\text{from}} - H_{\text{to}} > \varepsilon_H$ and $Q \geq -\varepsilon_Q$.

#### Pump

- OPEN → XHEAD: if head gain required exceeds $\omega^2 H_0$ (speed-adjusted shutoff head).
- OPEN → TEMPCLOSED: constant-HP pump with $Q \leq 0$.
- XHEAD / TEMPCLOSED → OPEN: reset at the start of each periodic status check; re-tested immediately.

#### Tank Inlet/Outlet Pipe

For each link incident to a tank node, the link is set to `TEMPCLOSED` (a temporary closure that reverts to OPEN at the start of the next status check and is immediately re-evaluated) when:

- The tank head $\geq h_{\max}$ **and** `overflow = false` **and** the link is delivering flow **into** the tank.
- The tank head $\leq h_{\min}$ **and** the link is removing flow **from** the tank.

When `overflow = true`, a full tank does **not** close its inlet links — excess volume exits freely (§5.3). Empty-tank outlet closure applies regardless of the overflow flag. This check runs at every status check iteration, not only after convergence.

#### PRV Status (tested after every iteration when `damp_limit = 0`, otherwise only when $\varepsilon_Q \leq \text{damp\_limit}$)

Here $H_s = z_{\text{to}(k)} + s_k$ (the absolute downstream setpoint), $H_1 = H_{\text{from}(k)}$, and $H_2 = H_{\text{to}(k)}$.

| Current | Transition | Condition |
|---|---|---|
| ACTIVE | → OPEN | $H_1 - K_m Q^2 < H_s - \varepsilon_H$ (upstream pressure too low to need reduction) |
| ACTIVE | → CLOSED | $Q < -\varepsilon_Q$ (reverse flow) |
| OPEN | → ACTIVE | $H_2 \geq H_s + \varepsilon_H$ (downstream pressure at setpoint) |
| OPEN | → CLOSED | $Q < -\varepsilon_Q$ |
| CLOSED | → ACTIVE | $H_1 \geq H_s + \varepsilon_H$ and $H_2 < H_s - \varepsilon_H$ |
| CLOSED | → OPEN | $H_1 < H_s - \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$ |
| XPRESSURE | → CLOSED | $Q < -\varepsilon_Q$ |

#### PSV Status (tested on the same schedule as PRV)

| Current | Transition | Condition |
|---|---|---|
| ACTIVE | → OPEN | $H_2 + K_m Q^2 > H_s + \varepsilon_H$ (downstream head forces no reduction needed) |
| ACTIVE | → CLOSED | $Q < -\varepsilon_Q$ (reverse flow) |
| OPEN | → ACTIVE | $H_1 < H_s - \varepsilon_H$ (upstream head below setpoint) |
| OPEN | → CLOSED | $Q < -\varepsilon_Q$ |
| CLOSED | → OPEN | $H_2 > H_s + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$ |
| CLOSED | → ACTIVE | $H_1 \geq H_s + \varepsilon_H$ and $H_1 > H_2 + \varepsilon_H$ |
| XPRESSURE | → CLOSED | $Q < -\varepsilon_Q$ |

Here $H_s = z_{\text{from}(k)} + s_k$ is the absolute upstream setpoint, $H_1 = H_{\text{from}(k)}$, and $H_2 = H_{\text{to}(k)}$.

#### FCV Status

| Current | Transition | Condition |
|---|---|---|
| ACTIVE | → XFCV | $H_1 - H_2 < -\varepsilon_H$ (negative available head) |
| ACTIVE | → XFCV | $Q < -\varepsilon_Q$ (reverse flow) |
| ACTIVE | → XFCV | $(H_1 - H_2) / Q^2 < K_m$ (available pressure gradient less than fully-open loss coefficient — valve cannot maintain setpoint) |
| XFCV | → ACTIVE | $Q \geq Q_s$ (flow meets or exceeds setpoint) |

### 3.10 Initialisation

**Node heads** are initialised once at the start of the simulation:

| Node type | Initial head |
|---|---|
| Junction | $H = \text{elevation}$ |
| Reservoir | $H = \text{elevation} \times F_{\text{pattern}}(0)$ if a head pattern is set; otherwise $H = \text{elevation}$ |
| Tank | $H = (\text{elevation} - \text{min\_level}) + \text{init\_level}$, i.e., `bottom_elevation` + `init_level` (see `../model/spec.md` §2.4.4) |

**Link flows** are set as follows before the first Newton-Raphson iteration of each hydraulic time step, if they need re-initialisation:

| Link type | Initial flow |
|---|---|
| Closed or status ≤ CLOSED | $Q_0$: a negligibly small positive placeholder; the specified value is $10^{-6}$ m³/s, several orders of magnitude below the smallest flow expected in a real network. |
| Pump | $\omega \times Q_{\text{design}}$ where $Q_{\text{design}}$ is read from the head curve: for `POWER_FUNCTION` curves the rated design flow is stored directly; for `CUSTOM` curves $Q_{\text{design}}$ is the midpoint of the first and last flow data points, i.e., $(x_0 + x_{L-1})/2$; for `CONST_HP` pumps $Q_{\text{design}} = 0.028317$ m³/s (fixed initial guess, independent of pipe geometry) |
| All other open links | $\pi D^2 / 4$ (cross-sectional area — equivalent to a nominal 1 velocity-unit through the full bore) |

These values do not need to satisfy flow balance; the GGA converges from arbitrary starting flows.

### 3.11 Newton-Raphson Iteration Procedure

This section defines the complete iteration algorithm that composes §3.1–§3.10 into the hydraulic solve for a single time step.

**Input**: an initialised network (§3.10) with junction heads $H$, link flows $Q$, link statuses, and a pre-computed sparse structure (§3.6 Phases 1–2).

**Output**: converged junction heads and link/nodal flows, or an **unbalanced** marker if convergence is not achieved within the iteration budget.

**Iteration budget**: let $M = \texttt{max\_iter}$. If $\texttt{extra\_iter} \geq 0$, the total iteration limit is $M + \texttt{extra\_iter}$; otherwise it is $M$. Status changes are frozen for iterations $m > M$.

**Procedure** — for each iteration $m = 1, 2, \ldots$:

1. **Linearise** (§3.2, §3.3): compute the linearisation coefficients $(P_k, Y_k)$ for every link (§3.2) and the non-link flow coefficients for emitters (§3.3.1), leakage (§3.3.3), and PDA demands (§3.3.2, when active).

2. **Assemble the linear system** $\mathbf{A}\,\mathbf{H} = \mathbf{F}$ (§3.4, §3.5):

a. Accumulate link contributions to $\mathbf{A}$ (diagonal and off-diagonal) and $\mathbf{F}$ (RHS). During this step, also accumulate the net linearised flow at each junction $i$:
$$x_i = \sum_{k:\,\text{to}(k)=i}\!\bigl(Y_k - P_k\,H_{\text{from}(k)}\bigr) \;-\; \sum_{k:\,\text{from}(k)=i}\!\bigl(Y_k - P_k\,H_{\text{to}(k)}\bigr)$$

b. Apply emitter, leakage, and PDA demand coefficients — each adds its linearised conductance to $A_{ii}$ and its correction to $F_i$ and $x_i$.

c. Form the node residual: $F_i \mathrel{+}= x_i - D_i$ for each junction $i$.

d. Apply active valve modifications (§3.5).

3. **Solve** (§3.6 Phase 3): factorise $\mathbf{A} = \mathbf{L}\mathbf{L}^\top$ and solve for $\mathbf{H}^{(m)}$ by forward/backward substitution. If the matrix is singular at a row corresponding to an active control valve node, demote that valve to OPEN (or XFCV for FCV) and restart iteration $m$.

4. **Extract heads**: copy the solved junction heads $\mathbf{H}^{(m)}$ into the working state. Fixed-grade nodes (reservoirs, tanks) retain their known heads.

5. **Update flows** (§3.7): compute updated link flows $Q_k^{(m)}$, emitter flows, leakage flows, and PDA demand flows from the new heads. Record the previous flows $Q_k^{(m-1)}$ before overwriting.

6. **Evaluate convergence** (§3.8): compute the relative flow accuracy $\varepsilon_Q$, the per-link head balance error $\max_k \epsilon_{H,k}$, and the per-link absolute flow change $\max_k |Q_k^{(m)} - Q_k^{(m-1)}|$. Determine whether all active criteria are satisfied.

When `damp_limit > 0` and $\varepsilon_Q \leq \texttt{damp\_limit}$, a relaxation factor of 0.6 is applied to all flow updates in step 5 and valve status checks in step 7 are simultaneously activated.

7. **Check statuses** (§3.9) — unless status changes are frozen ($m > M$):

a. PRV/PSV valve status: checked every iteration when `damp_limit = 0`; otherwise checked only when $\varepsilon_Q \leq \texttt{damp\_limit}$.

b. Pump, CV, FCV, and tank link status: checked every `check_freq` iterations up to iteration `max_check`, and always upon convergence.

8. **Test termination**:

a. If all convergence criteria (step 6) are satisfied **and** no status changed in step 7:

- Validate leakage secondary convergence (§3.8). If not satisfied, continue iterating.
- If $m > M$ (frozen phase), accept as **converged** immediately.
- Perform post-convergence control re-evaluation (§3.8 pswitch): evaluate junction-head simple controls. If any fire and change a link status, continue iterating (the counter is **not** reset).
- If no controls fired, accept as **converged**.

b. If $m \geq M$ and $\texttt{extra\_iter} < 0$, terminate as **unbalanced** (the caller may halt the simulation).

c. If $m \geq M$ and $\texttt{extra\_iter} \geq 0$, freeze all status changes for subsequent iterations.

∥ **Parallelism**: within each iteration, the linearisation (step 1), flow update (step 5), and convergence evaluation (step 6) operate independently per link or per node and may be parallelised. Assembly (step 2) requires coordination at shared junction nodes. The sparse solve (step 3) is inherently sequential.

---

