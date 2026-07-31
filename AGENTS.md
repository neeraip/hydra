# Hydra — Agent Instructions

Hydra is a water infrastructure simulation platform written in Rust, built as a suite of domain engines behind one shared toolchain (CLI, desktop GUI, Rust SDK).

The **water distribution** engine (`wds`) is the only one implemented today: it implements the Global Gradient Algorithm (GGA) hydraulic solver and a Lagrangian water quality engine, operating on the EPANET 2.3 data model. Correctness is defined by Hydra's own convergence criteria and physical conservation laws.

**Urban drainage** (`uds`, SWMM data model) and **open channel** (`och`, HEC-RAS data model) are registered in `hydra-common`'s engine registry as `Planned` — their crate names and engine keys are reserved, but neither is specced or implemented. Never write copy that presents Hydra as water-distribution-only, and never write copy that implies `uds`/`och` already work.

---

## Crate Responsibilities

| Crate | Owns | Does not own |
|---|---|---|
| `hydra-common` | Foundation contracts shared by all engines and applications: engine identity (descriptor + registry) and the reportable-output contract (block catalog, neutral fragment model). Depends on nothing in the workspace | Any engine logic; presentation/rendering; shared element schemas and unit systems (deferred by design until a second engine exists) |
| `hydra-engine-wds` | Complete simulation engine: data model; INP/OUT/RPT parsers and writers; unit conversion; GGA hydraulic solver; Lagrangian quality engine; controls; timestep; accounting; session API (`Simulation`); post-simulation analytics; report blocks implementing the `hydra-common` reportable-output contract; local filesystem reads for `.out` result files via an explicit path-based helper (`io::out_reader`) | Interface logic; network I/O; any other filesystem I/O (INP model bytes are supplied in memory by callers) |
| `hydra-engine-uds`, `hydra-engine-och` | Nothing yet — published scaffolds for the future urban-drainage and open-channel engines, so their crate names and versions track the workspace from the start | Any functionality (deliberately empty until their development begins) |
| `hydra-report` | Report generation: JSON report templates, document assembly from engine-neutral fragments, deterministic txt/csv/html renderers | Any engine knowledge (depends only on `hydra-common`); analysis math; file/output-path UX (CLI/GUI) |
| `hydra-sdk` | **Hydra's public API** — the single crate third parties depend on to build on Hydra; curated re-exports of the full integrator-facing surface | Any new logic |
| `hydra-cli` | CLI argument parsing; input source resolution; file I/O | All simulation logic |
| `hydra-gui` | Tauri command surface; project/scenario persistence; background run queue; React frontend | Solver algorithms; session logic |

**`hydra-engine-wds` is a self-contained black box.** Its internal module structure (`hydraulics/`, `quality/`, `simulation/`, `analysis/`, `model/`, `io/`) is an implementation detail. Callers depend only on its public re-export surface.

