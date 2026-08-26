# Hydra — Agent Instructions

Hydra is a water infrastructure simulation platform written in Rust, built as a suite of domain engines behind one shared toolchain (CLI, desktop GUI, Rust SDK).

Two engines are implemented today. The **water distribution** engine (`wds`) implements the Global Gradient Algorithm (GGA) hydraulic solver and a Lagrangian water quality engine on the EPANET data model (any 2.x input; the format is specified against 2.3). The **urban drainage** engine (`uds`) implements rainfall-runoff hydrology, Preissmann-slot dynamic-wave routing, two-dimensional overland flow coupled to the network (spec §15), and water quality on the SWMM data model; it is available from the CLI, the SDK, and the GUI, where a drainage model is imported, then edited, run and explored. The GUI runs mesh models and renders their surfaces on the canvas: the mesh comes from the model, so it shows from import, and a run's `.2d.out` sidecar colours it by depth along the same timeline. For both, correctness is defined by Hydra's own convergence criteria and physical conservation laws.

**2D overland flow** is functionality of the `uds` engine, not a separate
engine: the overland sections ride the SWMM input format (specs
`crates/engine-uds/src/overland/spec.md` §15 and interop §14.15–§14.16),
mirroring the direction SWMM's own data model is taking upstream (SWMM6).
There was once a third planned
engine — **open channel** (`och`, HEC-RAS data model) — withdrawn in favour
of this plan. Its registry key and crate name (`hydra-engine-och`, published
as an empty scaffold through workspace v12) stay reserved and must never be
reused for a different domain. Never resurrect `och` in docs or interfaces.

The engine registry's `EngineStatus::Planned` variant is permanent machinery
and stays, even while no planned engine is registered.

---

## Crate Responsibilities

Crates are named for what they *are*, never for the technology they use:
`hydra-gui`, not `hydra-tauri`; `hydra-demo`, not `hydra-wasm` (its name
until v12). The technology is an implementation detail; the purpose is the
identity.

| Crate | Owns | Does not own |
|---|---|---|
| `hydra-common` | Foundation contracts shared by all engines and applications: engine identity (descriptor + registry), the reportable-output contract (block catalog, neutral fragment model), and — since a second engine exists to validate them — the element-taxonomy, quantity, result-variable, and criteria contracts (spec §4–§7: engine-authored catalogs, opaque ids). Depends on nothing in the workspace | Any engine logic; presentation/rendering; a cross-engine simulation session contract (still deferred — only its dispatch home is assigned, spec §2.6) |
| `hydra-engine-wds` | Complete simulation engine: data model; unit conversion; GGA hydraulic solver; Lagrangian quality engine; controls; timestep; accounting; session API (`Simulation`); post-simulation analytics; report blocks implementing the `hydra-common` reportable-output contract | Interface logic; network I/O; any other filesystem I/O (INP model bytes are supplied in memory by callers) |
| `hydra-engine-uds` | Complete urban-drainage engine: SWMM data model; rainfall-runoff hydrology (infiltration, LID, snow, groundwater, RDII); dynamic-wave routing; structures and inlets; pollutant transport; controls; session API (`simulation::engine::Simulation`) | Interface logic; network I/O; any other filesystem I/O (model text and auxiliary-file contents are supplied in memory by callers) |
| `hydra-interop-swmm` | The SWMM dialect: INP parse/write, OUT/RPT writers, the streaming OUT reader (the filesystem carve-out), interface/climate/rain records, recognition of SWMM files, the §14 interop spec. Depends on `hydra-engine-uds` and `hydra-common` only | Any physics; any validation semantics; acquiring bytes |
| `hydra-interop-epanet` | The EPANET dialect, same shape, for `hydra-engine-wds` | Same exclusions |
| `hydra-engines` | Engine dispatch, implemented once for every application: the routing policy of the `hydra-common` recognition contract (§2.5.1) and the uniform run surface of §2.6 (`EngineSession` — open a model for its engine, step it, observe progress, persist results, collect warnings). Depends on `hydra-common` and every engine — the only layer that sees both | Any recognition logic of its own (each engine judges its own models); any solver logic (it drives sessions, never computes) |
| `hydra-report` | Report generation: JSON report templates, document assembly from engine-neutral fragments, deterministic txt/csv/html renderers | Any engine knowledge (depends only on `hydra-common`); analysis math; file/output-path UX (CLI/GUI) |
| `hydra-sdk` | **Hydra's public API** — the single crate third parties depend on to build on Hydra; curated re-exports of the full integrator-facing surface | Any new logic |
| `hydra-cli` | CLI argument parsing; input source resolution; file I/O | All simulation logic |
| `hydra-gui` | Tauri command surface; project/scenario persistence; background run queue; React frontend | Solver algorithms; session logic |
| `hydra-demo` | The engines in a browser: a `wasm_bindgen` surface over the SDK's run path, and a demo page that runs a dropped model and prints what the CLI prints. Not published — the artifact is the built bundle | All simulation logic; any output format of its own (the report and the diagnostics are the engine's and the CLI's) |

