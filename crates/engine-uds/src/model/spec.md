# Urban Drainage — Domain Specification

This document holds §2 of the urban drainage specification: the entities a model
is composed of, the state they carry, and the unit system all other documents
are written in.

---

## 2. Domain Model

### 2.1 Compartments

A drainage model describes water and material moving between four compartments:

- **Atmosphere** — supplies precipitation and deposits material on the land
  surface.
- **Land surface** — receives precipitation, loses water to evaporation and to
  infiltration, and delivers the remainder as runoff carrying its material load.
- **Subsurface** — receives infiltration, and returns part of it to the network
  as interflow.
- **Conveyance** — the network of channels, pipes, structures, and storage that
  carries water to receiving waters.

**No compartment is mandatory.** A model may be conveyance alone, driven by
supplied inflow time series, or surface alone, ending at parcel outlets. Every
subsystem specification states its behaviour when a compartment it draws from is
absent rather than assuming a complete model.

This decomposition is the structural difference between this engine and a
pressurised-network engine, in which the network *is* the model. Here the
network is one of four coupled compartments, and entities representing area
participate in the simulation without belonging to the network graph.

### 2.2 Entity Kinds

Entities fall into three kinds. The distinction is normative: it governs what
may be assumed about an entity's relationship to the network.

1. **Network elements** — vertices and edges of the conveyance graph. Only
   these have topology.
2. **Areal entities** — parcels, aquifers, and snow packs. These are simulated,
   hold state, and are referenced by network elements, but are neither vertices
   nor edges. A parcel is a surface, not a point or a channel.
3. **Shared definitions** — cross-section profiles, street sections, aquifer
   parameters, snow-pack parameters, unit-hydrograph groups, and control-measure
   designs. Each is defined once and instantiated by reference from many
   entities. They are neither elements nor tabulated data.

> No representation of this model may require every simulated entity to be a
> vertex or an edge. A schema admitting only a two-kind topology cannot express
> an urban drainage model.

### 2.3 Solution Method Is Not a Model Property

The model defined here is a description of a physical system. It does not
include, and its meaning does not depend on, any choice of solution technique.
Two consequences follow, and both are deliberate departures from the
predecessor:

- **A model means one thing.** The same model describes the same physical system
  regardless of how it is solved. Nothing in §2 has a meaning contingent on a
  numerical scheme.
- **Approximation is not a modelling choice.** The predecessor offers reduced
  forms of the momentum equation as user-selectable routing options, and several
  of its model semantics differ between them, so that selecting a routing method
  silently selects between different networks. This engine specifies the physics
  once; the treatment of a predecessor file that requests a reduced form is an
  import concern, specified in §14.

### 2.4 Surface Entities

**Precipitation gages** supply a precipitation record to one or more parcels,
from a supplied series or an external record — declared by file name, station
identifier, and the record's own depth unit — expressed as intensity, volume,
or cumulative volume over a fixed recording interval.

**Parcels** are areas of land receiving precipitation from exactly one gage,
discharging either to a network vertex or to another parcel, so that overland
flow may cascade. A parcel is idealised as a rectangular plane with an area, a
characteristic **width**, and a uniform slope, partitioned into three sub-areas:
impervious with depression storage, impervious without, and pervious. Only the
pervious sub-area infiltrates. A sub-area's runoff may be re-routed onto another
sub-area rather than to the outlet, representing — for example — roofs draining
onto lawns.

A parcel may host a **snow pack** governing accumulation and melt over its
plowable, impervious, and pervious fractions; a **groundwater connection** to an
aquifer; **control measures** occupying part of its area; and per-land-use
material accumulation state.

**Control measures** are depth-explicit representations of
low-impact-development practices, composed of layered elements — surface,
pavement, soil, storage, underdrain, and drainage mat — each with its own
governing relations. A design is defined once and deployed at specified sizes
across many parcels.

### 2.5 Subsurface Entities

