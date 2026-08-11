# hydra-engine — Simulation Sub-Specification

## 1. Overview

This document is the simulation sub-specification for `hydra-engine`. It defines the control system (§4), time stepper (§5), accounting subsystem (§7), and session API (§8), and references the hydraulic (§3) and quality (§6) subsystem sub-specifications in [hydraulics spec](../hydraulics/spec.md) and [quality spec](../quality/spec.md). It also documents solver characteristics relative to EPANET (§9).

The network data model consumed by all subsystems is defined in [model spec](../model/spec.md). Hydraulic algorithm details are specified in [hydraulics spec](../hydraulics/spec.md), and quality algorithm details are specified in [quality spec](../quality/spec.md). Throughout this document, bare references to §2 and its sub-sections (e.g., §2.1, §2.7) refer to `../model/spec.md`. For the system-level description of physical scope, see [`README.md`](../../../../README.md); for the unit system contract, see [model spec](../model/spec.md#3-unit-system).

---

## 4. Control System

Controls modify link statuses and settings during the simulation. They are the mechanism by which operational logic (pump scheduling, pressure regulation, tank management) is represented. There are two tiers: **simple controls** evaluated once per hydraulic step, and **rule-based controls** evaluated at sub-step resolution.

### 4.1 Simple Controls

Simple controls are evaluated **once per hydraulic time step**, before the hydraulic solve for that step begins. Their effect is therefore in force for the entire duration of the step.

One case is resolved inside the solver rather than here: a simple control whose trigger is a **junction** pressure or head cannot be evaluated before the solve, because the controlling pressure is itself an output of the solve. Such controls are re-evaluated after the hydraulics converge, by the `pswitch` mechanism described in [hydraulics spec](../hydraulics/spec.md) §3.8 — if the converged pressure has crossed the threshold, the link is switched and the system is re-solved, iterating until no such control changes state. Level triggers on **tank** nodes and time triggers are handled entirely here (their state does not change during the Newton iteration).

**Evaluation procedure** (applied to each enabled simple control in index order):

1. Determine whether the trigger condition is satisfied at time $t$:

| Trigger type | Condition |
|---|---|
| `TIMER` | $\lvert t - t_{\text{trigger}} \rvert \leq \varepsilon_t$ |
| `TIMEOFDAY` | $\min(d,\; 86400 - d) \leq \varepsilon_t$ where $d = (t + t_{\text{clock\_start}} - t_{\text{trigger}}) \bmod 86400$ |
| `HILEVEL` | $V(h_{\text{node}}) \geq V(H_{\text{threshold}}) - \lvert Q_{\text{net,tank}} \rvert$ |
| `LOWLEVEL` | $V(h_{\text{node}}) \leq V(H_{\text{threshold}}) + \lvert Q_{\text{net,tank}} \rvert$ |

Time triggers use an absolute tolerance $\varepsilon_t = 10^{-6}$ s rather than exact equality (EPANET compares integer seconds; simulation time here is real-valued). The adaptive stepper (§5.2.1) lands each step on the pending trigger time up to floating-point rounding, which the tolerance absorbs. $\varepsilon_t$ is far smaller than any achievable time step, so a trigger fires on exactly one step per occurrence. For `TIMEOFDAY` the distance $d$ is circular, covering rounding on either side of midnight.

Level controls compare **volumes** (not levels) to avoid ambiguity with non-cylindrical tanks. Both volumes are computed through the same $V(h)$ function: $V(h_{\text{node}})$ from the current head, $V(H_{\text{threshold}})$ from the control grade. This ensures floating-point consistency regardless of how the current level/volume was accumulated.

2. If the trigger fires and the resulting action (status and/or setting) differs from the link's current state, apply the action:
- **Action resolution**: when a control specifies a numeric setting without an explicit status keyword, the effective status depends on link type:
- *Pump or pipe*: setting $= 0$ → `CLOSED`; setting $> 0$ → `OPEN`.
- *Valve*: status = `ACTIVE` (always, regardless of setting value).
- When an explicit `OPEN` keyword is used on a pump, the setting defaults to $1.0$. For `CLOSED` on a pump, the setting defaults to $0.0$.
- Update `LinkStatus` and `LinkSetting`.
- For a PCV, recompute the minor-loss coefficient from the new setting.
- For a pump transitioning from CLOSED → OPEN, reset the pump's flow to its design-point initialisation value.

3. If multiple simple controls target the same link and both fire in the same step, the **last one in index order** wins.

### 4.2 Rule-Based Controls

Rule-based controls are evaluated at **rule time steps** that subdivide each hydraulic period. This allows the simulation to detect and respond to mid-step state changes (e.g., a tank crossing a level threshold part-way through a step).

#### 4.2.1 Rule Time Step

$\delta t_r$ is a user-settable parameter (`rule_timestep`; default: $\Delta t_h / 10$, clamped to at most $\Delta t_h$). Sub-step boundaries are aligned to even multiples of $\delta t_r$ measured from $t = 0$: the first sub-step within a hydraulic period is $\delta t_r - (t \bmod \delta t_r)$, which may be shorter than $\delta t_r$.

**Procedure** for a hydraulic period starting at $t$ with nominal duration $\Delta t_h$:

1. Advance the clock by the next sub-step interval $\delta$; update tank levels by $\delta$ (§5.3).
2. Evaluate all rules against the **most recent hydraulic solution** (the one computed at the start of this hydraulic period).
3. For every rule whose premise is satisfied and whose action differs from the current link state: add the action to the pending action set. Mark all conflicting actions by priority.
4. If any actions were collected: apply them (highest-priority wins; all non-conflicting actions are applied together), then **terminate** the rule sub-step loop.
5. Otherwise: continue to the next sub-step.

The rule sub-step loop runs until either a rule fires or the full $\Delta t_h$ is consumed.

**How rule firing interacts with hydraulic solving**: when the loop terminates due to a rule firing after $\delta$ seconds, the hydraulic period is **shortened** to $\delta$ seconds. Tank levels have already been advanced by $\delta$; the rule actions are in force. The outer time stepper then begins the next hydraulic period — a fresh hydraulic solve at time $t + \delta$ with the new settings applied. There is no nested re-solve within the remainder of the original period; the step simply ends at the firing point.

If no rules fire, the step proceeds at its full $\Delta t_h$.

#### 4.2.2 Premise Evaluation

Each premise has the form:

$$(\text{object},\ \text{attribute},\ \text{op},\ \text{threshold})$$

Supported `(object, attribute)` combinations:

| Object type | Attributes |
|---|---|
| Junction | `PRESSURE`, `HEAD`, `DEMAND` |
| Tank | `PRESSURE`, `HEAD`, `LEVEL`, `FILLTIME`, `DRAINTIME` |
| Reservoir | `HEAD` |
| Pipe / Valve | `FLOW`, `STATUS`, `SETTING` |
| Pump | `FLOW`, `STATUS`, `SETTING`, `POWER` |
| Simulation | `TIME`, `CLOCKTIME`, `DEMAND` (total system demand, m³/s) |

> **EXTENSION beyond EPANET:** EPANET's rule grammar carries a `POWER` keyword in its vocabulary but rejects any premise using it at parse time; Hydra wires the attribute up ($P = \gamma\,Q\,\Delta H$ at the pump's current operating point). Opt-in: rules that avoid `POWER` premises behave identically in both engines. (`SYSTEM DEMAND` premises exist in both.)

