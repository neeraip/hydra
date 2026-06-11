# hydra-engine — Analysis Sub-Specification

This document is the analysis sub-specification for `hydra-engine`.

## 1. Overview

The analysis module owns post-simulation analytics that are more expensive than
interactive UI transformations. Its outputs are persisted as an analysis
artifact so interfaces can render rich summaries without heavy on-the-fly
compute.

`hydra-engine`'s analysis module does not run hydraulics or quality simulation. It consumes
completed simulation outputs and publishes derived statistics.

---

## 2. Design Goals

1. GUI and CLI consumers must be able to read analysis results directly from a
persisted file with no expensive recomputation.
2. Analysis compute must be deterministic for a given simulation output.
3. The artifact must be versioned so schemas can evolve safely.

---

## 3. Artifact Contract

See `encode_analysis_artifact` in `analysis/artifact.rs` for the file schema,
location conventions, and stale-on-edit invalidation semantics.

---

## 4. Computation Ownership

`hydra-engine`'s analysis module owns:

1. Full-run histogram aggregation across time steps.
2. Percentile/quantile computations over simulation outputs.
3. Threshold exceedance counts and ratios.
4. Cross-variable summary statistics intended for dashboards.
5. Service-compliance analytics over node pressure time series.
6. Demand-delivery reliability analytics over junction demand time series.

On-demand analysis may be scoped to a caller-selected metric subset to reduce
compute time on large networks. The current selectable distribution modules are:

1. Pressure distribution
2. Head distribution
3. Flow distribution
4. Velocity distribution
5. Link-status distribution

If no selection is provided, implementations must compute all modules for
backward compatibility.

For persisted `.out` inputs, histogram/distribution analysis is computed using a
streaming two-pass scan over reporting periods:

1. Pass 1 computes exact min/max/mean inputs and status counts.
2. Pass 2 bins values into fixed histograms derived from pass-1 ranges.

To avoid unbounded memory growth on long-duration networks, implementations must
not require materializing all per-period values in memory for persisted-output
analysis. Percentiles may be estimated from histogram bins as long as the
method is deterministic for the same input artifact.

Status distribution values are aggregated over all link-time samples (all links
across all reported periods), not only a single period. For persisted `.out`
analysis, status codes are interpreted using EPANET `StatusType` values and
collapsed as follows:

1. `OPEN`: code 3
2. `ACTIVE`: codes 4 and 6
3. `CLOSED`: codes 0, 1, 2, and 7
4. `OTHER`: any unmapped code

Interface crates may still perform lightweight transformations on already
persisted arrays (for example, formatting labels), but they must not perform
bulk aggregation over all periods at render time.

`hydra-engine`'s analysis module performs analysis computation and byte-level
encode/decode against the analysis artifact types defined in `../model/spec.md` §4.4; it does not
define competing schemas.

Service-compliance and demand-reliability modules may be computed on demand and
returned directly to the caller without extending `analysis.json`. These
modules are still owned by `hydra-engine`'s analysis module and must remain deterministic for a
fixed input.

### 4.1 Service Compliance Module

See `compute_service_compliance_from_out` in `analysis/service_compliance.rs`
for inputs, outputs, and the streaming-pass memory contract.

### 4.2 Demand Reliability Module

See `compute_demand_reliability_from_out` in `analysis/demand_reliability.rs`
for inputs, outputs, and the streaming-pass memory contract.

---

## 5. Invalidation

See `encode_analysis_artifact` for the stale-on-edit invalidation rule.

---

## 6. Runtime Estimation API

See `estimate_simulation_runtime` in `simulation/estimator.rs` for the cost
model and determinism guarantee. Inputs: node count, link count, period count,
and selected analysis modules. Output: `RuntimeEstimate` (`Low`/`Medium`/`High`).
