# hydra-engine — Model Specification

This document is the sub-specification for the WD network data model, unit system, and model file formats.

> **Note**: References to solver subsystems in this document cite sub-specifications: hydraulics in [hydraulics spec](../hydraulics/spec.md), quality in [quality spec](../quality/spec.md), and controls/time-step/accounting/session API in [simulation spec](../simulation/spec.md).

---

## 2. Data Model

The data model is the single shared representation of the network. It is populated by the input layer, read by all solver subsystems, and mutated only through well-defined state-update operations during simulation. All properties below are expressed in the **internal unit system** (§3): SI base units. Unit annotations in property tables indicate the physical dimension of each stored value (e.g., "m" for metres, "m³/s" for volumetric flow rate).

A clear mutability contract governs parallelism safety:

- **Static** — set at load time, never written during simulation. Safe to read from any concurrent context.
- **Per-step state** — updated once per hydraulic time step, before any solver work for that step begins. Written sequentially; read freely during the step.
- **Solver working state** — written and read within a single Newton-Raphson iteration. Must not be accessed concurrently unless the accessor owns an exclusive slice.

### 2.1 Scalar Parameters

Global simulation parameters. All are static after loading.

| Parameter | Description | Constraints |
|---|---|---|
| `duration` | Total simulation duration (s) | > 0 |
| `hyd_step` | Nominal hydraulic time step (s) | > 0 |
| `qual_step` | Quality time step (s); default = `hyd_step` / 10, clamped to [`1`, `hyd_step`] | > 0; ≤ `hyd_step` |
| `report_step` | Reporting interval (s) | > 0; ≤ `duration` |
| `report_start` | Time at which reporting begins (s) | ≥ 0 |
| `pattern_step` | Global pattern time step (s) | > 0 |
| `pattern_start` | Clock offset applied to pattern indexing (s) | ≥ 0 |
| `start_clocktime` | Wall-clock time of simulation t=0 (s from midnight) | 0 – 86 400 |
| `head_loss_formula` | Head-loss model: `HAZEN_WILLIAMS`, `DARCY_WEISBACH`, or `CHEZY_MANNING` | — |
| `demand_model` | `DDA` or `PDA` | — |
| `flow_units` | Identifies the user-facing unit system for input and/or output (for I/O conversion only; does not affect internal representation) | — |
| `viscosity` | Kinematic viscosity of water (m²/s) | > 0 |
| `diffusivity` | Molecular diffusivity of quality constituent (m²/s) | > 0 |
| `specific_gravity` | Relative to water at 4 °C | > 0 |
| `demand_multiplier` | Global scale factor applied to all base demands | > 0 |
| `default_pattern` | Pattern ID applied to demand categories with no explicit pattern (nullable; if null, multiplier 1.0 is used) | nullable |
| `pda_min_pressure` | PDA: pressure below which demand = 0 (m) | — |
| `pda_required_pressure` | PDA: pressure at which full demand is delivered (m) | > `pda_min_pressure` |
| `pda_pressure_exponent` | PDA: pressure exponent $n_P$ | > 0 |
| `emitter_backflow` | Whether emitters may admit reverse flow | boolean |
| `quality_mode` | `NONE`, `CHEMICAL`, `AGE`, or `TRACE` | — |
| `trace_node` | Node ID for source tracing (when `quality_mode = TRACE`) | valid node ID or null |
| `max_iter` | Maximum Newton-Raphson iterations; default 200 | ≥ 1 |
| `extra_iter` | Extra frozen-status iterations on non-convergence (−1 = halt); default −1 | ≥ −1 |
| `head_tol` | Head tolerance $\varepsilon_H$ used in link status transitions (m); default 1.524×10⁻⁴ (= 0.0005 ft) | > 0 |
| `flow_change_tol` | Absolute flow tolerance $\varepsilon_Q$ used in link status transition tests (m³/s); default 2.832×10⁻⁶ (= 0.0001 ft³/s). **Distinct from `flow_tol`**: `flow_tol` governs solver convergence (relative criterion, §3.8); `flow_change_tol` appears only in link status transition conditions (§3.9). | > 0 |
| `flow_tol` | Relative flow accuracy for convergence ($\text{Hacc}$); default 0.001 | > 0 |
| `head_error_limit` | Optional absolute per-link head balance error limit (m); 0 = disabled; default 0 | ≥ 0 |
| `flow_change_limit` | Optional absolute maximum flow change per iteration (m³/s); 0 = disabled; default 0 | ≥ 0 |
| `damp_limit` | Relative flow accuracy threshold below which damping + valve checks activate; default 0 (disabled) | ≥ 0 |
| `rq_tol` | Minimum gradient clamp for emitter/pump coefficient linearisation; default $10^{-7}$ | > 0 |
| `check_freq` | Status check interval (iterations); default 2 | ≥ 1 |
| `max_check` | Iteration count after which status checks stop; default 10 | ≥ `check_freq` |
| `bulk_order` | Global bulk reaction order for pipe segments; default 1.0 | any real |
| `tank_order` | Tank bulk reaction order; default 1.0. Independent of `bulk_order` — allows different reaction kinetics in tanks vs pipes | any real |
| `wall_order` | Global wall reaction order (0 or 1) | 0 or 1 |
| `bulk_coeff` | Global bulk reaction rate coefficient | any real |
| `wall_coeff` | Global wall reaction rate coefficient | any real |
| `conc_limit` | Limiting concentration for bulk reactions | ≥ 0 |
| `energy_price` | Global unit energy cost ($/kWh); used when a pump has no per-pump price | ≥ 0 |
| `energy_price_pattern` | Optional pattern ID modulating the global energy price over time | nullable |
| `energy_efficiency` | Global default pump efficiency fraction; used when a pump has no efficiency curve and no per-pump `default_efficiency` | (0, 1] |
| `peak_demand_charge` | Global demand charge (cost per peak kW); 0 = disabled; default 0 | ≥ 0 |
| `roughness_reaction_factor` | Global roughness–reaction correlation factor $R_f$ for deriving wall coefficients from pipe roughness (§6.5.4); 0 = disabled; default 0 | any real |
| `rule_timestep` | Rule evaluation sub-step duration (seconds); default = `hydraulic_timestep` / 10, clamped to `hydraulic_timestep` | > 0 |
| `quality_tolerance` | Segment merge tolerance $C_{\text{tol}}$ (same units as quality constituent); default 0.01 | ≥ 0 |