**Aquifers** are two-zone subsurface stores beneath parcels, holding an
unsaturated moisture content and a saturated-zone depth. They receive
percolation, lose water to deep percolation and evapotranspiration, and exchange
flow with a designated network vertex through a parameterised relation — the
mechanism by which baseflow and groundwater infiltration enter sewers.

**Unit-hydrograph groups** describe the delayed entry of stormwater into
sanitary sewers through defects and illicit connections. Each converts a unit of
instantaneous precipitation into a triangular response defined by a fraction of
precipitation volume, a time to peak, and a recession ratio, organised in groups
of up to three per month.

### 2.6 Network Vertices

Every vertex has an invert elevation and may receive **external inflows** in
addition to the runoff and interflow delivered by areal entities. A direct
external inflow of water or of any constituent is the composite

$$Q_{ext}(t) = c_f\left[s_f\,S(t) + Q_{base}\,P(t)\right]$$

where $S(t)$ is an optional supplied series, $s_f$ its scale factor, $Q_{base}$ a
constant baseline, $P(t)$ that baseline's periodic modulation, and $c_f$ a units
factor. Sanitary dry-weather inflow multiplies an average value by up to four
periodic modulations of distinct periods.

There are four vertex kinds.

**Junctions** are connection points — manholes, fittings, confluences — with
negligible storage. A junction has a maximum depth at ground level; when the
hydraulic grade line reaches it, excess water either leaves the system or, where
ponding is enabled, is stored above the vertex over a specified area and
returned as capacity recovers.

**Outfalls** are terminal boundary vertices where water leaves to a receiving
body. The boundary condition is a free outfall (the smaller of critical and
normal depth in the connecting channel), normal depth, a fixed stage, a periodic
tidal stage, or a supplied stage series. A staged condition governs only where
it exceeds the critical-depth elevation. An outfall may carry a gate blocking
reverse flow, and may return its discharge onto a parcel.

**Storage units** are vertices with significant free-surface storage — ponds,
wet wells, detention basins, chambers. Geometry is given as surface area against
depth, by a functional relation, a tabulated relation, or one of a set of
analytical forms. Storage units may lose water to evaporation and to seepage.

**Flow dividers** split inflow between two outgoing channels by a prescribed
rule. A divider is a modelling abstraction rather than a physical structure: it
imposes a split that the momentum equations would otherwise determine. Its
treatment is specified in §7.

### 2.7 Network Edges

An edge connects two vertices; its orientation defines the positive flow
direction, and a negative flow denotes reversal.

**Channels** are the pipes and conduits of the network — the only edge kind with
hydraulic length, and the primary object of flow routing. Each carries a
cross-sectional profile, specified in §5, drawn from a library of standard
closed and open geometries, a surveyed irregular profile, a street
cross-section, or a supplied width-against-depth relation.

A channel additionally carries a roughness coefficient; invert offsets above its
end vertices; an optional count of identical parallel barrels; an optional
maximum-flow limit; optional entrance and exit loss coefficients; an optional
gate preventing reverse flow; and optional seepage and evaporation losses. A
channel may be designated a culvert, activating inlet-control capacity limits,
or a force main, using a pressurised friction relation while full (§7).

Channel slope is the invert drop over the *horizontal* distance,

$$S_0 = \frac{\Delta z}{\sqrt{L^2 - \Delta z^2}}$$

where $\Delta z$ is the invert drop and $L$ the channel length. Admissibility of
$\Delta z$ relative to $L$, and the treatment of adverse slopes, are part of
the validation-and-mutation contract of §14.

**Pumps** raise water between vertices according to a characteristic relating
delivered flow to wet-well volume, inlet depth, or delivered head, optionally
with variable speed. An ideal transfer pump sets outflow equal to inflow. Pumps
have activation and deactivation depths and may be modulated by control.

**Orifices** are openings in the wall or floor of a vertex, closable to a
variable degree, with free, submerged, and partially open discharge regimes.

**Weirs** are overflow structures — transverse, side-flow, V-notch, trapezoidal,
and embankment — each with a characteristic head-discharge relation, with
corrections for end contractions, submergence, and surcharge.

