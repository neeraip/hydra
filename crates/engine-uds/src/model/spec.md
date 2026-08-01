# hydra-engine-uds — Model Specification

This document holds §2–§5 of the urban drainage specification: the data model,
the unit system, the model file formats, and validation. §2 is given here; the
remaining sections follow.

---

## 2. Data Model

### 2.1 Environmental Compartments

An urban drainage model describes water and material moving between four
**environmental compartments**:

- **Atmosphere** — generates precipitation and deposits pollutants onto the land
  surface. Represented by rain gages.
- **Land Surface** — receives precipitation as rain or snow, and loses water to
  evaporation, to infiltration into the sub-surface, and to surface runoff
  carrying its pollutant load into the conveyance system. Represented by
  subcatchments.
- **Sub-Surface** — receives infiltration from the land surface and returns a
  portion to the conveyance system as groundwater interflow. Represented by
  aquifers.
- **Conveyance** — the channels, pipes, pumps, regulators, and storage units
  carrying water to outfalls or treatment. Represented as a directed graph of
  nodes and links.

**No compartment is mandatory.** A model may consist of conveyance alone, driven
entirely by user-supplied inflow hydrographs, or of hydrology alone, ending at
subcatchment outlets. A specification of any subsystem must therefore state its
behaviour when a compartment it draws from is absent, rather than assuming a
complete model.

This decomposition is the structural difference between this engine and a
pressurised-network engine, in which the network *is* the model. Here the
node-link graph is one of four coupled subsystems, and areal objects
participate in the simulation without belonging to the graph at all.

### 2.2 Object Families

Model objects fall into three kinds. The distinction is normative: it governs
what may be assumed about an object's relationship to the conveyance graph.

1. **Graph elements** — conveyance nodes and links. These are the vertices and
   edges of the directed graph, and only these have topology.
2. **Areal objects** — subcatchments, aquifers, and snow packs. These are
   simulated, hold state, and are referenced by graph elements, but are neither
   vertices nor edges. A subcatchment is a surface, not a point or a channel.
3. **Shared parameter sets** — transects, street sections, aquifer definitions,
   snow-pack parameter sets, unit-hydrograph groups, and LID designs. Each is
   defined once and instantiated by reference from many objects. They are
   neither elements nor tabulated data.

> No representation of this model may require every simulated object to be a
> node or a link. An element schema admitting only a two-kind topology cannot
> express an urban drainage model.

### 2.3 Hydrology Objects

**Rain gages** supply precipitation to one or more subcatchments, from a time
series or an external rainfall file, expressed as intensity, volume, or
cumulative volume over a fixed recording interval.

**Subcatchments** are parcels of land receiving precipitation from exactly one
rain gage. A subcatchment discharges either to a conveyance node or to another
subcatchment, so overland flow may cascade across parcels. Each is idealised as
a rectangular plane with an area, a characteristic **width**, and a uniform
slope, partitioned into three sub-areas: impervious with depression storage,
impervious without, and pervious (with depression storage). Only the pervious
fraction infiltrates. A sub-area's runoff may optionally be re-routed onto
another sub-area rather than to the outlet, representing — for example — roofs
draining onto lawns.

A subcatchment may additionally host a **snow pack** governing accumulation and
melt on its plowable, impervious, and pervious fractions; a **groundwater**
connection to an aquifer; **LID controls** occupying part of its area; and
per-land-use pollutant buildup state.

The five geometric parameters — area, imperviousness, width, slope, curb length
— are subject to the admissibility and mutation rules of §5.

**Aquifers** are two-zone (unsaturated and saturated) sub-surface reservoirs
beneath subcatchments. They receive percolation, lose water to deep percolation
and evapotranspiration, and exchange flow with a designated conveyance node
through a parameterised groundwater flow relation — the mechanism by which
baseflow and groundwater infiltration enter sewers.

**Unit hydrograph groups** describe rainfall-dependent infiltration and inflow:
the delayed entry of stormwater into sanitary sewers through defects and illicit
connections. Each unit hydrograph converts a unit of instantaneous rainfall into
a triangular response defined by a fraction of rainfall volume, a time to peak,
and a recession ratio, organised in groups of up to three per month.

**LID controls** are depth-explicit representations of low-impact-development
practices, composed of layered elements — surface, pavement, soil, storage,
underdrain, and drainage mat — each with its own governing relations. A design
is defined once and deployed at specified sizes in many subcatchments.

### 2.4 Conveyance Nodes