`FILLTIME` and `DRAINTIME` are computed from current tank state and expressed in **hours** (the EPANET convention for these attributes):

$$\text{FILLTIME} = \begin{cases} \dfrac{V_{\max} - V}{3600\,Q_{\text{net}}} & Q_{\text{net}} > 0 \\ \infty & \text{otherwise} \end{cases}, \qquad \text{DRAINTIME} = \begin{cases} \dfrac{V - V_{\min}}{3600\,(-Q_{\text{net}})} & Q_{\text{net}} < 0 \\ \infty & \text{otherwise} \end{cases}$$

where $Q_{\text{net}}$ is the net inflow (positive = filling, m³/s) and $V$, $V_{\min}$, $V_{\max}$ are the current, minimum, and maximum tank volumes (m³) respectively; the factor 3600 converts the seconds-valued volume/flow quotient into hours. These attributes are evaluated at the time the premise is checked during the rule sub-step.

**Units**: premise threshold values are stored in the internal unit system (see data model spec §3), with two exceptions kept in their on-disk units: `TIME`/`CLOCKTIME` thresholds are seconds, and `FILLTIME`/`DRAINTIME` thresholds are **hours** — they are stored exactly as written in the input (which keeps input round-trips lossless), and the evaluator produces an hours-valued left-hand side as defined above so the comparison is hours-to-hours. All other thresholds are converted to internal units at load time so that premise evaluation operates with no per-evaluation conversion.

**Logical combination**: consecutive premises within a rule are joined by `AND` or `OR`. `AND` binds more tightly than `OR`. A rule's overall truth value is the evaluation of this expression.

> **DEVIATION from EPANET:** EPANET does not apply operator precedence. It evaluates premises left-to-right with a single accumulated boolean: an `OR` clause is consulted only when the accumulated result is false, and a false result reaching an `AND` clause rejects the rule outright — so later `OR` alternatives are never consulted. For "A AND B OR C" with A false, EPANET yields false without consulting C, whereas Hydra evaluates (A AND B) OR C and lets C decide. Hydra keeps true precedence because it makes the rule text mean what it reads, while EPANET's behaviour is an accumulator artifact undocumented in its manual. Ported models that mix `AND` and `OR` within a single rule should be checked for intent.

#### 4.2.3 Action Application and Conflict Resolution

When a rule fires, its THEN actions are applied; when it does not fire (any premise false), its ELSE actions are applied (if any).

If two or more rules fire at the same sub-step and assign conflicting values to the same `(link, attribute)` pair, the rule with the **numerically highest priority value** wins. All non-conflicting actions are applied regardless.

Actions take effect immediately and persist until changed by a subsequent control event.

---

## 5. Time Stepper

The time stepper is responsible for advancing the simulation clock, computing the duration of each hydraulic time step, updating tank levels, and coordinating the hydraulic and quality engines across the full simulation period.

### 5.1 Extended-Period Loop

The top-level simulation loop is:

```text
t ← 0
while t < duration:
apply pattern multipliers at time t
apply simple controls at time t
solve hydraulics (§3)
record output snapshot at time t // §8
Δt ← adaptive_timestep(t) // §5.2
retry:
evaluate rule-based controls over [t, t+Δt] // §4.2
predict tank levels over Δt // §5.3 predictor
correct tank levels; estimate level error e_h // §5.3 corrector
if e_h > level_err_tol and Δt > Δt_floor:
restore pre-step state; Δt ← max(Δt/2, Δt_floor); goto retry
compute pump energy for this step (§7)
run quality sub-steps over [t, t+Δt] // §6
t ← t + Δt
```

Energy accounting and quality transport run only on accepted steps; a rejected trial leaves no trace in either.

The adaptive time step (§5.2) is computed **after** the hydraulic solve so that the current step's flow field — not the previous step's — is used to predict when tank-level-based controls will fire.

### 5.2 Adaptive Time Step

The actual time step used is the minimum of six quantities — the first constraint that would be violated determines the step:

$$\Delta t = \min\!\left(\Delta t_h,\ \Delta t_{\text{report}},\ \Delta t_{\text{tank}},\ \Delta t_{\text{pattern}},\ \Delta t_{\text{control}},\ t_{\text{duration}} - t\right)$$

One further cap applies: when the previous period was accepted only after one or more error rejections (§5.3), $\Delta t$ is additionally capped at twice that period's accepted interval. This prevents the stepper from re-attempting the full nominal step immediately after the error control has just established it is too coarse, and the cap lapses as soon as a period is accepted without rejection.

| Term | Definition |
|---|---|
| $\Delta t_h$ | User-specified nominal hydraulic time step |
| $\Delta t_{\text{report}}$ | Time remaining until the next reporting instant. Report instants fall at $t_{\text{rstart}} + k\cdot\Delta t_{\text{rep}}$ (offset by `report_start`, mirroring how the pattern term is offset by `pattern_start`): $\bigl(t_{\text{rstart}} + \lceil (t - t_{\text{rstart}})/\Delta t_{\text{rep}}\rceil\,\Delta t_{\text{rep}}\bigr) - t$, or $t_{\text{rstart}} - t$ before the first instant. When $t$ lands exactly on a reporting instant the expression is zero; the term is then a **full** $\Delta t_{\text{rep}}$, since the boundary sought is the strictly next one |
| $\Delta t_{\text{tank}}$ | Minimum over all tanks of the time to reach a level limit at the current net flow rate: $\min_{\text{tanks}} \Delta V_{\text{available}} / \lvert Q_{\text{net}} \rvert$ (tanks with $\lvert Q_{\text{net}} \rvert \leq Q_{\text{zero}}$ are skipped; $\Delta t_h$ if no tank qualifies). $Q_{\text{zero}} = 10^{-6}$ m³/s is the negligible-flow threshold (the same value as $Q_0$ in `../hydraulics/spec.md` §3.10); its SI value relative to EPANET's $10^{-6}$ ft³/s is covered by the DEVIATION note in `../hydraulics/spec.md` §3.9 |
| $\Delta t_{\text{pattern}}$ | Time remaining until the next pattern boundary: $\lceil (t + t_{\text{pstart}}) / \Delta t_p \rceil \cdot \Delta t_p - t - t_{\text{pstart}}$. As with the reporting term, this is zero whenever $t + t_{\text{pstart}}$ is an exact multiple of $\Delta t_p$ — including $t = 0$ — and the term is then a **full** $\Delta t_p$. Without that guard the step would collapse to zero at every pattern boundary |
| $\Delta t_{\text{control}}$ | Shortest time until a simple control fires (§5.2.1) |
| $t_{\text{duration}} - t$ | Time remaining until end of simulation |

