# Crate Layout

Hydra is a multi-crate Rust workspace:

| Crate | Role |
|---|---|
| `hydra-common` | Foundation contracts shared by every engine and application: engine identity, and the reportable-output contract (block catalog, neutral fragment model). Depends on nothing else in the workspace |
| `hydra-engine-wds` | Water-distribution engine: data model, parsers, unit conversion, GGA hydraulic solver, Lagrangian quality engine, session API, analytics, report blocks |
| `hydra-engine-uds` | Urban-drainage engine — a published scaffold, deliberately empty until its development begins |
| `hydra-engine-och` | Open-channel engine — likewise a published scaffold |
| `hydra-report` | Report generation: templates, document assembly from engine-neutral fragments, and the txt/csv/html/PDF renderers. Knows nothing about any engine — it depends only on `hydra-common` |
| `hydra-sdk` | Umbrella facade: re-exports the complete user-facing API with all dependency versions pre-pinned |
| `hydra-cli` | Command-line interface: resolves input, writes output files, generates reports; no simulation logic |
| `hydra-gui` | Desktop application: Tauri shell with deck.gl canvas, timeline playback, network editor |

<!-- PLANNED-ENGINE: uds,och — revise the two scaffold rows above and this paragraph as each engine ships. -->
The two empty engine scaffolds exist so their crate names and versions track the
workspace from the start, rather than being introduced mid-life — see
[Engines](../engines.md) for what each engine covers. The split
between `hydra-common`, the engines, and `hydra-report` is what lets a report be
assembled from any engine's output: engines emit neutral fragments, and the
report layer renders them without knowing which engine produced them.

`hydra-cli` and `hydra-gui` are downstream consumers of Hydra in exactly the same way a third-party integrator would be: they depend on the umbrella crate and never import from `hydra-engine-wds` directly. Anyone who wants a different interface (HTTP, gRPC, Python bindings, etc.) follows the same pattern.
