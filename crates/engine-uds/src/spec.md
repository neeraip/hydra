# hydra-engine-uds — Urban Drainage Specification

This document is §1 of the urban drainage specification. It states the engine's
purpose and scope, the compatibility obligations every other document inherits,
the registry that assigns section numbers across the specification, and the
conventions all of its documents are written to.

---

## 1. Overview and Scope

### 1.1 Purpose and Domain

The urban drainage engine simulates the quantity and quality of runoff from
urban catchments and its conveyance through drainage systems — storm sewers,
sanitary and combined sewers, open channels, storage units, and flow regulators
— over single events or continuous multi-year periods.

Its physics differ in kind from those of the water distribution engine, and the
distinction is worth stating because it drives every design decision that
follows. A distribution network is pressurised and solved for equilibrium: every
node has a head and every link a flow that jointly satisfy conservation at an
instant. A drainage network is a **rainfall-runoff-routing** system with a free
surface. Precipitation falls on subcatchments, becomes runoff after losses to
infiltration, evaporation, and depression storage, and the resulting hydrographs
are routed through the conveyance network by solving forms of the Saint-Venant
equations. Flow may be driven by gravity or pumped; conduits may transition
between open-channel and pressurised (surcharged) states; and the network admits
backwater, flow reversal, ponding, and tidal boundary conditions.

Three consequences follow, and each shapes a later document:

- **The state is a hydrograph, not an equilibrium.** Time is intrinsic. There is
  no meaningful steady solution to fall back on, and the time step is itself a
  computed quantity rather than a user setting alone.
- **The domain has a compartment the distribution engine has no analogue for.**
  Hydrology — meteorology, infiltration, groundwater, snowmelt, and
  rainfall-dependent infiltration and inflow — generates the loads that
  conveyance then routes. It is a peer subsystem, not a boundary condition.
- **Not every element is a node or a link.** A subcatchment is a surface, and
  land use, pollutant, and control-rule objects describe behaviour rather than
  topology. Any shared element schema must accommodate this without forcing a
  drainage model into a two-kind topology it does not have.

### 1.2 Relationship to SWMM

This engine operates on the SWMM data model, as the water distribution engine
operates on the EPANET data model. Its conceptual and mathematical reference is
the accompanying analysis of SWMM 5.2.4, which describes how the predecessor
engine works: its algorithms, constants, empirical relations, and numerical
devices.

**The analysis is the map of the predecessor; this specification is the design of
the successor.** They are different documents with different obligations. The
analysis is faithful to SWMM including its defects, and is pinned to a tagged
release so that "SWMM-compatible" has a precise meaning. This specification
draws concepts, mathematics, and algorithms from it but owes it no fidelity
beyond §1.3. Rewriting SWMM is explicitly not the goal.

Where this specification and the analysis differ, the declarative register is
used: *the analysis gives X; this specification requires Y*, followed by the
reason. Never an advisory or apologetic register.

### 1.3 Compatibility Commandments

Two obligations bind every document in this specification. They are the only
respects in which the predecessor constrains the successor.

#### 1.3.1 File compatibility is absolute

Input and output files must remain compatible with the predecessor's, both
**structurally** and **functionally**.

- **Structural** compatibility means the file parses: every section, keyword,
  field order, and numeric format the predecessor accepts is accepted here, and
  every file this engine writes is readable by the predecessor's own readers.
- **Functional** compatibility means the same file describes the same network.
  A parser that reads every field correctly and then builds a different model
  has failed this obligation while satisfying the first.

Functional compatibility makes every **validation-time mutation** part of the
file contract rather than an implementation curiosity. The predecessor silently
adjusts a model as it loads it — raising a weir crest to the downstream invert,
reversing an adverse-slope conduit, lifting a node's depth to its highest crown,
compiling a street into a transect, converting offsets, enlarging an infeasible
radius. Each such mutation is a rule about what the input file *means*, and is
specified as such rather than left to be rediscovered.

This obligation is a floor on behaviour, not a ceiling on capability. It
constrains what files mean, not how results are computed.

#### 1.3.2 The interior is free

Subject to §1.3.1, any algorithm may be replaced wherever accuracy or
performance improves. Result values need not match the predecessor's and are
expected to differ. The standard is that accuracy and performance are at least
close to, and preferably better than, the predecessor's.

This freedom is exercised with a distinction in mind. The predecessor's numbers
carry different kinds of authority, and only the first two are binding:

| Kind | Example | Obligation |
|---|---|---|
| Physical law | conservation of mass and momentum | Adopt |
| Empirical fit | roughness relations, buildup and washoff coefficients | Adopt; record provenance |
| Numerical device | an artificial slot admitting pressurised flow to a free-surface scheme | Free to replace |
| Legacy workaround | a fixed-interval snap compensating for an integer clock | Replace, and say so |

Every deliberate divergence carries a **DEVIATION** note at the point of
divergence, naming what the predecessor does, what this engine does instead, and
why. A divergence without a note is a defect, not a feature.

#### 1.3.3 Extensions are opt-in

Capability beyond the predecessor's is welcome where genuinely rewarding, on one
condition: a file that does not use the extension behaves exactly as it would
without it. An extension may add meaning to input the predecessor rejects or
ignores, but may never change the meaning of input the predecessor accepts.

### 1.4 Specification Structure

The specification is one numbered sequence distributed across several documents.
Section numbers are **globally unique**: a reference to §7.3 identifies exactly
one section, from anywhere in the specification, without naming a document.

Each document owns a disjoint, contiguous range of top-level sections:

| Sections | Document | Subject |
|---|---|---|
| 1 | Overview | Purpose, compatibility, structure, conventions |
| 2–5 | Model | Data model, unit system, file formats, validation |
| 6–8 | Routing | Cross-section geometry, flow routing, structures and regulators |
| 9–10 | Hydrology | Runoff, infiltration, groundwater, snowmelt, RDII; LID controls |
| 11 | Quality | Buildup, washoff, transport, treatment |
| 12–15 | Simulation | Controls, time stepping, continuity accounting, session API |
| 16 | Analysis | Post-simulation analytics |

Three rules keep this scheme intact:

1. **A new top-level section takes the next free number within its owning
   document's range.** It never restarts at 1, and never borrows a number from
   another range.
2. **Ranges are extended, not reused.** If a document exhausts its range, the
   registry above is amended — by extending that range and renumbering what
   follows, or by appending a new range — as a deliberate, reviewable change.
3. **Documents are addressed by subject, never by storage location.** Which
   file a range lives in is a wiring detail. A range may be split across
   documents, or documents merged, without altering a single cross-reference.

The registry exists because the alternative has been tried. A specification cut
into per-subsystem documents that each begin at §1 produces duplicate section
numbers, references that resolve to several candidate sections, and eventual
citation by file and line — an addressing scheme that has stopped working.

### 1.5 Conventions

**Language.** These documents are language- and platform-agnostic. They specify
behaviour, not its realisation: no programming language, module layout, data
structure, or memory representation appears in them. British spelling is used
throughout.

**Mathematics.** Formulae are given in display mathematics, with every symbol
defined on first use in an accompanying sentence. Inline mathematics is reserved
for trivial or auxiliary expressions. Eponymous pairs take an en-dash —
Green–Ampt, Saint-Venant, Cash–Karp — rather than a hyphen.

**Units.** Quantities are carried internally in SI. Where the predecessor's file
formats, empirical relations, or reported values are expressed in other units,
the conversion is specified at the boundary where it occurs, and the boundary
itself is named. A constant embedding a unit system — a coefficient that differs
between US customary and SI forms of the same relation — is identified as such
rather than presented as dimensionless.

**Worked examples.** A numeric example carries the input values, the intermediate
quantities, and the result. Where an example is given in both unit systems, each
is computed independently from the physical quantities; neither is derived from
the other by conversion. Examples that cross-check only against each other have
been observed to carry identical errors through review.

**Deviations.** A divergence from the predecessor is recorded as a blockquote
beginning `**DEVIATION from SWMM:**`, at the point of divergence.

**Gaps.** Where a decision is required that this specification does not make,
that is a defect in the specification. It is recorded and resolved here before
implementation proceeds — never decided in implementation.

**Concurrency.** Operations that may be performed concurrently are marked **∥**.
An operation not so marked is specified as sequential, and implementing it
concurrently is a deviation requiring the same treatment as any other.

### 1.6 Status

<!-- PLANNED-ENGINE: uds — replace this subsection with the engine's supported
     capability set when the urban drainage engine ships. -->

This specification is under development and the engine is not yet implemented.
The urban drainage engine is registered as *planned*: its identity and import
formats are declared, and models cannot yet be opened or simulated. Documents
outside §1 are written in the order the registry lists them, and a section
absent from the specification is unspecified behaviour rather than deferred
behaviour — it is not implemented until it is specified.