> **DEVIATION from EPANET:** EPANET has no lower floor on the quality time step; for very small hydraulic steps the quality step can be 0 (from integer division). Hydra enforces a minimum of 1 second to avoid zero-length sub-steps.

### 2.2 Patterns

A pattern is a repeating sequence of dimensionless multipliers.

**Properties**:

| Property | Description | Constraints |
|---|---|---|
| `id` | Unique string identifier | non-empty |
| `factors` | Ordered list of multipliers $[F_0, F_1, \ldots, F_{L-1}]$ | length ≥ 1 |

**Indexing**: at simulation time $t$, the elapsed period count is $p = \lfloor (t + t_{\text{pattern\_start}}) / \Delta t_p \rfloor$. The active multiplier for this pattern is $F[p \bmod L]$.

**Mutability**: static.

### 2.3 Curves

A curve is a piecewise-linear mapping from an $x$-value to a $y$-value.

**Properties**:

| Property | Description | Constraints |
|---|---|---|
| `id` | Unique string identifier | non-empty |
| `kind` | `PUMP_HEAD`, `PUMP_EFFICIENCY`, `PUMP_VOLUME` (constant-HP), `TANK_VOLUME`, `GPV_HEADLOSS`, `PCV_LOSS_RATIO` | — |
| `points` | Ordered list of $(x_i, y_i)$ pairs | length ≥ 2; $x$ strictly increasing |

**Additional invariants by kind**:
- `PUMP_HEAD`: $y$ strictly decreasing (head must fall with increasing flow)
- `PUMP_EFFICIENCY`: $y \in (0, 100]$
- `TANK_VOLUME`: $y$ strictly increasing
- `GPV_HEADLOSS`: $y$ non-decreasing (head loss does not decrease with increasing flow)

**Evaluation**: for a query value $x$, find the unique segment $[x_{k-1}, x_k]$ that brackets $x$ (extrapolation linearly from the nearest endpoint segment when $x$ is outside the curve range). Return $y = y_{k-1} + (y_k - y_{k-1}) \cdot (x - x_{k-1}) / (x_k - x_{k-1})$.

**Mutability**: static.

### 2.4 Nodes

All nodes share a common identity and base properties. There are three node types.

#### 2.4.1 Common Node Properties

| Property | Description | Constraints |
|---|---|---|
| `id` | Unique string identifier | non-empty |
| `index` | Unique integer index (1-based, assigned at load time) | — |
| `elevation` | Elevation above datum (m) | any real |
| `initial_quality` | Initial constituent concentration or age | ≥ 0 |
| `source` | Optional quality source (see §2.7) | nullable |

#### 2.4.2 Junction

A junction is an ordinary demand node. Its head is an **unknown** solved at every hydraulic step.

| Property | Description | Constraints |
|---|---|---|
| `demands` | List of demand categories (see §2.5) | may be empty |
| `emitter_coeff` | Emitter discharge coefficient $K_e$ (m³/s per m$^{n_e}$) | ≥ 0; 0 = no emitter |
| `emitter_exp` | Emitter pressure exponent $n_e$ | > 0; default 0.5 |

**Per-step state**: `head` (m), `demand_flow` (m³/s), `emitter_flow` (m³/s), `leakage_flow` (m³/s), `actual_quality`.

#### 2.4.3 Reservoir

A reservoir is a fixed-grade node. Its head is **known** at all times and does not appear as an unknown in the linear system.

| Property | Description | Constraints |
|---|---|---|
| `head_pattern` | Optional pattern ID modulating head | nullable |

**Head at time $t$**: if `head_pattern` is set, $H = \text{elevation} \times F_{\text{pattern}}(t)$; otherwise $H = \text{elevation}$.

**Per-step state**: `net_flow` (m³/s, the sum of all connected link flows, for accounting).

#### 2.4.4 Tank

A tank is a storage node whose head evolves over time.

| Property | Description | Constraints |
|---|---|---|
| `min_level` | Minimum water level above bottom (m) | ≥ 0 |
| `max_level` | Maximum water level above bottom (m) | > `min_level` |
| `init_level` | Initial water level above bottom (m) | `min_level` ≤ value ≤ `max_level` |
| `diameter` | Diameter for cylindrical tank (m) | > 0; used only if no `vol_curve` |
| `vol_curve` | Optional curve ID mapping level → volume (m³) | nullable; kind = `TANK_VOLUME` |
| `mix_model` | `CSTR`, `TWO_COMPARTMENT`, `FIFO`, or `LIFO` | — |
| `mix_fraction` | Inlet-zone volume fraction for `TWO_COMPARTMENT` | (0, 1); ignored for other models |
| `bulk_coeff` | Bulk reaction rate coefficient (overrides global) | any real |
| `overflow` | Whether overflow is permitted when full | boolean |