**Outlets** are general-purpose head-discharge devices whose outflow is an
arbitrary function of head or depth. They represent devices fitting none of the
standard structures.

**Street sections and inlets** pair a roadway cross-section with an inlet
capacity relation to model dual drainage — flow on the street surface captured
by inlets and entering the sewer below. Specified in §7.

### 2.8 Constituents and Land Use

**Constituents** are user-defined substances, any number of them, carried by
runoff and routed through the network. Each has a concentration unit, optional
background concentrations in precipitation, groundwater, infiltration, and
sanitary flow, an initial network concentration, a first-order decay
coefficient, and optionally a fixed-fraction relation to another constituent.

**Land uses** partition a parcel's area into categories governing material
accumulation during dry weather and mobilisation during runoff, together with
removal parameters representing periodic street cleaning.

### 2.9 Tabulated Relations and Series

**Tabulated relations** carry a typed role — storage geometry, diversion, tidal
stage, pump characteristic, outlet rating, control mapping, cross-section shape,
and weir coefficient. Interpolation travels with the role, not with the table.

**Series** are timestamped value sequences supplying precipitation, boundary
stage, external inflows, and evaporation. Behaviour outside a series' range is
specified with its consumer.

**Periodic modulations** are repeating multiplier sets — monthly, daily, hourly,
and weekend-hourly — applied to sanitary inflows and external-inflow baselines.

### 2.10 State

The simulation is distributed and discrete in time. The system state advances as
$X_t = f(X_{t-1}, I_t, P)$, with outputs $Y_t = g(X_t, P)$, where $I_t$ are
external inputs — precipitation, temperature, boundary stages, control settings
— and $P$ the constant parameters.

The state is small relative to the model's scope:

| Entity | State |
|---|---|
| Parcel | Ponded depth for each of the three sub-areas independently; the infiltration state of the chosen relation; groundwater moisture content and saturated-zone depth; snow-pack depth, free water, temperature, and cold content |
| Network vertex | Water depth |
| Channel | Discharge and flow area |
| Constituent | Accumulated surface mass and ponded mass per parcel, with time since last removal; concentration per vertex and per edge |

Everything else the engine reports — velocities, volumes, flooding, loads — is
derived from this state, the inputs, and the parameters.

This state defines what initial conditions a user must supply, and marks the
seam between the surface and network halves of the engine, which advance on
different time scales (§10). It also defines what a checkpoint must persist; the
persistence contract is specified in §12.

### 2.11 Units and Physical Constants

All internal computation is carried in SI. Conversion occurs only at boundaries
— when reading a model, writing results, and at the programmatic interface —
and each boundary is named where it is specified.

Physical constants take their exact standard values:

| Constant | Value |
|---|---|
| Gravitational acceleration $g$ | 9.80665 m/s² |
| Density of water and its temperature dependence | as specified where used |
| Kinematic viscosity of water | as specified where used, at the stated temperature |

> **CORRESPONDENCE:** the predecessor computes internally in US customary units
> and uses $g = 32.2$ ft/s² (9.81456 m/s²), 0.08 % above standard, together with
> a second value of 32.16 ft/s² local to its inlet capacity relations. This
> engine uses the exact standard value throughout. The resulting difference in
> discharge through a head-driven structure is below 0.05 %, one to two orders
> of magnitude smaller than the uncertainty in the empirical coefficients those
> relations carry, so no coefficient is refitted.
>
> *Source: `consts.h:38–39`; the second value at `inlet.c:1666` and `:1749`.*

A constant that embeds a unit system — a coefficient differing between US
customary and SI forms of the same relation — is identified as such where it
appears, rather than presented as dimensionless.

Concentrations are expressed in mass or count per unit volume independently of
the unit system in which a model was supplied.

Certain relations inherited from the predecessor are defined only in terms of a
particular unit system, their coefficients changing meaning with it. These are
enumerated and given their conversion treatment in §14, since the requirement
originates at the interoperability boundary rather than in the physics.
