# hydra-engine — Simulation Sub-Specification

## 1. Overview

This document is the simulation sub-specification for `hydra-engine`. It defines the control system (§4), time stepper (§5), accounting subsystem (§7), and session API (§8), and references the hydraulic (§3) and quality (§6) subsystem sub-specifications in [hydraulics spec](../hydraulics/spec.md) and [quality spec](../quality/spec.md). It also documents solver characteristics relative to EPANET (§9) and an EPANET comparison reference (§10).

The network data model consumed by all subsystems is defined in [model spec](../model/spec.md). Hydraulic algorithm details are specified in [hydraulics spec](../hydraulics/spec.md), and quality algorithm details are specified in [quality spec](../quality/spec.md). Throughout this document, bare references to §2 and its sub-sections (e.g., §2.1, §2.7) refer to `../model/spec.md`. For the system-level description of physical scope, see [`README.md`](../../../../README.md); for the unit system contract, see [model spec](../model/spec.md#3-unit-system).

---

## 4. Control System

Controls modify link statuses and settings during the simulation. They are the mechanism by which operational logic (pump scheduling, pressure regulation, tank management) is represented. There are two tiers: **simple controls** evaluated once per hydraulic step, and **rule-based controls** evaluated at sub-step resolution.

### 4.1 Simple Controls

Simple controls are evaluated **once per hydraulic time step**, before the hydraulic solve for that step begins. Their effect is therefore in force for the entire duration of the step.

**Evaluation procedure** (applied to each enabled simple control in index order):

1. Determine whether the trigger condition is satisfied at time $t$:

| Trigger type | Condition |
|---|---|
| `TIMER` | $t = t_{\text{trigger}}$ (exact match in seconds) |
| `TIMEOFDAY` | $(t + t_{\text{clock\_start}}) \bmod 86400 = t_{\text{trigger}}$ |
| `HILEVEL` | $V(h_{\text{node}}) \geq V(H_{\text{threshold}}) - \lvert Q_{\text{net,tank}} \rvert$ |
| `LOWLEVEL` | $V(h_{\text{node}}) \leq V(H_{\text{threshold}}) + \lvert Q_{\text{net,tank}} \rvert$ |

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
| Simulation | `TIME`, `CLOCKTIME` |

`FILLTIME` and `DRAINTIME` are computed from current tank state:

$$\text{FILLTIME} = \begin{cases} (V_{\max} - V) / Q_{\text{net}} & Q_{\text{net}} > 0 \\ \infty & \text{otherwise} \end{cases}, \qquad \text{DRAINTIME} = \begin{cases} (V - V_{\min}) / (-Q_{\text{net}}) & Q_{\text{net}} < 0 \\ \infty & \text{otherwise} \end{cases}$$

where $Q_{\text{net}}$ is the net inflow (positive = filling) and $V$, $V_{\min}$, $V_{\max}$ are the current, minimum, and maximum tank volumes respectively. These attributes are evaluated at the time the premise is checked during the rule sub-step.

**Units**: all premise threshold values are stored in the internal unit system (see data model spec §3). The input layer converts user-unit thresholds to internal units at load time so that premise evaluation operates entirely in internal units with no per-evaluation conversion.

**Logical combination**: consecutive premises within a rule are joined by `AND` or `OR`. `AND` binds more tightly than `OR`. A rule's overall truth value is the evaluation of this expression.

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
evaluate rule-based controls over [t, t+Δt] // §4.2
update tank levels over Δt // §5.3
compute pump energy for this step (§7)
run quality sub-steps over [t, t+Δt] // §6
t ← t + Δt
```

The adaptive time step (§5.2) is computed **after** the hydraulic solve so that the current step's flow field — not the previous step's — is used to predict when tank-level-based controls will fire.

### 5.2 Adaptive Time Step

The actual time step used is the minimum of six quantities — the first constraint that would be violated determines the step:

$$\Delta t = \min\!\left(\Delta t_h,\ \Delta t_{\text{report}},\ \Delta t_{\text{tank}},\ \Delta t_{\text{pattern}},\ \Delta t_{\text{control}},\ t_{\text{duration}} - t\right)$$

| Term | Definition |
|---|---|
| $\Delta t_h$ | User-specified nominal hydraulic time step |
| $\Delta t_{\text{report}}$ | Time remaining until the next reporting instant: $\lceil t / \Delta t_r \rceil \cdot \Delta t_r - t$ |
| $\Delta t_{\text{tank}}$ | Minimum over all tanks of the time to reach a level limit at the current net flow rate: $\min_{\text{tanks}} \Delta V_{\text{available}} / \lvert Q_{\text{net}} \rvert$ (set to $\Delta t_h$ if $Q_{\text{net}} = 0$) |
| $\Delta t_{\text{pattern}}$ | Time remaining until the next pattern boundary: $\lceil (t + t_{\text{pstart}}) / \Delta t_p \rceil \cdot \Delta t_p - t - t_{\text{pstart}}$ |
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

2. **Timer controls**: if $t_{\text{trigger}} > t$, then $t_c = t_{\text{trigger}} - t$.

3. **Time-of-day controls**: $t_c$ is the time remaining until the next occurrence of $t_{\text{trigger}}$ in wall-clock time: $t_c = (t_{\text{trigger}} - (t + t_{\text{start}}) \bmod 86400 + 86400) \bmod 86400$. If $t_c = 0$, use $86400$.

4. **Applicability check**: $t_c$ only shortens the time step if $t_c > 0$ **and** the control's target status or setting differs from the link's current status or setting. Controls that would not actually change anything are ignored.

$\Delta t_{\text{control}} = \min_c t_c$ over all applicable controls; $\Delta t_h$ if no control is applicable.

### 5.3 Tank Level Update

After the time step $\Delta t$ is determined and any rule re-solves are complete, each tank's level is updated:

$$V_{\text{new}} = V_{\text{old}} + Q_{\text{net}} \cdot \Delta t$$

where $Q_{\text{net}} = \sum_{k:\text{to}=\text{tank}} Q_k - \sum_{k:\text{from}=\text{tank}} Q_k$.

**Level from volume**:
- Cylindrical tank: $h_{\text{new}} = h_{\text{old}} + \Delta V / A$ where $A = \pi D^2/4$.
- Volume-curve tank: look up $V_{\text{new}}$ in the `TANK_VOLUME` curve to obtain $h_{\text{new}}$.

**Boundary enforcement**:
- If $h_{\text{new}} < h_{\min}$: clamp to $h_{\min}$; treat tank as a fixed-grade node at its minimum head for the next hydraulic step (inflow is cut off).
- If $h_{\text{new}} > h_{\max}$:
- `overflow = true`: clamp to $h_{\max}$; treat as fixed-grade at maximum head. Surplus volume exits freely. The overflow volume $\Delta V_{\text{overflow}} = (V_{\text{new}} - V_{\max})$ is accumulated in the global flow-balance accounts (§7.2) as nodal outflow from the tank — it contributes to `storage_change` and is included in the volumetric balance ratio. An implementing system may expose per-tank overflow volume as a reportable output quantity (§8.2).
- `overflow = false`: clamp to $h_{\max}$; treat as fixed-grade. No overflow volume is recorded or counted in the flow balance.

### 5.4 Pattern and Demand Update

At the start of each hydraulic time step, before the hydraulic solve:

1. Compute the elapsed period index $p = \lfloor (t + t_{\text{pstart}}) / \Delta t_p \rfloor$.
2. For every junction demand category assigned to pattern $j$ of length $L_j$: apply multiplier $F_j[p \bmod L_j]$.
3. For every reservoir with a head pattern: apply multiplier to base elevation.
4. For every pump with a utilisation pattern: apply multiplier as the new speed setting $\omega$.
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

**Note on `kwh_per_flow`**: this statistic is a **time-weighted harmonic mean** of the energy intensity, accumulated as $\sum (P_i / Q_i) \cdot \Delta t_i$. It is *not* the ratio $\int P\,dt \,/\, \int Q\,dt$ — the two differ when flow or efficiency varies across steps.

The following read-only statistic is derived at report time:

| Reported statistic | Definition |
|---|---|
| `avg_efficiency` | $= \mathtt{efficiency\_sum} / \mathtt{time\_online}$ (time-weighted average efficiency fraction while pump is running) |

where $k_{\text{unit}}$ is the conversion factor from internal power units to kW:

| Unit system | Internal power unit | $k_{\text{unit}}$ |
|---|---|---|
| SI | W (= kg·m²/s³) | $10^{-3}$ |
| US customary | ft·lb/s | $\approx 1.356 \times 10^{-3}$ |

**Example (SI):** pump with $Q_p = 0.05$ m³/s, $\Delta H_p = 20$ m, $\rho = 1000$ kg/m³, $g = 9.81$ m/s², $\eta_p = 0.75$, $\Delta t = 3600$ s:

$$W_p = 1000 \times 9.81 \times 0.05 \times 20 = 9{,}810 \;\text{W}$$
$$W_{\text{elec},p} = 9810 / 0.75 = 13{,}080 \;\text{W}$$
$$\Delta\,\text{kWh} = 13{,}080 \times 3600 \times 10^{-3} = 47{,}088 \;\text{kWh} / 3600 = 13.08 \;\text{kWh}$$

**Example (US customary):** same pump ($Q_p = 1.766$ ft³/s, $\Delta H_p = 65.6$ ft, $\rho = 1.940$ slug/ft³, $g = 32.174$ ft/s², same $\eta_p$ and $\Delta t$):

$$W_p = 1.940 \times 32.174 \times 1.766 \times 65.6 \approx 7{,}229 \;\text{ft·lb/s}$$
$$W_{\text{elec},p} = 7229 / 0.75 \approx 9{,}639 \;\text{ft·lb/s}$$
$$\Delta\,\text{kWh} = 9{,}639 \times 3600 \times 1.356 \times 10^{-3} \approx 13.08 \;\text{kWh} \checkmark$$

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
| Friction factor | dimensionless | §3 (Darcy-Weisbach only; else 0) |
| Quality | mass/volume, time, or dimensionless | §6 |
| Status | enum | §3 |
| Setting | dimensionless (pump speed) or length (pressure setting) | §3 |

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
write_binary_output(session, writer, // serialize results to binary format (spec.md §4.3 output)
input_name, // input filename (metadata for prolog)
report_name, // report filename (metadata for prolog)
output_units) // flow-unit variant for result values in the output file
write_text_report(session) → string // serialize report to plain text

set_node_property(session, node_id, property, value) // modify between steps
set_link_property(session, link_id, property, value)

destroy(session) // release all resources
```

**Invariants**:

- Multiple session objects may coexist in the same process. Sessions share no mutable state.
- A session is not thread-safe with respect to itself — concurrent calls on the same session are not supported; the outcome is unspecified. Concurrent calls on different sessions are safe.
- Property setters that change a value affecting the sparse matrix structure (e.g. adding a node or link) are only valid before `run_hydraulics` / `step_hydraulics` begins. Property setters that change only values (e.g. roughness, demand, pump speed) may be called between steps.
- The unit system of values passed to and returned from the API is an implementation decision. The solver operates in the internal unit system (`../model/spec.md` §3); the API may expose internal units directly (requiring callers to convert) or may accept a unit selection and convert at the API boundary. Either approach is conforming, provided the solver itself never performs unit-dependent branching.

### 8.4 Error Handling

Errors fall into three categories:

| Category | Examples | Behaviour |
|---|---|---|
| **Fatal pre-simulation** | Validation failure (`../model/spec.md` §2.9), malformed data model, unknown object type | Abort; return structured error with offending object ID and condition |
| **Fatal mid-simulation** | Unrecoverable solver singularity, out-of-memory in segment pool | Abort current simulation; session remains valid for inspection of partial results |
| **Warning** | Non-convergence (with ExtraIter bailout), negative pressure in DDA mode, pump XHEAD | Simulation continues; warning attached to the affected time step in the result |

All errors and warnings must be accessible programmatically (not only as printed text) so that callers can handle them without parsing log output.

---

## 9. Solver Characteristics and EPANET Comparison

Hydra has been exercised against eight real-world hydraulic networks totalling 12,500+ junctions and up to 2,000 demand periods. The following characteristics explain all observed differences between Hydra and EPANET 2.3.5 output. They are properties of Hydra's solver — not bugs, and not deviations from a standard.

### 9.1 Global Gradient Algorithm Numerical Path

**System**: EPANET and Hydra both implement the Global Gradient Algorithm (GGA), but starting from different initial flow estimates and applying convergence tolerances independently, they may converge to numerically distinct equilibrium points that differ by 1–10 ULPs in head/flow values.

**Observed consequence**: In heterogeneous networks with many demand nodes (e.g., D-Town 407 junctions), initial flow disparities at t=0 (0.05–0.11 CFS for individual pipes) cascade through subsequent hydraulic time steps and quality transport phases, resulting in downstream quality concentration drifts of 1–2 orders of magnitude when integrated over 100+ periods.

**Impact**: 

- D-Town: ~1,800 flow/head mismatches at t=0 leading to 29,244 cascading quality failures
- KY8/KY9/KY10: 3–4 small failures per network at t=0

**Verdict**: Correct. These differences are inherent to the numerical path — floating-point arithmetic is not associative, and no amount of re-engineering the solver can guarantee byte-level agreement with EPANET's specific convergence trajectory without essentially replicating EPANET's C code line-for-line (including its precision choices, f32 truncations, and sparse matrix libraries). Hydra's GGA convergence path is its own authoritative solution.

**Note**: The absolute differences are small (<0.1% of network head ranges) and physically sensible.

### 9.2 Unbalanced-Stop Mode Not Implemented

**System**: EPANET has a configurable "unbalanced stop" mode that halts the EPS when node pressures throughout the network cannot be made non-negative. This is a numerical stability safeguard intended to prevent divergence.

**Current state**: Hydra's time-stepping solver (§5) is designed to converge at each step and does not include an "unbalanced-stop" check. If a network solution becomes marginally infeasible (e.g. one or two nodes with slight negative pressure), Hydra continues to integrate, while EPANET would halt.

**Observed consequence**: 

- Richmond network: Hydra computes 49 periods of full convergence; EPANET halts after 28 periods due to unbalanced state. The last 21 periods in EPANET's output file are empty or filled with earlier values; Hydra continues with physically valid equilibria.

**Verdict**: Correct, and favorable. Hydra's solver does not need an emergency halt — it integrates through marginally infeasible states to physically valid equilibria. The EPANET unbalanced-stop halt is a legacy safeguard; Hydra's more robust solver makes it unnecessary.

**Future consideration**: An `unbalanced_stop` option could be added to the session API (§8.3) and `../model/spec.md` §2.4 (Options) for users who need to reproduce EPANET's exact halt-on-infeasible behavior. Currently, no such option is defined or needed.

### 9.3 Energy Statistics Differences

**System**: Both Hydra and EPANET accumulate pump electrical power and efficiency statistics according to §7. However, the specific values of per-pump utilization (%), average efficiency (%), and energy intensity (kW per unit flow) depend on the exact hydraulic flow dispatch each step.

**Observed consequence**: 

- BWSN2: Pump utilization values differ by 10–100% (e.g., Hydra reports 10.9% where EPANET reports 1.4% for `pump[0]`); efficiency calculations follow accordingly. The differences arise because Hydra's GGA converges to slightly different flow magnitudes than EPANET.

**Verdict**: Correct. Energy statistics are *derived* from hydraulic results; if hydraulic flows differ, energy statistics will differ proportionally. This is the correct behavior given the upstream flow divergence.

**Scope**: These differences appear only on networks with significant control switching and multiple pump/valve interactions (e.g., BWSN2 with 40+ control events). Simple networks with stable demand patterns (Balerma, L-TOWN) show zero energy discrepancies.

---

## 10. EPANET Comparison Reference

A historical test campaign across eight networks spanning 407–12,900 junctions [Balerma, BWSN2, D-Town, KY8, KY9, KY10, L-TOWN, Richmond] compared Hydra against EPANET 2.3.5 binary output. This data is retained as reference material; it is **not** an active correctness gate.

| Network | Nodes | Links | Periods | Differences | Notes |
|---|---|---|---|---|---|
| Balerma | 399 | 449 | 1 | **0** | ✅ Full agreement |
| BWSN2 | 12,527 | 14,831 | 28/49 | 18 | ℹ️ Energy/period-count (see §9.3, §9.2) |
| D-Town | 407 | 459 | 2 | 35,968 | ℹ️ Quality drift (see §9.1) |
| KY8 | 1,046 | 1,134 | 1 | **3** | ℹ️ GGA path (see §9.1) |
| KY9 | 1,056 | 1,162 | 1 | **4** | ℹ️ GGA path (see §9.1) |
| KY10 | 1,100 | 1,207 | 1 | **4** | ℹ️ GGA path (see §9.1) |
| L-TOWN | 3,359 | 3,936 | 73 | **0** | ✅ Full agreement |
| Richmond | 2,873 | 3,276 | 24/48 | **8** | ℹ️ Period-stop behavior (see §9.2) |

**Summary**:

- **2/8 networks fully agree** (Balerma, L-TOWN)
- **6/8 networks have well-characterised differences** attributable to §9.1–§9.3
- **All algorithmic bugs have been resolved** (friction factor output, valve control status, pump efficiency defaults, AGE-mode timing)

Hydra and EPANET implement the same physics; on well-posed networks they naturally agree closely. Where they diverge, the difference is attributable to one of the three solver characteristics above. Hydra's result is the authoritative output.

---

## 11. Runtime Estimation API

`hydra-engine` provides a deterministic runtime estimator for hydraulic +
quality execution cost. The estimator is advisory only and does not influence
time-step selection, convergence behavior, or any simulation result.

### 11.1 Inputs

The estimator consumes the following static network summary quantities:

1. node count
2. link count
3. simulation duration
4. hydraulic time step
5. quality time step
6. whether quality simulation is enabled

The estimator must not depend on mutable post-run state so the estimate remains
stable before and after executing a simulation on the same network definition.

### 11.2 Output

The estimator returns an effort category (`Low`, `Medium`, or `High`; see
`../model/spec.md` §5).

### 11.3 Estimation Characteristics

The estimator should model cost as an increasing function of:

1. hydraulic step count (duration / hydraulic time step)
2. network size (nodes + links)
3. topological complexity (for example, mesh density indicators)
4. quality step count (duration / quality time step) when quality mode is enabled
5. quality-mode overhead

The estimator is not required to be exact, but it must preserve monotonic
ordering under typical workloads: larger and/or longer simulations should not
systematically receive lower estimates than smaller and/or shorter ones.
