# Key Concepts

This page explains the terminology used throughout Hydra's documentation and in `.inp` files. If you are familiar with EPANET, most of these will be review.

---

## Network Elements

A water distribution network in Hydra is made up of **nodes** (points) and **links** (connections between nodes).

### Nodes

| Term | Description |
|---|---|
| **Junction** | A point in the pipe network where water is consumed or where pipes connect. Most demand nodes are junctions. |
| **Reservoir** | An infinite-capacity water source at a fixed head (e.g. a river, lake, or large supply tank). Acts as a boundary condition for the hydraulic solver. |
| **Tank** | A storage vessel with a finite volume. Water level rises and falls during the simulation as water flows in and out. |

### Links

| Term | Description |
|---|---|
| **Pipe** | A passive conduit between two nodes. Carries flow and produces headloss due to friction. |
| **Pump** | An active link that adds energy to the flow. Defined by a head-flow curve or a constant power rating. |
| **Valve** | A control device that regulates flow or pressure. Types are PRV (pressure-reducing), PSV (pressure-sustaining), FCV (flow-control), TCV (throttle-control), GPV (general-purpose), PBV (pressure-breaker), and PCV (positional-control). |

---

## Hydraulic Concepts

| Term | Description |
|---|---|
| **Head** | The total energy of water at a point, expressed as a height of water (metres or feet). Equal to elevation + pressure head + velocity head. Velocity head is typically negligible in distribution networks. |
| **Pressure** | Gauge pressure at a node — the difference between the total head and the node elevation. Positive pressure means the water is above atmospheric. |
| **Headloss** | The loss of energy as water flows through a pipe, due to friction and minor losses. Higher flow or smaller diameter means more headloss. |
| **Demand** | The rate at which water is withdrawn at a junction (litres/second, gallons/minute, etc.). |
| **Emitter** | A pressure-dependent outflow device at a junction, used to model sprinklers, leaks, or irrigation outlets. Flow is proportional to a power of the local pressure. |
| **Demand-Driven Analysis (DDA)** | Hydraulic mode where all demands are fully satisfied regardless of pressure. The default for most network models. |
| **Pressure-Dependent Analysis (PDA)** | Hydraulic mode where demand delivered at each junction depends on the local pressure. More realistic under low-pressure or deficit conditions. |
| **FAVAD leakage** | Background pipe leakage modelled using the Fixed and Variable Area Discharge method. Specified per pipe in the `[LEAKAGE]` section. |

---

## Time and Patterns

| Term | Description |
|---|---|
| **Extended-Period Simulation (EPS)** | A simulation that runs for a period of time (hours or days) and tracks how the system state evolves — as opposed to a single steady-state snapshot. |
| **Hydraulic timestep** | The interval at which the solver recomputes the full network hydraulic state. Typically 1 hour. |
| **Reporting step** | The interval at which results are saved. Must be a multiple of the hydraulic timestep. |
| **Pattern** | A time series of multipliers applied to a base value (demand, pump speed, reservoir head, etc.) to simulate variation over the simulation period. A multiplier of 1.0 means the base value is used unchanged. |
| **Curve** | An XY dataset defining a relationship: pump head vs. flow, pump efficiency vs. flow, tank volume vs. level, or valve headloss vs. flow. |

---

## Water Quality

| Term | Description |
|---|---|
| **Chemical constituent** | A dissolved substance (e.g. chlorine, fluoride) tracked through the network. Reactions consume or produce the constituent as it moves through pipes and tanks. |
| **Water age** | The time elapsed since water entered the network from a source. Longer age can indicate stale or degraded water. |
| **Source trace** | Tracks the fraction of water at each point in the network that originated from a specified source node. Useful for source blending analysis. |
| **Bulk reaction** | A chemical reaction occurring in the water volume (e.g. chlorine decay in the bulk flow). |
| **Wall reaction** | A chemical reaction at the pipe wall (e.g. chlorine consumption by biofilm or pipe material). |
| **Quality source** | An injection of a constituent into the network at a node. Types include concentration setpoint, mass injection, flow-paced booster, and setpoint booster. |

---

## File Formats

| Extension | Description |
|---|---|
| `.inp` | EPANET network input file. Plain text, defines all network elements, options, and patterns. This is the file you load into Hydra. |
| `.out` | Binary output file. Contains time-series results for every node and link at every reporting step. EPANET-compatible — usable by existing post-processing tools. |
| `.rpt` | Plain-text report. Summary of simulation results in EPANET report style. |
| `.json` | JSON report. Summary-level results including warnings, energy usage, and flow/mass balance. |