**Each engine crate is a self-contained black box.** `hydra-engine-wds`'s internal module structure (`hydraulics/`, `quality/`, `simulation/`, `analysis/`, `model/`) is an implementation detail; callers depend only on its public re-export surface. `hydra-engine-uds` is likewise self-contained (`hydrology/`, `hydraulics/`, `transport/`, `simulation/`, `model/`, `overland/`). The interop crates keep their dialect sources under `src/dialect/` for the shared-source test mount.

**`hydra-sdk` is Hydra's public API, not an in-house convenience layer.** Its surface is sized by what a third-party integrator building on Hydra needs — never by what the official applications happen to use. Do not propose narrowing a re-export because the in-house apps don't exercise it; wholesale module re-exports (e.g. the engine's `io`) are correct when the module is genuinely public-facing.

**`hydra-cli`, `hydra-gui` and `hydra-demo` are reference consumers of that public API.** They depend on the umbrella crate under the exact contract any third-party integrator has — and double as the prime examples of building software on it. They never import from `hydra-engine-wds`, `hydra-common`, `hydra-report`, or any other internal crate directly.

**The engines must keep working on `wasm32-unknown-unknown`.** They have no filesystem code at all — the path-based `.out` readers live in the interop crates — and no threads by default, which is what makes a browser build possible at all. The interop crates must stay wasm-clean under the same rules, their readers' explicit path-based streaming being the one filesystem carve-out. The one concurrency door is `hydra-engine-uds`'s `threads` cargo feature: it parallelises exactly the spec's ∥ iteration phases (uds §6.4), takes its width from the model's own `THREADS` option, is off by default and never enabled for a wasm build, and produces byte-identical results at any width — so a serial build and a threaded one cannot disagree, and a test that holds on one holds on both. Three things break it, and only the first is caught by compiling:

| Break | Example | Guarded by |
|---|---|---|
| A dependency that will not build for wasm | a build script needing a host | `just check-wasm` |
| A host call that compiles and panics at runtime | `SystemTime::now()` | the engines' `clippy.toml` |
| A dependency that compiles and panics at runtime | `chrono` without `wasmbind` | `just test-wasm` |

Only the first is visible to a compiler. `just test-wasm` (`crates/demo/tests/browser.rs`) runs a model in headless Chrome, and is the only check that executes engine code on wasm at all — both bugs found while bringing the browser build up compiled cleanly and passed every host test. It needs a **system Chrome**, unlike the layout tests, which drive Playwright's own download.

Keep that file small. Everything it could assert about behaviour is already asserted on the host, where a failure names a line instead of a trap; it exists to answer one question, which is whether the engine survives a real run.

**`hydra-sdk` contains no logic** — only re-exports. Never add functions, structs, or trait implementations to it. (Downstream crates import it under the alias `hydra`.)

**Engines are format-blind** (workspace decision, 2026-08-26): models
enter an engine as typed data, results leave as typed streams, and no
engine knows any file format. All dialect tooling — `.inp` parse/write,
`.out`/`.rpt` writers, the `.out` streaming reader, interface/climate/rain
file handling, recognition of a dialect's files — belongs in one interop
crate per dialect (`hydra-interop-swmm`, `hydra-interop-epanet`), each
depending only on its engine's public data model and `hydra-common`.
Validation and mutation of the parsed model are engine semantics and stay
in the engines. Checkpoints stay in the engines (persistence of private
state, not an exchange format). Acquiring bytes (reading files from disk,
HTTP) belongs in `hydra-cli` or `hydra-gui`; the one filesystem carve-out
is the interop crates' explicit path-based streaming of `.out` result
files, which exists so large results never have to be loaded whole.

Two standing rules follow: **new capabilities are data-model-first** —
their `.inp` mapping is a separate deliberate interop decision, which may
legitimately be "not expressible in the legacy format" — and export
refusals are the exporter's sentences, stated once, in interop code.
`.inp` remains the CLI/GUI compatibility format indefinitely; a native
portable format is deliberately deferred until the public data model has
been stable for years. The dialect sources compile twice by design: once
as their crate and once mounted into their engine's test build (against
`crate::engine_api`), because engine unit tests must parse models and a
dev-dependency cycle would hand them a second, type-incompatible build
of the engine. Integration tests use the real dev-dependency.

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
| `crates/engine-uds/src/overland/spec.md` | Two-dimensional overland flow: mesh, marcher, coupling, meteorology, conservation |
| `crates/engine-uds/src/simulation/spec.md` | Controls, orchestration, accounting, statistics, session API |
| `crates/engine-uds/src/report_blocks/spec.md` | Post-simulation analytics: report-block catalog, derivations, options |
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

