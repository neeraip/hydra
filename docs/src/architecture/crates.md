# Crate Layout

Hydra is a multi-crate Rust workspace:

| Crate | Role |
|---|---|
| `hydra-sdk` | Umbrella facade: re-exports the complete user-facing API with all dependency versions pre-pinned |
| `hydra-common` | Thin shared infrastructure: engine-agnostic types (`Coordinate`, `Crs`) |
| `hydra-engine-wds` | Complete simulation engine: data model, parsers, unit conversion, GGA hydraulic solver, Lagrangian quality engine, session API, analytics |
| `hydra-cli` | Command-line interface: resolves input, writes output files; no simulation logic |
| `hydra-gui` | Desktop application: Tauri shell with deck.gl canvas, timeline playback, network editor |

`hydra-cli` and `hydra-gui` are downstream consumers of Hydra in exactly the same way a third-party integrator would be: they depend on the umbrella crate and never import from `hydra-engine-wds` or `hydra-common` directly. Anyone who wants a different interface (HTTP, gRPC, Python bindings, etc.) follows the same pattern.
