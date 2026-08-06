# Hydra — Agent Instructions

Hydra is a water infrastructure simulation platform written in Rust, built as a suite of domain engines behind one shared toolchain (CLI, desktop GUI, Rust SDK).

Two engines are implemented today. The **water distribution** engine (`wds`) implements the Global Gradient Algorithm (GGA) hydraulic solver and a Lagrangian water quality engine on the EPANET data model (any 2.x input; the format is specified against 2.3). The **urban drainage** engine (`uds`) implements rainfall-runoff hydrology, Preissmann-slot dynamic-wave routing, and water quality on the SWMM data model; it is available from the CLI, the SDK, and the GUI, where a drainage model is imported, run and explored but not yet editable. For both, correctness is defined by Hydra's own convergence criteria and physical conservation laws.

**Open channel** (`och`, HEC-RAS data model) is registered in `hydra-common`'s engine registry as `Planned` — its crate name and engine key are reserved, but it is neither specced nor implemented. Never write copy that presents Hydra as one or two engines only by design, and never write copy that implies `och` already works.

### The `PLANNED-ENGINE` tag

Every disclaimer that exists **only** because an engine is unimplemented carries
a `PLANNED-ENGINE` tag naming the engine keys it is waiting on, so the full set
is one grep away when an engine ships:

```sh
grep -rn "PLANNED-ENGINE" --exclude-dir=target --exclude-dir=node_modules .
grep -rn "PLANNED-ENGINE: och" .    # just the open-channel ones
```

Use the comment syntax of the host file — `<!-- … -->` in Markdown, `//` in Rust
and TypeScript, `#` in TOML — and say what to do when the engine lands, not just
that the disclaimer exists:

```
<!-- PLANNED-ENGINE: och — drop this paragraph when the open channel engine ships. -->
// PLANNED-ENGINE: och — revise the paragraph above as each engine ships.
```

Tag the *temporary* statement, not the permanent one. "Planned engines cannot be
selected" is tagged; the engine registry's `EngineStatus::Planned` variant is
not — it is permanent machinery, and its own tests already guard the status
values. When adding a disclaimer anywhere, tag it; when shipping an engine,
start from this grep.

---

## Crate Responsibilities

| Crate | Owns | Does not own |
|---|---|---|
| `hydra-common` | Foundation contracts shared by all engines and applications: engine identity (descriptor + registry), the reportable-output contract (block catalog, neutral fragment model), and — since a second engine exists to validate them — the element-taxonomy, quantity, and result-variable contracts (spec §4–§6: engine-authored catalogs, opaque ids). Depends on nothing in the workspace | Any engine logic; presentation/rendering; a cross-engine simulation session contract (still deferred — only its dispatch home is assigned, spec §2.6) |
| `hydra-engine-wds` | Complete simulation engine: data model; INP/OUT/RPT parsers and writers; unit conversion; GGA hydraulic solver; Lagrangian quality engine; controls; timestep; accounting; session API (`Simulation`); post-simulation analytics; report blocks implementing the `hydra-common` reportable-output contract; local filesystem reads for `.out` result files via an explicit path-based helper (`io::out_reader`) | Interface logic; network I/O; any other filesystem I/O (INP model bytes are supplied in memory by callers) |
| `hydra-engine-uds` | Complete urban-drainage engine: SWMM data model; INP import and OUT/RPT writers; rainfall-runoff hydrology (infiltration, LID, snow, groundwater, RDII); dynamic-wave routing; structures and inlets; pollutant transport; controls; session API (`simulation::engine::Simulation`); local filesystem reads for `.out` result files via an explicit path-based helper (`io::out_reader`, §14.9) | Interface logic; network I/O; any other filesystem I/O (model text and auxiliary-file contents are supplied in memory by callers) |
| `hydra-engine-och` | Nothing yet — a published scaffold for the future open-channel engine, so its crate name and versions track the workspace from the start | Any functionality (deliberately empty until its development begins) |
| `hydra-engines` | Engine dispatch, implemented once for every application: the routing policy of the `hydra-common` recognition contract (§2.5.1) and the uniform run surface of §2.6 (`EngineSession` — open a model for its engine, step it, observe progress, persist results, collect warnings). Depends on `hydra-common` and every engine — the only layer that sees both | Any recognition logic of its own (each engine judges its own models); any solver logic (it drives sessions, never computes) |
| `hydra-report` | Report generation: JSON report templates, document assembly from engine-neutral fragments, deterministic txt/csv/html renderers | Any engine knowledge (depends only on `hydra-common`); analysis math; file/output-path UX (CLI/GUI) |
| `hydra-sdk` | **Hydra's public API** — the single crate third parties depend on to build on Hydra; curated re-exports of the full integrator-facing surface | Any new logic |
| `hydra-cli` | CLI argument parsing; input source resolution; file I/O | All simulation logic |
| `hydra-gui` | Tauri command surface; project/scenario persistence; background run queue; React frontend | Solver algorithms; session logic |