Every node has an invert elevation and may receive **external inflows** in
addition to the runoff and groundwater delivered by hydrology objects. A direct
external inflow of flow or of any constituent is the composite

$$Q_{ext}(t) = c_f\left[s_f\,TS(t) + Q_{base}\,P(t)\right]$$

where $TS(t)$ is an optional time series, $s_f$ its scale factor, $Q_{base}$ a
constant baseline, $P(t)$ that baseline's own pattern, and $c_f$ a units factor
(mass-type pollutant inflows carrying their own conversion). Dry-weather
sanitary inflow multiplies an average value by up to four patterns, subject to
the slot and dispatch rules of §4.

There are four node types.

**Junctions** are ordinary connection points — manholes, fittings, confluences —
with negligible storage. A junction has a maximum (ground or rim) depth; when
the hydraulic grade line reaches it, excess water either leaves the system or,
where **ponding** is enabled, is stored atop the node over a specified ponded
area and returned as capacity recovers.

**Outfalls** are terminal boundary nodes where water leaves to a receiving body.
The boundary stage is **free** (the smaller of critical and normal depth at the
connecting conduit), **normal**, **fixed**, **tidal** (a repeating 24-hour stage
curve), or a **time series**. For the staged variants the stage governs only
where it exceeds the critical-depth elevation; below that the node sits at
critical depth, except that a conduit on a positive offset whose invert lies
above the stage leaves the node at the stage height and the conduit in free
fall. An outfall may carry a flap gate blocking reverse flow, and may route its
discharge back onto a subcatchment. Its permitted link count is
routing-dependent (§2.9).

**Storage units** are nodes with significant free-surface storage — ponds, wet
wells, detention basins, chambers. Geometry is given by a functional relation
(surface area as a power function of depth), by a tabulated area-versus-depth
curve, or by one of four analytical forms: elliptical cylinder, elliptical cone,
elliptical paraboloid, and rectangular pyramid. Storage units may lose water to
evaporation and to seepage.

**Dividers** split inflow, including any node overflow, between two outflow
conduits by a prescribed rule: **cutoff** (divert all inflow above a threshold),
**overflow** (divert whatever the non-diverted conduit declines to accept, that
link being routed first), **tabular** (a diverted-flow-versus-inflow curve), or
**weir**. Diverted flow is clamped to the inflow, except on the overflow path,
which returns its split before the clamp applies. The weir and tabular rules are
evaluated in the user's units (§3). Divider behaviour is routing-dependent
(§2.9).

### 2.5 Conveyance Links

A link connects two nodes; its orientation defines the positive flow direction,
and a negative flow denotes reversal. There are five link types.

**Conduits** are the pipes and channels of the network — the only link type with
hydraulic length, and the primary object of flow routing. Each carries a
cross-sectional shape drawn from one of four descriptions: a library of standard
closed and open geometries; an **irregular transect** (a surveyed
station-elevation profile with separate left-bank, main-channel, and right-bank
roughness); a **street** cross-section (a curb-and-gutter roadway profile); or a
user-supplied **custom shape** curve of width against depth. Geometry is
specified in §6.

A conduit additionally carries a Manning roughness coefficient; upstream and
downstream invert offsets above its end nodes; an optional count of identical
parallel **barrels**, routing solving one barrel and scaling volumes and losses
by the count; an optional maximum-flow limit; optional entrance and exit minor
loss coefficients; an optional flap gate; and optional seepage and evaporation
losses. A conduit may be designated a **culvert**, activating inlet-control
capacity limits, or a **force main**, using a pressurised friction relation
while full (§8).

Conduit slope is drop over *horizontal* distance,

$$S_0 = \frac{\Delta z}{\sqrt{L^2 - \Delta z^2}}$$

where $\Delta z$ is the invert drop and $L$ the conduit length. The degenerate
case $\Delta z \ge L$, which would make the horizontal distance imaginary, and
the flooring and sign conventions applied to $S_0$, are specified in §5.

**Pumps** raise water between nodes according to a pump curve of five types:
flow varying stepwise with wet-well volume (Type 1) or with inlet depth
(Type 2); flow varying continuously with delivered head (Type 3) or with inlet
depth (Type 4); and a variable-speed Type 3 (Type 5). An **ideal** transfer pump
sets outflow equal to inflow. Pumps have on and off depth setpoints and may have
their speed modulated by control rules.

**Orifices** are openings in the side or bottom of a node's wall or floor,
closable to a variable degree by control rules, with distinct free, submerged,
and partially-open discharge regimes. Geometry is circular or rectangular, and a
flap gate may prevent reverse flow.