#### 5.2.1 Control Time Step

$\Delta t_{\text{control}}$ is the shortest predicted time until a simple control (§4.1) would fire and change a link's status or setting. It is computed from the post-solve state so that the current flow field governs the prediction.

For each enabled simple control $c$:

1. **Level controls** (`HILEVEL` / `LOWLEVEL`): if the control references a tank node $n$:
- Let $h$ be the tank's current head and $Q_{\text{net}}$ the net inflow (positive = filling).
- If $\lvert Q_{\text{net}} \rvert \leq Q_{\text{zero}}$, skip (no flow, no crossing).
- If $h < G_c$ and $c$ is `HILEVEL` and $Q_{\text{net}} > 0$ (tank filling toward the threshold), or $h > G_c$ and $c$ is `LOWLEVEL` and $Q_{\text{net}} < 0$ (tank draining toward the threshold):

  $$t_c = \operatorname{round}\!\left(\frac{V(G_c) - V_{\text{current}}}{Q_{\text{net}}}\right)$$

  where $V(G_c)$ is the tank volume at head $G_c$ and $V_{\text{current}}$ is the current tank volume. The result is rounded to the nearest whole second.

2. **Timer controls**: if $t_{\text{trigger}} > t + \varepsilon_t$ (the trigger has not yet fired at $t$; see §4.1), then $t_c = t_{\text{trigger}} - t$.

3. **Time-of-day controls**: $t_c$ is the time remaining until the next occurrence of $t_{\text{trigger}}$ in wall-clock time: $t_c = (t_{\text{trigger}} - (t + t_{\text{start}}) \bmod 86400 + 86400) \bmod 86400$. If $t_c \leq \varepsilon_t$ (the current occurrence fires at $t$ itself), use $86400$.

4. **Applicability check**: $t_c$ only shortens the time step if $t_c > 0$ **and** the control's target status or setting differs from the link's current status or setting. Controls that would not actually change anything are ignored.

$\Delta t_{\text{control}} = \min_c t_c$ over all applicable controls; $\Delta t_h$ if no control is applicable.

### 5.3 Tank Level Update

Tank volumes are the extended-period simulation's only differential state: the network solve of §3 is algebraic, and the whole system is an index-1 DAE whose differential part is $\mathrm{d}V_j/\mathrm{d}t = Q_{\text{net},j}(H)$ per tank $j$, with

$$Q_{\text{net}} = \sum_{k:\text{to}=\text{tank}} Q_k - \sum_{k:\text{from}=\text{tank}} Q_k.$$

Hydra integrates this state with a **Heun predictor–corrector** carrying a local error estimate, rather than the predecessor's uncontrolled first-order step.

**Predictor.** Over each advance interval $\delta$ — a rule sub-step (§4.2) or the remainder of the hydraulic period — the level is advanced explicitly with the flows of the most recent solve:

$$V^{*} = V_{\text{old}} + Q_{\text{net}}^{t} \cdot \delta$$

This is the trajectory rule-based controls evaluate against (§4.2); it is first-order, and the error control below keeps it within tolerance, which bounds the discrepancy any rule trigger can see.

The update is applied unconditionally: Hydra intentionally does not skip it when $\lvert Q_{\text{net}} \rvert \leq Q_{\text{zero}}$ (as EPANET does) — the update is exact at any flow magnitude, and the difference is numerically negligible.

**Corrector.** At the end of the hydraulic period — after rule evaluation has fixed the actually-advanced interval $\Delta t$ and the predictor has produced $V^{*}$ for every tank — the network equilibrium is solved once more at the predicted tank levels and end-of-period settings, yielding $Q_{\text{net}}^{*}$, and each tank's volume is corrected trapezoidally:

$$V_{\text{new}} = V_{\text{old}} + \frac{\Delta t}{2}\left(Q_{\text{net}}^{t} + Q_{\text{net}}^{*}\right)$$

The corrector solve is an ordinary §3 solve; implementations should warm-start it from the predictor state, from which it typically converges in a small number of iterations. The next hydraulic period's opening solve then proceeds from the corrected volumes, so the algebraic constraint always holds at the state the simulation reports and continues from.

A tank whose predictor level was clamped by boundary enforcement during this period (below) takes its predictor value for the period and is excluded from the error estimate: the trapezoid would otherwise average across the flow discontinuity the clamp introduces, and the adaptive step (§5.2) already lands boundary crossings exactly.

**Local error estimate.** The predictor–corrector gap is a direct estimate of the predictor's local truncation error. Converted to a level error through the local surface area,

$$e_h = \max_{\text{tanks}} \frac{\lvert V_{\text{new}} - V^{*} \rvert}{A(h_{\text{new}})}$$

where $A(h)$ is the tank's surface area at level $h$ (for a volume-curve tank, the local slope $\mathrm{d}V/\mathrm{d}h$ of the active curve segment).

**Smoothness precondition.** The gap above is a truncation error only where $Q_{\text{net}}$ varies smoothly across the interval. The corrector re-solves the equilibrium at the predicted levels, and that solve may reclassify link statuses under `../hydraulics/spec.md` §3.9 — a pump driven past its maximum head, a check valve closing, a PRV or PSV changing mode. When it does, $Q_{\text{net}}^{t}$ and $Q_{\text{net}}^{*}$ are samples of two different flow regimes, and $e_h$ measures the size of the switch, not the integrator's error.

Only §3.9 can do this. Simple controls (§4.1) are applied once at the period's opening and rules (§4.2) are evaluated outside the corrector, so neither can fire within it.