**`hydra-sdk` is Hydra's public API, not an in-house convenience layer.** Its surface is sized by what a third-party integrator building on Hydra needs — never by what the official applications happen to use. Do not propose narrowing a re-export because the in-house apps don't exercise it; wholesale module re-exports (e.g. the engine's `io`) are correct when the module is genuinely public-facing.

**`hydra-cli` and `hydra-gui` are reference consumers of that public API.** They depend on the umbrella crate under the exact contract any third-party integrator has — and double as the prime examples of building software on it. They never import from `hydra-engine-wds`, `hydra-common`, `hydra-report`, or any other internal crate directly.

**`hydra-sdk` contains no logic** — only re-exports. Never add functions, structs, or trait implementations to it. (Downstream crates import it under the alias `hydra`.)

**Serialisation and output formatting** belong in `hydra-engine-wds`. Acquiring model bytes (reading INP files from disk, making HTTP calls) does not — that belongs in `hydra-cli` or `hydra-gui`. The one filesystem carve-out inside the engine is the explicit path-based streaming of `.out` result files (`io::out_reader`), which exists so large results never have to be loaded whole.

---

## Specifications

The solver algorithm specs live inside each module directory and are embedded as
module-level documentation via `#![doc = include_str!("spec.md")]`. They are the
authoritative definition of Hydra's mathematical behaviour:

| Spec file | Covers |
|---|---|
| `crates/common/src/spec.md` | Foundation contracts: engine identity/registry, reportable-output contract |
| `crates/report/src/spec.md` | Report templates, document model, txt/csv/html renderer formats |
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
is documented in the source code itself — `crates/cli/src/main.rs` and
`crates/gui/src/commands/` (`commands.rs` is a thin re-export façade; the command
implementations live in its submodules). No separate spec files exist for those crates.

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

### Foundation contracts (hydra-common)

1. Update `crates/common/src/spec.md`. Keep the layer slim: no engine
   vocabulary (result classes, element kinds) may enter these contracts —
   engine-specific meaning travels only through opaque ids and
   engine-authored text.
2. Only then write or change implementation code.

### Report generation (hydra-report)

1. Update `crates/report/src/spec.md`. The same no-engine-knowledge rule
   applies: this layer only presents given blocks of data; it must depend
   on nothing but `hydra-common`. The template JSON and renderer output
   formats are compatibility surfaces — changes need the same care as
   file-format changes.
2. Only then write or change implementation code.

### Umbrella (hydra-sdk)

Update the re-export list and `README.md` examples when the public API changes. No spec document needed. When deciding whether something belongs in the sdk, the question is "does a third party building on Hydra need this?" — never "do our apps use this?".

### CLI (hydra-cli) and GUI (hydra-gui)

Make changes directly. No spec document to update — behaviour is documented in source.

If the change also requires a session API change, follow the solver workflow first.

---

## Version Management

See [RELEASING.md](RELEASING.md) for the release process and version bump commands.

---

## Git Discipline

**Never run `git commit` or `git push` unless the user explicitly asks you to commit or push.** Making file changes is sufficient; the user will commit and push when ready.

**The `just ci` recipe is the authoritative pre-commit gate.** Before committing, everything the `just ci` recipe checks must pass — never commit on a red tree. You do not have to invoke `just ci` literally: run the individual recipes it chains (`just fmt-check`, `just clippy`, `just docs-api`, `just test`, `just lint-frontend`, …), the subset relevant to the files you touched, or `just ci` as one batch — whatever is fastest. What matters is that everything the recipe would check is green, not how you run it. Note that `rustfmt`, `clippy -D warnings`, and rustdoc warnings are CI failures even when the test suite passes, so a green `cargo test` alone is not sufficient.

**Never create git tags** unless the user explicitly asks for a tag or release.

**Commit messages follow Conventional Commits:**

```
<type>(<optional scope>): <description>
```

Valid types: `feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `test`, `ci`, `perf`, `build`, `revert`.
Use the imperative mood in the description ("add", "fix", "remove", not "added", "fixes", "removed").
Keep the subject line under 72 characters. Add a body if the change needs more context.
PR titles follow the same format (see `.github/copilot-instructions.md`).

**Scope breaking changes to a single release track.** `just release-status`
assigns commits to release tracks (Library / CLI / GUI) by the *files they
touch*, not by the commit scope — then reads the `!`/`BREAKING CHANGE` marker to
suggest a bump level. A cross-cutting break like `refactor(engine,gui)!: …` that
edits both `crates/engine-wds` and `crates/gui` therefore double-counts the
breaking signal into **both** tracks, pushing the GUI to a MAJOR suggestion even
though the GUI is an application with no public API to break. To keep the break
scoped to the library, split the engine-API change and its GUI follow-on edits
into separate commits (`refactor(engine)!: …` + a plain `refactor(gui): …`).

---

## Communication

Be concise. Responses should communicate what was done and any decisions or blockers — nothing more. Avoid preamble, summaries of what you are about to do, and closing affirmations ("I've successfully…", "Let me know if…").

Brief inline progress notes during multi-step work are fine (e.g. "running cargo check…", "reading spec…"). Full verbose reasoning traces are not.

---

## Implementation Rules

- **Spec compliance:** Implement the exact algorithm in the spec — same equations, same convergence criteria, same defaults. Mark gaps with `// TODO: spec section missing for <subsystem>` and surface them. Mark intentional deviations with `// SPEC-DEVIATION: <reason>`.
- **Numeric precision:** All hydraulic and quality quantities use `f64`. Never narrow to `f32` for intermediate values.
- **Parallelism:** Only parallelise operations marked **∥** in the owning spec. Do not introduce parallelism for anything else without updating the spec first.
- **Error handling:** Solver and model crates return `Result` with domain-specific error types. No `unwrap()` or `expect()` outside test code. Every `unsafe` block requires a `// SAFETY:` comment.
- **Testing:** Use fast, targeted commands during iteration (`cargo check`, `cargo test -p <crate> <name>`, and in `crates/gui/frontend` `npx tsc --noEmit` / `npx biome check <file>` for pinpoint checks). But the check that declares a task **complete** must be a `just` recipe, because only the recipes carry the exact flags and whole-tree scope CI enforces (`clippy -D warnings`, `--locked`, `RUSTDOCFLAGS=-D warnings`, frozen lockfile, whole-tree Biome, the `tauri/custom-protocol` feature): `just lint` (all static checks), `just verify` (adds the full Rust + frontend test suites), or `just ci` (the complete CI gate). A green targeted run is not proof CI is green — note that `cargo test` never exercises the React/TypeScript frontend at all.
