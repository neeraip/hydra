# SDK Overview

`hydra-sdk` is the umbrella crate for Hydra's public API. Add it to your `Cargo.toml`:

```toml
[dependencies]
hydra-sdk = "4"
```

It re-exports every type needed to parse networks, run simulations, query results, run post-simulation analytics, and generate reports — with all internal dependency versions pre-pinned.

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
| `io::parse(&bytes)` | Parse EPANET `.inp` bytes into a `Network`, failing if the result would not be simulable |
| `io::parse_tolerant(&bytes)` | Parse and return the `Network` **with** its validation errors instead of failing — for editors and inspectors that must show an invalid model. A non-empty error list means it must not be simulated |
| `io::write_inp(&network)` | Serialise a `Network` back to `.inp` bytes |
| `io::rpt_writer::build_text_report(&sim)` | Build a plain-text `.rpt` report string |
| `io::rpt_writer::build_json_report(&sim)` | Build a JSON report string |
| `io::out_writer::write_binary_output(&mut w, &sim, input_file, report_file, units)` | Write EPANET-compatible `.out` binary |
| `io::out_reader` | Read and inspect existing `.out` files |
| `io::compute_network_digest` | Stable content digest of a `Network` (also re-exported at the crate root) |

### Engine Identity

```rust
use hydra_sdk::common;
```

Every Hydra engine publishes an immutable descriptor. Applications resolve a
project's stored engine key against the registry rather than hardcoding
names, colours, or file filters.

| Type / function | Purpose |
|---|---|
| `common::ENGINES` | Every engine compiled into this distribution, in presentation order |
| `common::engine_by_key(key)` | Resolve a key to its descriptor, or an `UnknownEngineError` |
| `common::EngineDescriptor` | `key`, `label`, `pill`, `accent`, `summary`, `status`, `import` |
| `common::EngineStatus` | `Available` or `Planned` — a planned engine is registered but has no implementation |
| `common::ImportFormat` | A source-model format the engine reads: `label` plus `extensions` |

`import` is a file-picker filter, never a validity test — `wds` and `uds` both
claim `.inp` with incompatible contents, so only the owning engine's parser
can decide whether a file really is its model.

### Reports

```rust
use hydra_sdk::{report, report_catalog, produce_report_block};
```

Report generation is split in two: the engine produces neutral content
fragments, and `report` turns them into documents. The report layer knows
nothing about engines.

| Type / function | Purpose |
|---|---|
| `report_catalog()` | The engine's block catalog — queryable without running a simulation |
| `produce_report_block(id, out_path, network, options)` | Materialise one block for a completed run |
| `report::ReportTemplate` | An ordered list of block references plus a document title (JSON) |
| `report::assemble(template, catalog, context, produce)` | Pair a template with a producer to build a render-ready document |
| `report::render_txt` / `render_csv` / `render_html` | Deterministic renderers — identical inputs give byte-identical output |
| `report::render_pdf` | Typeset PDF; behind hydra-sdk's `report-pdf` feature, and the only renderer that can fail (`PdfError`) |
| `common::BlockDescriptor` / `Fragment` | The catalog entry and produced-content types the two halves exchange |

### Also re-exported

Beyond the tables above, `hydra-sdk` re-exports several supporting items:

- **Version constants** — `HYDRA_VERSION` and the per-subsystem `HYDRA_*_VERSION` strings.
- **Runtime estimation** — `estimate_simulation_runtime`, `estimate_simulation_runtime_from_summary`, and `RuntimeEstimate`. The millisecond-level forms `estimate_simulation_runtime_millis_from_summary` and `classify_simulation_runtime_millis` are also available when you want the raw prediction or the bucketing separately.
- **Threshold binning** — `threshold_bands(values, edges)` counts values into the bands defined by ascending edges, with the outer two unbounded so nothing is dropped. It is the same binning the `wds.*-thresholds` report blocks use, so an interface presenting that view counts identically.
- **Threshold binning** — `threshold_bands`, the shared band-counting used by the `*-thresholds` report blocks, so an interface presenting the same view counts identically.
