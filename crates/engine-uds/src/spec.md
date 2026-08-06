# hydra-engine-uds — Urban Drainage Specification

This document is §1 of the urban drainage specification. It states what the
engine solves, what it owes the predecessor it interoperates with, the registry
that assigns section numbers, and the conventions the other documents inherit.

---

## 1. Overview and Scope

### 1.1 What This Engine Solves

The engine simulates the movement of water and waterborne material through an
urban drainage system: from precipitation falling on land, through the surface
and subsurface, into a network of channels, pipes, and structures, and out to
receiving waters.

Three mathematical problems compose it.

**Surface and subsurface water balance.** Each parcel of land is a store whose
depth $d$ evolves under precipitation, evaporation, infiltration, and outflow,

$$\frac{\mathrm{d}d}{\mathrm{d}t} = i - e - f - q(d)$$

where $i$ is the precipitation rate, $e$ the evaporation rate, $f$ the
infiltration rate, and $q(d)$ the runoff rate from the parcel once its depth
exceeds the depth retained in surface depressions. Infiltration is the dominant
subtraction and admits several constitutive forms. Beneath the surface, moisture
and a water table evolve as a coupled pair, returning part of the infiltrated
water to the network as interflow. Where snow is present, an energy-balance
store precedes the water balance. The output is a time series of flow and
constituent load per parcel.

**Free-surface flow on a network.** The conveyance system is a directed graph
whose edges carry the one-dimensional shallow-water equations,

$$\frac{\partial A}{\partial t} + \frac{\partial Q}{\partial x} = q_L,
\qquad
\frac{\partial Q}{\partial t}
+ \frac{\partial}{\partial x}\!\left(\frac{Q^2}{A}\right)
+ gA\frac{\partial H}{\partial x}
+ gAS_f = 0$$

where $A$ is flow area, $Q$ discharge, $x$ distance along the channel, $q_L$ the
lateral inflow per unit length supplied by the surface balance, $H$ the
hydraulic head, $S_f$ the friction slope, and $g$ gravitational acceleration.
Vertices close the system with continuity between the flows incident on them and
the rate of change of their stored volume. Pumps, weirs, orifices, and other
structures enter as internal boundary conditions relating discharge to head
across a vertex pair; outfalls impose external ones.

**Constituent transport.** Material accumulates on surfaces between storms,
is mobilised by runoff, and is then advected through the network subject to
decay and treatment. Concentration does not influence flow.

**The coupling is a cascade, one step at a time.** The surface balance
supplies the network as a source term; the network supplies transport as a
velocity field. Two influences do run backwards — a subsurface discharge
relation may read the receiving vertex's routed stage, and sewer surcharge
returns to the street through inlets — but both are lagged one step: within
any single step the influence graph is loop-free. That per-step
directedness is a structural property of the problem, not of any
implementation of it, and it is what permits the three processes to advance
on separate time scales.

### 1.2 What Is Genuinely Difficult

Two features distinguish this problem from a pressurised-network one, and the
specification is organised around them.

**Conduits change state.** A conduit may run partly full with a free surface,
or completely full and pressurised. The shallow-water equations presuppose a
free surface; a full pipe has none. The transition between the two regimes is
the central numerical difficulty of network hydraulics, and a large share of the
predecessor's machinery exists to negotiate it.

**The system is driven, not steady.** There is no equilibrium to fall back on.
Time is intrinsic, the forcing is a measured or synthetic rainfall record, and
the answer is a hydrograph. Accuracy therefore means accuracy in time — peak
magnitude, peak timing, and volume — not the residual of a converged snapshot.

### 1.3 Relationship to SWMM

SWMM is this engine's predecessor, and is three distinct things to this
specification. Distinguishing them is the point of this subsection.

1. **A domain reference.** SWMM records which phenomena matter in urban
   drainage and which constitutive relations are established engineering
   practice — infiltration models, buildup and washoff forms, structure
   ratings, the vocabulary practitioners model in. This is inherited.
2. **An interoperability boundary.** Its file formats are how models and
   results move between this engine and the rest of the world. This is
   honoured, and is specified in §14.
3. **A set of numerical methods.** This is *not* inherited. SWMM's solution
   strategy was shaped by the computing hardware of its era, and this engine is
   free to solve the same physics by any means that is at least as accurate.

The accompanying analysis of SWMM 5.2.4 documents the predecessor faithfully,
including its defects. It is the map of the predecessor; this is the design of
the successor. Where the two differ, the declarative register is used: *the
predecessor does X; this engine does Y*, followed by the reason.

