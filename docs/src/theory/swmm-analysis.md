# SWMM: A Conceptual and Mathematical Analysis

## Introduction

[SWMM](https://github.com/pyswmm/Stormwater-Management-Model) (Storm Water Management Model) is a computational engine for simulating the quantity and quality of runoff from urban catchments and its conveyance through drainage systems — storm sewers, sanitary and combined sewers, open channels, storage units, and flow regulators — over single events or continuous multi-year periods. Where a water distribution engine solves for a pressurised network in hydraulic equilibrium at each time step, SWMM is a **rainfall-runoff-routing** model: precipitation falls on subcatchments, becomes runoff after losses to infiltration, evaporation, and depression storage, and the resulting hydrographs and pollutographs are routed through a free-surface conveyance network by solving forms of the Saint-Venant equations. Flows may be driven by gravity or pumped, conduits may transition between open-channel and pressurised (surcharged) states, and the network may include backwater, flow reversal, ponding, and tidal boundary conditions.

This document provides a self-contained, mathematical and conceptual description of every major subsystem: how the physical system is represented as modelling objects, how runoff is generated on subcatchments, how infiltration and other hydrologic losses are computed, how flows are routed through the conveyance network under the steady, kinematic wave, and dynamic wave options, how special structures (pumps, orifices, weirs, outlets, culverts, force mains) behave, how control rules operate, how pollutants build up, wash off, and are transported and treated, and how continuity is tracked. The goal is to give the reader a complete algorithmic and mathematical understanding of the system; implementation-specific details such as memory layout are omitted, but input/output behaviour is described precisely. The analysis describes **SWMM 5.2.4** — tag `OWA_v5.2.4` of the community-maintained repository — with the EPA SWMM Reference Manuals (Volume I, Hydrology, EPA/600/R-15/162A; Volume II, Hydraulics, EPA/600/R-17/111; Volume III, Water Quality, EPA/600/R-16/093) as secondary references. Wherever the manuals and the source code disagree, the source is authoritative, and each such discrepancy is noted in place.

---

## Table of Contents

- [SWMM: A Conceptual and Mathematical Analysis](#swmm-a-conceptual-and-mathematical-analysis)
  - [Introduction](#introduction)
  - [Table of Contents](#table-of-contents)
  - [1. System Representation](#1-system-representation)
    - [1.1 Environmental Compartments](#11-environmental-compartments)
    - [1.2 Hydrology Objects](#12-hydrology-objects)
    - [1.3 Conveyance Nodes](#13-conveyance-nodes)
    - [1.4 Conveyance Links](#14-conveyance-links)
    - [1.5 Water Quality Objects](#15-water-quality-objects)
    - [1.6 Data Objects](#16-data-objects)
    - [1.7 The State-Vector View](#17-the-state-vector-view)
  - [2. Simulation Architecture](#2-simulation-architecture)
  - [3. Hydrology](#3-hydrology)
    - [3.1 Meteorology](#31-meteorology)
    - [3.2 Surface Runoff](#32-surface-runoff)
    - [3.3 Infiltration](#33-infiltration)
      - [Horton](#horton)
      - [Modified Horton](#modified-horton)
      - [Green–Ampt](#greenampt)
      - [Modified Green–Ampt](#modified-greenampt)
      - [Curve Number](#curve-number)
    - [3.4 Groundwater](#34-groundwater)
    - [3.5 Snowmelt](#35-snowmelt)
    - [3.6 Rainfall-Dependent Inflow/Infiltration (RDII)](#36-rainfall-dependent-inflowinfiltration-rdii)
  - [4. Flow Routing Theory](#4-flow-routing-theory)
  - [5. Dynamic Wave Analysis](#5-dynamic-wave-analysis)
    - [5.1 Conduit Flow Update](#51-conduit-flow-update)
    - [5.2 Node Head Update](#52-node-head-update)
    - [5.3 Picard Iteration](#53-picard-iteration)
    - [5.4 Surcharge](#54-surcharge)
    - [5.5 Flooding and Ponding](#55-flooding-and-ponding)
    - [5.6 Time-Step Control](#56-time-step-control)
    - [5.7 Initial Conditions](#57-initial-conditions)
  - [6. Cross-Section Geometry](#6-cross-section-geometry)
    - [6.1 Shape Families](#61-shape-families)
    - [6.2 Transects](#62-transects)
    - [6.3 Storage Geometry and Characteristic Depths](#63-storage-geometry-and-characteristic-depths)
  - [7. Pumps and Flow Regulators](#7-pumps-and-flow-regulators)
    - [7.1 Pumps](#71-pumps)
    - [7.2 Orifices](#72-orifices)
    - [7.3 Weirs](#73-weirs)
    - [7.4 Outlets](#74-outlets)
  - [8. Advanced Hydraulics](#8-advanced-hydraulics)
    - [8.1 Conduit Evaporation and Seepage](#81-conduit-evaporation-and-seepage)
    - [8.2 Minor Losses and Force Mains](#82-minor-losses-and-force-mains)
    - [8.3 Culverts and Roadway Weirs](#83-culverts-and-roadway-weirs)
    - [8.4 Streets and Inlets](#84-streets-and-inlets)
      - [On-Grade Capture](#on-grade-capture)
      - [On-Sag Capture](#on-sag-capture)
      - [Capture Transfer and Statistics](#capture-transfer-and-statistics)
  - [9. Water Quality](#9-water-quality)
    - [9.1 Pollutants and Sources](#91-pollutants-and-sources)
    - [9.2 Buildup and Street Sweeping](#92-buildup-and-street-sweeping)
    - [9.3 Washoff](#93-washoff)
    - [9.4 Transport and Treatment](#94-transport-and-treatment)
  - [10. LID Controls](#10-lid-controls)
  - [11. Control Rules](#11-control-rules)
  - [12. Continuity Accounting](#12-continuity-accounting)
  - [13. Units and Physical Constants](#13-units-and-physical-constants)
  - [14. Input and Output](#14-input-and-output)
    - [Input](#input)
    - [Interface Files](#interface-files)
    - [Output](#output)
      - [Binary Output File Layout](#binary-output-file-layout)
  - [15. The Engine as a Library](#15-the-engine-as-a-library)
    - [Run-Loop Lifecycle](#run-loop-lifecycle)
    - [Query and Mutation](#query-and-mutation)
    - [Rainfall Injection and Checkpointing](#rainfall-injection-and-checkpointing)
  - [16. Cross-Cutting Engine Contracts](#16-cross-cutting-engine-contracts)

---

## 1. System Representation

### 1.1 Environmental Compartments

SWMM conceptualises an urban drainage system as water and material flows between four **environmental compartments**:

- The **Atmosphere** compartment generates precipitation and deposits pollutants onto the land surface. It is represented by rain gage objects.
- The **Land Surface** compartment receives precipitation as rain or snow and loses water through evaporation back to the atmosphere, infiltration into the sub-surface, and surface runoff (with its pollutant load) into the conveyance system. It is represented by subcatchment objects.
- The **Sub-Surface** compartment receives infiltration from the land surface and transfers a portion of it to the conveyance system as groundwater interflow. It is represented by aquifer objects.
- The **Conveyance** compartment is the network of channels, pipes, pumps, regulators, and storage units that carries water to outfalls or treatment. It is represented as a directed graph of nodes and links. Inflows to this compartment can come from surface runoff, groundwater interflow, rainfall-dependent infiltration/inflow, sanitary dry-weather flow, or user-defined time series.

Not every compartment need be present in a model: a pure conveyance model may be driven entirely by user-supplied inflow hydrographs, and a pure hydrology model may end at subcatchment outlets. This compartmental decomposition is the fundamental structural difference from a pressurised-network model such as EPANET, in which the network *is* the entire model: in SWMM the node-link conveyance graph is only one of four coupled subsystems, and areal objects (subcatchments, aquifers, snow packs) participate in the simulation without being part of the graph at all.

### 1.2 Hydrology Objects

**Rain gages** supply precipitation to one or more subcatchments. A gage's data come from a user-supplied time series or an external rainfall file, expressed as intensity, volume, or cumulative volume over a fixed recording interval.

**Subcatchments** are parcels of land that receive precipitation from exactly one rain gage and generate runoff and pollutant loads. A subcatchment discharges either to a conveyance node or to another subcatchment, allowing overland flow to cascade across parcels. Each subcatchment is idealised as a rectangular plane of a given area, characteristic **width**, and uniform slope, partitioned into three sub-areas: an impervious fraction with depression storage, an impervious fraction without, and a pervious fraction (with depression storage). Only the pervious fraction infiltrates. Runoff from each sub-area may optionally be re-routed onto another sub-area rather than directly to the outlet, modelling e.g. rooftops draining onto lawns. Overland flow is generated by treating each sub-area as a **nonlinear reservoir** (§3). A subcatchment may additionally host: a **snow pack** object governing snow accumulation and melt on its plowable, impervious, and pervious fractions; a **groundwater** connection to an aquifer; **LID controls** occupying a portion of its area (layered as surface, pavement, soil, storage, underdrain, and — on green roofs — drainage mat); and per-land-use pollutant buildup state.

**Aquifers** are two-zone (unsaturated/saturated) sub-surface reservoirs placed beneath subcatchments. They receive percolation from the surface, lose water to deep percolation and evapotranspiration, and exchange flow with a designated conveyance node through a user-parameterised groundwater flow equation — the mechanism by which baseflow and groundwater infiltration enter sewers.

**Unit hydrographs** (organised in groups of up to three per month, the "RTK" parameterisation) describe rainfall-dependent infiltration/inflow (RDII) — the delayed entry of stormwater into sanitary sewers through defects and illicit connections. Each unit hydrograph converts a unit of instantaneous rainfall into a triangular response defined by a fraction of rainfall volume ($R$), a time to peak ($T$), and a recession ratio ($K$).

**LID controls** are depth-explicit representations of low-impact-development practices — bio-retention cells, rain gardens, green roofs, infiltration trenches, permeable pavement, rain barrels/cisterns, rooftop disconnection, and vegetative swales — composed of layered storage elements (surface, soil, storage, drain) with their own governing equations (§10). They are defined once and deployed in multiple subcatchments at specified sizes.

### 1.3 Conveyance Nodes

Nodes are the points of the conveyance graph. Every node has an invert elevation and may receive **external inflows**, in addition to the runoff and groundwater delivered by hydrology objects. A direct external inflow (of flow or of any constituent) is the composite $cf\,(sf \cdot TS(t) + \text{baseline} \cdot P(t))$ — an optional time series with scale factor $sf$, plus a constant baseline modulated by its own monthly/daily/hourly/weekend pattern, times a units factor $cf$ (mass-type pollutant inflows carry their own conversion factor); dry-weather sanitary inflow multiplies an average value by up to four patterns of distinct types, the weekend-hourly pattern *replacing* (not multiplying) the hourly one on weekends; and RDII arrives per §3.6. There are four node types:

**Junctions** are ordinary connection points — manholes, pipe fittings, or channel confluences — with negligible storage volume. A junction has a maximum (ground/rim) depth; when the hydraulic grade line reaches it, excess water is either lost from the system or, if **ponding** is enabled, stored atop the node in a user-specified ponded area and returned as capacity recovers.

**Outfalls** are terminal boundary nodes where water leaves the system to a receiving body. An outfall's boundary stage may be: **free** (the smaller of critical and normal depth at the connecting conduit), **normal** (normal-depth), **fixed** (constant stage), **tidal** (a repeating 24-hour stage curve, indexed by elapsed routing time from the curve's first hour — coinciding with clock time only for midnight starts), or a **time series**. For the staged variants, the stage governs only when it exceeds the critical-depth elevation — a low receiving stage never draws the boundary below critical depth — and outfalls may carry their own flap gate blocking reverse flow. An outfall connects to exactly one link (of any type — pumps and regulators may discharge directly to it; under steady and kinematic wave it may have no outlet links while its inflow count goes unchecked, whereas dynamic wave enforces a single connecting link that may even be an outlet link) and may optionally route its discharge back onto a subcatchment. Under steady and kinematic wave routing, any *non-outfall*, non-storage terminal node with no outlet links behaves identically — its inflow leaves the system without being counted as flooding; under dynamic wave such a node is an ordinary interior node whose overflow counts as flooding.

**Storage units** are nodes with significant free-surface storage volume — ponds, wet wells, detention basins, chambers. Their geometry is described either by a functional relation (surface area as a power function of depth), by a tabulated area-versus-depth **storage curve**, or by one of four analytical shapes new in 5.2 (elliptical cylinder, elliptical cone, elliptical paraboloid, rectangular pyramid). Storage units may lose water through evaporation and through seepage into the soil.

**Dividers** split inflow (including any node overflow) between two outflow conduits according to a prescribed rule: a **cutoff** divider diverts all inflow above a threshold; an **overflow** divider diverts whatever the non-diverted conduit declines to accept (the non-diverted link is deliberately routed first); a **tabular** divider uses a diverted-flow-versus-inflow curve; and a **weir** divider applies the weir equation at head $f\,d_{max}$ — discharging $C_W (f\,d_{max})^{1.5} = q_{max}f^{3/2}$ — with fraction $f = (Q_{in} - q_{min})/(q_{max} - q_{min})$, $q_{max} = C_W d_{max}^{1.5}$, switching to an orifice-like $q_{max}\sqrt{f}$ when surcharged ($f > 1$). Diverted flow is always clamped to the inflow. Dividers are only meaningful under steady and kinematic wave routing; under dynamic wave analysis they behave as ordinary junctions, since the full momentum treatment determines the flow split naturally.

### 1.4 Conveyance Links

Links connect nodes and carry flow; a link's orientation defines positive flow direction, and negative flows denote reversal. There are five link types:

**Conduits** are the pipes and channels of the network — the only link type with hydraulic length and the primary object of flow routing. Each conduit has a cross-sectional **shape** drawn from one of four descriptions:

- a library of more than twenty standard closed and open geometries (circular, rectangular, trapezoidal, egg, horseshoe, arch, elliptical, and others);
- an **irregular transect** (a surveyed station-elevation profile with left-bank/main-channel/right-bank roughness, in the manner of HEC-2/HEC-RAS river sections);
- a **street** cross-section (a curb-and-gutter roadway profile for dual-drainage street modelling); or
- a user-supplied **custom shape** curve of width versus depth.

Conduits carry:

- a Manning roughness coefficient;
- upstream and downstream invert offsets above their end nodes (expressible as heights or, via a global option, absolute elevations);
- an optional count of identical parallel **barrels** (routing solves one barrel at $Q/N$ and scales volumes and losses back by $N$);
- an optional maximum-flow limit;
- optional entrance/exit minor loss coefficients;
- an optional flap gate preventing reverse flow; and
- optional seepage and evaporation losses.

Conduit slope is drop over *horizontal* distance, floored at a 0.001-ft elevation drop (or a user minimum slope); under dynamic wave an adverse-slope conduit is silently reversed internally, with all reported flows carrying a direction multiplier so output keeps the user's orientation. A conduit may also be designated a **culvert** (activating inlet-control capacity limits per FHWA HDS-5) or a **force main** (using Hazen–Williams or Darcy–Weisbach friction while pressurised, §8).

**Pumps** raise water between nodes according to a **pump curve** of five types:

- Type 1 (flow varies stepwise with wet-well volume);
- Type 2 (flow varies stepwise with inlet depth);
- Type 3 (flow varies continuously with delivered head);
- Type 4 (flow varies continuously with inlet depth);
- Type 5 (a variable-speed Type 3);

plus an **ideal** transfer pump whose outflow equals its inflow. Pumps have on/off depth setpoints and may have their speed modulated by control rules.

**Orifices** are openings in the side (**side orifice**) or bottom (**bottom orifice**) of a node's wall or floor, closable to a variable degree by control rules, discharging according to the orifice equation with distinct free, submerged, and partially-open regimes. Orifice geometry is circular or rectangular; an optional flap gate prevents reverse flow.

**Weirs** are overflow structures of five types, each with its characteristic head-discharge exponent and coefficient, with corrections for end contractions, submergence, and surcharge:

- **transverse**;
- **side-flow**;
- **V-notch** (triangular);
- **trapezoidal**; and
- **roadway** (an FHWA HDS-5 embankment-overtopping weir for culvert/roadway systems).

**Outlets** are general-purpose head-discharge devices: their outflow is an arbitrary user-defined function of head or depth, given either as a power function or a rating curve. They model devices with bespoke ratings — vortex valves, flow-duration-control devices — that fit none of the standard structures.

*(A sixth object family new in 5.2 — **streets** and **inlets** — pairs street cross-sections with FHWA HEC-22 inlet capacity calculations to model dual drainage: flow on the street surface, captured by inlets, entering the below-ground sewer. It is treated in §8.)*

### 1.5 Water Quality Objects

**Pollutants** are user-defined constituents (any number) carried by runoff and routed through the conveyance system. Each has a concentration unit (mg/L, µg/L, or counts/L), optional rainfall/groundwater/RDII/dry-weather-flow background concentrations, an initial network concentration, a snow-only buildup flag, a first-order decay coefficient, and optionally a **co-pollutant** relationship (its concentration set as a fixed fraction of another pollutant's).

**Land uses** partition a subcatchment's area into categories (residential, industrial, …) that govern pollutant **buildup** during dry weather and **washoff** during runoff, each by a choice of functional forms (§9), plus street-sweeping parameters that periodically remove accumulated buildup.

### 1.6 Data Objects

**Curves** are tabulated x-y relations, linearly interpolated (except Type 1/2 pump curves, which are read stepwise), serving typed roles: storage (area vs. depth), diversion (diverted flow vs. inflow), tidal (stage vs. hour), pump (per the five pump types), rating (outlet discharge vs. head), control (setting vs. controller variable), shape (conduit width vs. depth), and weir coefficient curves.

**Time series** are timestamped value sequences used for rainfall, outfall stage, external inflows, and evaporation.

**Time patterns** are repeating multiplier sets — monthly, daily, hourly, and weekend-hourly — that modulate dry-weather sanitary inflows.

**Control rules** are IF-THEN-ELSE statements over the simulation state (node depths, link flows, timing, …) with priorities, which switch pumps and adjust regulator settings; actions may be immediate or modulated through PID controllers (§11).

Beyond these, a model references **shared parameter sets** that are neither network elements nor data tables: **transects** and **street sections** (geometry shared by many conduits), **aquifers** (shared by many subcatchments), **snow pack** parameter sets, **unit-hydrograph groups**, and **LID designs** — each defined once and instantiated by reference.

### 1.7 The State-Vector View

SWMM is a distributed discrete-time simulation: at each time step the entire system state advances as $X_t = f(X_{t-1}, I_t, P)$ and outputs are computed as $Y_t = g(X_t, P)$, where $I_t$ are external inputs (precipitation, temperature, boundary stages, control settings) and $P$ the constant parameters. The state vector is remarkably small relative to the model's scope. Per subcatchment: runoff depth $d$, the infiltration state of the chosen method (e.g. cumulative infiltration, Horton-curve position, or moisture deficit), groundwater moisture content and saturated-zone depth, and snow pack depth/free-water/temperature/cold-content. Per conveyance node: water depth $y$. Per conduit: flow rate $q$ and flow area $a$. Per pollutant: surface buildup mass and ponded mass per subcatchment (with the last-swept time), and concentration per node and link. Everything else reported by the engine — velocities, volumes, flooding, loads — is derived from these states, the inputs, and the parameters.

The state vector closely tracks what a **hotstart** file checkpoints — though not exactly: the file also persists node lateral inflows, storage residence times, and regulator settings; stores link depth rather than flow area; and omits LID layer states entirely, so LID antecedent conditions are lost across a hotstart. The state vector likewise defines what initial conditions the user must supply, and marks the seam between the hydrologic and hydraulic halves of the engine, which advance on different clocks (§2).

---

## 2. Simulation Architecture

SWMM advances three clocks with independent step sizes: a **runoff clock** ($\Delta t_{roff}$, itself split into a *wet* step used while any precipitation, snow cover, surface runoff, or non-dry LID unit exists anywhere — a draining LID holds the short step long after rain ceases — and a much longer *dry* step used otherwise), a **routing clock** ($\Delta t_{rout}$, typically far shorter — seconds to a minute under dynamic wave), and a **reporting clock** ($\Delta t_{rpt}$). The main loop per routing step $[T,\ T + \Delta t_{rout}]$:

1. While the runoff clock lags the end of the routing step, compute hydrology for a full runoff step — precipitation, snowmelt, infiltration, evaporation, groundwater, overland flow, buildup/washoff — and advance it.
2. Route flow and quality through the conveyance network over the routing step, using the runoff results (computed on the coarser runoff grid) **linearly interpolated** to routing times as lateral node inflows.
3. If the reporting clock has been reached, interpolate results to the report time and write them to the output file — at most one report time is serviced per routing step.

Interpolation admits exceptions: precipitation, infiltration, and evaporation rates are held piecewise-constant within a runoff step (a report time inside the step receives the step-start value), groundwater elevation and soil moisture are reported at their end-of-step values, and climate-file temperatures are interpolated sinusoidally (§3.1). Both wet and dry runoff steps are truncated to end exactly at the next rainfall-interval boundary of any gage or the next evaporation-change date, so all forcing is constant within a step; a wet step longer than a used time-series-fed gage's recording interval is permanently reduced to that interval with a warning (file-fed gages impose no such reduction). Guidance is wet step ≲ subcatchment time of concentration, dry step of hours to a day.

Three option-driven behaviours modify the routing loop itself:

- An **`[EVENT]`** list restricts routing to date windows: between events the routing step stretches to the next runoff or report time, no lateral inflows are applied and no flow or quality routing occurs (hydrology continues; state freezes); overlapping events are clipped to the next event's start.
- A **steady-state skip** option bypasses flow routing for a step when no control action fired, the previous step's system flow error is within a tolerance, and no node's lateral inflow changed by more than a relative tolerance (a zero↔nonzero change counts as 100%) — quality routing and outflow accounting still run.
- A **rule step** option evaluates control rules only at fixed intervals (the routing step is trimmed to land on them), though pump startup/shutoff depth targets still apply every step.

Lateral inflows are assembled at the *start* of each routing step — runoff, groundwater, and LID drains interpolated to the old routing time; external, dry-weather, and RDII inflows evaluated at the step-start date — with near-zero inflows truncated, and a *negative* external inflow legal and booked as an outflow removing mass at the node's concentration.

The dual-clock design is an economy: hydrology on a 15-minute wet / 1-day dry grid costs a small fraction of the routing effort, and continuous multi-year simulations remain tractable while routing runs at whatever step stability demands (§5). The **state vector** of §1.7 is, up to the small deviations noted there, what is saved to and restored from a **hotstart file**, allowing a simulation to resume from a prior ending condition (e.g. to establish non-zero antecedent conditions before a design storm). Internally, all computation is in feet and seconds regardless of the user's unit system, and dates are 8-byte doubles counting decimal days since 30 December 1899 (the Delphi epoch).

## 3. Hydrology

### 3.1 Meteorology

**Precipitation** enters through rain gages, from user time series or external files (NCDC formats, Environment Canada formats, and a standard user-prepared station format), recorded as intensity, volume, or cumulative volume on a fixed interval. User-supplied data are interpreted as start-of-interval values — end-of-interval records must be shifted back one interval — while NCDC and Canadian files carry end-of-interval stamps that SWMM converts automatically. External files are pre-collated into a binary *rainfall interface file* read during simulation. Radar or gridded rainfall is accommodated by one gage per grid cell or by area-weighting rainfall onto subcatchments.

**Temperature** (needed only for snowmelt and Hargreaves evaporation) comes from a time series (linear interpolation) or a daily climate file of max/min values, which SWMM converts to instantaneous temperatures by **sinusoidal interpolation**: the minimum is assumed at sunrise and the maximum three hours before sunset, with sunrise/sunset computed from solar declination, latitude, and a user-supplied longitude correction in minutes (conventionally four minutes per degree from the standard meridian), and half-sine arcs fitted between successive extremes.

**Evaporation** applies to ponded water, groundwater, channels, storage units, and LIDs, from one of five sources: a constant, monthly averages, a time series (honouring each entry's exact timestamp — values may vary within a day), climate-file daily values, or the **Hargreaves** formula computed from 7-day running averages of climate-file temperatures ($E = 0.0023\,(R_a/\lambda)\,T_r^{1/2}(T_a + 17.8)$ mm/day, with extraterrestrial radiation $R_a$ from latitude and day of year). Climate-file (pan) values are scaled by user monthly pan coefficients (≈0.7), and a `DRY_ONLY` switch suppresses all surface evaporation during rainfall. **Wind speed** — monthly averages (default zero) or climate-file daily values — enters only the rain-on-snow melt equation. Days missing from a climate file inherit the most recent recorded value of each variable.

Beyond the per-method infiltration patterns (§3.3), optional monthly **`[ADJUSTMENTS]`** modify the forcing itself: a *multiplicative* factor on all gage rainfall (also applied during RDII preprocessing) and *additive* offsets to temperature and to potential evaporation, each by calendar month. Per-subcatchment monthly patterns may further scale the pervious sub-area's depression storage and Manning roughness; impervious sub-areas are never adjusted.

### 3.2 Surface Runoff

Each subcatchment sub-area is a **nonlinear reservoir**: an idealised rectangular plane of area $A$, characteristic width $W$, slope $S$, and Manning roughness $n$, holding a ponded depth $d$ with depression storage $d_s$. Mass balance and a Manning wide-channel rating for the outflow give the governing ODE

$$\frac{\partial d}{\partial t} = i - e - f - \alpha\,(d - d_s)^{5/3}, \qquad \alpha = \frac{1.49\,W\sqrt{S}}{A\,n},$$

with $i$ the rainfall/snowmelt input, $e$ surface evaporation, $f$ infiltration (pervious sub-areas only), and outflow zero while $d \le d_s$. The wide-channel assumption sets hydraulic radius equal to $d - d_s$, whence the 5/3 exponent. Each of the three sub-areas (§1.2) integrates its own copy of this ODE — pervious and impervious sub-areas differ in roughness, depression storage, and infiltration, but share $W$ and $S$, with the impervious $\alpha$ prorated so both impervious sub-areas use $W/(A_2{+}A_3)$ — using the same **adaptive fifth-order Runge–Kutta** integrator as the groundwater module. The filling phase up to $d_s$ is handled analytically before the integrator engages. Subcatchment runoff is the area-weighted sum $\sum q_j A_j$. Two area caveats: run-on from upstream subcatchments and outfalls is spread over the **non-LID area only**, and snow packs likewise cover only the non-LID area (LID units receive raw precipitation directly). When snowmelt is simulated, the rain/snow split and catch factor apply gage-wide, so a subcatchment *without* a snow-pack object receives SCF-scaled snowfall as immediate liquid input.

All geometry is lumped into $\alpha$: the model has no internal spatial variation, so the **width parameter** is the primary shape calibration handle ($W \approx$ area / average *maximum* overland-flow length — the flow-path length to the drainage divide — with a skew correction for off-centre drainage; increasing $W$ sharpens and advances the hydrograph). This spatial uniformity is also what makes **re-routing** trivial: a fraction of impervious runoff may be directed onto the pervious sub-area, or (mutually exclusively) a fraction of pervious runoff onto the impervious sub-area *with* depression storage, and a subcatchment's outflow onto another subcatchment — in each case applied like additional rainfall on the receiver, delayed by one time step per hop. Setting $n = 0$ bypasses the nonlinear routing — ponded water above depression storage converts instantly to runoff each step, though depression storage, evaporation, and infiltration still apply — which together with parameter choices lets the method emulate simple runoff-coefficient or SCS-volume models.

### 3.3 Infiltration

Infiltration is computed on the pervious sub-area of each subcatchment by one of **five** methods, selected per subcatchment. All methods share two conventions. First, the actual infiltration rate is the smaller of the potential (capacity) rate and the water available, $f = \min(f_p,\ i_a)$, where the available rate includes ponded water: $i_a = i + d/\Delta t$ with $d$ the current ponded depth (the Curve Number method excepted — since 5.2 it folds run-on into ponded depth only, so run-on infiltrates at the held rate but never advances the event's cumulative-rainfall curve). Second, every method carries a **recovery** model that regenerates capacity during dry weather, so that continuous simulation across many storms is meaningful. Two distinct optional adjustments scale the constants: a monthly **conductivity pattern** (global or per-subcatchment) scales $f_0$, $f_\infty$, and $K_s$ — and the Green–Ampt upper-zone depth $L_u$ by its square root — while a separate monthly **soil-recovery pattern** scales every recovery/regeneration coefficient (and divides the Green–Ampt inter-event timer). The constants below are exact only absent both. Internally all quantities are in feet and seconds.

#### Horton

The classic exponential decay of capacity from an initial rate $f_0$ to an equilibrium rate $f_\infty$ ($\approx K_s$):

$$f_p = f_\infty + (f_0 - f_\infty)e^{-k_d t}$$

with decay coefficient $k_d$ (s⁻¹). SWMM does not evaluate this at wall-clock time: because actual infiltration can be rainfall-limited ($f < f_p$), it tracks an **equivalent time** $t_p$ on the Horton curve such that the curve's cumulative infiltration matches what actually infiltrated. Cumulative capacity is

$$F(t_p) = f_\infty t_p + \frac{f_0 - f_\infty}{k_d}\left(1 - e^{-k_d t_p}\right)$$

and each wet step either advances $t_p$ by $\Delta t$ (when infiltration proceeded at capacity, or the curve has flattened — SWMM treats $t_p > 16/k_d$ as flat) or solves $F(t_p^{old}) + f\,\Delta t = F(t_p^{new})$ for $t_p^{new}$ by Newton–Raphson (when rainfall-limited). An optional cap $F_{max}$ makes the surface impermeable beyond a total infiltrated volume. During dry steps the state recovers along an exponential drying curve with coefficient $k_r$; the wetting- and drying-curve mapping collapses to the closed form

$$t_p \leftarrow -\frac{1}{k_d}\ln\!\left[1 - e^{-k_r\Delta t}\left(1 - e^{-k_d t_p}\right)\right].$$

$k_r$ derives from a user drying time $T_{dry}$ (days) via $k_r = 3.912/T_{dry}$ (98% recovery definition). Recovery is purely empirical, independent of evaporation. When $F_{max}$ is active, the spent volume is tracked and wound back along the recovery curve during dry weather, so capacity under the cap regenerates.

#### Modified Horton

Akan's reformulation replaces elapsed time as the state with **cumulative excess infiltration** $F_e$ — the volume infiltrated above the equilibrium rate — on the argument that only water accumulating near the surface reduces capacity, while $f_\infty$ percolates away harmlessly:

$$f_p = \max\!\left(f_0 - k_d F_e,\ f_\infty\right), \qquad F_e = \sum_i \max(f_i - f_\infty,\ 0)\,\Delta t_i .$$

This behaves better under low-intensity rainfall (plain Horton decays capacity even when little water has actually entered the soil). The scheme is fully explicit — no Newton solve — and dry-period recovery is a simple exponential decay of the state, $F_e \leftarrow F_e\,e^{-k_r \Delta t}$. Its $F_{max}$ handling is idiosyncratic: once active, each wet step sets $F_e \leftarrow \max(F_e, F_{max})$, so infiltration shuts off after a single wet step and only dry-weather decay restores it.

#### Green–Ampt

The Mein–Larson two-stage form of the sharp-wetting-front model. The soil is parameterised by saturated conductivity $K_s$, wetting-front suction head $\psi_s$, and an initial moisture deficit $\theta_d$; the engine adds the current ponded depth $d$ to the suction head throughout. Before the surface saturates, all rainfall infiltrates ($f = i_a$); saturation occurs once cumulative infiltration reaches

$$F_s = \frac{K_s\,(\psi_s + d)\,\theta_d}{i_a - K_s}$$

(defined only while $i_a > K_s$). Thereafter capacity follows

$$f_p = K_s\left(1 + \frac{(\psi_s + d)\,\theta_d}{F}\right),$$

which SWMM integrates over the step in cumulative form — $F_2 = C + \psi_s\theta_d\ln(F_2 + \psi_s\theta_d)$, solved for $F_2$ by Newton–Raphson — avoiding overshoot on long steps; sub-10-second steps with $F > 0.01\,(\psi_s + d)\,\theta_d$ use the explicit point rate instead. Recovery tracks the moisture deficit $\theta_{du}$ of an upper soil zone of fixed thickness $L_u = 4\sqrt{K_s}$ (inches, $K_s$ in in/hr): wet steps deplete the deficit by $f\Delta t/L_u$, dry steps regenerate it at rate $k_r\,\theta_{dmax}$ with $k_r = \sqrt{K_s}/75$ hr⁻¹. After a dry spell longer than $T_r = 0.06/k_r$ a new event begins with $\theta_d = \theta_{du}$ and $F = 0$.

#### Modified Green–Ampt

A fifth selectable method (5.1.010) differing from Green–Ampt in exactly one respect: during low-intensity periods ($i_a \le K_s$) it does **not** reset the event state when the inter-event timer expires, so cumulative infiltration $F$ keeps building through light rain and surface saturation arrives sooner. This is also the variant invoked internally by LID surface layers and storage-node seepage.

#### Curve Number

An incremental adaptation of the SCS/NRCS relation $Q = P^2/(P + S_{max})$ with $S_{max} = 1000/CN - 10$ (inches). SWMM omits the usual initial-abstraction term (depression storage plays that role) and differences the cumulative form: each wet step updates event totals $P$ and $F = P - P^2/(P+S_e)$ and takes $f_p$ as $\Delta F/\Delta t$, where $S_e$ is the storage capacity remaining at the start of the current event. During rainless gaps within an event the previous rate is held so ponded water can continue to infiltrate. Remaining capacity $S$ depletes with infiltrated volume and recovers during dry weather at $k_r S_{max}$ per hour with $k_r = 1/(24\,T_{dry})$; a dry spell longer than $0.06/k_r$ hours starts a new event with $S_e = S$. The curve number itself is clamped to $[10, 99]$. Because tabulated urban curve numbers already lump impervious cover, a CN subcatchment should be modelled as fully pervious.

### 3.4 Groundwater

Each subcatchment may sit on an independent two-zone aquifer: an **unsaturated upper zone** of uniform moisture content $\theta$ and depth $d_U$, above a **saturated lower zone** of depth $d_L$ (the water-table height over the aquifer bottom), with $d_U = E_G - E_B - d_L$ for ground and bottom elevations $E_G, E_B$. The unknowns are $\theta$ and $d_L$. Six volumetric fluxes (per unit area) connect the zones to the surface and the conveyance system: surface infiltration $f_I$ (the §3.3 result scaled by pervious fraction, capped by upper-zone storability), upper-zone evapotranspiration $f_{EU}$, percolation $f_U$, lower-zone ET $f_{EL}$, deep percolation $f_L$, and lateral groundwater discharge $f_G$ to a designated conveyance node.

Moisture accounting reduces to a coupled ODE pair in $(\theta, d_L)$, driven by the zone flux sums $f_{UZ} = f_I - f_{EU} - f_U$ and $f_{LZ} = f_U - f_{EL} - f_L - f_G$, which SWMM integrates over each runoff time step with **adaptive fifth-order Runge–Kutta**, clamping $\theta \in [\theta_{WP}, \phi)$ and $d_L \in [0, E_G - E_B)$ — and jumping the water table to the surface whenever $\theta$ reaches porosity. A subcatchment's `[GROUNDWATER]` line may override the shared aquifer's bottom elevation, initial water table, initial moisture, and node threshold elevation. The key constitutive relations: percolation uses an exponential unsaturated-conductivity model with a finite suction-gradient factor,

$$f_U = K_s\,e^{-(\phi-\theta)HCO}\left(1 + \frac{2\,\psi_{TS}\,(\theta - \theta_{FC})}{d_U}\right)$$

($\psi_{TS}$ the aquifer's tension-slope parameter; zero below field capacity $\theta_{FC}$, capped at $d_U(\theta - \theta_{FC})/\Delta t$ — the manual's simpler $f_U = K_s e^{-(\phi-\theta)HCO}$ omits the gradient factor the code applies); ET is drawn in priority order surface → upper zone → lower zone, the upper-zone share as a user fraction $UEF$ of potential ET (first prorated by the pervious area fraction — the only surface through which subsurface ET is exerted — and optionally rescaled by a per-aquifer monthly pattern) and the lower-zone share declining linearly to zero at a cutoff water-table depth $DEL$, capped by whatever ET remains after the surface and upper-zone draws — with **no subsurface ET at all during steps with surface infiltration**, and no upper-zone ET at or below the wilting point; deep percolation is a linear reservoir $f_L = DP\, d_L/(E_G - E_B)$, capped at $d_L/\Delta t$.

The lateral discharge to the drainage system — the term that generates baseflow and groundwater infiltration into sewers — is the user-configurable power function

$$f_G = A1\,(d_L - h^*)^{B1} \;-\; A2\,(h_{SW} - h^*)^{B2} \;+\; A3\,d_L\,h_{SW}$$

where $h_{SW}$ is the surface-water stage at the receiving node (a fixed value or the live routed stage) and $h^*$ a threshold height defaulting to the node invert. When $d_L \le h^*$ the whole function returns zero — all three terms, so surface water cannot recharge a depleted aquifer through the $A2$ or $A3$ terms; a zero exponent degrades its term to the bare coefficient. Notably, the terms are evaluated in **user length units** and the result converted from user groundwater-flow units, making $A1/A2/A3$ unit-system-dependent whenever the exponents differ from 1 (custom expressions likewise run in user units). Choices of the five coefficients reproduce standard conceptualisations — a linear reservoir ($B1{=}1$, $A2{=}A3{=}0$), Dupuit–Forchheimer seepage, or Hooghoudt tile drainage — and negative $f_G$ (bank storage from channel to aquifer) is admitted when the interaction term is unused. The flux is bounded each step by what the aquifer stores, what the unsaturated zone can accept, and what the node can supply. User-defined expressions may customise both sinks — but asymmetrically: a deep-percolation expression **replaces** $f_L$, while a lateral-flow expression is **added to** the power-function $f_G$ (a pure replacement only if $A1 = A2 = A3 = 0$). The aquifer carries no water quality: infiltrate arrives at the node clean unless a constant concentration is assigned.

Note that the manual states the ODE pair inconsistently — the §5.2 derivation carries $(\phi - \theta)$ denominators where the §5.4 integration sidebar uses $\phi$, and the infiltration-cap formula flips the sign of $f_U$ between statements — and the pinned source implements a **third** form matching neither in full: $\partial d_L/\partial t = f_{LZ}/(\phi - \theta)$ (the §5.2 denominator), but $\partial\theta/\partial t = f_{UZ}/(E_G - E_B - d_L)$ with no $\theta f_{LZ}$ coupling term, and an infiltration cap $(E_G - E_B - d_L)(\phi - \theta)/(F_{perv}\,\Delta t)$ containing no $f_U$ term of either sign. The code's forms, not either manual variant, are SWMM's actual behaviour.

### 3.5 Snowmelt

Snow state is kept as **depth of water equivalent** and simulated per subcatchment on a three-way split that differs from the runoff sub-areas: pervious (SA1), **plowable** impervious (a user fraction $SNN$ of impervious area — streets and lots subject to snow removal, always fully snow-covered), and remaining impervious (SA3, rooftops). Precipitation falls as snow when air temperature $T_a \le SNOTMP$, with gage snowfall scaled by a snow catch factor $SCF$ to correct wind under-catch.

**Melt** is computed per surface by two regimes. During rain ($i > 0.02$ in/hr), Anderson's energy-budget equation for saturated, radiation-free conditions applies:

$$SMELT = \left(0.001167 + 7.5\gamma U_A + 0.007\,i\right)(T_a - 32) + 8.5\,U_A(e_a - 0.18)$$

(in/hr; $U_A = 0.006\,u$ for wind speed $u$ in mph; $\gamma = 0.000359\,p_a$ with atmospheric pressure $p_a$ computed from the site's average elevation; $e_a$ saturation vapour pressure at $T_a$). Otherwise, when $T_a \ge T_{base}$, a degree-day law $SMELT = DHM\,(T_a - T_{base})$ applies, with the melt coefficient varying sinusoidally through the year between a December 21 minimum $DHMIN$ and June 21 maximum $DHMAX$; below $T_{base}$ no melt occurs and the step instead updates the cold-content account. Street de-icing is represented by lowering a surface's $T_{base}$ rather than by explicit chemistry.

Two mechanisms delay and shape melt. A **cold content** account (heat deficit, in water-equivalent inches) must be paid off before any liquid melt leaves: its antecedent temperature index snaps to the air temperature during snowfall heavier than 0.02 in/hr, otherwise relaxes toward it with the $TIPM$ weight rescaled from its 6-hour basis to the time step ($1 - (1-TIPM)^{\Delta t/6\mathrm{hr}}$), is capped at the base temperature, and the deficit itself is bounded by an assumed snow specific heat of 0.007 in w.e. per °F per inch of pack; the negative-melt ratio $RNM$ scales the cold-content exchange rate. A **free-water reservoir** additionally requires the pack's liquid-holding capacity ($FWFRAC \times$ pack depth) to fill before runoff releases. Partial snow cover on SA1 and SA3 is handled by **areal depletion curves** (fraction of area snow-covered vs. relative pack depth $WSNOW/SI$), two watershed-wide curves with Anderson's temporary-linear-curve adjustment after fresh snowfall on partial cover — 100% cover is assumed until 25% of the new snow melts ($SBWS = AWE + 0.75\,SNO/SI$); melt and cold-content exchange scale by the covered fraction $ASC$. Once the plowable surface's depth reaches a trigger $WEPLOW$, its **entire current depth** is redistributed by five constant fractions — to the other sub-areas, to another subcatchment, out of the system, or to immediate melt.

The net result per surface, $RI = ASC \cdot SMELT + (1-ASC)\,i$ plus any immediate melt, replaces gage rainfall as the input to infiltration and overland flow; the two impervious sub-area results are area-averaged onto the runoff model's impervious sub-areas, while the pervious result carries over directly. When a pack thins below 0.001 in it is flushed as immediate melt. Snow is assumed not to alter infiltration or surface roughness.

### 3.6 Rainfall-Dependent Inflow/Infiltration (RDII)

RDII — stormwater entering sanitary and combined sewers through defects and illicit connections — is modelled independently of the runoff/groundwater machinery, as a rainfall-convolved inflow at designated nodes. The kernel is the **RTK triangular unit hydrograph**: $R$ the fraction of rainfall volume entering the sewer, $T$ the time to peak, $K$ the recession-to-peak ratio (base $= T + KT$, peak ordinate $Q_{peak} = 2R/(T+KT)$ per unit area). Because observed RDII responses are multi-modal, each **unit-hydrograph group** sums up to three triangles of increasing duration — rapid inflow, mixed, slow infiltration — and each group may vary by calendar month. Each triangle carries an **initial abstraction** account ($IA_{max}$, initial depletion $IA_0$, recovery rate $IA_r$) that absorbs rainfall before convolution and regenerates in dry weather.

RDII flows are computed for the whole simulation **before routing begins** and written to an interface file (unless an `IGNORE_RDII` option suppresses the subsystem): per node, gage rainfall — with any monthly rainfall adjustment applied — is sampled onto a processing grid set to the minimum of the wet runoff step and the shortest rising or falling limb across all months and all three unit hydrographs, depleted by initial abstraction, and convolved; results are emitted at the wet runoff step and held piecewise-constant during routing, with flows below 0.0001 cfs zeroed. Monthly parameters are selected by the month each rainfall increment *fell in*, not the month of the response. The per-area result is scaled by a user **sewershed area** (which need not correspond to any subcatchment — RDII-only models are common). Each month's three $R$ values must individually be non-negative and sum to at most 1. The R-T-K parameters have no meaningful defaults; they are calibrated against flow-monitor records with dry-weather flow subtracted.

## 4. Flow Routing Theory

Conveyance routing solves the one-dimensional **Saint-Venant equations** — continuity and momentum for gradually-varied unsteady free-surface flow —

$$\frac{\partial A}{\partial t} + \frac{\partial Q}{\partial x} = 0, \qquad
\frac{\partial Q}{\partial t} + \frac{\partial (Q^2/A)}{\partial x} + gA\frac{\partial H}{\partial x} + gA\,S_f = 0,$$

with $A$ flow area, $Q$ flow, $H = Z + Y$ hydraulic head, and friction slope from Manning: $S_f = (n/1.486)^2\,Q|U|\,/\,(A R^{4/3})$ (the $|U|$ making friction oppose the flow direction). SWMM offers three levels of approximation:

- **Steady flow routing** simply translates each conduit's inflow hydrograph to its outlet within the step — no storage, delay, or attenuation, though evaporation/seepage losses are first subtracted — with flow area back-computed from the Manning rating and flow capped at conduit capacity. It shares kinematic wave's topology restrictions and serves for screening and preliminary sizing.
- **Kinematic wave** keeps continuity but reduces momentum to $S_0 = S_f$: flow is always at Manning normal depth, $Q = \beta\,\Psi(A)$ with $\beta = 1.486\sqrt{S_0}/n$ and section factor $\Psi = A R^{2/3}$. Hydrographs translate *and attenuate* through conduit storage, but backwater, reversal, pressurisation, and entrance/exit losses are unrepresentable. A conduit's accepted inflow is capped at its full-flow capacity, the rejected excess remaining at the upstream node as flooding or ponding, and any node with storage limits its outflow to inflow plus stored volume per step. The network must be a directed acyclic graph with junctions limited to one outlet **link** of any type (storage nodes exempt — they may have several), no adverse-slope conduits (a user minimum slope rectifies them here, with a warning), and regulators permitted only as outlets of storage nodes — which also always discharge freely, taking the upstream node's own invert as tailwater, never submerged; dividers function only here, splitting flow by their §1.3 rules. Non-storage node depths are reconstructed as the maximum over connecting conduits of end depth plus offset, capped at full depth.
- **Dynamic wave** solves the full pair over the general network graph — loops, multiple outfalls, backwater, reverse flow, surcharge — and is the production method (§5). Its own topology demands: at least one outfall must exist, and a dummy conduit or ideal pump must be the sole link leaving its upstream node (with no dummy link leaving a node fed *only* by dummy links or ideal pumps, and no storage node a dummy outflow).

At validation SWMM silently normalises geometry that would otherwise be inconsistent: a node's maximum depth is raised (with a warning) to the crown of its highest connecting link, and a regulator whose crest sits below its downstream node's invert has the crest raised to that invert under dynamic wave.

Kinematic wave's numerical scheme is a weighted implicit (Wendroff) four-point difference of continuity over each conduit, with both space and time weights fixed at **0.6** — unconditionally stable for weights above 0.5, so no Courant restriction applies. Conduits are processed in topological order; at each, the known upstream flow yields a scalar nonlinear equation in downstream area, $\beta\Psi(A_2) + C_1 A_2 + C_2 = 0$, solved by bracketed Newton–Raphson to 0.1% of full area. Storage nodes (in both steady and kinematic modes) iterate a trapezoidal mass balance against their head-dependent outflow rating with under-relaxation 0.55 and tolerance 0.005 ft, executing at most 9 balance passes (a 10-cap loop counting from 1). Junction flooding sheds any net inflow surplus as overflow (optionally banked as a ponded volume re-injected as capacity recovers).

## 5. Dynamic Wave Analysis

The dynamic-wave engine is a staggered node-link scheme: conduits carry the momentum equation for flow, nodes carry continuity for head, and the two are advanced together by fixed-point (Picard) iteration within each time step.

### 5.1 Conduit Flow Update

Substituting continuity into momentum and discretising over a conduit of length $L$ (implicit backward Euler in time, end-difference in space, overbars denoting conduit-average values) gives the update SWMM actually computes:

$$Q^{t+\Delta t} = \frac{Q^t + \Delta Q_{inertia} + \Delta Q_{pressure}}{1 + \Delta Q_{friction} + \Delta Q_{losses}}$$

$$\Delta Q_{inertia} = \sigma\left[2\bar U(\bar A^{t+\Delta t} - \bar A^{t}) + \bar U^2\frac{(A_2 - A_1)\Delta t}{L}\right],\quad
\Delta Q_{pressure} = -g\bar A\frac{(H_2 - H_1)\Delta t}{L},\quad
\Delta Q_{friction} = \frac{g\,(n/1.486)^2\,|\bar U|\,\Delta t}{\bar R^{4/3}}$$

Friction (and entrance/exit/average local losses, treated likewise) sit in the denominator — an implicit linearisation that keeps the update stable as flows approach zero. The **inertial damping factor** $\sigma$ scales the inertial terms by the Froude number ($\sigma = 1$ for $Fr \le 0.5$, tapering linearly to $0$ at $Fr = 1$), suppressing the terms that destabilise trans- and supercritical flow; user options force $\sigma = 1$ (keep all inertia) or $\sigma = 0$ (the local-inertial formulation — distinct from the diffusion wave, which also drops $\partial Q/\partial t$), and closed conduits flowing full always use $\sigma = 0$. The **upstream weighting** of the pressure/friction areas is this same Froude-based $\sigma$ (computed before any user damping override): none at $Fr \le 0.5$, fully upstream at $Fr \ge 1$, applied only in positive, non-full, downstream-sloping flow.

Several limits then constrain the updated flow:

- The velocity used in forming the momentum terms (not the resulting flow) is capped at 50 ft/s.
- A flow that reverses sign between successive iterates is clamped to 0.001 cfs in the new direction.
- Flow out of an essentially dry node is suppressed.
- A user conduit flow limit, when given, caps $|Q|$ every iteration.
- A positive computed flow is limited to Manning **normal flow** when the water-surface slope is *less* than the bed slope (upstream depth below downstream) or the upstream Froude number is at least 1 — user-selectable criteria (slope, Froude, both, or *neither*, disabling the limit entirely), except that conduits adjoining an outfall always apply the slope test and never the Froude test; the check is skipped for full upstream ends, critical/dry flow classes, and culvert-coded conduits.

Special flow classes at nearly-dry or critical-depth ends substitute critical/normal depth for the nodal head on the affected end — with a linear *fasnh* ramp of the downstream area contribution across the band between critical and normal depth — and a conduit classed dry at both ends (or at either end alone) carries exactly zero flow for the trial while retaining a nominal $\partial Q/\partial H$. Multi-barrel conduits solve one barrel and scale back (§1.4).

### 5.2 Node Head Update

Each node integrates $\partial H/\partial t = \sum Q / A_S$, where the surface area $A_S$ sums the node's own storage area (zero for junctions) and each connecting conduit's contribution — nominally the trapezoidal area of the conduit's *adjacent half* (width-weighted, not half the total), but reapportioned by flow class: a free-fall (critical) end contributes zero with the far node taking area over the full length, a dry end contributes only absent an offset, and closed-conduit top widths are frozen at 96% of full depth (98.53% under the slot) so surface areas never collapse at the crown. Weirs and outlets contribute no surface area; orifices contribute half their surface area — the equivalent-pipe water surface for a side orifice, the bare opening area for a bottom one — to each end (dropped at storage-node and critical-class ends). The total is floored at a user-adjustable default minimum of 12.566 ft² (a 4-ft manhole). The volume change uses trapezoidal averaging of the net inflow across the step.

### 5.3 Picard Iteration

Within a time step, each trial runs in two phases: all true conduits solve from the last-iteration heads (an OpenMP-parallel loop under the `THREADS` option — nodal accumulation stays serial, so results are thread-count-invariant), then dummy conduits, pumps, and regulators solve *serially in link-definition order*, each immediately updating its node flows — so a pump's available-volume clamp sees the accumulation so far, making pump/regulator results sensitive to link order. Flows are under-relaxed by $\theta = 0.5$ against the previous iterate (pumps exempt); node heads update and under-relax by 0.5 (skipped for surcharged nodes); iteration continues until every non-outfall node's head change is below the head tolerance (default **0.005 ft**) — outfalls are excluded, their depths reset from the boundary condition each pass — with a hard minimum of 2 trials and a user-adjustable maximum defaulting to **8** (links between converged nodes are bypassed). Non-convergence is tallied and reported but does not halt the simulation.

### 5.4 Surcharge

A non-storage node is surcharged when its head exceeds the crown of its highest connecting **link** — orifice and weir opening tops participate, not conduits alone; closed storage nodes surcharge only when a supplementary surcharge depth is specified and the full depth exceeded, and ponded nodes never do. Two treatments exist:

- The classic **EXTRAN point iteration**: with no free surface, continuity degenerates to $\sum Q = 0$, and the head correction is the Newton step $\Delta H = -\sum Q\,/\,\sum(\partial Q/\partial H)$ over connecting links, where $\partial Q/\partial H$ falls out of the flow-update denominator. SWMM 5.2 blends this smoothly with the free-surface update over a transition zone up to 25% above the crown (an exponential weighting $e^{-15 f_H}$ of the not-surcharged surface area against $\sum \partial Q/\partial H$), and damps the correction to 0.6 at terminal upstream nodes.
- The **Preissmann slot** method (an optional `SURCHARGE_METHOD` introduced in 5.1.013 — **EXTRAN remains the default**): closed conduits acquire a narrow hypothetical slot above the crown — width $0.5423\,e^{-(y/y_{full})^{2.4}}$ of the maximum width, floored at 1% (Sjöberg's formula) — so depth may exceed the crown and the ordinary free-surface equations remain valid everywhere; hydraulic radius freezes at its full-pipe value, and the special surcharge branch is never taken.

### 5.5 Flooding and Ponding

A non-ponded node whose head would exceed its ground (plus optional surcharge depth) is pinned there and the surplus inflow is lost as reported flooding; with ponding enabled, the surplus accumulates in a user-specified ponded area atop the node — a virtual storage whose head may rise above ground and which drains back as the system recovers.

### 5.6 Time-Step Control

With a variable step enabled (the 5.2.4 default — Courant factor 0.75, with `PARTIAL` inertial damping the companion default), the step is the minimum over conduits of the Courant time $\frac{L}{|U| + \sqrt{gA/W}}$ (expressed via $Fr/(1{+}Fr)$ and scaled by the user's Courant factor; conduits with $Fr \le 0.01$ or negligible flow exempt) and over nodes of the time to change head by a quarter of the crown height at the recent rate (outfalls, near-dry, and surcharged nodes exempt), floored at a minimum step, quantised *down* to a whole millisecond, and starting the run at the minimum step. Because the scheme is iterative and semi-implicit, Courant factors above 1 are usable. The optional **conduit lengthening** transform trades short conduits for stability: $L' = \max[L,\ \Delta t(\sqrt{g y_{full}} + U_{full})]$ with slope rescaled by $L/L'$ and roughness by $\sqrt{L/L'}$ — preserving the conveyance factor $\beta$ exactly (the manual's own $\sqrt{}$-rescale of slope contradicts the code). The rule of thumb for stability is $\Delta t \approx L/\sqrt{g\,y_{full}}$; the standard diagnostics are the continuity error, a per-link flow-instability index, and a capacity-limited flag raised when a conduit's upstream end is full with HGL slope exceeding the bed slope (§12).

### 5.7 Initial Conditions

Default zero depths and flows; user-supplied initial conduit flows imply Manning normal depth; node depths without user values are seeded from connecting-link depths plus offsets; a hotstart file bypasses these depth-seeding heuristics (derived volumes and areas are still computed from its state).

## 6. Cross-Section Geometry

### 6.1 Shape Families

Every conduit shape must supply a consistent family of geometric functions — area $A(Y)$, top width $W(Y)$, hydraulic radius $R(Y)$, the inverses $Y(A)$ and $A(\Psi)$, and the **section factor** $\Psi(A) = A\,R(A)^{2/3}$ with its derivative — because the routing methods consume geometry only through these. Three implementation families cover the shape library:

- **Closed-form shapes** (rectangular, trapezoidal, triangular, parabolic, power-law) use analytic formulas, with $\Psi'(A) = (\tfrac{5}{3} - \tfrac{2}{3}P'R)R^{2/3}$ where derivable and central differences ($\Delta A = 0.001 A_{full}$) otherwise. An open rectangle may declare one or both side walls frictionless ("shared"), removing them from the wetted perimeter.
- **Tabulated shapes** (circular; ellipsoid and arch — in 23/102 standard US sizes stored in inches, or with arbitrary user axes under fixed proportionality constants; the seven legacy masonry sewer shapes — basket-handle, catenary, egg, gothic, horseshoe, semi-circular, semi-elliptical) interpolate normalised property tables (all five circular tables are 51-entry; egg, horseshoe, basket-handle, the ellipses, and arch use 26-entry area/radius/width tables; gothic, catenary, semi-elliptical, and semi-circular carry only 21-entry width tables, recovering area by inverse lookup on 51-entry depth tables and hydraulic radius from 51-entry section-factor tables) scaled by full-flow normalisers such as $A_{full} = 0.7854\,Y_{full}^2$, $R_{full} = 0.25\,Y_{full}$ for circles. Table lookups are linear except over the two lowest depth segments, where **quadratic** interpolation applies (with a linear fallback) — near-empty geometry of every tabulated shape depends on it; near-empty circular sections switch entirely to analytic formulas via Newton iteration on the subtended angle.
- **Composite and user shapes**: sediment-filled circular, rectangular-triangular, rectangular-round, and modified basket-handle piece together the primitives (bottom/top radii smaller than half the width are silently raised to it); **custom shapes** integrate a user width-vs-depth curve into 51-point tables — the curve describes a *unit-height* shape scaled by the conduit's full depth, is anchored at (0,0), truncated above unit height or extended at its last width, and produces a **closed** section (both the bottom and the closing top width count as wetted perimeter; $A_{max} = 0.96\,A_{full}$); and **transects** (below) do the same from surveyed geometry.

Closed shapes embed a critical subtlety: the section factor **peaks below full depth** (e.g. at 97% area for rectangles, 0.9756 for circles), meaning Manning flow at ~94% depth ($0.938\,Y_{full}$) exceeds full-pipe flow. SWMM stores $\Psi_{max}$ and the area at which it occurs, interpolating the non-monotone tail linearly, and the inverse $A(\Psi)$ handles the two-branch ambiguity by bracketed lookup or Newton–bisection to 0.01% of $A_{full}$.

### 6.2 Transects

Transects represent natural channels by station-elevation pairs (up to 1,500 stations; an X1-line multiplier and offset can rescale the survey) with distinct left-overbank, main-channel, and right-overbank Manning coefficients — an omitted overbank $n$ defaults to the channel's, and NC-line values persist as defaults into subsequent transects. Preprocessing appends vertical end walls at both ends (contributing wetted perimeter) and builds the 51-point tables by sweeping depth: each depth accumulates area, width, and wetted perimeter segment-by-segment, with composite roughness handled through **conveyance summation** — a new conveyance segment starts at each bank-roughness change *and* wherever the ground re-emerges above the water line (multi-thread sections sum correctly), each contributing $K_i = (1.486/n_i)A_i R_i^{2/3}$ — and the table's hydraulic-radius entry back-computes an effective $R$ from total conveyance as $R = (n_C K / 1.49 A)^{3/2}$. (The forward conveyance uses 1.486 but the back-computation 1.49 — an internal inconsistency of the code itself.) A meander modifier substitutes the shorter overbank (valley) length for the meandering main-channel length as the conduit's effective length, inflating main-channel roughness by the modifier's square root to preserve friction loss. Street cross-sections (§8) compile to transects through the same machinery.

### 6.3 Storage Geometry and Characteristic Depths

Storage geometry integrates the surface-area description into volume: functional curves $A = c_0 + c_1 Y^{c_2}$ integrate analytically; tabular curves trapezoidal-integrate, with depth-from-volume solved analytically per segment (or by Newton–bisection for the functional form). Below a tabular curve's first point, area is assumed to grow linearly from zero ($V = \tfrac{a_1}{2 y_1}y^2$); above its last, area extrapolates along the final segment's slope — the regimes governing shallow and overfull storage. Curve lookups made through the extrapolating table reader — storage surface-area, custom-inlet capture, and exfiltration bottom-area curves — behave similarly: below the first point they extrapolate proportionally through the origin, above the last along the final slope; outlet rating, pump, and weir-coefficient curves instead clamp to their end values. **Critical depth** — needed at free outfalls and free-fall discontinuities — uses exact formulas where they exist (rectangular, triangular, parabolic, power-law) and otherwise interval enumeration (25 fixed depth intervals with linear interpolation) or Ridder's method on $A^3/W = Q^2/g$ to 0.001 ft, seeded by a circular-pipe approximation. **Normal depth** inverts the section factor: $Y_N = Y(A(\Psi = Q\,n/1.486\sqrt{S_0}))$.

## 7. Pumps and Flow Regulators

### 7.1 Pumps

Pumps are links whose flow comes from a user curve, in five types plus one degenerate: Type 1 (stepwise flow vs. inlet wet-well **volume**), Type 2 (stepwise vs. inlet **depth**), Type 3 (continuous head-difference vs. flow — the centrifugal characteristic), Type 4 (continuous flow vs. inlet depth, a variable-speed in-line profile), Type 5 (a variable-speed Type 3), and the **ideal** pump (outflow ≡ inflow; must be its node's only outlet). The curve flow scales by a speed setting $\omega$, driven by startup/shutoff wet-well depths or control rules; at storage inlet nodes (and the virtual wet well a Type 1 pump receives at a non-storage node) flow is clamped so the node cannot be drawn below empty ($Q \le Q_{in} + V_N/\Delta t$), while Type 2–4 pumps at non-storage nodes fall back to $Q = Q_{in}$ when the projected end-of-step depth would go negative (Type 5 is omitted from this check); pumps contribute no surface area to their nodes, and reverse flow is never allowed. Energy is tallied as $0.7457\,\Delta H\,Q\,\Delta t/3600/8.814$ kWh (no efficiency factor).

### 7.2 Orifices

Orifices (side or bottom, circular or rectangular, coefficient $C_d$, optional flap gate) discharge by Torricelli:

$$Q = C_d A_O \sqrt{2gH_e}$$

where $C_d$ is the discharge coefficient, $A_O$ the opening area, and $H_e$ the effective head, which switches between free-discharge (head above opening centre/invert) and differential (submerged tailwater) regimes. An **unsubmerged inlet** degrades smoothly to weir behaviour: below a threshold head the flow follows

$$Q = C_W L (H_1 - Z_O)^{1.5}$$

where $H_1$ is the upstream head and $Z_O$ the opening elevation, with $C_W L$ matched to the orifice equation at the threshold (side) or $C_W = 3.33$ with perimeter-based $L$ (bottom), plus a Villemonte submergence factor $[1 - ((H_2 - Z_O)/(H_1 - Z_O))^{1.5}]^{0.385}$ on the heads above the crest. A partially-open setting $\omega$ (sluice-gate fraction, optionally slewing at a user open/close rate) re-computes the opening area from the §6 geometry. Flap gates charge the Armco head loss $\Delta H = (4U^2/g)\,e^{-1.15 U/\sqrt{H_e}}$, subtracted and re-solved. Under dynamic wave an orifice masquerades as an equivalent short pipe, contributing surface area to its end nodes and analytic $\partial Q/\partial H$ ($0.5\,Q/H_e$ submerged, $1.5\,Q/(H_1 - Z_O)$ as a weir) to the surcharge update.

### 7.3 Weirs

Weirs come as transverse rectangular, V-notch, trapezoidal (sum of both parts), and side-flow (Engels, reverting to the transverse form under reverse flow), with head-discharge relations:

$$Q = C_W L_e H_e^{3/2} \qquad \text{(transverse rectangular)}$$

$$Q = C_W \tan(\theta/2)H_e^{5/2} \qquad \text{(V-notch)}$$

$$Q = C_W L_e^{0.83} H_e^{1.67} \qquad \text{(side-flow, Engels)}$$

where $C_W$ is the weir discharge coefficient, $L_e$ the effective crest length, $H_e$ the effective head, and $\theta$ the notch angle. Effective crest length subtracts end contractions ($L_e = L - 0.1\,n_c H_e$); a partially-raised crest ($\omega < 1$) turns a V-notch into a trapezoid; submergence applies Villemonte with the type's own head exponent. Weirs default to *surchargeable* (roadway weirs excepted): above the opening they switch to an equivalent-orifice form $Q = C_O\sqrt{H_e}$ (coefficient matched to the weir equation at the opening top), while a weir with surcharging disabled simply caps the head at its opening height and continues weir-equation flow. Under steady/kinematic routing, all regulators discharge freely — head is always computed against the upstream node's invert as tailwater, never submerged. $\partial Q/\partial H$ is the analytic exponent-scaled ratio per type.

### 7.4 Outlets

Outlets are the catch-all: flow from a power function $Q = aH_e^b$ or tabulated rating curve of either upstream depth or head difference, scaled by the setting, with flap-gate reversal blocking — the vehicle for vortex valves and other bespoke devices.

## 8. Advanced Hydraulics

### 8.1 Conduit Evaporation and Seepage

Conduit evaporation and seepage are uniformly-distributed lateral losses: $q_E = e_t W(\bar Y)$ (open channels) and $q_S = s f_c W(\bar Y)$ (seepage, with monthly adjustment $f_c$ and the width capped at the depth of maximum width, since seepage is vertical), together bounded by the conduit volume per step (dynamic wave) or the flow magnitude (steady/kinematic). The momentum equation itself gains Strelkoff's lateral-outflow term $-\bar U q_L/2$, which after substituting continuity becomes a $+2.5\,\bar U q_L$ term in the dynamic-wave flow-update numerator; the lost volume debits the appropriate node; kinematic wave adds $q_L L/\phi$ into its $C_2$ constant. **Storage units** evaporate at the potential rate times a user-supplied realisation fraction $f_E$ (1 normally, 0 for roofed units), applied to the start-of-step surface area, and seep by a Green–Ampt formulation with ponded depth added to the suction head, applied separately to bottom and sloped-side areas.

### 8.2 Minor Losses and Force Mains

**Minor losses** (entrance, exit, average, with velocities evaluated at the respective locations) enter the flow-update denominator as $\frac{\Delta t}{2L}\sum K_{m,i}|U_i|$ — dynamic wave only. **Force mains** (circular, dynamic wave) swap the Manning friction term for Hazen–Williams ($\Delta Q_{friction} = 0.6g|\bar U|^{0.852}\Delta t / C_{HW}^{1.852}R_{full}^{1.1667}$ — note the 7/6 hydraulic-radius exponent) or Darcy–Weisbach ($f|\bar U|\Delta t/8R_{full}$, with Swamee–Jain $f$, laminar $64/Re$ below 2000 with $Re$ floored at 10, a linear blend from $f = 0.032$ between 2000 and 4000, and the fully-rough form above $Re = 10^{10}$) **only while pressurised**; partly-full flow uses an equivalent Manning $n$ (slope-dependent for Hazen–Williams: $n = 1.067\,C^{-1}(D/S_0)^{0.04}$). Force mains also carry their own conduit-lengthening compensation (dividing friction by the length factor rather than rescaling $n$), and the force-main cross-section's section factor uses the Hazen–Williams exponent $A R^{0.63}$ instead of Manning's $R^{2/3}$ — altering its normal-flow limit.

### 8.3 Culverts and Roadway Weirs

**Culverts** designated by an FHWA HDS-5 code get an **inlet-control** capacity check layered on the ordinary dynamic-wave (outlet-control) solution: unsubmerged flow from either the form-1 critical-energy equation (Ridder's method on critical depth) or the form-2 power law, submerged flow from the quadratic HDS-5 relation, a linear transition between (SWMM places the unsubmerged limit at $H_1 < Z_1 + 0.95\,Y_{full}$), and the smaller of the two flows governs. Each relation carries a slope-correction term $S_{cf}S_O$ ($-0.5\,S_O$ standard, $+0.7\,S_O$ mitered per HDS-5 — though the pinned code applies $+7.0\,S_O$ for mitered inlets, a 10× divergence from the published coefficient). The constants come from a compiled table of **57 inlet configurations** storing the HDS-5 Table H-2 values (form, $K$, $M$, $c$, $Y$), of which codes 5, 37, and 46 are the mitered ones; submergence begins at $y = Y_{full}(16c + Y - S_{cf}S_O)$, fixing the transition band's upper bound. **Roadway weirs** apply the FHWA head-dependent coefficient only when both a road width and surface type are given (otherwise the user's constant $C_D$); the "charts" are small digitised piecewise-linear tables — low-head coefficients looked up against *absolute head in feet* below $h/W_{road} = 0.15$ and against the ratio above, with submergence factors bottoming at 0.40 (paved) / 0.24 (gravel); they are typically paired in parallel with a culvert to model embankment overtopping.

### 8.4 Streets and Inlets

Streets and inlets (new in 5.2) implement HEC-22 dual drainage. A street section — crown width, curb height, cross slope, optional depressed gutter and backing, and a one- or two-sided flag (default two; approach flow halves and capture doubles per side) — compiles into a §6 transect, so street conduits route like any channel. **Inlet designs** comprise: grates (seven standard types with open-area ratios and splash-over velocity fits, plus a generic type with user-supplied values), curb openings with three throat geometries, slotted drains, drop grates/curbs, custom-curve inlets, and the implicit **combination inlet** formed when one design defines both a grate and a curb. Placement rules are shape-checked: street inlets (grate/curb/combo/slotted) belong only in street cross-sections, **drop inlets only in rectangular or trapezoidal open channels**, custom inlets anywhere with a diversion or rating curve; an invalid placement is removed with a warning, and a conduit holds at most one inlet usage (a second definition overwrites). Each usage line adds modifiers: a replicate count (on-grade replicates evaluate *sequentially*, each seeing the previous one's bypass; on-sag they multiply), a clogging percentage scaling both capture and the open area used for backflow apportioning, a per-inlet flow cap, and a local gutter depression added to the street's continuous one. `AUTOMATIC` placement resolves to on-grade when the bypass node has an outgoing link, else on-sag; drop-curb inlets always compute in depth-driven (on-sag) mode capped by approach flow, and custom inlets ignore placement entirely (diversion curve = flow-driven, rating curve = depth-driven).

#### On-Grade Capture

On-grade capture starts from the gutter-spread relation

$$Q = \frac{0.56}{n}\sqrt{S_L}\,S_x^{1.67}\,T^{2.67}$$

where $n$ is the gutter's Manning roughness, $S_L$ the longitudinal slope, $S_x$ the cross slope, and $T$ the spread — composite-gutter corrected via the frontal-flow ratio $E_o$. Grates apply a frontal efficiency (above splash-over)

$$R_f = 1 - 0.09(V - V_o)$$

and a side efficiency

$$R_s = [1 + 0.15V^{1.8}/S_xL^{2.3}]^{-1}$$

where $V$ is the gutter velocity, $V_o$ the splash-over velocity, and $L$ the grate length. Curb openings use the equivalent slope $S_e = S_x + (a/W)E_o$, the full-capture length

$$L_T = 0.6\,Q^{0.42}S_L^{0.3}(nS_e)^{-0.6}$$

and the efficiency

$$E = 1 - (1 - L/L_T)^{1.8}$$

where $L$ here is the curb-opening length. On-grade slotted drains are treated as curb openings of equal length; combination inlets capture through the curb "sweeper" (curb length beyond the grate) first, then the grate on the remainder at recomputed spread.

#### On-Sag Capture

On-sag capture is weir flow at shallow depth, orifice flow at depth — curb openings *linearly interpolating* across the transition band, grates and slotted drains switching outright at their equal-flow depths (no discontinuity either way): grate weir $3.0\,P\,d^{1.5}$ ($P = L_g + 2W_g$; full perimeter for drop grates) switching at $d = 1.79\,A_o/P$ to orifice $0.67\,A_o\sqrt{2gd}$; curb weir $3.0\,L\,d^{1.5}$ (or $2.3(L + 1.8W)d^{1.5}$ with crest at $h + a$ when depressed and no longer than 12 ft) to orifice $0.67\,hL\sqrt{2g\,d_{eff}}$ above $d = 1.4h$, with throat-angle head corrections; slotted weir $2.48\,L\,d^{1.5}$ to orifice $0.8\,Lw\sqrt{2gd}$ at $d = 2.587w$; a combination adds curb-orifice flow over the grate length once the grate is in orifice mode. (The inlet equations use HEC-22's $g = 32.16$ ft/s², not the engine's 32.2.)

#### Capture Transfer and Statistics

Captured flow transfers from the street conduit's downstream (bypass) node to the sewer capture node each routing step, carrying pollutant mass at the bypass node's previous-step concentration; sewer surcharge returns as backflow at the capture node's concentration, apportioned by open-area ratio among standard inlets (by count among custom ones) sharing the node — flooding that stays inside the model, with continuity accounting corrected accordingly. Under steady/kinematic routing, on-sag capture is additionally limited to the inlet's share of bypass-node inflow plus stored volume per step. One structural caveat: the gutter-spread factor is cached at validation from the conduit's *bed* slope under a normal-flow assumption, so on-grade capture is insensitive to dynamic-wave backwater — though the reported maximum street spread *is* depth-based and does reflect it. Per-inlet statistics (flow/capture/backflow period counts after report start, capture efficiency at peak approach flow, average efficiency, bypass/backflow frequencies, peak flows) feed the street-flow summary, which lists every street conduit with or without an inlet.

## 9. Water Quality

### 9.1 Pollutants and Sources

A **pollutant** is any constituent expressible as an additive concentration (mass or organism counts per volume) — which deliberately excludes pH, conductivity, turbidity, and colour. Each carries optional background concentrations in rainfall, groundwater, RDII, and dry-weather flow; a first-order decay coefficient (days⁻¹) active in the conveyance system — which may be *negative* to model growth, though growth is inert on the steady-flow path; a snow-only buildup flag (de-icing chemicals); and an optional **co-pollutant** relation $C_{total,i} = C_i + f_{ij}C_j$ — the HSPF-style potency factor, applying to buildup/washoff loads only, with $f_{ij}$ free to exceed 1 since it bridges the two constituents' units. The complete source inventory: direct wet deposition (a constant rain concentration applied to the *precipitation* volume and mixed into the ponded store — despite the manual's legacy §2.4 wording, it is neither runoff-rate-scaled nor a concentration floor), surface buildup/washoff by land use, groundwater and RDII at constant concentrations, pattern-modulated dry-weather flow, and external time series (which may specify mass loads directly, needing no flow).

### 9.2 Buildup and Street Sweeping

**Land uses** partition each subcatchment purely for quality: each (pollutant, land use) pair owns one buildup and one washoff function. Buildup $b$ (mass per area or per curb length) grows with dry time $t$ by one of three forms — power $\min(B_{max}, K_B t^{N_B})$, exponential $B_{max}(1 - e^{-K_B t})$, or saturation $B_{max}t/(K_B + t)$ — but the true state is the accumulated **mass**: each dry step inverts the function to find the equivalent time for the current mass, advances it, and re-evaluates, so washoff and sweeping simply rewind the clock rather than reset it. (The pinned source adds a fourth, *external* option — a scaled user time-series loading rate capped at a maximum — that bypasses the inversion mechanism.) Each form precomputes a **time-to-maximum** — power $(B_{max}/K_B)^{1/N_B}$, swapped for a flat 3650 days when $\log_{10}(B_{max})/N_B > 3.5$ (a blow-up guard, not a cap — the test ignores $K_B$, so for small enough $K_B$ the computed value exceeds 3650), exponential $-\ln(0.001)/K_B$, saturation $1000\,K_B$ — beyond which buildup is pinned exactly at $B_{max}$; the power exponent is validated to $[0.01, 10]$. Initial buildup comes from a user areal loading or, absent one, from evaluating the buildup function over the antecedent dry days; each land use also carries an initial days-since-last-swept that offsets its first sweeping. Buildup pauses during wet steps (runoff > 0.001 in/hr), and snow-only pollutants accumulate only while snow depth is at least 0.001 in. **Street sweeping** runs on a per-land-use interval within a seasonal window; each pass removes the fraction (availability × efficiency) of current buildup, and is suppressed when rainfall exceeds 0.001 in/hr, when more than 0.05 in of snow lies on the plowable impervious area, or when the interval is zero.

### 9.3 Washoff

Three per-(pollutant, land-use) washoff models, all cut off below 0.001 in/hr of runoff:

- **Exponential**: $w = K_W q^{N_W} m_B$ (mass/hr) — first-order in remaining buildup $m_B$, driven by runoff intensity $q$ over the whole subcatchment; each step depletes buildup by $\min(w\,\Delta t,\ m_B)$ *before* BMP removal is applied. Source-limited by construction, producing the classic first-flush hysteresis. The exponent $N_W$ generalises the original linear $k = K_W q$, whose $q$-cancellation forced concentration to decrease monotonically; the classic $K_W = 4.6$ in⁻¹ is Burdoin's separate calibration ("half an inch of runoff in an hour removes 90% of the load").
- **Rating curve**: $w = K_W Q^{N_W}$ (mass/sec, unlike exponential's mass/hr) — the concentration is evaluated on the **land-use share** of flow $f\,Q_{sub}$ ($f$ the land-use area fraction), so the washoff rate works out to exactly $K_W(f\,Q_{sub})^{N_W}$ rather than the linear proration $f\,K_W Q_{sub}^{N_W}$ (the two differ whenever $N_W \ne 1$). No inherent source limit unless buildup is also modelled as a cap, in which case exhaustion drops the load abruptly to zero.
- **EMC**: a constant concentration (the rating curve with $N_W = 1$), converted by the 28.3 L/ft³ factor.

Rain and run-on loads are not simply added: they mix through the **ponded water** atop the subcatchment (a completely-mixed store consistent with the nonlinear reservoir), which introduces one extra state per pollutant per subcatchment — the ponded mass. Infiltration removes mass proportionally; evaporation removes mass proportionally too — the new ponded mass is the mixed concentration times the evaporation-reduced depth, so nothing concentrates and the evaporated share of mass simply vanishes from the ledger; and a step with **no inflow at all writes any residual ponded mass off to final storage** — it does not persist for resuspension in the next storm. Per-land-use BMP removal fractions discount the washoff stream, and their area-weighted average discounts the ponded stream; the outflow concentration is total load over outflow (the pre-re-routing runoff rate drives the washoff *rate*, but both load streams are exported on the post-re-routing outflow volume). Loads to another subcatchment become its run-on at the next step.

### 9.4 Transport and Treatment

Conveyance transport treats every conduit and storage node as a **completely-mixed reactor** (the WASP/QUASAR box-model lineage) rather than solving advection–dispersion. At each routing step: node inflow loads are accumulated (subcatchments, DWF, external, groundwater, RDII, plus each inflowing link at its previous concentration); non-storage nodes holding negligible volume take the flow-weighted mixture (a junction actually holding water — surcharged or ponded — updates as a mixed reactor instead); storage nodes and conduits update by the deliberately robust mixing formula

$$c(t{+}\Delta t) = \frac{c(t)\,V(t)\,e^{-K_1\Delta t} + C_{in}Q_{in}\Delta t}{V(t) + Q_{in}\Delta t}$$

chosen over the analytical CSTR solution (exact only under constant-inflow, averaged-volume assumptions) because it stays stable as volumes vanish and never overshoots a step input. Two code-level deviations from this manual form: the pinned source evaluates the decay factor as the linear truncation $(1 - K_1\Delta t)$ floored at zero — the exponential survives only on the steady-flow path — and clamps the mixed result to at most the larger of the reactor and inflow concentrations. Under dynamic wave (which yields one flow per conduit) the mixing inflow is volume-adjusted, $Q_{in} \leftarrow \max(0,\ Q_{in} + (V_2 + V_{losses} - V_1)/\Delta t)$. The dry thresholds are concrete: below **1 litre** of volume or **1 mm** of depth an element's remaining mass is flushed to final storage and its concentration zeroed — unconditionally for conduits, but only in the absence of inflow for the mixed-reactor nodes (initial concentrations seed only elements wet at start; a wet no-inflow junction keeps its previous concentration). Volume-less links (pumps, regulators, dummy conduits) pass their upstream node concentration through; evaporation concentrates by $1 + V_{evap}/V$; steady-flow routing replaces conduit contents with the upstream node concentration decayed by $e^{-K_1\Delta t}$ and scaled by the evaporation factor.

**Treatment** attaches a user expression to any (node, pollutant): either `c = …` (resulting concentration) or `r = …` (fractional removal applied to the inflow concentration), written over:

- pollutant symbols — a pollutant's **bare name** (`TSS`) for its concentration and `R_<name>` for its fractional removal;
- hydraulic variables:
  - `FLOW`, in user flow units;
  - `DEPTH` and `AREA`, as old/new-step averages in user length units;
  - `DT`, in seconds;
  - for storage nodes, `HRT` — the residence time in *hours*, updated as $\theta \leftarrow (\theta + \Delta t)\,V/(V + Q_{in}\Delta t)$ — zero elsewhere;
- and the expression language's 19 functions (`sin cos tan cot asin acos atan acot sinh cosh tanh coth abs sgn sqrt log log10 exp step`, case-insensitive, with `+ - * / ^` and scientific literals).

Domain violations do not error: square roots and logarithms of non-positive arguments, powers of non-positive bases, and NaN results all silently evaluate to **zero**. A subtle semantic: a referenced pollutant symbol denotes the *combined-influent* concentration when **that pollutant's** equation at the node is removal-type (also the default when it has no equation there), and the node's pre-treatment concentration otherwise — equivalent only at nodes holding no volume. This small expression language expresses constant EMCs, co-removal, concentration-switched removal, $n$-th-order kinetics, the k-C* wetland model, and quiescent gravity settling. Guardrails: treated concentration bounded by [0, untreated]; removals ≤ 1; removal-form yields zero without inflow; a treatment expression at a node **overrides** the pollutant's global decay there; and co-pollutants receive no automatic co-treatment.

## 10. LID Controls

LID units are depth-explicit **layered moisture-accounting models** embedded in subcatchments — a deliberate middle path between curve-number credits (no dynamics) and Richards-equation soil physics (too costly for hundreds of units). The generic unit (a bio-retention cell) stacks a **surface** layer (ponding depth $d_1$, void fraction $\phi_1$), a **soil** layer (moisture $\theta_2$ across thickness $D_2$), and a **storage** layer (depth $d_3$, void fraction $\phi_3$) with optional underdrain; each layer's state advances by a flux balance of the form

$$\phi_1\frac{\partial d_1}{\partial t} = i + q_0 - e_1 - f_1 - q_1,\qquad
D_2\frac{\partial \theta_2}{\partial t} = f_1 - e_2 - f_2,\qquad
\phi_3\frac{\partial d_3}{\partial t} = f_2 - e_3 - f_3 - q_3 .$$

The constitutive fluxes echo the engine's own hydrology, with LID-specific parameters: surface-to-soil infiltration is **modified** Green–Ampt in the amended-media parameters (except permeable pavement, whose surface intake is inflow-plus-ponding capped by the clog-reduced pavement permeability — no Green–Ampt; and vegetative swales, which use the parent subcatchment's live native infiltration); soil percolation follows the exponential form $K_{2S}e^{-k_{slope}(\phi_2 - \theta_2)}$, zero below field capacity, where $k_{slope}$ is the LID soil layer's *own* conductivity-slope input, not the aquifer's $HCO$; exfiltration to native soil is its saturated conductivity, further capped by the groundwater module's available upper-zone storage when an aquifer is modelled; ET cascades top-down through the layers from the same potential-ET series — but all sub-surface ET is suppressed while surface infiltration is active, the storage layer's evaporation is zeroed while the soil or pavement layer above is saturated (in trenches, while the surface is ponded), and rain barrels evaporate nothing at all, covered or not. The underdrain is a power law $q_3 = C_{3D}h_3^{\eta_{3D}}$ ($\eta = 0.5$ recovers the orifice equation) with head cases spanning storage, saturated-soil, and ponded regimes — extended in 5.1.013 with open/close head thresholds (hysteretic on prior drain flow) and an optional multiplier-vs-head curve; notably the drain equation is evaluated in **user units** (head in in/mm, flow in in/hr or mm/hr), an exception to the engine's internal ft–s convention that makes drain coefficients unit-system-dependent. Surface excess over the berm overflows — by Manning routing $\alpha(d - D_1)^{5/3}W_1/A_1$ when roughness, slope, and width are all non-zero, instantaneously otherwise; swale cross-sections clamp top and bottom widths to at least 0.5 ft. A strictly **ordered set of min-limits** keeps every flux within what its layers can supply and accept, with a special equal-flux rule under full saturation.

The other unit types are configurations of this template: **rain gardens** drop the storage layer; **green roofs** replace it with a drainage mat drained by Manning flow along the roof; **infiltration trenches** drop the soil layer; **permeable pavement** inserts a pavement layer (with block-paver area fraction, permeability, and optional sand filter); **rain barrels** are pure storage (void fraction forced to 1, sealed bottom) with a delayable drain valve and an optional cover flag — uncovered barrels receive direct rainfall, covered ones none; **rooftop disconnection** is a lone surface layer with a gutter-capacity-limited drain; **vegetative swales** are a lone surface layer with trapezoidal, depth-varying geometry and Manning outflow. Any gravel storage layer — and the pavement layer — may **clog**: conductivity declines linearly with cumulative void-volumes of inflow treated, controlled by a single clogging factor per layer, and pavement permeability may optionally **regenerate** on a fixed-day cycle (5.1.013+).

Numerically, **vegetative swales** integrate their layer-state vector by the iterated **trapezoidal method** ($\Omega = 0.5$, 1 mm tolerance, at most 20 iterations) — the one implicit solve in the quality domain; **every other unit type advances by a single explicit Euler step**, which testing showed sufficient. Deployment is per-unit-area: each unit captures a specified **percentage of the subcatchment's non-LID impervious-area runoff** (percentages across units validated to sum ≤ 100%; a "capture ratio" of areas is only a sizing heuristic) and, since 5.1.013, a percentage of pervious-area runoff as well — both reduced by any internal sub-area re-routing first. Direct rainfall always lands on the unit, but **run-on from upstream subcatchments reaches LID units only when a unit occupies its entire subcatchment**; otherwise run-on bypasses all LIDs. Each unit takes an **initial saturation** percentage that pre-fills its soil and storage layers (and correspondingly shrinks the soil Green–Ampt deficit). Surface overflow joins subcatchment runoff, exfiltration joins infiltration, and underdrain flow is tracked separately, routable to its own subcatchment or node — defaulting to the parent subcatchment's outlet; drain flow to a node is interpolated between runoff-step values at each routing step, while drain flow to a subcatchment arrives one runoff step delayed. Independently, a unit's *entire* outflow can be returned onto the pervious area (surface flow always; drain flow only when its destination is the subcatchment's own outlet). An optional per-unit **detailed report file** logs eight flux rates and four storage levels each runoff step, compressing dry spells to their boundary records. **Water quality in LIDs is volume-based**: outflows carry the subcatchment's computed washoff concentration unchanged (with mixing corrections for direct-rainfall loads), so load reduction is proportional to runoff reduction — full capture is 100% removal. No media treatment chemistry is represented, though 5.1.013+ accepts per-pollutant percent removals applied to underdrain loads only (an empirical credit, not process chemistry).

## 11. Control Rules

Rules are `RULE name / IF premise / {AND|OR premise}* / THEN action / {AND action}* / [ELSE action*] / PRIORITY p` blocks, parsed by a strict state machine. Premises take the form `object id attribute relop value-or-reference`: objects are gages, nodes, links (conduit/pump/orifice/weir/outlet), or the simulation itself; attributes include node depth/max-depth/head/volume/inflow, link flow/depth/velocity/status/setting (conduits adding full-flow/full-depth/length/slope, new in 5.2), a link's time-open/time-closed, a gage's current intensity or its past-*n*-hours rainfall (up to 48 h), and simulation time/date/clock-time/day/month/day-of-year. Values compare in **user units**; every time-valued comparison (elapsed time, clock time, time-open/-closed) carries a half-step tolerance window for both `=` and `<>`. Attribute applicability is enforced at evaluation, not parse: velocity and the conduit attributes return "missing" for non-conduits, status only for conduits and pumps, and setting only for pumps/orifices/weirs (an `OUTLET … SETTING` premise parses but is silently always false) — a missing operand makes the premise false, never an error. Boolean evaluation is sequential with short-circuiting: an `OR` premise is evaluated only when the running result is false, so it disjoins with the *immediately preceding* premise — `A AND B OR C` evaluates as `A AND (B OR C)`, not conventional precedence. SWMM 5.2 adds named `VARIABLE` and `EXPRESSION` declarations usable as premise left-hand sides.

Actions set link controls only: conduit open/closed, pump on/off or speed setting, orifice/weir/outlet setting in $[0,1]$ — as a constant, a `CURVE` lookup (evaluated at the last-compared premise's left-hand value, the same source as the PID set-point), a `TIMESERIES` lookup at the current time, or a `PID` controller. Conflicts resolve through a per-link pending-action slot where a strictly higher rule priority replaces, ties keeping the earlier rule; an action "fires" only if it actually changes the link's target setting (the changes count feeds steady-state detection), and modulated actions are excluded from the report's action log. Rules are normally evaluated every routing step; a `RULE_STEP` option restricts evaluation to a fixed clock (§2). The PID form interprets its three parameters as gain $K_p$, integral time $K_i$ (minutes), and derivative time $K_d$ (minutes), applying a velocity-form update on the normalised error $e = (x_{sp} - x)/x_{sp}$ (normalised by the controlled value instead when the set-point is zero; integral term dropped when $K_i = 0$):

$$\Delta u = K_p\left[(e_0 - e_1) + \frac{e_0\,\Delta t}{K_i} + K_d\,\frac{e_0 - 2e_1 + e_2}{\Delta t}\right]$$

added to the link's target setting each step, floored at 0 for all links and capped at 1 for non-pumps — with small-error dead-banding, and a "stuck-controller" reset zeroing the error history when successive errors differ by less than $10^{-4}$. The set-point is taken from the rule's last-compared premise — so the premise both triggers the rule and defines what the controller regulates toward.

## 12. Continuity Accounting

Four independent balances are tallied over the whole run, each reporting $100(1 - \text{outflow}/\text{inflow})$ — its sign-preserving mirror $100(\text{inflow}/\text{outflow} - 1)$ when only outflow exists, and effectively zero when the totals agree within 1 ft³ (0.001 mass units for the quality balances):

- **Runoff**: rainfall + run-on + initial ponding + initial snow **vs.** evaporation + infiltration + runoff + LID drains + plowed-out snow + final ponding + final snow.
- **Groundwater**: infiltration + initial storage **vs.** upper/lower-zone ET + deep percolation + lateral groundwater flow + final storage.
- **Flow routing**: initial stored volume + wet-weather + RDII inflows **vs.** final storage + flooding + evaporation + seepage — with the dry-weather, groundwater, external-inflow, and outflow terms each *signed*, a negative total crossing to the other side. (A "reacted" slot exists in the flow totals but is never accumulated — reaction losses appear only in the quality balance. Under steady-flow routing, final storage excludes link volumes while initial storage includes them.)
- **Quality** (per pollutant, worst |error| reported): all inflow loads + initial stored mass **vs.** flooding, outflow, reactions, seepage, and final stored mass (negative outflow mass is re-credited as external inflow; count-unit pollutants report as log₁₀, and mass totals convert to user units at reporting); and an analogous **surface-loading** balance covers buildup/washoff (initial + buildup + deposition vs. sweeping + infiltration + BMP removal + washoff + remaining).

A per-step flow error (rates in vs. rates out) feeds the **steady-state skip** decision (§2), not the time-step diagnostics. A continuity table prints only when its error exceeds 10% — a *signed* comparison, so a large negative error does not by itself trigger it — or the CONTINUITY report option is on. Per-node cumulative inflow/outflow volumes are also tracked (initial volume seeding inflow; outfalls and terminal nodes counting inflow as outflow; final volume added to outflow), driving the node-inflow summary's flow-balance column and the "highest continuity errors" top-five.

**Reported statistics.** Alongside the balances, the engine accumulates summary statistics on every routing step *after the report start date* ("hours" quantities are step-time integrals; maxima carry occurrence dates):

- per-subcatchment water-balance totals, peak runoff, and runoff coefficient;
- groundwater flux totals and time-weighted average moisture/water-table;
- per-pollutant washoff loads;
- node average/maximum depths (plus a separate maximum sampled only at reporting intervals), flooding (hours, volume, peak overflow, peak ponding — a node "floods" when over full volume or overflowing), and surcharge (dynamic wave only; hours and clearances above crown/below rim);
- storage average/max volumes and losses;
- outfall flow-frequency, average/max flow, and total loads (plus the system-wide maximum simultaneous outfall flow);
- link maxima (|flow|, velocity, depth, capacity ratios);
- conduit time-in-flow-class across the seven dynamic-wave classes, hours normal-flow-limited, under inlet control, at full flow, capacity-limited, and full at either end;
- pump utilisation, startup count, min/avg/max flow, volume, energy (kWh), and time off both ends of its curve;
- routing time-step min/avg/max with a log-binned frequency table, average iterations, percent non-converging, and percent of time in steady state; and
- top-five "highest" lists — node continuity errors, Courant-critical elements (counted as occurrences of being the step-limiting element), flow-instability indices, and most-frequently non-converging nodes.

The report file additionally carries a rainfall-file summary, an RDII sewershed summary, the control-actions log, an options echo, and (on request) per-object time-series tables — a text channel separate from the binary output.

## 13. Units and Physical Constants

**Unit system selection**: the user's **flow unit selects the entire unit system**, for every quantity:

| Flow units | System |
|------------|--------|
| CFS, GPM, MGD | US customary |
| CMS, LPS, MLD | SI |

Internally *all* computation runs in feet, square/cubic feet, cfs, and °F, with time in seconds (and dates as decimal days since 1899-12-30); conversion happens only at input parsing and output writing, through a fixed factor table. Key examples:

| Conversion | Factor |
|------------|--------|
| Rainfall, ft/s → in/hr | ×43,200 |
| Rainfall, ft/s → mm/hr | ×1,097,280 |
| Manning's $n$ | s/m$^{1/3}$ in both systems, whence the recurring 1.486 = 1/0.3048$^{1/3}$ factor |

**Physical constants**, together with the two localised exceptions to the internal-units convention:

| Constant / convention | Value | Notes |
|-----------------------|-------|-------|
| Gravitational acceleration $g$ | 32.2 ft/s² | |
| Kinematic viscosity | $1.1\times10^{-5}$ ft²/s | |
| $g$ in the HEC-22 inlet equations | 32.16 ft/s² | Localised exception |
| LID underdrain equation | — | Evaluates in user units (§10) — localised exception |

The various structure coefficients of §7 complete the constant set. Concentrations are mg/L, µg/L, or counts/L regardless of system.

## 14. Input and Output

### Input

The input is the text INP file of ~57 bracketed sections (matched by prefix), grouping: options and interface-file directives; hydrology objects (`[RAINGAGES]`, `[SUBCATCHMENTS]`, `[SUBAREAS]`, `[INFILTRATION]`, `[AQUIFERS]`, `[GROUNDWATER]`/`[GWF]`, `[SNOWPACKS]`, `[HYDROGRAPHS]`, `[RDII]`); network objects (`[JUNCTIONS]`, `[OUTFALLS]`, `[STORAGE]`, `[DIVIDERS]`, `[CONDUITS]`, `[PUMPS]`, `[ORIFICES]`, `[WEIRS]`, `[OUTLETS]`, `[XSECTIONS]`, `[TRANSECTS]`, `[LOSSES]`); quality (`[POLLUTANTS]`, `[LANDUSES]`, `[BUILDUP]`, `[WASHOFF]`, `[COVERAGES]`, `[TREATMENT]`, `[LOADINGS]`); inflows (`[INFLOWS]`, `[DWF]`, `[PATTERNS]`); controls, curves, time series; LID (`[LID_CONTROLS]`, `[LID_USAGE]`); map/display metadata; and the 5.2 street-drainage trio `[STREETS]`, `[INLETS]`, `[INLET_USAGE]`. Global **process switches** (`IGNORE_RAINFALL/SNOWMELT/GWATER/RDII/ROUTING/QUALITY`, plus `ROUTE_MODEL NONE`) disable whole subsystems — quality ignoring also strips all pollutant variables from the binary output — and subsystems with no objects are ignored automatically. Time steps interlock at validation: the report step must be ≥ the routing step (fatal otherwise), the dry step is raised to the wet step, and the routing step is clamped to the wet step. Date/time conventions: INP dates are M/D/Y with `-`/`/` separators (3-letter month names accepted), times decimal-hours or h:m:s; decoded times round to the nearest second; every conversion of elapsed time to a calendar date adds **+1 ms** (so each reporting timestamp and date-driven lookup sits 1 ms past nominal); and elapsed-time labels measure from *report* start.

### Interface Files

Ancillary interface files carry data between runs. The **rainfall interface file** (binary, `SWMM5-RAIN` stamp) collates external files into per-station records of (date, depth) pairs for non-zero periods only — gages are matched by *station ID* (shared stations share data; one station in two files is fatal), file-fed gages become volume-type at the file's interval (NWS/Canadian formats override the declared interval and shift end-of-interval stamps), NWS accumulation codes split totals evenly across their span, and decreasing cumulative readings reset the accumulator. The **routing interface file** (text) carries a header (title, report step, constituent names/units, node names) then one line per node per period; on reading, values are linearly interpolated in time, nodes and pollutants matched by name (unmatched pollutants zero), and flows converted from the *file's* units; outflows are saved for outlet nodes only, and one file cannot serve as both inflow and outflow. The **hotstart file** (`SWMM5-HOTSTART4` stamp; versions 1–4 readable, with older versions carrying progressively less state) checkpoints approximately the §1.7 state vector (see the caveats there) — runoff state as doubles, routing state as floats, link settings re-applied through the control machinery — and its compatibility check covers **object counts and flow units only**: a reordered model silently loads the wrong state.

### Output

Output has two faces. The text **report file** carries the input summary, continuity balances, per-object summary tables (including the 5.2 street-flow table), and diagnostics. The binary **`.out` file** is the machine interface.

#### Binary Output File Layout

The `.out` file is written as the following record sequence:

| Record | Contents |
|--------|----------|
| Header | Magic number 516114522; version int (52004); flow-units code; object counts |
| ID name table | Object ID names |
| Pollutant units | Per-pollutant concentration-unit codes |
| Static property tables | Subcatchment areas; node type/invert/max-depth; link type/offsets/max-depth/length |
| Result-variable code lists | Codes of the reported result variables |
| Per-reporting-period records (fixed size) | An 8-byte timestamp followed by float results for every reported subcatchment (8 vars + washoff per pollutant), node (6 vars + quality), link (5 vars + quality), and 15 system-wide series |
| Epilog | Six ints giving the table offsets, period count, error code, and the magic number again — so readers navigate by seeking −24 bytes from EOF |

Node and link values are period-interpolated (or period-averaged on request — though regulator/pump settings are never averaged, and pump flows are not interpolated across on/off transitions); subcatchment values are always interpolated and system values are current-step totals — all already in user units. Two reader caveats: per-object results appear **only for objects flagged in `[REPORT]`** (all off by default — an unconfigured run's binary file holds only the 15 system series), and when the report start postdates the simulation start, the stored start-date field is deliberately backdated one period before the first record.

## 15. The Engine as a Library

SWMM is an embeddable shared library; the command-line tool is a thin progress-callback wrapper over the same public API. Three tiers:

### Run-Loop Lifecycle

`swmm_open` (parse and validate) → `swmm_start(saveResults)` (initialise state; collate the rainfall interface file; pre-compute RDII; read any hotstart file; `saveResults = FALSE` skips the binary output) → repeated `swmm_step`, each advancing exactly one routing step and returning elapsed decimal days, `0.0` signalling completion — or `swmm_stride(seconds)`, which advances a fixed span of simulation time by temporarily capping the routing step — → `swmm_end` → optional `swmm_report` (write the text report) → `swmm_close`. The window between `open` and `start` is where pre-run modification is legal; the run-total mass-balance errors are queryable only between `end` and re-`start`.

### Query and Mutation

A property-code get/set surface (5.2) exposes counts, names, indices, and current values for gages, subcatchments, nodes, and links, plus system values; `swmm_getSavedValue` re-reads any object's binary-file results by reporting period after `swmm_end` (a link's *setting* is served from the file's capacity slot). Mid-simulation **setters** change boundary forcing and controls while the model runs: gage rainfall override (taking precedence over every data source — except that a gage deferring to a shared co-gage adopts the co-gage's value before its own override is consulted), node lateral inflow, outfall stage (converting the outfall to fixed-stage), and link target settings (conduits excluded, with no report logging; the OWA toolkit's separate `swmm_setLinkSetting` is the converse — it logs each application to the report as a virtual "ToolkitAPI" rule but does not exclude conduits). Setting the routing step mid-run silently zeroes the Courant factor — disabling variable time-stepping for the remainder of the run. The OWA toolkit layer (65 functions of its own; ~90 counting the 25 core exports alongside it) extends this with object enumeration across all types, simulation-date get/set, property get/set under explicit timing rules (geometry pre-start only; loss coefficients, flow limits, LID drain/roughness/clogging parameters mutable mid-run), current-time results for every object including concentrations, mid-run summary statistics and mass-balance totals, persistent programmatic node inflows, and direct node/link concentration overrides.

### Rainfall Injection and Checkpointing

Two distinct rainfall mechanisms exist: the property-based intensity override, and `swmm_setGagePrecip`, which converts a gage to the `RAIN_API` data source fed each step by the caller. `swmm_hotstart` both loads a state file before start and **saves an on-demand checkpoint mid-run** — the programmatic complement of the `[FILES]` hotstart directives.

## 16. Cross-Cutting Engine Contracts

The preceding sections follow SWMM's physical subsystems; this one collects the engine-wide contracts that span them — behaviours visible only in the code's architecture, each referenced from the sections carrying its fragments.

**The unit-boundary contract.** All internal computation is US customary (ft, ft², cfs, °F, seconds); conversion factors (`UCF`, the flow-unit table) are applied *only at boundaries* — input parsing, report writing, binary output, and the API get/set surface (§13). The documented exceptions where interior computation runs in user units are the LID underdrain equation (§10) and the groundwater lateral-flow power function (§3.4); the HEC-22 inlet equations additionally use their own $g = 32.16$ ft/s².

**The old/new state discipline.** Nearly every dynamic quantity is stored as an old/new pair, rolled at each step's start; results read out at intermediate times by weighted interpolation between the pair. This is the mechanism that reconciles the three clocks of §2 — runoff results interpolate onto routing times, and routing results onto reporting times — and it is why every module exposes a "set old state" operation.

**The state-serialization schema.** Modules expose paired get-state/set-state vectors (snowpack, groundwater, gage, infiltration, sub-area depths). These vectors *are* the persistence schema, consumed by three clients — the hotstart file, the runoff interface file, and mid-run checkpointing (§14, §15) — so a module's state vector, once defined, is a compatibility contract.

**Validation as mutation.** The validation pass does not merely check the model — it *rewrites* it: node maximum depths raised to link crowns, regulator crests raised to downstream inverts, conduit slopes floored or adverse-slope conduits reversed, offsets converted between conventions, elevations snapped, infeasible shape radii enlarged, street sections compiled into transects, and equivalent lengths/surface areas computed for orifices (§1, §4, §6–§8). A behaviourally-faithful reading of an INP file is the *post-validation* model, not the literal text.

**Option plumbing.** Roughly forty scalar analysis options are parsed in one place and consumed deep inside distant modules (§2, §5, §14). The `IGNORE_*` family silently amputates entire subsystems, and subsystems with no objects are ignored automatically — so the *effective* process set is a joint function of options and object counts.

**The interface-file lifecycle.** Rainfall, runoff, RDII, hotstart, and routing files all follow one four-state mode pattern — none / scratch / use / save — decoupling expensive stages so they can be precomputed once and replayed (§14). This is a single architectural idea, not five unrelated formats.

**The numerical toolkit.** Four solution devices recur everywhere: bracketed Newton–Raphson and Ridder root-finding (§3.3, §6, §8), adaptive Cash–Karp RK5 integration (§3.2, §3.4), Picard successive approximation with under-relaxation (§4, §5, §10), and stateful bracketed table lookup (§6). Curves and time series share one table representation with two semantic modes — sorted x-lookup versus date-cursored streams — and the cursor-stateful lookups are *not thread-safe*.

**The threading model.** OpenMP parallelism exists in exactly one place — the dynamic-wave per-conduit and per-node loops (§5); nodal flow accumulation stays serial, so results are thread-count-invariant. Everything else is strictly sequential and order-dependent, most consequentially the topological routing order (§4) and the link-definition-order sensitivity of pump/regulator solves (§5).

**In-loop instrumentation.** Continuity accounting and statistics are not post-processing: mass-balance and statistics updates are woven through the inner loops of every physical module, each with its own definition of what counts as inflow, outflow, or loss at that point (§12). A numerically-matching re-computation of SWMM's balances must replicate these call sites, not just the ledger formulas.

**One expression language, three bindings.** The tokenized math-expression evaluator (19 functions, zero-on-domain-violation semantics, §9.4) serves three unrelated features — control-rule variables/expressions (§11), treatment equations (§9.4), and custom groundwater flow equations (§3.4) — each with its own variable-binding table.

**The template/instance idiom.** Shared parameter sets instantiated per-consumer recur throughout: snowmelt parameter sets → per-subcatchment snowpacks, LID process designs → deployed units, unit-hydrograph groups → per-node RDII, aquifers → per-subcatchment groundwater, transects/streets → per-conduit geometry (§1.6). The instance may override selected template values (aquifers being the fullest example, §3.4).

**The OWA stratum.** The community fork overlays the EPA core in a marked, separable layer: the toolkit API (§15), callback-driven runs, API rainfall/inflow/quality injection paths, extra state (reactor concentrations, residence time), and structs exposed for interoperability. The physics of §2–§12 belongs to the EPA core; the pinned tag's identity is EPA 5.2.4 physics plus this instrumentation stratum.

**Error discipline.** One global error code short-circuits every phase; API entry points are guarded by open/started state checks; warnings accumulate without halting (several of §4's silent mutations announce themselves only as warnings). Keyword tables, enum orders, and report strings are maintained in positional correspondence across three files — parsing correctness is positional — a fragile invariant of the INP grammar's implementation (§14).