On such a step every tank takes its **predictor** value $V^{*}$ — exactly as a clamped tank does above — and the step is accepted and excluded from the error estimate. The trapezoid is deliberately not used: averaging $Q_{\text{net}}^{t}$ with a $Q_{\text{net}}^{*}$ drawn from a different regime integrates neither of them. The predictor is regime-consistent across the interval and first-order, and §5.2 is what keeps that interval short.

Rejection is not available on such a step, and this is a termination property rather than a preference: halving cannot reduce a discontinuity, only approach it, so a rejected switch step retries at the same switch one interval later and continues until $\Delta t_{\text{floor}}$ — paying a solve per halving to arrive at the same discontinuity it started from.

The consequence is stated plainly: a step that crosses a §3.9 regime boundary is first-order in tank level, not second-order, and carries no error bound. Second-order accuracy is claimed only where the flow field is smooth. Locating a §3.9 crossing in time — the analogue of §5.2.1's control scheduling, but for a boundary discovered inside the solve rather than predicted before it — would restore the order across switches and is not attempted here.

**Acceptance.** If $e_h \leq$ `level_err_tol`, the step is accepted. Otherwise, and provided the smoothness precondition above holds, the step is **rejected**: the complete pre-step state — tank volumes, link statuses and settings, rule and accounting state — is restored, the interval is halved, and the period is retried from $t$. Rejection is transactional; no effect of a rejected trial survives. The retry interval is floored at $\Delta t_{\text{floor}} = 1$ s; a step at the floor is accepted unconditionally, and if its estimate still exceeds the tolerance the step is marked with a level-accuracy warning, the analogue of the unbalanced marking in `../hydraulics/spec.md` §3.8.

`level_err_tol` is a session option with default $10^{-3}$ m. Setting it to 0 disables the corrector, the estimate, and rejection entirely, restoring the predecessor's first-order behaviour. Networks with no tanks have no differential state: the corrector solve is skipped and the scheme adds no cost.

> **DEVIATION from EPANET:** EPANET advances tank levels by a single explicit Euler step with the net flow frozen at its start-of-step value, with no error measure of any kind — the only integration-accuracy control available to the user is the hydraulic time step itself, applied uniformly whether or not any tank needs it. Hydra's predictor–corrector is second-order in tank level and carries a per-step local error estimate that adapts the step where — and only where — the tank trajectory demands it. Results differ from EPANET wherever Euler error was material: fast-turnover tanks, coarse hydraulic steps, and level-triggered controls whose firing times shift accordingly. The difference is the removal of an uncontrolled first-order error, and the predecessor's behaviour remains available via `level_err_tol = 0`. This is an intentional improvement, not an oversight.

**Level from volume**:
- Cylindrical tank: $h_{\text{new}} = h_{\text{old}} + \Delta V / A$ where $A = \pi D^2/4$.
- Volume-curve tank: look up $V_{\text{new}}$ in the `TANK_VOLUME` curve to obtain $h_{\text{new}}$.

**Boundary enforcement**:
- If $h_{\text{new}} < h_{\min}$: clamp to $h_{\min}$; treat tank as a fixed-grade node at its minimum head for the next hydraulic step (inflow is cut off).
- If $h_{\text{new}} > h_{\max}$:
- `overflow = true`: clamp to $h_{\max}$; treat as fixed-grade at maximum head. Surplus volume exits freely. The overflow volume $\Delta V_{\text{overflow}} = (V_{\text{new}} - V_{\max})$ is accumulated in the global flow-balance accounts (§7.2) as nodal outflow from the tank — it is added to `total_outflow` (the water has left the tank and is not part of the final stored volume) and is thereby included in the volumetric balance ratio. An implementing system may expose per-tank overflow volume as a reportable output quantity (§8.2).
- `overflow = false`: clamp to $h_{\max}$; treat as fixed-grade. No overflow volume is recorded or counted in the flow balance.

### 5.4 Pattern and Demand Update

At the start of each hydraulic time step, before the hydraulic solve:

1. Compute the elapsed period index $p = \lfloor (t + t_{\text{pstart}}) / \Delta t_p \rfloor$.
2. For every junction demand category assigned to pattern $j$ of length $L_j$: apply multiplier $F_j[p \bmod L_j]$.
3. For every reservoir with a head pattern: apply multiplier to base elevation.
4. For every pump with a speed pattern: the pattern multiplier **is** the new speed setting $\omega$ — it replaces the pump's current setting rather than scaling `init_setting`, matching EPANET, whose file format defines the pattern's multipliers as the speed schedule itself. A pump declaring both `init_setting ≠ 1` and a speed pattern therefore has its initial speed superseded from the first step; this must be surfaced as a non-fatal load-time warning (§8.4) so the dead field is visible rather than silently ignored.
5. For every quality source with a pattern: apply multiplier to the base source value.

### 5.5 Simulation State at Step Boundaries

The only state that must persist across hydraulic step boundaries (i.e., state that cannot be recomputed from scratch at the next step) is:

| State item | Owner | Notes |
|---|---|---|
| Tank levels / volumes | Each tank | Drives the next step's boundary conditions |
| Link flows | All links | Used as the initial iterate for the next Newton-Raphson solve |
| Link statuses and settings | All links | Carried forward; may be overwritten by controls |
| Accumulated pump energy | Each pump | Running totals for §7 |
| Quality segment lists | Each pipe and tank | Large; persists across all quality sub-steps |
| Mass / flow balance accumulators | Global | Running totals for §7 and §8 |

---

## 7. Accounting

The accounting subsystem accumulates energy statistics for each pump and global volumetric flow balance totals. It does not affect the simulation state — it is a pure observer updated after each hydraulic step.

### 7.1 Pump Energy

After each hydraulic step of duration $\Delta t$, for each pump $p$ with flow $Q_p$ and head gain $\Delta H_p$:

**Offline pumps**: a pump whose status for the step is `CLOSED`, `XHEAD`, or `TEMPCLOSED` is not running — it draws no electrical power and accumulates nothing for the step (no energy, cost, online time, or contribution to the peak power draw). This matches EPANET, where any status at or below `CLOSED` yields zero power and efficiency.

**Hydraulic power** (in internal power units):

$$W_p = \rho g Q_p \Delta H_p$$

where $\rho$ is the fluid density and all quantities are in the chosen internal unit system.

**Flow guard**: if $Q_p \leq Q_0$ (the same negligibly small positive threshold as `../hydraulics/spec.md` §3.10), use $Q_p = Q_0$ for all energy computations below. This avoids division by zero in the electrical-power and KwHrsPerFlow calculations.