**Every claim these specifications make about the predecessor is made against
SWMM 5.2.4**, in the Open Water Analytics build at commit `27dc699`. Where a
`CORRESPONDENCE` note cites a file and line — `culvert.c:209` — it is that
source that is meant, and the citation is there so a reader can check the claim
rather than take it. A claim about someone else's software that cannot be
checked is an assertion, and this engine's whole argument for departing from
the predecessor rests on those claims being true.

### 1.4 Obligations, in Three Tiers

Compatibility with the predecessor is not one obligation but three, of
decreasing strength. Conflating them is what leads an engine to inherit defects
in the name of fidelity.

#### Tier 1 — Interoperability (binding)

A model expressed in the predecessor's input format is read, and is understood
to mean what its author meant. Results are written in formats the predecessor's
readers accept. This tier is not negotiable: it is the entire reason the formats
are supported.

It binds **syntax and interpretation** — that a field is accepted, and that its
value denotes the quantity its author intended. It does not bind the engine to
reproduce arithmetic, and it does not make the predecessor's incidental
behaviours part of the model.

#### Tier 2 — Result correspondence (bounded)

Results are at least as accurate as the predecessor's, judged against
measurement or analytical solution rather than against the predecessor's output.
Where results differ from the predecessor's in a way a user would notice, the
difference is attributable to a stated improvement, and is recorded as a
**CORRESPONDENCE** note at the point where it arises.

Agreement with the predecessor is evidence, not the objective. A divergence
that moves results toward the truth satisfies this tier; one that cannot be
explained does not, and is a defect.

Evidencing correspondence differs from an engine that had a prior
implementation to difference against: no predecessor-faithful mode exists
here, and building one would contradict Tier 3. Three instruments serve
instead: **analytic solutions** where the mathematics admits them;
**self-convergence**, comparing a coarse-step run against the same model at a
refined step; and **corpus comparison** against the predecessor's own
validation networks, with every visible divergence attributed to a stated
cause. An unattributed divergence is a defect regardless of which engine is
right.

#### Tier 3 — Method (free)

How the equations of §1.1 are discretised, integrated, and solved is entirely
this engine's own. No obligation attaches to the predecessor's choice of scheme,
iteration strategy, step control, or internal representation.

#### Triage

Every behaviour of the predecessor falls into exactly one of three classes, and
the class determines its standing here:

| Class | Example | Standing |
|---|---|---|
| **File syntax** | field order, keyword spelling, record layout | Tier 1 — reproduced |
| **Model semantics** | what a parameter denotes, what a structure does | Tier 1 — the *intent* is reproduced; an incidental consequence of the predecessor's implementation is not |
| **Numerical artifact** | stability limiters, iteration schemes, lookup tables, floors that keep a quantity finite | Tier 3 — no standing; evaluated on merit |

Classifying a behaviour is itself a specification act. Where a behaviour's class
is disputable, the classification is stated with its reasoning rather than
assumed.

### 1.5 Numerical Devices Are Not Requirements

The predecessor's method carries a body of scaffolding that exists to make its
particular scheme workable rather than to represent anything physical:
mechanisms for admitting pressurised flow into a free-surface formulation,
damping terms that suppress inertia to preserve stability, transformations that
lengthen short conduits, floors on top width and surface area that keep
quantities from collapsing, tabulated geometry standing in for closed-form
evaluation, and fixed-relaxation iteration in place of a convergent solve.

None of these is inherited by default. Each is evaluated on whether a scheme
chosen under §1.4 Tier 3 still needs it, and is adopted only if it does.

Two cautions apply. First, several of these devices **change computed results**,
so removing one is not a neutral act and its effect is recorded under Tier 2.
Second, a device that compensates for a deficiency in a scheme this engine does
not use may still encode a real physical limit; the specification distinguishes
the two before discarding either.

What replaces a discarded device is held to a positive standard: **every state
the engine integrates carries a local error estimate, and step control is
accuracy-driven, with stability as a constraint rather than the criterion.**
The predecessor error-controls its hydrology — an embedded-pair integrator
with step rescaling — yet governs its routing clock by stability alone, with
no measure anywhere of the error committed. That asymmetry is not carried
forward. Where the engine cannot meet a tolerance at its smallest permitted
step, it proceeds and says so, per entity: degraded accuracy is reported,
never silent.