**Each engine crate is a self-contained black box.** `hydra-engine-wds`'s internal module structure (`hydraulics/`, `quality/`, `simulation/`, `analysis/`, `model/`, `io/`) is an implementation detail; callers depend only on its public re-export surface. `hydra-engine-uds` is likewise self-contained (`hydrology/`, `hydraulics/`, `transport/`, `simulation/`, `model/`, `io/`, with specs additionally under `interop/`).

**`hydra-sdk` is Hydra's public API, not an in-house convenience layer.** Its surface is sized by what a third-party integrator building on Hydra needs — never by what the official applications happen to use. Do not propose narrowing a re-export because the in-house apps don't exercise it; wholesale module re-exports (e.g. the engine's `io`) are correct when the module is genuinely public-facing.

**`hydra-cli` and `hydra-gui` are reference consumers of that public API.** They depend on the umbrella crate under the exact contract any third-party integrator has — and double as the prime examples of building software on it. They never import from `hydra-engine-wds`, `hydra-common`, `hydra-report`, or any other internal crate directly.

**`hydra-sdk` contains no logic** — only re-exports. Never add functions, structs, or trait implementations to it. (Downstream crates import it under the alias `hydra`.)

**Serialisation and output formatting** belong in the engine crates. Acquiring model bytes (reading INP files from disk, making HTTP calls, reading a model's auxiliary climate/hotstart/interface files) does not — that belongs in `hydra-cli` or `hydra-gui`. The one filesystem carve-out inside an engine is `hydra-engine-wds`'s explicit path-based streaming of `.out` result files (`io::out_reader`), which exists so large results never have to be loaded whole.

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
| `crates/engine-uds/src/spec.md` | uds charter: scope, principles, correspondence, status |
| `crates/engine-uds/src/model/spec.md` | SWMM data model and unit system |
| `crates/engine-uds/src/hydrology/spec.md` | Runoff, infiltration, LID, snowmelt, groundwater, RDII, climate |
| `crates/engine-uds/src/hydraulics/spec.md` | Section geometry, dynamic-wave routing, structures, inlets |
| `crates/engine-uds/src/transport/spec.md` | Buildup, washoff, treatment, network transport |
| `crates/engine-uds/src/simulation/spec.md` | Controls, orchestration, accounting, statistics, session API |
| `crates/engine-uds/src/interop/spec.md` | INP import, interface files, OUT/RPT output, recognition |

**Always update the relevant spec before changing solver/model/analysis implementation.**
If a spec and its implementation disagree, the spec wins — fix the implementation
(unless the spec is genuinely wrong, in which case fix the spec first).

If implementing something requires a decision not covered by the spec, **stop**.
Surface the gap and update the spec first. Do not invent behaviour.

Specs are language- and platform-agnostic. No references to Rust, crates, or file layouts.
Formulae are in LaTeX with every symbol defined on first use. Intentional deviations
from the engine's predecessor are labelled — `> **DEVIATION from EPANET:** <reason>`
in the wds specs; the uds specs record their stance toward SWMM in their
correspondence sections.

Operations safe to parallelise are marked **∥** in the solver specs. These are the
**only** operations the implementation may parallelise.

CLI and GUI behaviour (argument parsing, file layout, Tauri command surface, run queue)
is documented in the source code itself — `crates/cli/src/main.rs` and
`crates/gui/src/commands/` (`commands.rs` is a thin re-export façade; the command
implementations live in its submodules). No separate spec files exist for those crates.

---

## Workflow

**Changes always flow downward: spec → implementation.**

### Solver algorithms (hydraulics, quality/transport, hydrology, simulation)

1. Update the relevant sub-spec in the owning engine crate (`crates/engine-wds/`
   or `crates/engine-uds/`) to define the new behaviour.
2. Only then write or change implementation code.

### Data model and parsers (engine model/io)

1. Update the owning engine's spec — `crates/engine-wds/src/model/spec.md`, or
   for uds `crates/engine-uds/src/model/spec.md` (data model) and
   `crates/engine-uds/src/interop/spec.md` (file formats).
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

This section is about replies in chat. It does not apply to specs, code
comments, or commit messages, which have their own style and are meant to be
fuller.

Say what was done, what was decided, and what is blocked. Nothing else. Skip
preamble, plans of what you are about to do, and sign-offs like "I've
successfully…" or "Let me know if…". Short progress notes during long work are
fine ("running cargo check…"). Reasoning traces are not.

### Length

Match the reply to the question. A yes/no question gets a yes or a no, plus the
reason if it is not obvious. A code change gets a few lines: what changed, and
why. Only a design question or a review needs more than a short paragraph.

If a reply is getting long, most of it is probably restating things the user
already knows.

### Words

Use the shorter word. Say "use" not "utilise", "about" not "regarding", "show"
not "surface", "start" not "commence", "so" not "accordingly". If the word would
sound odd said out loud in a standup, pick another one.

Keep sentences short. One idea each. Split a long sentence in two instead of
joining it with a dash.

### Habits that read as machine-written

- **Em-dashes.** Use a full stop or a comma instead. One per reply at most.
- **"Not X, but Y."** Just say Y.
- **Lists of three.** "Clear, precise and honest" is one point dressed as three.
  Pick the one that matters.
- **Ending on a neat line.** Do not close a section with a summarising phrase
  that sounds quotable. Stop when the point is made.
- **"Worth noting", "worth flagging", "the thing is".** Say the thing.
- **Bolding a phrase and then explaining it.** Explain it.
- **Filler adverbs**: deliberately, genuinely, precisely, quietly, actually,
  plainly, honestly. Cut them and read the sentence again. It almost always
  means the same.
- **Repeating the question before answering it.** Answer it.
- **Explaining what you are about to explain.** Start explaining.

---

## Implementation Rules

- **Spec compliance:** Implement the exact algorithm in the spec — same equations, same convergence criteria, same defaults. Mark gaps with `// TODO: spec section missing for <subsystem>` and surface them. Mark intentional deviations with `// SPEC-DEVIATION: <reason>`.
- **Numeric precision:** All hydraulic and quality quantities use `f64`. Never narrow to `f32` for intermediate values.
- **Parallelism:** Only parallelise operations marked **∥** in the owning spec. Do not introduce parallelism for anything else without updating the spec first.
- **Error handling:** Solver and model crates return `Result` with domain-specific error types. No `unwrap()` or `expect()` outside test code. Every `unsafe` block requires a `// SAFETY:` comment.
- **Testing:** Use fast, targeted commands during iteration (`cargo check`, `cargo test -p <crate> <name>`, and in `crates/gui/frontend` `npx tsc --noEmit` / `npx biome check <file>` for pinpoint checks). But the check that declares a task **complete** must be a `just` recipe, because only the recipes carry the exact flags and whole-tree scope CI enforces (`clippy -D warnings`, `--locked`, `RUSTDOCFLAGS=-D warnings`, frozen lockfile, whole-tree Biome, the `tauri/custom-protocol` feature): `just lint` (all static checks), `just verify` (adds the full Rust + frontend test suites), or `just ci` (the complete CI gate). A green targeted run is not proof CI is green — note that `cargo test` never exercises the React/TypeScript frontend at all.

---

## Interface Rules

**Never name an element kind in words alone.** Wherever the interface
identifies an element's kind, it shows the kind's glyph — either the glyph
by itself, or the glyph beside the name. Never the name on its own.

An element id is unique only *within* its class: a junction `2` and a pipe
`2` are two different elements that happen to share a name. So a kind is
not decoration, it is half the identity, and the reader needs it at a
glance rather than in a word they must stop and read. The glyph also
carries the kind's colour, which a word does not, and it is the same mark
the canvas, the network list and the editor tables use — so the reader
learns one vocabulary instead of one per surface.

Render it with `TypeBadge` (`components/ui/TypeBadge.tsx`), the single
renderer for element badges. Never hand-roll a coloured dot, letter or
stripe: those drift from the catalog and from each other, and a stripe
tinted by kind says less than the badge while looking like it says the
same.

The exception is running prose. A sentence that happens to contain a kind's
name — "Editing drainage models here isn't built yet", "3 catchments drain
here" — is describing, not identifying, and needs no glyph.

---

## Regression Discipline

Every defect that reaches the running app gets a test that would have
caught it, committed with the fix. The point is not coverage — it is that
the same mistake cannot be made twice silently.

**Test the decision, not the symptom.** Most defects here have been a
decision buried where nothing can call it: a ternary inside JSX, a branch
inside an effect. Extract it into a named exported function taking plain
data, and test that. This is a design improvement first and a testability
one second — a decision with a name and a docstring is a decision someone
can review. `resultsPath`, `readScaleMode`, `railOpenForLocation`,
`clearableCountOf`, and `splitForTruncation` all exist because a bug proved
they should.

**Watch for one identifier meaning two things.** The recurring defect shape
in this codebase is a single value answering two questions that later
diverge: a result catalog that also meant "how periods are encoded"; a
variable id that also meant "which engine's criteria apply"; a rail
preference that also meant "is a rail on screen". When you find yourself
reading a field to answer a question it was not named for, split it and
give each half a test asserting they are independent.

**Layers, and what belongs in each:**

| Layer | Use for | Where |
|---|---|---|
| Rust unit | Solver maths, parsers, catalogs, DTO shape and gating | Beside the code, `#[cfg(test)]` |
| Pure TS | Decisions, formatting, preference migration, geometry | `*.test.ts`, `environment: node` |
| Component | What the user actually reads: which elements render, what dismisses, what is offered | `*.test.tsx` with `@vitest-environment jsdom` |
| Layout | Boxes: a width that must not depend on its content, a row that must fit its own second line | `*.layout.test.tsx`, real Chromium |

Component tests need the `@vitest-environment jsdom` docblock; without it
they run in Node and fail on `document`. Cleanup is automatic
(`src/test-setup.ts`).

**Layout tests run in a browser because jsdom performs none.** It answers
every question about width, height or overflow with a zero, so a box that
sizes to its content instead of its declaration is invisible to the other
three layers — two such bugs reached users before this layer existed.
`*.layout.test.tsx` files run under the `layout` project against real
Chromium (`just test-layout`; `just setup-layout-tests` fetches the
browser once). They load `app.css`, because layout is a product of the
cascade: without the global `border-box` reset a column declared 680px
wide measures 776.

Keep this layer small and keep it measuring *numbers about elements*.
Screenshot diffing is deliberately not done: font rasterisation differs
between a developer's machine and CI, so image baselines churn without
catching much, while "this box is 680px wide whatever it holds" is stable
and says exactly what broke. Assert against the component's own exported
styles rather than a copy — `SETTINGS_COLUMN` exists so its test cannot
drift from what ships.

**Cross-boundary invariants get a test on each side.** The Rust DTO and the
TypeScript interface are hand-mirrored, so a claim that matters on both
sides (an engine publishes a catalog but does not serve generic periods)
is asserted in both places. Neither test alone would have caught the drift.

**Two things make a component untestable in jsdom, and both are worth
knowing before spending an afternoon on it.** A **virtualised list**
renders no rows: `useVirtualizer` measures its scroll element, jsdom
reports zero height, and the list concludes nothing is visible — so the
row behaviour of the network list, the editor tables and the curve points
table cannot be reached there. And **`AppProvider` cannot be mounted**: it
registers Tauri event listeners that do not exist under jsdom and throws
before its children render, so a component reading app state has to have
that one hook mocked rather than the provider supplied.

**Not currently covered, and known:** no end-to-end test drives the real
Tauri shell; nothing checks *appearance* — the layout layer measures
geometry, so a stray gradient or a wrong colour still passes everything
here; and no test exercises a virtualised row's own interactions, for the
reason above.

