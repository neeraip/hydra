# SDK Overview

`hydra-sdk` is the umbrella crate for Hydra's public API. Add it to your `Cargo.toml`:

```toml
[dependencies]
hydra-sdk = "1"
```

It re-exports every type needed to parse networks, run simulations, query results, and run post-simulation analytics — with all internal dependency versions pre-pinned.

## Modules and Key Types

### Session API

The primary entry point. Import `Simulation` to parse, run, and query a network.

| Type / function | Purpose |
|---|---|
| `Simulation` | Creates and drives a simulation session |
| `SessionError` | Error type returned by all session methods |
| `SimWarning` / `WarningKind` | Non-fatal diagnostics produced during a run |
| `NodeQuantity` | Enum of per-node result variables (Head, GaugePressure, Demand, Quality) |
| `LinkQuantity` | Enum of per-link result variables (Flow, MeanVelocity, UnitHeadLoss, FrictionFactor, Quality, Status, Setting) |
| `NodeResult` / `LinkResult` | Batch result containers |
| `ResultRanges` | Min/max envelopes across all nodes/links/time |
| `HydSnapshot` | Single-step hydraulic state snapshot |
| `PumpEnergy` | Per-pump energy and efficiency metrics |
| `FlowBalance` / `MassBalance` | Network-wide accounting at simulation end |
| `WritableSimulation` | Trait required by the I/O writers |

### Analytics

Post-simulation analysis functions that operate on a saved `.out` file.

| Type / function | Purpose |
|---|---|
| `compute_demand_reliability_from_out` | Per-junction demand reliability metrics |
| `compute_service_compliance_from_out` | Per-node pressure compliance metrics |
| `DemandReliabilityReport` / `DemandReliabilitySummary` | Demand reliability results |
| `ServiceComplianceReport` / `ServiceComplianceSummary` | Pressure compliance results |
| `DemandReliabilityNode` / `ServiceComplianceNode` | Per-node entries within each report's `nodes` list |
| `DemandReliabilityOptions` | Options for reliability computation (deficit tolerance) |
| `compute_demand_reliability_from_out_with_options` | Reliability variant taking explicit `DemandReliabilityOptions` |
| `ServiceComplianceThresholds` | Min/max pressure thresholds for compliance check |

### Data Model

The full network data model, mirroring the EPANET `.inp` structure.

| Type | Purpose |
|---|---|
| `Network` | Top-level container returned by `io::parse` |
| `Node` / `NodeKind` | Polymorphic node (Junction, Reservoir, Tank) |
| `Link` / `LinkKind` | Polymorphic link (Pipe, Pump, Valve) |
| `Pattern` / `Curve` | Time patterns and XY curves |
| `SimulationOptions` | All `[OPTIONS]` and `[TIMES]` settings |
| `QualityMode` | Chemical, age, or source-trace quality mode |
| `FlowUnits` / `HeadLossFormula` | Unit system and head-loss formula enums |
| `ValidationError` | Structural network validation errors |

### I/O

```rust
use hydra_sdk::io;
```

| Function / module | Purpose |
|---|---|
| `io::parse(&bytes)` | Parse EPANET `.inp` bytes into a `Network` |
| `io::write_inp(&network)` | Serialise a `Network` back to `.inp` bytes |
| `io::rpt_writer::build_text_report(&sim)` | Build a plain-text `.rpt` report string |
| `io::rpt_writer::build_json_report(&sim)` | Build a JSON report string |
| `io::out_writer::write_binary_output(&mut w, &sim, input_file, report_file, units)` | Write EPANET-compatible `.out` binary |
| `io::out_reader` | Read and inspect existing `.out` files |
| `io::analysis_io` | Read/write serialised analysis-artifact files |
| `io::compute_network_digest` | Stable content digest of a `Network` (also re-exported at the crate root) |

### Also re-exported

Beyond the tables above, `hydra-sdk` re-exports several supporting items:

- **Version constants** — `HYDRA_VERSION` and the per-subsystem `HYDRA_*_VERSION` strings.
- **Runtime estimation** — `estimate_simulation_runtime`, `estimate_simulation_runtime_from_summary`, `RuntimeEstimate`, and the analysis-side `estimate_analysis_runtime_millis`.
- **Analysis artifacts** — `build_analysis_artifact` (and `*_from_out` / `*_with_progress` / `*_with_progress_and_selection` variants), `encode_analysis_artifact`, `decode_analysis_artifact`, `AnalysisSelection`, `AnalysisComputeError`, `AnalysisBytesError`.