Tanks have **no** head pattern: their head is always derived from the simulated water level (EPANET's `[TANKS]` section has no pattern column). Head patterns exist only on reservoirs (§2.4.3), which are unaffected.

**Derived**: `bottom_elevation` = `elevation` − `min_level`. `Head` = `bottom_elevation` + current level. `Cross-section area` $A$ = $\pi d^2/4$ for cylindrical tanks; for `vol_curve` tanks, $A(h) = dV/dh$ evaluated from the curve.

**Invariant**: $\text{min\_level} \leq \text{init\_level} \leq \text{max\_level}$.

**Per-step state**: `level` (m), `volume` (m³), `head` (m), `net_flow` (m³/s), `quality` (or segment list for FIFO/LIFO).

### 2.5 Demand Categories

Each junction has zero or more demand categories. The total demand at each time step is their sum.

| Property | Description | Constraints |
|---|---|---|
| `base_demand` | Base withdrawal rate (m³/s) | any real (negative = inflow) |
| `pattern` | Pattern ID (nullable; falls back to default pattern, then 1.0) | — |
| `name` | Optional label | — |

**Instantaneous demand** at time $t$: $d_i(t) = \text{base\_demand} \times D_{\text{mult}} \times F_{\text{pattern}}(t)$.

### 2.6 Links

All links share common identity and base properties. There are three link types: pipe, pump, and valve.

#### 2.6.1 Common Link Properties

| Property | Description | Constraints |
|---|---|---|
| `id` | Unique string identifier | non-empty |
| `index` | Unique integer index (1-based) | — |
| `from_node` | Start node index (positive flow direction: from → to) | valid node index |
| `to_node` | End node index | valid node index; ≠ `from_node` |
| `init_status` | Initial status: `OPEN`, `CLOSED`, or `ACTIVE` | see per-type rules |
| `init_setting` | Initial speed (pump) or setting (valve) | see per-type rules |

**Per-step state**: `flow` (m³/s, signed), `status`, `setting`, `quality`.

#### 2.6.2 Pipe

| Property | Description | Constraints |
|---|---|---|
| `length` | Pipe length (m) | > 0 |
| `diameter` | Internal diameter (m) | > 0 |
| `roughness` | Hazen-Williams $C$, Darcy-Weisbach $\varepsilon$ (m), or Manning $n$ | > 0 |
| `minor_loss` | Minor loss coefficient $K_m$ | ≥ 0 |
| `check_valve` | Whether reverse flow is blocked | boolean |
| `bulk_coeff` | Bulk reaction rate coefficient (overrides global; `null` = use global) | nullable |
| `wall_coeff` | Wall reaction rate coefficient (overrides global; `null` = use global) | nullable |
| `leak_coeff_1` | FAVAD full-pipe fixed-area discharge coefficient $K_1$ (m³/s per m$^{0.5}$); split across end nodes at load time (§2.10) | ≥ 0 |
| `leak_coeff_2` | FAVAD full-pipe variable-area discharge coefficient $K_2$ (m³/s per m$^{1.5}$); split across end nodes at load time (§2.10) | ≥ 0 |

**Derived resistance coefficient** $R$: computed from the chosen head-loss formula, `length`, `diameter`, and `roughness`. Recomputed if any of those change.

#### 2.6.3 Pump

| Property | Description | Constraints |
|---|---|---|
| `curve_type` | `POWER_FUNCTION`, `CONST_HP`, or `CUSTOM` | — |
| `head_curve` | Curve ID for head vs. flow (kind = `PUMP_HEAD`) | required unless `CONST_HP` |
| `power` | Rated power (W) | > 0; only for `CONST_HP` |
| `efficiency_curve` | Optional curve ID for efficiency vs. flow | nullable; kind = `PUMP_EFFICIENCY` |
| `default_efficiency` | Fallback efficiency when no curve (fraction) | (0, 1] |
| `speed_pattern` | Optional pattern ID modulating speed setting | nullable |
| `energy_price` | Unit energy price ($/kWh; overrides global) | nullable |
| `price_pattern` | Optional pattern ID modulating energy price | nullable |

**Speed scaling**: all head and flow values scale by the affinity laws — $\Delta H(\omega, Q) = \omega^2 \Delta H_1(Q/\omega)$. `init_setting` is the initial relative speed $\omega$ (1.0 = rated).

**Pump curve coefficients** ($H_0$, $r$, $N$) for `POWER_FUNCTION` type: derived at load time from the head curve data (direct read from curve or 3-point fit; see §3.2). Stored statically.

#### 2.6.4 Valves

All valves share `diameter` (m) and `minor_loss` ($K_m$, the fully-open minor-loss coefficient). The `init_setting` is the initial setpoint value whose meaning depends on type.

| Type | Setting meaning | Active-state constraint |
|---|---|---|
| `PRV` | Downstream pressure setpoint (m) | Downstream head = elevation + setting |
| `PSV` | Upstream pressure setpoint (m) | Upstream head = elevation + setting |
| `FCV` | Flow setpoint (m³/s) | Flow through valve = setting |
| `TCV` | Loss coefficient $s$ (dimensionless) | N/A — always resistance-type |
| `GPV` | Head-loss curve ID | N/A — always resistance-type |
| `PCV` | Percent-open setting (0–100) | N/A — always resistance-type; loss ratio from `PCV_LOSS_RATIO` curve |
| `PBV` | Fixed head-loss setpoint $h_s$ (m) | N/A — always resistance-type |

PRV, PSV, and FCV have discrete states: `OPEN`, `CLOSED`, `ACTIVE`, `XPRESSURE` (PRV/PSV: reverse pressure gradient), or `XFCV` (FCV: cannot enforce setpoint). TCV, GPV, PCV, and PBV have no discrete states — they always contribute a resistance.

A valve with `init_status = OPEN` or `CLOSED` and `init_setting = MISSING` is **fixed**: its status will not be changed by automatic status logic for the duration of the simulation.

### 2.7 Quality Sources

Each node may have at most one quality source.

| Property | Description | Constraints |
|---|---|---|
| `node` | Node index | valid node index |
| `type` | `CONCENTRATION`, `MASS`, `SETPOINT`, or `FLOWPACED` | — |
| `base_value` | Base injection value | ≥ 0 |
| `pattern` | Optional modulating pattern ID | nullable |

**Effective value at time $t$**: `base_value` × $F_{\text{pattern}}(t)$ (or `base_value` if no pattern).

### 2.8 Controls

#### 2.8.1 Simple Controls

A simple control fires at most once per hydraulic time step evaluation.

| Property | Description |
|---|---|
| `link` | Link index to act on |
| `trigger_type` | `TIMER`, `TIMEOFDAY`, `HILEVEL`, or `LOWLEVEL` |
| `trigger_time` | Absolute simulation time (TIMER) or seconds from midnight (TIMEOFDAY) |
| `trigger_node` | Node index for level triggers |
| `trigger_grade` | Hydraulic grade threshold for level triggers (m) |
| `action_status` | Target status (`OPEN` or `CLOSED`; nullable) |
| `action_setting` | Target setting value (nullable) |
| `enabled` | Whether this control is active | 

#### 2.8.2 Rule-Based Controls

A rule is evaluated at each rule time step (which subdivides the hydraulic step).

| Property | Description |
|---|---|
| `priority` | Numeric priority; higher value wins conflicts |
| `premises` | Ordered list of premise clauses (see below) |
| `then_actions` | Actions to apply when all premises are true |
| `else_actions` | Actions to apply when any premise is false |

**Premise**: a logical predicate of the form: `(object, attribute, operator, value)` where:
- `object` is a node or link (by index) or the simulation clock
- `attribute` is head, pressure, demand, flow, status, setting, power, fill-time, drain-time, or clocktime/time
- `operator` is `=`, `≠`, `<`, `>`, `≤`, `≥`
- `value` is a numeric threshold

**Premise value units**: `TIME` and `CLOCKTIME` thresholds are stored in seconds. `FILLTIME` and `DRAINTIME` thresholds are stored in **hours** — the EPANET convention for these attributes — and are *not* converted at the input boundary; the rule evaluator converts the tank's computed fill/drain time into hours before comparison (see [simulation spec](../simulation/spec.md) §4.2.2). All other premise thresholds are converted to internal SI units at load time (§3).

Consecutive premises are joined by `AND` or `OR`. `AND` binds more tightly than `OR` (standard precedence).

**Action**: `(link_index, attribute, value)` where `attribute` is `STATUS` or `SETTING`.

**Conflict resolution**: when two rules fire at the same rule time step and their THEN/ELSE actions assign different values to the same link attribute, the rule with the **numerically higher priority value** wins.

### 2.9 Graph Topology Constraints

The full list of constraints and their fatal-error semantics is documented on
`Network::validate()` in `model/validation.rs`.

---

### 2.10 FAVAD Load-Time Aggregation

The `leak_coeff_1` ($K_1$) and `leak_coeff_2` ($K_2$) fields on each `Pipe` are **per-pipe input values** representing the full-pipe FAVAD discharge coefficients. Before the first hydraulic solve, they must be aggregated into per-junction resistance coefficients $c_{\text{fa},i}$ and $c_{\text{va},i}$ used by the hydraulic engine (§3.3.3).

For each pipe $p$, compute the contribution to each qualifying end node $v$ (where $v$ is a junction, not a reservoir or tank):

$$k_{1,p,v} = \begin{cases} \tfrac{1}{2}\,K_{1,p} & \text{both end nodes of pipe } p \text{ are junctions} \\ K_{1,p} & \text{one end node of pipe } p \text{ is a fixed-grade node (reservoir or tank)} \end{cases}$$

Apply the same rule for $k_{2,p,v}$ using $K_{2,p}$.

For each junction $i$, sum contributions from all incident pipes:

$$K_{\text{fa},i} = \sum_{p \ni i} k_{1,p,i}, \qquad K_{\text{va},i} = \sum_{p \ni i} k_{2,p,i}$$

Derive the per-junction resistance coefficients (inverting $Q = K H^{1/2}$ and $Q = K H^{3/2}$ to the head-as-function-of-flow forms):

$$c_{\text{fa},i} = \begin{cases} 1/K_{\text{fa},i}^{2} & K_{\text{fa},i} > 0 \\ 0 & \text{otherwise} \end{cases} \qquad c_{\text{va},i} = \begin{cases} 1/K_{\text{va},i}^{2/3} & K_{\text{va},i} > 0 \\ 0 & \text{otherwise} \end{cases}$$

These derived values are not stored in the data model proper; they are computed once at load time (before the first solve) and held in a separate pre-computed working structure. They are never recomputed during the simulation unless the network topology or pipe FAVAD coefficients change.

---

## 3. Unit System

Hydra defines two user-facing unit contexts:

| Context | Description |
|---|---|
| **Input** | The unit system in which the network description is expressed. Converted to the implementation's internal representation at the input boundary before any value is stored. |
| **Output** | The unit system in which results are delivered to the caller. Converted from the internal representation at the output boundary. |

These two unit systems are **independent of each other**. An implementation may, for example, accept a network described in US customary units and report results in SI.

**Hydra's internal representation uses SI base units throughout**: metres (m) for lengths, heads, and elevations; cubic metres per second (m³/s) for flows and demands; metres per second (m/s) for velocity; cubic metres (m³) for volume; watts (W) for power. This is not an implementation detail — any layer above the I/O boundary may rely on all model quantities being in SI. The following invariants hold:

1. Every quantity must be converted from the external representation to SI at the input boundary — never inside a solver subsystem.
2. Every quantity must be converted from SI to the external representation at the output boundary — never inside a solver subsystem.
3. No unit conversion occurs inside the hydraulic engine, quality engine, or accounting subsystem. All conversions are performed exclusively at the input and output boundaries.

**Unit-system-dependent formula constants**: some well-known hydraulic formulas (Hazen-Williams, Chezy-Manning, pump energy) embed empirical constants whose numeric value depends on the unit system in which lengths, flows, and heads are expressed. Since Hydra uses SI internally, the correct value for each such constant is the one from the SI column of the tables in §3. §3 expresses each such constant symbolically (e.g., $\alpha_{\text{HW}}$, $k_M$, $k_{\text{unit}}$) and tabulates the concrete values for both SI and US customary systems. The implementation must use the SI column values consistently across all formulas.

**Named flow unit variants**: input formats expose named flow unit options that identify the unit system and scale factor applied at the input boundary. These are not distinct formula systems — they fall into two coherent groups:

| Group | Named variants |
|---|---|
| US customary (ft, ft³/s) | CFS, GPM, MGD, IMGD, AFD |
| SI/metric (m, m³/s) | LPS, LPM, MLD, CMH, CMD, CMS |

Within each group, the named variant affects only the scalar applied at the input boundary. It does not change which formula constants apply.

### 3.1 Flow Unit Conversion Factors

Each named variant defines a scalar $q_{\text{cf}}$ such that $Q_{\text{internal}} = Q_{\text{user}} / q_{\text{cf}}$, where the internal base flow unit is m³/s:

| Variant | Full name | $q_{\text{cf}}$ (user units per m³/s) |
|---|---|---|
| CFS | cubic feet/second | 35.315 |
| GPM | US gallons/minute | 15850.3 |
| MGD | million US gallons/day | 22.824 |
| IMGD | imperial million gallons/day | 19.005 |
| AFD | acre-feet/day | 70.045 |
| LPS | litres/second | 1000.0 |
| LPM | litres/minute | 60000.0 |
| MLD | megalitres/day | 86.400 |
| CMH | cubic metres/hour | 3600.0 |
| CMD | cubic metres/day | 86400.0 |
| CMS | cubic metres/second | 1.0 |

The named variant determines **only** the flow (and demand) conversion factor. All other dimension conversion factors are determined by the **group** (US customary or SI), not by the specific variant within the group.

### 3.2 Dimension Conversion Factors by Group

The internal unit is always SI. The factor converts from the user-facing unit to the internal SI unit: $\text{value}_{\text{internal}} = \text{value}_{\text{user}} / \text{factor}$.

| Dimension | Internal unit | SI (factor) | SI user unit | US customary (factor) | US user unit |
|---|---|---|---|---|---|
| Elevation / Head | m | 1.0 | m | 3.2808 | ft |
| Length | m | 1.0 | m | 3.2808 | ft |
| Diameter | m | 1000 | mm | 39.370 | in |
| Velocity | m/s | 1.0 | m/s | 3.2808 | ft/s |
| Head loss (per unit length) | m/m | 1.0 | m/m | 1.0 | ft/ft |
| Volume | m³ | 1.0 | m³ | 35.315 | ft³ |
| Flow / Demand | m³/s | (per variant) | (per variant) | (per variant) | (per variant) |
| Power | W | 0.001 | kW | 0.001341 | hp |
| Friction factor | — | 1.0 | — | 1.0 | — |
| Quality (concentration) | mg/L | 1.0 | mg/L | 1.0 | mg/L |

**Pressure** is handled separately because its user-facing unit is in principle configurable independently of the flow unit group. The factor converts from user-facing pressure to internal head in metres: $h_{\text{m}} = p_{\text{user}} / \text{factor}$:

| Pressure unit | Factor (user units per m of head) | Notes |
|---|---|---|
| psi | $1.4219 \times S_g$ | Default for US customary; $S_g$ = specific gravity |
| m (metres of head) | 1.0 | Default for SI; also the internal unit (no conversion) |
| kPa | $9.807 \times S_g$ | Optional SI unit (EPANET `PRESSURE kPa` option); not currently selectable in Hydra |
| ft (feet of head) | 3.2808 | Direct length conversion |

The defaults match EPANET: US-customary models report pressure in psi, and SI models report pressure in **metres of head** unless the EPANET `PRESSURE kPa` option is set. Hydra does not currently parse the `PRESSURE` unit option, so pressure is always psi (US group) or metres (SI group).

---

## 4. Model File Formats

A **model file** is a structured document that describes a complete network (topology, physical properties, operational data, and simulation options) as defined in §2. `hydra-engine` owns all format parsing and output serialisation — callers supply raw bytes and receive a validated `Network`, or supply a completed `Simulation` and receive serialised output bytes.

One format is currently defined. Additional formats may be added in future.

### 4.1 Format Detection

Format is **always detected from file contents**, not from the file extension. Any extension, including no extension, is accepted.

| First non-whitespace character | Detected format |
|---|---|
| `[` (section header) | INP (§4.3) |
| `;` (INP comment line) | INP (§4.3) |
| Anything else | Error: unrecognised format |

Accepting a leading `;` allows INP files that begin with comment lines before their first section header.

### 4.2 Parse Complexity

The parser must complete in **at most two sequential passes** over the input, with no re-reads.

### 4.3 INP Format — EPANET 2.3 Compatibility

The INP format is the plain-text network description format used by EPANET. Supporting it allows existing EPANET networks to be run with Hydra without conversion.

**Supported version:** EPANET 2.3 only. Older EPANET file versions (2.0, 2.2) may use different section names, option keywords, or value encodings. Parsers should reject or warn on constructs not present in EPANET 2.3.

**Supported sections:** all sections defined in the EPANET 2.3 input format — `[TITLE]`, `[JUNCTIONS]`, `[RESERVOIRS]`, `[TANKS]`, `[PIPES]`, `[PUMPS]`, `[VALVES]`, `[TAGS]`, `[DEMANDS]`, `[STATUS]`, `[PATTERNS]`, `[CURVES]`, `[CONTROLS]`, `[RULES]`, `[ENERGY]`, `[EMITTERS]`, `[QUALITY]`, `[REACTIONS]`, `[SOURCES]`, `[LEAKAGE]`, `[MIXING]`, `[OPTIONS]`, `[TIMES]`, `[REPORT]`, `[COORDINATES]`, `[VERTICES]`, `[LABELS]`, `[BACKDROP]`, `[END]`.

**Section-to-core mapping notes:**

- `[TAGS]`, `[COORDINATES]`, `[VERTICES]`, `[LABELS]`, `[BACKDROP]`: display/annotation data only — not passed to the core session. Components may preserve these for their own output.
- `[TITLE]`: stored in the data model (`Network.title`) and written to the binary output prolog (§4.5.2). Up to three title lines are preserved.
- `[REPORT]`: controls output filtering and verbosity. These are component-level settings, not simulation parameters.
- `[TIMES] Statistic`: the `STATISTIC` keyword within `[TIMES]` (values: `NONE`, `AVERAGED`, `MINIMUM`, `MAXIMUM`, `RANGE`) controls how per-timestep results are post-processed before output. `NONE` writes every reporting step individually. The other modes aggregate across all reporting steps (time-weighted average, element-wise minimum/maximum, or max−min range). This is a post-processing mode; the core always delivers all per-step results regardless of this setting.

**INP serialisation:** the writer re-serialises a `Network` to INP text, converting every stored SI value back to the user unit system declared by `flow_units` with the **exact inverse** of the load-time conversion (§3), so that a parse → write → parse cycle reproduces the same internal values and a second write is byte-identical (writer idempotence). Conventions that are not obvious from the section column layouts:

- `[OPTIONS]`: the default pattern (§2.1 `default_pattern`) is written with the EPANET keyword `PATTERN <id>` — the only spelling the parser recognises. `BACKFLOW ALLOWED NO` is written when `emitter_backflow` is false; the default (`YES`) is not written. `EMITTER EXPONENT <e>` is written when the junctions' shared emitter exponent differs from the default 0.5 (the exponent is a single global value in INP, applied to every junction at load time).
- `[VALVES]`: for a `GPV` the Setting column holds the head-loss curve ID (kind = `GPV_HEADLOSS`), not a number. For a `PCV` with a loss-ratio curve (kind = `PCV_LOSS_RATIO`), the curve ID is written as the optional 8th column after MinorLoss.
- `[TANKS]`: when the Overflow column must be written (`overflow = true`) for a tank without a volume curve, the VolCurve column holds the `*` placeholder (accepted by the parser as "no curve") so that whitespace splitting keeps the overflow flag in the 9th field.
- `[JUNCTIONS]` / `[DEMANDS]`: a junction with a single demand category is written in `[JUNCTIONS]` only. A junction with n ≥ 2 categories writes its first category in `[JUNCTIONS]` **and all n categories (including the first) in `[DEMANDS]`** — under the parser's EPANET-compatible semantics the first `[DEMANDS]` line for a junction *replaces* the `[JUNCTIONS]`-derived category and subsequent lines *append*, so exactly the same n categories are reconstructed. A category name (§2.5) is written as the 4th `[DEMANDS]` field, and only when the category also has a pattern (with an empty Pattern column the name would shift into it under whitespace splitting).
- `[REACTIONS]`: coefficients are stored per-second internally and written per-day. Wall coefficients and the roughness–reaction correlation factor additionally carry the wall-order-dependent length-dimension factor documented in the [quality spec](../quality/spec.md) §6.5.2; the writer applies the exact inverse of the load conversion.
- `[REPORT]`: `PAGESIZE`, `STATUS`, `SUMMARY`, `MESSAGES`, `ENERGY`, `NODES`, `LINKS`, `FILE`, and the per-field options (`<FIELD> YES|NO`, `<FIELD> PRECISION n`, `<FIELD> BELOW v`, `<FIELD> ABOVE v`) are all re-serialised; field entries are written in sorted field-name order so output is deterministic.
- `[LEAKAGE]`: the on-disk values are the FAVAD coefficients C₁ (mm² of fixed leak area per 100 length units of pipe) and C₂ (mm of leak-area expansion per metre of head, per 100 length units of pipe). At load they become the per-pipe discharge coefficients $K_1 = C_d\sqrt{2g} \cdot 10^{-6} \cdot C_1 \cdot L/100$ and $K_2 = C_d\sqrt{2g} \cdot 10^{-3} \cdot C_2 \cdot L/100$ (with $C_d = 0.6$, $L$ in metres; §2.6.2, §2.10); the writer inverts these formulas.
- `[TIMES]`: duration values are written as `H:MM` when they fall on a whole minute and as `H:MM:SS` otherwise (the parser accepts both; whole-minute rounding would destroy sub-minute steps such as a 20 s quality timestep, and a bare number would be re-read as decimal **hours**). `Pattern Timestep` is written whenever it differs from the parser's default of 3600 s — comparing against the hydraulic timestep instead would silently drop a non-default pattern step that happens to equal the hydraulic step. `Rule Timestep` is always written, since its parser default is derived from the hydraulic timestep and cannot be reconstructed reliably.
- `[OPTIONS]` `VISCOSITY` / `DIFFUSIVITY`: written in EPANET's **relative-multiplier** form — the stored SI value divided by the reference constants 1.022×10⁻⁶ m²/s (viscosity) and 1.208×10⁻⁹ m²/s (diffusivity). The parser interprets values above 10⁻³ (viscosity) resp. 10⁻⁴ (diffusivity) as multipliers of these references and smaller values as absolute; absolute magnitudes (≈10⁻⁹) are destroyed by fixed-precision decimal formatting, so the writer must never emit the absolute form. Omitted when equal to the reference default.
- `[OPTIONS]` `HTOL` / `QTOL` / `RQTOL`: written when they differ from the §2.1 defaults (`head_tol` = 1.524×10⁻⁴ m, `flow_change_tol` = 2.832×10⁻⁶ m³/s, `rq_tol` = 10⁻⁷), applying the exact inverse of the load conversion: `HTOL` = `head_tol` × elevation factor, `QTOL` = `flow_change_tol` × flow factor, `RQTOL` unconverted. Values are written in shortest round-trip decimal form (no fixed precision) because these tolerances can be far smaller than any fixed decimal precision.
- `[RULES]`: premise values for `TIME` and `CLOCKTIME` are always written as `H:MM(:SS)` literals. A bare numeric value would be re-parsed as **hours**, multiplying the stored seconds by 3600 on every save/load cycle. `FILLTIME`/`DRAINTIME` premise values are stored in hours (§2.8.2) and written back unchanged.
- `[MIXING]`: only tanks with a **non-default** mixing configuration are written. The default is `MIXED` (CSTR) with `mix_fraction` = 1.0 (the parser's default fraction when the column is absent); a tank is written when its model is not CSTR or its fraction is not 1.0. The Fraction column is written for `2COMP` always, and for any other model whenever the fraction differs from 1.0 (so an explicitly-parsed fraction survives).
- `[LABELS]` and `[BACKDROP]` are display no-ops: parsed leniently, never written.

**Malformed data lines:** a data line in a recognised section that has fewer fields than the section's required columns, or a field that fails numeric conversion or range validation, must produce a parse error identifying the section, the 1-based source line number, and the offending field or value. This applies uniformly to the object-defining sections (`[JUNCTIONS]`, `[RESERVOIRS]`, `[TANKS]`, `[PIPES]`, `[PUMPS]`, `[VALVES]`) and the node/link property sections (`[DEMANDS]`, `[EMITTERS]`, `[QUALITY]`, `[MIXING]`, `[SOURCES]`, `[STATUS]`, `[LEAKAGE]`). Display/annotation sections (`[COORDINATES]`, `[VERTICES]`, `[TAGS]`, `[REPORT]`) remain lenient: under-length lines and unknown IDs there are skipped, matching EPANET.

**Duplicate identifiers:** defining two nodes with the same ID (across `[JUNCTIONS]`, `[RESERVOIRS]`, and `[TANKS]`) or two links with the same ID (across `[PIPES]`, `[PUMPS]`, and `[VALVES]`) is a parse error naming the duplicated ID. This matches EPANET, which treats a duplicate ID as a hard error (error 215). A node and a link may share the same ID.

**Option range validation:** `TRIALS`, `CHECKFREQ`, and `MAXCHECK` values below 1 (or non-numeric/NaN) are parse errors naming the option, enforcing the §2.1 constraints `max_iter ≥ 1` and `check_freq ≥ 1` at the input boundary instead of silently truncating to 0.

**Non-supported constructs:** constructs specific to the EPANET 2 Toolkit's binary project format are not supported and must produce a parse error identifying the offending section or keyword.

> **DEVIATION from EPANET:** unknown (undocumented) section names are silently ignored rather than rejected. This is deliberate leniency for forward compatibility — files written by newer tools with extra metadata sections still load.

> **DEVIATION from EPANET:** unknown keywords inside `[OPTIONS]` are silently ignored rather than rejected, for the same forward-compatibility reason. Recognised keywords with invalid values are still parse errors.

The INP parser uses the two-pass strategy described in §4.2.

### 4.4 Analysis Artifact Format (`analysis.json`)

See `encode_analysis_artifact` / `decode_analysis_artifact` in
`analysis/artifact.rs` for the file schema and lifecycle (including
stale-on-edit invalidation).

### 4.5 Binary Results Format (`.out`)

The binary results file persists the full time series of a completed simulation. It is an extension of the EPANET 2.3 binary output format: all integers are 4-byte little-endian (`INT4`), all reals are 4-byte little-endian IEEE 754 (`REAL4`), and string fields are fixed-width, zero-padded byte arrays (IDs: 32 bytes; title lines: 80 bytes; filenames: 260 bytes).

#### 4.5.1 Version History

| Version code | Meaning |
|---|---|
| `20012` | EPANET 2.3-compatible baseline layout. No network digest — readers report the digest as *absent*. |
| `20013` | Hydra extension: identical to `20012` except the epilog (§4.5.6) grows from 12 to 20 bytes, inserting a 64-bit network digest (§4.5.7) between the warning flag and the closing magic number. |

Writers always produce the newest version. Readers must accept **both** versions; the only layout difference is the epilog length. Any other version code is rejected as unsupported.

#### 4.5.2 Prolog

| Field | Type / size |
|---|---|
| magic number = 516114521 | INT4 |
| format version (§4.5.1) | INT4 |
| node count $N_n$ (junctions + reservoirs + tanks) | INT4 |
| tank count $N_t$ (reservoirs + tanks) | INT4 |
| link count $N_l$ (pipes + pumps + valves) | INT4 |
| pump count $N_p$ | INT4 |
| valve count | INT4 |
| quality flag (0 = none, 1 = chemical, 2 = age, 3 = trace) | INT4 |
| trace node index (1-based; 0 when not tracing) | INT4 |
| flow-unit code (§3.1 table order: CFS = 0 … CMS = 10) | INT4 |
| pressure-unit code (0 = psi, 1 = kPa, 2 = m) | INT4 |
| report statistic code (0 = series) | INT4 |
| report start time (s) | INT4 |
| report step (s) | INT4 |
| duration (s) | INT4 |
| 3 title lines | 3 × 80 bytes |
| input filename, report filename | 2 × 260 bytes |
| chemical name, chemical units | 2 × 32 bytes |
| node IDs | $N_n$ × 32 bytes |
| link IDs | $N_l$ × 32 bytes |
| link from-node indices (1-based) | $N_l$ × INT4 |
| link to-node indices (1-based) | $N_l$ × INT4 |
| link type codes (0 = CV, 1 = pipe, 2 = pump, 3 = PRV, 4 = PSV, 5 = PBV, 6 = FCV, 7 = TCV, 8 = GPV) | $N_l$ × INT4 |
| tank/reservoir node indices (1-based) | $N_t$ × INT4 |
| tank cross-section areas (m², internal units; 0 for reservoirs) | $N_t$ × REAL4 |
| node elevations (output length units) | $N_n$ × REAL4 |
| link lengths (output length units; 0 for pumps/valves) | $N_l$ × REAL4 |
| link diameters (output diameter units; 0 for pumps) | $N_l$ × REAL4 |

Prolog size: $884 + 36 N_n + 52 N_l + 8 N_t$ bytes.

#### 4.5.3 Energy Section

One 28-byte record per pump — INT4 link index (1-based), then six REAL4 values: percent of time online, average efficiency (%), average energy intensity (kWh per flow unit), average kW, peak kW, average cost per day — followed by a single trailing REAL4 demand charge. Size: $28 N_p + 4$ bytes.

#### 4.5.4 Dynamic Results

One block per reporting period, laid out column-major (all values of one variable, then the next):

- node variables ($4 N_n$ REAL4): demand, head, pressure, quality
- link variables ($8 N_l$ REAL4): flow, velocity, unit headloss, quality, status (status code cast to REAL4), setting, reaction rate, friction factor

Block size: $4(4 N_n + 8 N_l)$ bytes per period.

#### 4.5.5 Network Reactions

Four REAL4 values: average bulk, wall, tank, and source reaction rates (mass/hour). Size: 16 bytes.

#### 4.5.6 Epilog

| Field | Type / size | Versions |
|---|---|---|
| number of reporting periods written | INT4 | all |
| warning flag (0 = no warnings) | INT4 | all |
| network digest (§4.5.7), unsigned 64-bit little-endian | 8 bytes | ≥ 20013 only |
| magic number = 516114521 (integrity check) | INT4 | all |

Epilog size: 12 bytes (version 20012), 20 bytes (version 20013). The magic number is always the final 4 bytes of the file.

#### 4.5.7 Network Digest

The network digest binds a results file to the topology of the network that produced it, so a consumer can detect that results are stale after the model has been edited. It is the **FNV-1a 64-bit** hash (offset basis 14695981039346656037, prime 1099511628211) of the following byte stream:

1. For each node, in network order: the node ID's UTF-8 bytes, then one `0x0A` byte.
2. A single `0x00` byte (node/link separator).
3. For each link, in network order: the link ID's UTF-8 bytes, `0x1F`, the from-node ID's UTF-8 bytes, `0x1F`, the to-node ID's UTF-8 bytes, then `0x0A`.

The digest is deterministic and **order-sensitive**: reordering nodes or links, renaming any element, or rewiring a link's endpoints all change the digest. It intentionally covers identity and connectivity only — property edits (demands, diameters, options) do not change it.

## 5. Runtime Estimation Types

See `RuntimeEstimate` in `model/network.rs`. Allowed values: `Low`, `Medium`, `High`.
The estimate is advisory and deterministic for identical inputs.