**Efficiency**: if the pump has an efficiency curve, $\eta_p = \eta(Q_p / \omega_p)$ evaluated from the curve at the speed-adjusted flow; otherwise $\eta_p = \eta_{\text{default}}$. After evaluation (and after the Sarbu-Borza correction below, if applicable), clamp the efficiency to $[0.01, 1.0]$. The 1 % floor prevents division by zero in the electrical-power calculation.

**Variable-speed efficiency correction (Sarbu-Borza formula)**: when the pump operates at a speed setting $\omega_p \neq 1.0$ and an efficiency curve is supplied, apply the following correction to the curve-evaluated efficiency $\eta_1$ (expressed as a percentage, 0–100):

$$\eta_{\omega} = 100 - \frac{100 - \eta_1}{\omega_p^{\,0.1}}$$

Use $\eta_{\omega}$ (converted back to fraction) as $\eta_p$. At $\omega_p = 1.0$ the formula yields $\eta_{\omega} = \eta_1$ and no correction is applied. When no efficiency curve is supplied, the correction is not applied.

**Electrical power**:

$$W_{\text{elec},p} = W_p / \eta_p$$

**Accumulated statistics** per pump:

| Statistic | Update |
|---|---|
| `kwh` | $+= W_{\text{elec},p} \cdot \Delta t \cdot k_{\text{unit}}$ |
| `kwh_per_flow` | $+= (W_{\text{elec},p} \cdot k_{\text{unit}} / Q_p) \cdot \Delta t$ |
| `time_online` | $+= \Delta t$ if $Q_p > 0$ |
| `max_kw` | $\max(W_{\text{elec},p} \cdot k_{\text{unit}})$ |
| `total_cost` | $+= W_{\text{elec},p} \cdot \Delta t \cdot k_{\text{unit}} \cdot \text{price}(t)$ |
| `efficiency_sum` | $+= \eta_p \cdot \Delta t$ if $Q_p > 0$ |

**Note on `kwh_per_flow`**: the raw accumulator is the time *integral* $\sum (P_i / Q_i) \cdot \Delta t_i$ of the energy intensity $P/Q$ (with $Q$ floored at $10^{-6}$ m³/s); it becomes a time-weighted mean only after the report-time division by online hours below. It is *not* the ratio $\int P\,dt \,/\, \int Q\,dt$ — the two differ when flow or efficiency varies across steps.

The following read-only statistics are derived at report time:

| Reported statistic | Definition |
|---|---|
| `avg_efficiency` | $= \mathtt{efficiency\_sum} / \mathtt{time\_online}$ (time-weighted average efficiency fraction while pump is running) |
| `pct_online` | $= \mathtt{time\_online} / \text{duration} \times 100$ |
| `avg_kw` | $= \mathtt{kwh} \cdot 3600 / \mathtt{time\_online}$ (average power over online time) |
| `kwh_per_flow` (reported) | $= \mathtt{kwh\_per\_flow} / \mathtt{time\_online}$, then unit-converted to kWh/m³ (SI) or kWh/Mgal (US) |
| `avg_cost` | $= \mathtt{total\_cost} / (\text{duration}/86400)$ — cost per day; a zero-duration analysis accounts energy as one hour |

where $k_{\text{unit}}$ is the conversion factor from internal power units to kW:

| Unit system | Internal power unit | $k_{\text{unit}}$ |
|---|---|---|
| SI | W (= kg·m²/s³) | $10^{-3}$ |
| US customary | ft·lb/s | $\approx 1.356 \times 10^{-3}$ |

**Example (SI):** pump with $Q_p = 0.05$ m³/s, $\Delta H_p = 20$ m, $\rho = 1000$ kg/m³, $g = 9.81$ m/s², $\eta_p = 0.75$, $\Delta t = 3600$ s:

$$W_p = 1000 \times 9.81 \times 0.05 \times 20 = 9{,}810 \;\text{W}$$
$$W_{\text{elec},p} = 9810 / 0.75 = 13{,}080 \;\text{W}$$
$$\Delta\,\text{kWh} = \frac{13{,}080 \;\text{W} \times 3600 \;\text{s}}{3.6 \times 10^{6} \;\text{J/kWh}} = 13.08 \;\text{kWh}$$

**Example (US customary):** same pump ($Q_p = 1.766$ ft³/s, $\Delta H_p = 65.6$ ft, $\rho = 1.940$ slug/ft³, $g = 32.174$ ft/s², same $\eta_p$ and $\Delta t$):

$$W_p = 1.940 \times 32.174 \times 1.766 \times 65.6 \approx 7{,}229 \;\text{ft·lb/s}$$
$$W_{\text{elec},p} = 7229 / 0.75 \approx 9{,}639 \;\text{ft·lb/s}$$
$$\Delta\,\text{kWh} = \frac{9{,}639 \times 1.356 \;\text{W} \times 3600 \;\text{s}}{3.6 \times 10^{6} \;\text{J/kWh}} \approx 13.07 \;\text{kWh} \checkmark$$

**Energy cost** $\text{price}(t)$ at time $t$ ($/kWh) is determined as follows:

1. **Base cost**: use the pump’s own `energy_price` if it is set ($> 0$); otherwise use the global `energy_price`.
2. **Pattern modulation**: if the pump has a `price_pattern`, multiply the base cost by that pattern’s multiplier at $t$; otherwise multiply by the global `energy_price_pattern` multiplier at $t$ (or 1.0 if no global pattern is set).

This means each pump’s effective energy tariff is independently time-varying: a pump-specific `price_pattern` fully overrides the global pattern modulation for that pump, while the base cost override is independent of the pattern override.

**Global peak demand charge**: throughout the simulation, maintain a running maximum of the total simultaneous electrical power draw across all pumps:

$$P_{\text{peak}} = \max_t \sum_p W_{\text{elec},p}(t)$$

At report time, the total peak demand cost is $\text{peak\_demand\_cost} = \mathtt{peak\_demand\_charge} \times P_{\text{peak}} \times k_{\text{unit}}$ (in the same currency as `total_cost`). If `peak_demand_charge = 0`, this cost is zero. $P_{\text{peak}}$ is updated after every hydraulic step.

### 7.2 Volumetric Flow Balance

Integrated over the full simulation, for each hydraulic step $\Delta t$:

| Quantity | Update |
|---|---|
| `total_inflow` | $+= \left(\sum_{\substack{\text{reservoirs} \\ Q_{\text{net}} < 0}} \lvert Q_{\text{net}} \rvert + \sum_{\text{junctions with } D_i < 0} \lvert D_i \rvert\right) \cdot \Delta t$ (only reservoirs that are **supplying** the network — i.e., net flow out of the reservoir — count as inflow) |
| `total_outflow` | $+= \left(\sum_{\text{junctions with } D_i \geq 0} D_i + \sum_{\text{junctions}} Q_{e,i} + \sum_{\text{junctions}} Q_{\text{leak},i} + \sum_{\substack{\text{reservoirs} \\ Q_{\text{net}} \geq 0}} Q_{\text{net}}\right) \cdot \Delta t$ (absorbing reservoirs — net inflow into the reservoir — count as outflow) |
| `demand_deficit` | $+= \sum_{\text{junctions}} \max(0, D_{\text{full},i} - D_i) \cdot \Delta t$ (PDA mode only; tracked for reporting but **not** included in the balance ratio) |
| `storage_change` | final total tank volume − initial total tank volume |

**Balance ratio**:

$$\rho_v = \frac{\text{total\_outflow} + \max(0, +\Delta V_{\text{storage}})}{\text{total\_inflow} + \max(0, -\Delta V_{\text{storage}})}$$

where $\Delta V_{\text{storage}}$ is positive when tanks fill overall (storage increases, which is output) and negative when tanks drain (which is input). Reservoirs are split directionally: those supplying the network contribute to `total_inflow`; those absorbing water contribute to `total_outflow`. The demand deficit is reported separately alongside the ratio but is not incorporated into it.

A value of $\rho_v \approx 1$ confirms global volume conservation.

---

## 8. Session API

`hydra-engine` exposes a session API (§8.3) through which a caller can load a validated `Network`, drive the simulation, retrieve results, and serialize output. Model-file parsing is owned by `hydra-engine`'s I/O layer (`../model/spec.md` §4). `hydra-engine` performs no filesystem or network I/O; callers supply bytes and receive structured results.

### 8.1 Input Contract

Model-file bytes are parsed by `hydra-engine`'s I/O layer (`../model/spec.md` §4), which performs format detection, parsing, unit conversion, and validation. The session receives a `Network` via `load()`.

Alternatively, a caller may construct a `Network` programmatically and pass it to `load()`. In this case all numeric values must be in the internal unit system (`../model/spec.md` §3) and the caller is responsible for conversion.

The caller is responsible for the model's *stated* content only. Anything an implementation derives from that content for its own convenience — lookup structures, resolved references, precomputed coefficients — is the session's to build at `load()`, from the network as given, whatever the caller left in those fields. A caller who assembles a network by hand, or who edits one after parsing it, has no obligation to maintain derived state and no way to know what an implementation keeps. Deriving it anywhere but `load()` makes correctness depend on a caller honouring a contract that is not stated here, and the failure is silent: a reference that cannot be resolved yields a default rather than an error, so the run answers a different question without saying so.

#### 8.1.1 Data Model Completeness

The data model passed to `load()` must be capable of expressing every entity and property defined in `../model/spec.md` §2. No property may be silently omitted; every required field must be present and valid. (G5)

#### 8.1.2 Post-Population Validation

After the data model is fully populated — whether via file parsing or programmatic construction — the validation checks defined in `../model/spec.md` §2.9 must be run. Any failure is a fatal error; the data model is considered invalid and the simulation must not proceed. The error must identify the offending object by its string ID and the condition violated.

### 8.2 Result API

The following quantities are available from the session API at each **reporting time step** (every $\Delta t_{\text{report}}$ seconds, starting at `report_start`). The unit system in which values are delivered is an implementation decision (see `../model/spec.md` §3).

#### 8.2.1 Reported Quantities

The "Dimension" column gives the physical quantity; the unit in which it is delivered is an implementation decision (`../model/spec.md` §3).

**Per node**:

| Quantity | Dimension | Source subsystem |
|---|---|---|
| Hydraulic head | length | §3 |
| Gauge pressure | length (head above elevation) | §3 |
| Demand delivered | volume/time | §3 |
| Quality (concentration / age / trace) | mass/volume, time, or dimensionless | §6 |

**Per link**:

| Quantity | Dimension | Source subsystem |
|---|---|---|
| Flow rate | volume/time | §3 |
| Mean velocity | length/time | §3 |
| Unit head loss | length/length | §3 |
| Friction factor | dimensionless | §3 (derived — see below) |
| Quality | mass/volume, time, or dimensionless | §6 |
| Status | enum | §3 |
| Setting | dimensionless (pump speed) or length (pressure setting) | §3 |

**Friction factor** is a *derived* reporting quantity, not a solver input, and is
produced for **every pipe whatever head-loss formula is active** — not only
Darcy-Weisbach. It is back-computed from the head loss the solve actually
produced, by inverting the Darcy-Weisbach relation:

$$f = \frac{2 g D^{5} \pi^{2}}{16}\cdot\frac{|H_{\text{from}} - H_{\text{to}}|}{L\,Q^{2}}$$

so under Hazen-Williams or Chezy-Manning it reports the equivalent
Darcy-Weisbach friction factor that would reproduce the observed loss. It is
zero for non-pipe links (pumps, valves) and for pipes carrying negligible flow,
those being the cases where the inversion is undefined rather than cases where
the quantity is suppressed.

**Status annotations (output-only)**: the following status values are computed at reporting time and do not influence the hydraulic solve:

| Status | Applies to | Meaning |
|---|---|---|
| `XFLOW` | Pump | Pump flow exceeds $\omega \times Q_{\max}$ at the current operating point |
| `FILLING` | Tank | Tank has net inflow during the reported step |
| `EMPTYING` | Tank | Tank has net outflow during the reported step |
| `OVERFLOWING` | Tank | Tank is at maximum level with `overflow = true` and net inflow |

$Q_{\max}$ is the theoretical maximum flow capacity of the pump at the current operating point, defined per curve type:

- **Power-function curve** ($H = h_0 - r Q^n$): $Q_{\max} = (h_0 / r)^{1/n}$ — the zero-head flow of the fitted curve.
- **Custom curve** (piecewise-linear): $Q_{\max} = Q_{\text{last}}$ — the highest flow data point on the head curve.
- **Constant-power pump**: $Q_{\max} = \infty$ (XFLOW is never triggered).

**Aggregate (once per simulation)**:

| Quantity | Source subsystem |
|---|---|
| Per-pump energy statistics (kWh, cost, efficiency, peak demand) | §7.1 |
| Mass balance ratio $\rho_m$ | `../quality/spec.md` §6.9 |
| Volumetric flow balance ratio $\rho_v$ | §7.2 |

