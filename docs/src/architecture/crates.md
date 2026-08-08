# Crate Layout

Hydra is a multi-crate Rust workspace:

| Crate | Role |
|---|---|
| `hydra-common` | Foundation contracts shared by every engine and application: engine identity, and the reportable-output contract (block catalog, neutral fragment model). Depends on nothing else in the workspace |
| `hydra-engine-wds` | Water-distribution engine: data model, parsers, unit conversion, GGA hydraulic solver, Lagrangian quality engine, session API, analytics, report blocks |
| `hydra-engine-uds` | Urban-drainage engine: SWMM data model and import, rainfall-runoff hydrology, dynamic-wave routing, water quality, controls, session API, predecessor-format output |
| `hydra-engine-och` | Open-channel engine — a published scaffold, deliberately empty until its development begins |
| `hydra-engines` | Engine dispatch: given a model of unknown provenance, decides which engine owns it. The one layer that sees both the registry and every engine, so the routing policy lives here once instead of in each interface |
| `hydra-report` | Report generation: templates, document assembly from engine-neutral fragments, and the txt/csv/html/PDF renderers. Knows nothing about any engine — it depends only on `hydra-common` |
| `hydra-sdk` | Umbrella facade: re-exports the complete user-facing API with all dependency versions pre-pinned |
| `hydra-cli` | Command-line interface: resolves input, writes output files, generates reports; no simulation logic |
| `hydra-gui` | Desktop application: Tauri shell with deck.gl canvas, timeline playback, network editor |
| `hydra-wasm` | The engines compiled to WebAssembly, and the browser demo built on them. Not published to crates.io — the artifact is the built bundle |

<!-- PLANNED-ENGINE: och — revise the scaffold row above and this paragraph as each engine ships. -->
The empty engine scaffold exists so its crate name and versions track the
workspace from the start, rather than being introduced mid-life — see
[Engines](../engines.md) for what each engine covers. The split
between `hydra-common`, the engines, and `hydra-report` is what lets a report be
assembled from any engine's output: engines emit neutral fragments, and the
report layer renders them without knowing which engine produced them.

`hydra-cli`, `hydra-gui` and `hydra-wasm` are downstream consumers of Hydra in exactly the same way a third-party integrator would be: they depend on the umbrella crate and never import from `hydra-engine-wds` directly. Anyone who wants a different interface (HTTP, gRPC, Python bindings, etc.) follows the same pattern — the wasm crate doubles as proof that the pattern reaches into a browser, where there is no filesystem and no thread to lean on.