## User-Facing Copy

This section covers everything a person using Hydra reads: GUI text
(labels, dialogs, toasts, refusal messages), CLI output and diagnostics,
report text, the marketing site, the docs site, and the READMEs. It does
not cover code comments, specs, commit messages, or chat replies, which
have their own styles.

**Write plain English.** Short sentences. Everyday words. One idea per
sentence. Domain vocabulary is welcome when it is the precise term
(infiltration, dynamic wave, hotstart). Rhetorical flourishes are not.
If a sentence would sound like ad copy read aloud, rewrite it until it
sounds like a person explaining something.

**No em dashes.** Not one, anywhere in user-facing copy. Use a full
stop, a comma, a colon, or parentheses instead. An em dash in a code
comment or a spec is fine.

`just em-dashes` checks this, and `just lint` runs it. It reads string
literals and JSX text but not comments, skips tests and the pinned
theory snapshots, and permits the lone placeholder glyph that stands
for a value the app does not have. Three manual sweeps each missed
some before it existed.

---

## Regression Discipline

Every defect that reaches the running app gets a test that would have
caught it, committed with the fix. The point is not coverage — it is that
the same mistake cannot be made twice silently.

**A passing test is not evidence until something has tried to break it.**
Every hollow test this repository has had executed the code it was hollow
about, so coverage would have called all of them green: a drain
coefficient compared at two values the model could not tell apart, a snow
model with no snow in it, a release that restored a setting nothing had
changed. What finds them is changing the code and seeing whether the suite
still calls it correct. Do that by hand for a decision you have just
written, and mechanically with `just mutants <file>` for anything whose
semantics a reader would otherwise take on trust. It is slow, so it is not
in `just ci`; it belongs on what you changed. Its first run on the rain
parser found the condition-code handling was wrong and untested at once.

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

**`AppProvider` cannot be mounted** in jsdom: it registers Tauri event
listeners that do not exist there and throws before its children render,
so a component reading app state has to have that one hook mocked rather
than the provider supplied.

**Virtualised lists used to be untestable too**, and are not any more.
`useVirtualizer` measures its scroll container with `offsetHeight`, jsdom
performs no layout and answers zero, and the list concluded no row was
visible — so the network list, the editor tables and the curve points
table all rendered an empty `<tbody>`. `src/test-setup.ts` now gives
every element a plausible box, so rows mount and their behaviour can be
asserted. Two things follow: a virtualised list's *rows* are ordinary
component-test material, and a test wanting a real measurement still
belongs in the layout project, because that box is a stub and not a
layout.

**Not currently covered, and known:** no end-to-end test drives the real
Tauri shell. The network list's rows are covered now
(`NetworkListRow.test.tsx` renders the row and asserts its badge, its value
column, the second line's two conditions, the zoom control, hover, and that
the click gestures still reach the decisions `NetworkList.test.ts` tests in
isolation — deleting either call left every one of those green). The editor
tables are covered
(`KindTable.test.tsx` renders the component and asserts row listing, header
sort, per-row edit addressing and virtualised mounting), and the layout layer
does assert colour as well as geometry: `PrimaryButton.layout.test.tsx` pins
exact RGB and chroma bounds, `selectPopup.layout.test.tsx` pins per-theme
lightness and a contrast gap. What is still deliberately absent is screenshot
diffing, for the reason given above.