**Weirs** are overflow structures of five types — transverse, side-flow,
V-notch, trapezoidal, and roadway — each with its characteristic head-discharge
exponent and coefficient, with corrections for end contractions, submergence,
and surcharge.

**Outlets** are general-purpose head-discharge devices whose outflow is an
arbitrary function of head or depth, given as a power function or a rating
curve. They represent devices fitting none of the standard structures.

A sixth family, **streets and inlets**, pairs street cross-sections with inlet
capacity relations to model dual drainage — flow on the street surface captured
by inlets and entering the below-ground sewer. It is specified in §8.

### 2.6 Water Quality Objects

**Pollutants** are user-defined constituents, any number of them, carried by
runoff and routed through the conveyance system. Each has a concentration unit,
optional rainfall, groundwater, RDII, and dry-weather background concentrations,
an initial network concentration, a snow-only buildup flag, a first-order decay
coefficient, and optionally a **co-pollutant** relation setting its
concentration as a fixed fraction of another's.

**Land uses** partition a subcatchment's area into categories governing
pollutant buildup during dry weather and washoff during runoff, each by a choice
of functional forms, together with street-sweeping parameters that periodically
remove accumulated buildup.

### 2.7 Data Objects and Shared Parameter Sets

**Curves** are tabulated relations serving typed roles — storage, diversion,
tidal, pump, rating, control, shape, and weir coefficient. Interpolation travels
with the role, not with the curve: curves are interpolated linearly, except Type
1 and Type 2 pump curves, which are read stepwise.

**Time series** are timestamped value sequences used for rainfall, outfall
stage, external inflows, and evaporation. Behaviour outside a series' range
depends on the consumer and is specified in §4.

**Time patterns** are repeating multiplier sets — monthly, daily, hourly, and
weekend-hourly — modulating dry-weather sanitary inflows and external-inflow
baselines.

**Control rules** are conditional statements over simulation state, with
priorities, that switch pumps and adjust regulator settings; their actions may
be immediate or modulated. They are specified in §12.

**Shared parameter sets** — transects, street sections, aquifer definitions,
snow-pack parameter sets, unit-hydrograph groups, and LID designs — are defined
once and instantiated by reference, per §2.2.

### 2.8 The State Vector

The simulation is distributed and discrete in time. The system state advances as
$X_t = f(X_{t-1}, I_t, P)$ and outputs are computed as $Y_t = g(X_t, P)$, where
$I_t$ are the external inputs — precipitation, temperature, boundary stages,
control settings — and $P$ the constant parameters.

The state vector is small relative to the model's scope:

| Object | State |
|---|---|
| Subcatchment | Ponded depth for each of the three sub-areas independently; the infiltration state of the chosen method; groundwater moisture content and saturated-zone depth; snow-pack depth, free water, temperature, and cold content |
| Conveyance node | Water depth |
| Conduit | Flow rate and flow area |
| Pollutant | Surface buildup mass and ponded mass per subcatchment, with last-swept time; concentration per node and per link |

Everything else the engine reports — velocities, volumes, flooding, loads — is
derived from these states, the inputs, and the parameters.

This vector defines what initial conditions a user must supply, and marks the
seam between the hydrologic and hydraulic halves of the engine, which advance on
different clocks (§13). It is closely related to, but not identical with, what a
checkpoint file persists; the differences are specified in §4.

### 2.9 The Routing-Dependent Model Contract

The flow routing method is nominally a choice of solution technique. It is not
only that: several model semantics differ by routing method, so **the effective
model is a function of the input file and the routing option together**. No
statement of what a file means is complete without naming the routing method in
force.

The couplings are:

| Behaviour | Steady and kinematic wave | Dynamic wave |
|---|---|---|
| Outfall link count | May have no outlet links; inflow count unchecked | Exactly one connecting link enforced, which may be an outlet link |
| Terminal non-outfall, non-storage node with no outlet links | Inflow leaves the system and is **not** counted as flooding | An ordinary interior node whose overflow **is** counted as flooding |
| Dividers | Perform their split rule | Behave as ordinary junctions, the momentum treatment determining the split |
| Adverse-slope conduit | Retains its adverse sign | Silently reversed internally, reported flows carrying a direction multiplier so output keeps the user's orientation |

Two further couplings are mutations rather than semantics — the raising of
regulator crests, and the interaction between the user minimum slope and the
adverse-slope sign convention — and are specified with the other mutations in
§5.

Every subsystem specification that depends on any behaviour in this section
references it here rather than restating it, so that the set of couplings has
one home and can be checked for completeness.