### 8.3 Core API Contract

The core must expose the following logical operations through its public API. How the API is surfaced — as a native function-call interface, a foreign-function interface, a shared-memory protocol — is an implementation detail. The logical operations and their invariants are what this specification defines.

The simulation is modelled as a **session** with the following lifecycle:

```text
// ── Parsing (owned by hydra-engine's I/O layer, not the session) ──
network = parse(bytes) // from hydra-engine I/O: format detection, conversion, validation
// → error on unrecognised format, parse failure, or validation failure

// ── Session lifecycle ──
session = create() // allocate empty project
load(session, network) // accept a parsed Network (or programmatically built)
// → validates data model; error on failure

run_hydraulics(session) // full hydraulic EPS in one call
-- or --
step_hydraulics(session) → Δt // one hydraulic step; returns actual step taken
// caller may modify model properties between steps

run_quality(session) // full quality EPS in one call (requires hydraulics done)
-- or --
step_quality(session) → Δt // one quality sub-cycle

// ── Result retrieval ──
get_node_result(session, node_id, quantity, time) → value
get_link_result(session, link_id, quantity, time) → value
get_pump_energy(session, pump_id) → EnergyStats
get_mass_balance(session) → MassBalance
get_flow_balance(session) → FlowBalance

// ── Output serialization ──
write_binary_output(session, writer, // serialize results to binary format (model spec §4.5)
input_name, // input filename (metadata for prolog)
report_name, // report filename (metadata for prolog)
output_units) // flow-unit variant for result values in the output file
write_text_report(session) → string // serialize report to plain text

set_node_property(session, node_id, property, value) // modify between steps
set_link_property(session, link_id, property, value)

destroy(session) // release all resources
```

**Streaming serialization**: an implementation may additionally serialize
results incrementally while the session is being stepped, rather than in one
call after the run. A report period may be emitted to the stream only once
every value it carries is **final** — its snapshot can no longer change as the
session advances. With no quality analysis configured, a snapshot is final as
soon as the hydraulic phase records it. With quality enabled, a snapshot's
quality and reaction values are provisional until the quality phase — which
replays the hydraulic history after hydraulics completes — has advanced
through that snapshot's time and written its results back
(`../quality/spec.md`); only then is the snapshot final. Emitting a period
before it is final is non-conforming: the stream would persist provisional
values that the completed run no longer holds.

**Invariants**:

- Multiple session objects may coexist in the same process. Sessions share no mutable state.
- A session is not thread-safe with respect to itself — concurrent calls on the same session are not supported; the outcome is unspecified. Concurrent calls on different sessions are safe.
- Property setters that change a value affecting the sparse matrix structure (e.g. adding a node or link) are only valid before `run_hydraulics` / `step_hydraulics` begins. Property setters that change only values (e.g. roughness, demand, pump speed) may be called between steps.
- The unit system of values passed to and returned from the API is an implementation decision. The solver operates in the internal unit system (`../model/spec.md` §3); the API may expose internal units directly (requiring callers to convert) or may accept a unit selection and convert at the API boundary. Either approach is conforming, provided the solver itself never performs unit-dependent branching.

**Mutation semantics**: A property mutation must change subsequent simulation behaviour — never only the stored model. If an implementation caches quantities derived from a mutable property, the mutation must refresh those caches. Mutations never alter results already recorded; a value takes effect from the next hydraulic solve (or, for initial quality, from quality initialisation) onward. The precise semantics per property:

- **Pipe roughness** (`set_link_property`): the stored roughness is updated and the pipe's head-loss resistance coefficient (`../hydraulics/spec.md` §3.2) is re-derived from the pipe's current length, diameter, and roughness under the network's head-loss formula. The new resistance is used from the next hydraulic solve onward. On a non-pipe link the mutation is accepted but has no effect (roughness is not a pump or valve property).
- **Initial link status / initial link setting** (`set_link_property`): the stored initial value is updated, and additionally:
  - **Before the first hydraulic step**: the link's live state — status, setting, and initial flow estimate — is re-derived under the same initialisation rules applied at load (`../hydraulics/spec.md` §3.10, including the valve `ACTIVE` status resolution). The simulation must produce the same results as if the network had been loaded with the mutated value from the start.
  - **After stepping has begun**: the value is applied to the link's live state as an operational status/setting change under the same rules as a control action (§4.2.3), taking effect from the next hydraulic solve. The link's current flow is preserved as the next Newton-Raphson initial iterate; completed steps are unaffected.
- **Node elevation** (`set_node_property`): the stored elevation is updated together with every cached elevation-derived quantity the solver consumes: the elevation datum used to convert pressure-based valve settings to heads (`../hydraulics/spec.md` §3.5, §3.9) and the tank head limits corresponding to minimum and maximum levels (`../hydraulics/spec.md` §3.9). Quantities derived live from the stored elevation — fixed-grade reservoir head (re-derived every step), pressure-dependent demand, tank level-to-head conversion at tank updates, and pressure reporting — pick the new value up without further action. Before the first hydraulic step, initial reservoir and tank heads are re-derived from the new elevation; after stepping has begun, a tank's current level and volume are preserved and its head is re-derived from the new elevation at the next tank level update.
- **Initial node quality** (`set_node_property`): consumed at quality initialisation (`../quality/spec.md`); mutations at any time before the quality phase initialises take effect. Mutations after quality initialisation do not retroactively change the quality state.

### 8.4 Error Handling

Errors fall into three categories:

| Category | Examples | Behaviour |
|---|---|---|
| **Fatal pre-simulation** | Validation failure (`../model/spec.md` §2.9), malformed data model, unknown object type | Abort; return structured error with offending object ID and condition |
| **Fatal mid-simulation** | Unrecoverable solver singularity, out-of-memory in segment pool | Abort current simulation; session remains valid for inspection of partial results |
| **Warning** | Non-convergence with `extra_iter` $\geq 0$ (frozen-status extra iterations), negative pressure in DDA mode, pump XHEAD, pump initial speed superseded by a speed pattern (§5.4, reported once at load) | Simulation continues; warning attached to the affected time step in the result. With `extra_iter` $= -1$ a non-converged step instead halts the simulation after its results are recorded (§9.2) |

All errors and warnings must be accessible programmatically (not only as printed text) so that callers can handle them without parsing log output.

---

## 9. Solver Characteristics