Error control implies rejection, and rejection implies that a time step is a
**transaction**: a trial that can be discarded with no surviving effect. The
simulation contract (§10) requires rejectable trial steps as a structural
property — state designed to be snapshotted and restored — because
retrofitting reversibility onto a solver that mutates in place is a rewrite,
and the same property is what a mid-run checkpoint (§12) persists.

### 1.6 Specification Structure

The specification is one numbered sequence distributed across several documents.
Section numbers are **globally unique**: a reference to §7.3 identifies exactly
one section, from anywhere in the specification, without naming a document.

Each document owns a disjoint, contiguous range:

| Sections | Document | Subject |
|---|---|---|
| 1 | Overview | Purpose, obligations, structure, conventions |
| 2 | Domain | Entities, state, units |
| 3–4 | Hydrology | Surface water balance; subsurface and snow |
| 5–7 | Hydraulics | Cross-section geometry; network flow; structures |
| 8 | Transport | Constituent buildup, washoff, advection, treatment |
| 9–12 | Simulation | Operational control; time integration and coupling; conservation; session interface |
| 13 | Analysis | Post-simulation analytics |
| 14 | Interoperability | Predecessor file formats, import, and export |

The ordering is deliberate. The physics is specified first, on its own terms;
interoperability is specified last, as an adapter between the predecessor's
formats and a model defined independently of them. A reader should be able to
understand everything this engine computes without reading §14.

Three rules keep the scheme intact:

1. **A new top-level section takes the next free number within its owning
   document's range.** It never restarts at 1, and never borrows from another
   range.
2. **Ranges are extended, not reused.** If a document exhausts its range, this
   registry is amended as a deliberate, reviewable change.
3. **Documents are addressed by subject, never by storage location.** Which
   file a range occupies is a wiring detail, and a range may move between
   documents without altering a single cross-reference.

### 1.7 Conventions

**Language.** These documents are language- and platform-agnostic. They specify
behaviour, not its realisation: no programming language, module layout, data
structure, or memory representation appears in them. British spelling is used
throughout.

**Mathematics.** Formulae are given in display mathematics, with every symbol
defined on first use in an accompanying sentence. Inline mathematics is reserved
for trivial or auxiliary expressions. Eponymous pairs take an en-dash —
Green–Ampt, Saint-Venant, Cash–Karp — rather than a hyphen.

**Units.** Quantities are carried internally in SI. Conversion occurs only at
boundaries, and each boundary is named where it is specified. A constant that
embeds a unit system is identified as such rather than presented as
dimensionless. Physical constants take their exact standard values; a rounded
constant is adopted only where it is inseparable from an empirical relation
fitted with it, and then the pairing is stated.

**Worked examples.** A numeric example carries its inputs, intermediate
quantities, and result. Where an example is given in more than one unit system,
each is computed independently from the physical quantities; neither is derived
from the other by conversion. Examples that cross-check only against each other
have been observed to carry identical errors through review.

**Correspondence notes.** A difference from the predecessor that a user would
observe in results is recorded as a blockquote beginning
`**CORRESPONDENCE:**`, at the point where it arises, naming what the predecessor
computes, what this engine computes, and why the difference is an improvement.

**Gaps.** Where a decision is required that this specification does not make,
that is a defect in the specification. It is recorded and resolved here before
implementation proceeds — never decided in implementation.

**Concurrency.** Operations that may be performed concurrently are marked **∥**.
An operation not so marked is specified as sequential, and implementing it
concurrently is a change requiring the same treatment as any other.

### 1.8 Status

The urban drainage engine is registered as *available*: models are opened,
validated, simulated, and written per this specification's sections — model
and import (§2, §14), hydrology (§3, §4), section geometry (§5), dynamic-wave
routing (§6), structures and inlets (§7), transport (§8), controls (§9),
orchestration (§10), accounting and statistics (§11), the session (§12), and
interoperability (§14).

Deferred capabilities are typed refusals, never approximations: a model
needing one is refused with a named reason. Currently deferred: rainfall,
runoff, and RDII *interface-file formats* (§14.8 specifies routing and
hotstart files; the other three are declared but not yet served),
file-sourced rain gages (supply the record as a series), the engine's own
native checkpoint format (§12.3 — predecessor hotstart files version 3+
are served), and archival climate-record formats (user-format climate
files are served). A section absent from the specification remains unspecified
behaviour rather than deferred behaviour — it is not implemented until it
is specified.
