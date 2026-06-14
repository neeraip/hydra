# Hydra — Agent Instructions

Hydra is a water distribution network simulator written in Rust. It implements the Global Gradient Algorithm (GGA) hydraulic solver and a Lagrangian water quality engine, operating on the EPANET 2.3 data model. Correctness is defined by Hydra's own convergence criteria and physical conservation laws.

---

## Crate Responsibilities

| Crate | Owns | Does not own |
|---|---|---|
| `hydra-engine-wds` | Complete simulation engine: data model; INP/OUT/RPT parsers and writers; unit conversion; GGA hydraulic solver; Lagrangian quality engine; controls; timestep; accounting; session API (`Simulation`); post-simulation analytics | Interface logic; filesystem/network I/O |
| `hydra-sdk` | Curated public re-exports — the umbrella crate | Any new logic |
| `hydra-cli` | CLI argument parsing; input source resolution; file I/O | All simulation logic |
| `hydra-gui` | Tauri command surface; project/scenario persistence; background run queue; React frontend | Solver algorithms; session logic |

**`hydra-engine-wds` is a self-contained black box.** Its internal module structure (`hydraulics/`, `quality/`, `simulation/`, `analysis/`, `model/`, `io/`) is an implementation detail. Callers depend only on its public re-export surface.

**`hydra-cli` and `hydra-gui` are downstream consumers of Hydra** — they depend on the umbrella crate and never import from `hydra-engine-wds` directly. This is the same contract any third-party integrator has.

**`hydra` contains no logic** — only re-exports. Never add functions, structs, or trait implementations to it.

**Serialisation and output formatting** belong in `hydra-engine-wds`. Reading from disk or making HTTP calls do not — those belong in `hydra-cli` or `hydra-gui`.

---

## Specifications

The solver algorithm specs live inside each module directory and are embedded as
module-level documentation via `#![doc = include_str!("spec.md")]`. They are the
authoritative definition of Hydra's mathematical behaviour:

| Spec file | Covers |
|---|---|
| `crates/engine-wds/src/model/spec.md` | Network data model, unit system, INP/OUT/RPT formats |
| `crates/engine-wds/src/hydraulics/spec.md` | GGA Newton-Raphson solver, valve models, demand models |
| `crates/engine-wds/src/quality/spec.md` | Lagrangian transport, mixing, reactions, source tracing |
| `crates/engine-wds/src/simulation/spec.md` | Session API, controls, timestep orchestration, accounting |
| `crates/engine-wds/src/analysis/spec.md` | Post-simulation analytics |

**Always update the relevant spec before changing solver/model/analysis implementation.**
If a spec and its implementation disagree, the spec wins — fix the implementation
(unless the spec is genuinely wrong, in which case fix the spec first).

If implementing something requires a decision not covered by the spec, **stop**.
Surface the gap and update the spec first. Do not invent behaviour.

Specs are language- and platform-agnostic. No references to Rust, crates, or file layouts.
Formulae are in LaTeX with every symbol defined on first use. Intentional deviations
from EPANET are labelled: `> **DEVIATION from EPANET:** <reason>`.

Operations safe to parallelise are marked **∥** in the solver specs. These are the
**only** operations the implementation may parallelise.

CLI and GUI behaviour (argument parsing, file layout, Tauri command surface, run queue)
is documented in the source code itself — `hydra-cli/src/main.rs` and
`hydra-gui/src/commands.rs`. No separate spec files exist for those crates.

---

## Workflow

**Changes always flow downward: spec → implementation.**

### Solver algorithms (hydraulics, quality, simulation)

1. Update the relevant sub-spec in `crates/engine-wds/` to define the new behaviour.
2. Only then write or change implementation code.

### Data model and parsers (hydra-engine-wds model/io)

1. Update `crates/engine-wds/src/model/spec.md`.
2. Only then write or change implementation code.

### Post-simulation analytics (hydra-engine-wds analysis)

1. Update `crates/engine-wds/src/analysis/spec.md`.
2. Only then write or change implementation code.

### Facade (hydra)

Update the re-export list and `README.md` examples when the public API changes. No spec document needed.

### CLI (hydra-cli) and GUI (hydra-gui)

Make changes directly. No spec document to update — behaviour is documented in source.

If the change also requires a session API change, follow the solver workflow first.

---

## Version Management

See [RELEASING.md](../RELEASING.md) for the release process and version bump commands.

---

## Git Discipline

**Never run `git commit` or `git push` unless the user explicitly asks you to commit or push.** Making file changes is sufficient; the user will commit and push when ready.

**Never create git tags** unless the user explicitly asks for a tag or release.

---



Be concise. Responses should communicate what was done and any decisions or blockers — nothing more. Avoid preamble, summaries of what you are about to do, and closing affirmations ("I've successfully…", "Let me know if…").

Brief inline progress notes during multi-step work are fine (e.g. "running cargo check…", "reading spec…"). Full verbose reasoning traces are not.

---

## Implementation Rules

- **Spec compliance:** Implement the exact algorithm in the spec — same equations, same convergence criteria, same defaults. Mark gaps with `// TODO: spec section missing for <subsystem>` and surface them. Mark intentional deviations with `// SPEC-DEVIATION: <reason>`.
- **Numeric precision:** All hydraulic and quality quantities use `f64`. Never narrow to `f32` for intermediate values.
- **Parallelism:** Only parallelise operations marked **∥** in the owning spec. Do not introduce parallelism for anything else without updating the spec first.
- **Error handling:** Solver and model crates return `Result` with domain-specific error types. No `unwrap()` or `expect()` outside test code. Every `unsafe` block requires a `// SAFETY:` comment.
- **Testing:** Run `cargo check` after every edit. Run `cargo test` before considering any task complete.
