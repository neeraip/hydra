# Numerical Methods

This page names the mathematics each engine runs, and points to where every
equation is written down. The authoritative statements live in the engine
specifications: language-agnostic documents that define each solver's
equations, constants, convergence criteria, and every deliberate deviation
from the predecessor tools, with worked numeric examples in both unit
systems. The [Specifications](specs.md) page lists them all.

## Water distribution (`wds`)

**Hydraulics.** The engine solves extended-period network hydraulics with
the Global Gradient Algorithm: a Newton iteration over nodal heads, with the
per-iteration linear system factorised by sparse Cholesky over a
fill-reducing ordering. Head-loss models (Hazen-Williams, Darcy-Weisbach
with the full friction-factor regimes, Chezy-Manning), valves, pumps, and
pressure-driven demand enter the iteration as the specification defines
them.

**Convergence.** A step converges on relative flow change across the
network, and the specification additionally requires the mass-balance
residual to close. Both criteria, their defaults, and their deviations from
EPANET's stopping rule are stated in the
[hydraulics specification](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/hydraulics/spec.md).

**Water quality.** Transport is Lagrangian: constituent mass moves in
segments advected along links, with first- and zero-order bulk and wall
reactions, four tank-mixing models, and source injection. Water age and
source tracing are the same machinery with different reaction terms. The
[quality specification](https://github.com/neeraip/hydra/blob/main/crates/engine-wds/src/quality/spec.md)
defines the segment algebra and the mass-balance accounting.

## Urban drainage (`uds`)

**Routing.** The engine routes the full one-dimensional Saint-Venant
equations with the dynamic wave, in every model. Closed conduits stay on
the same equations through surcharge via the Preissmann slot: a narrow
hypothetical slot above the crown whose width is derived from a stated
pressure-wave celerity,

$$w_{slot} = \frac{g\,A_{full}}{c^2}$$

so pressurisation is a property of the geometry rather than a separate
equation branch. The
[hydraulics specification](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/hydraulics/spec.md)
derives the closure and states the iteration and error control around each
routing step.

**Hydrology.** Runoff is a nonlinear reservoir per subcatchment, fed by the
gage network and drained by the infiltration families (Horton and Modified
Horton, Green-Ampt and Modified Green-Ampt, Curve Number), LID layers,
snowmelt, groundwater, and RDII unit hydrographs. The
[hydrology specification](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/hydrology/spec.md)
defines each process.

**Water quality.** Buildup, washoff, treatment expressions, and network
transport are defined in the
[transport specification](https://github.com/neeraip/hydra/blob/main/crates/engine-uds/src/transport/spec.md).

## Relationship to the predecessors' mathematics

The [Theory](../theory/epanet-analysis.md) section holds Hydra's analyses of
EPANET and SWMM themselves: pinned studies of the predecessor solvers that
the engines were designed against. They describe EPANET and SWMM, not
Hydra; where Hydra deliberately deviates, the engine specifications label
the deviation and the reason.