Hydra and EPANET implement the same physics, and on well-posed networks they
agree closely. Where results differ, the difference is attributable to one of
the characteristics below. These are properties of Hydra's solver — not bugs,
and not deviations from a standard.

Specific magnitudes are deliberately not quoted. Both engines' accuracy and
performance have moved independently, so any particular measured difference
dates quickly and a specification is the wrong place to pin one.

**Note conventions.** Throughout all sub-specifications, two blockquote note types mark Hydra's relationship to EPANET (the OWA v2.3.5 baseline):

- **DEVIATION from EPANET** — Hydra deliberately behaves differently on ground both engines share, because the divergence is more accurate, more robust, or follows Hydra's SI-idealised conventions. Comparison runs differ wherever the note's conditions occur. Input/output *file* compatibility is never affected.
- **EXTENSION beyond EPANET** — Hydra offers an optional capability EPANET lacks (the lineage precedent being OWA's FAVAD leakage relative to USEPA EPANET). Extensions are strictly opt-in: inputs that do not use them behave identically in both engines.

### 9.1 Global Gradient Algorithm Numerical Path

**System**: EPANET and Hydra both implement the Global Gradient Algorithm (GGA), but starting from different initial flow estimates and applying convergence tolerances independently, they may converge to numerically distinct equilibrium points that differ by 1–10 ULPs in head/flow values.

**Consequence**: in heterogeneous networks with many demand nodes, small initial flow disparities cascade through subsequent hydraulic time steps and into quality transport, where they compound — quality is an integrator, so a difference too small to see in heads can become visible in concentrations over a long run.

**Verdict**: Correct. These differences are inherent to the numerical path — floating-point arithmetic is not associative, and no amount of re-engineering the solver can guarantee byte-level agreement with EPANET's specific convergence trajectory without essentially replicating EPANET's C code line-for-line (including its precision choices, f32 truncations, and sparse matrix libraries). Hydra's GGA convergence path is its own authoritative solution.

**Note**: The absolute differences are small (<0.1% of network head ranges) and physically sensible.

### 9.2 Unbalanced-Stop (`extra_iter = -1`) Halt Behaviour

**System**: EPANET's `UNBALANCED STOP` option (`extra_iter = -1`) halts the extended-period simulation when a hydraulic solve fails to converge within the iteration limit. The trigger is **non-convergence of the hydraulic solve** — not negative pressures. In EPANET the halt flag is set immediately after the unbalanced solve returns, before the step's results are saved; the results are then saved as usual and the simulation terminates at the start of the next step.

**Hydra behaviour**: Hydra implements the same option with equivalent observable semantics. When a hydraulic solve exhausts its iteration budget without converging:

- `extra_iter` $\geq 0$: a non-convergence warning is attached to the step and the simulation continues, using the unbalanced solution (after the frozen-status extra iterations, if any) — see §8.4.
- `extra_iter` $= -1$: the non-convergence warning is attached, the step's results are recorded as usual, and the simulation then terminates. No further steps are taken; already-recorded results remain available through the session API (§8.2). This matches EPANET's save-then-halt ordering: the unbalanced step's results appear in the output, and nothing after it does.

**Consequence**: because the two engines' iteration trajectories differ (§9.1), the *step at which* non-convergence first occurs can differ. An `UNBALANCED STOP` run may therefore terminate at a different period in each engine even though both apply the same halt rule — and where one engine converges throughout, it never halts at all while the other stops partway and leaves the remaining periods unwritten.

**A second, harder stop path** exists in both engines and is not the one above.
When Cholesky factorisation breaks down and the failing row does **not** belong to
an active control valve — so the [hydraulics spec](../hydraulics/spec.md) §3.6
valve-demotion recovery cannot apply — the
solve is unrecoverable. EPANET returns its cannot-solve error, which its
error-propagation macro treats as fatal (only codes above its warning band
short-circuit the extended-period loop); Hydra returns the equivalent solver
error from the session. The two stop paths differ observably and should not be
conflated:

| | Trigger | Failing step's results |
|---|---|---|
| Unbalanced stop | solve exhausts its iteration budget, `extra_iter = -1` | **saved**, then the run ends |
| Unrecoverable solve | singular matrix, no valve to demote | **not saved**, the run aborts |

Note that EPANET's non-fatal conditions — negative pressures, a disconnected
network, pumps out of range, an FCV unable to supply flow — are returned through
the same channel as errors but fall below the macro's threshold, so they never
stop the run. That they are non-fatal is a property of that threshold rather
than an explicit decision in the control flow.

**Verdict**: Correct. Both halt rules are identical between the engines;
divergent halt points are a downstream effect of the §9.1 numerical-path
differences, not a behavioural deviation.

### 9.3 Energy Statistics Differences

**System**: Both Hydra and EPANET accumulate pump electrical power and efficiency statistics according to §7. However, the specific values of per-pump utilization (%), average efficiency (%), and energy intensity (kW per unit flow) depend on the exact hydraulic flow dispatch each step.

**Consequence**: where the two engines dispatch flow differently, every derived energy figure follows. Utilisation is especially sensitive, being a proportion of *time online* — a pump that switches near a control threshold can land on either side of it, so a small flow difference becomes a large utilisation difference.

**Verdict**: Correct. Energy statistics are *derived* from hydraulic results; if flows differ, energy statistics differ with them.

**Scope**: differences concentrate in networks with substantial control switching and interacting pumps and valves. Networks with stable demand patterns and little switching show none.

---

## 10. Runtime Estimation API

`hydra-engine` provides a deterministic runtime estimator for hydraulic +
quality execution cost. The estimator is advisory only and does not influence
time-step selection, convergence behavior, or any simulation result.

### 10.1 Inputs

The estimator consumes the following static network summary quantities:

1. node count
2. link count
3. simulation duration
4. hydraulic time step
5. quality time step
6. whether quality simulation is enabled

The estimator must not depend on mutable post-run state so the estimate remains
stable before and after executing a simulation on the same network definition.

### 10.2 Output

The estimator returns an effort category (`Low`, `Medium`, or `High`; see
`../model/spec.md` §5).

### 10.3 Estimation Characteristics

The estimator should model cost as an increasing function of:

1. hydraulic step count (duration / hydraulic time step)
2. network size (nodes + links)
3. topological complexity (for example, mesh density indicators)
4. quality step count (duration / quality time step) when quality mode is enabled
5. quality-mode overhead

The estimator is not required to be exact, but it must preserve monotonic
ordering under typical workloads: larger and/or longer simulations should not
systematically receive lower estimates than smaller and/or shorter ones.
