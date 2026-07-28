# Hydra Common — Foundation Contract

Status: **v1.3 — ratified 2026-07-28** (v1.1 added opaque per-block options
to the production contract, §3.4; v1.2 added the chart fragment item,
§3.3; v1.3 added engine availability and import formats, §2.1–2.3).
This file is the module documentation
of the `hydra-common` crate and follows the same spec-first workflow as the
engine specs: implementation changes flow from changes here, never the
reverse.

---

## 1. Purpose and Scope

The common layer is the foundation every engine and every application may
depend on. It depends on nothing else in the workspace. It exists so that
applications can host *any* Hydra engine — present or future — through one
uniform surface instead of per-engine hardcoded knowledge.

**v1 is deliberately slim.** It defines exactly two contracts:

1. **Engine identity** — what an engine *is* (§2).
2. **Reportable output** — what an engine can contribute to a report (§3).

**Explicit non-goals for v1** (ratified 2026-07-28): a shared element
schema (node / link / subcatchment / …), a unified unit system, and any
cross-engine simulation contract. These are deferred until at least one
additional engine implementation exists to exercise them; abstracting them
from a single implementation risks baking water-distribution assumptions
into the foundation. Nothing in v1 may presuppose the shape those future
contracts will take.

---

## 2. Engine Identity

### 2.1 Engine descriptor

Every engine publishes one immutable descriptor:

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Stable machine identifier | Lowercase ASCII, short domain-umbrella abbreviation (`wds`, `uds`, `och`). **Never changes once released** — it is persisted in project metadata and report templates. |
| `label` | Human-facing product name | Practitioner-familiar term (e.g. "Water Distribution"). May be revised between releases. |
| `pill` | Two-character badge | Uppercase, exactly 2 characters (e.g. "WD"). |
| `accent` | Brand color for this engine | `#rrggbb` hex string. |
| `summary` | One-sentence description of the engine's domain | Plain text, no markup. |
| `status` | Whether this distribution can actually run the engine | `available` or `planned` (§2.3). |
| `import` | Source-model formats the engine imports | Ordered list of import-format descriptors (§2.2); may be empty for an engine with no import path. |

The `key` and the `label`/`pill` pair are two deliberately separate naming
systems: the key carries the accurate domain umbrella; the label carries
the familiar practitioner term. They are allowed to diverge and must not
be derived from one another.

### 2.2 Import formats

An engine's models originate in some external tool's file format. The
descriptor names those formats so applications can offer a correctly
filtered file picker for *any* engine without hardcoding per-engine file
knowledge:

| Field | Meaning | Constraints |
|---|---|---|
| `label` | Human-facing format name | Plain text, e.g. "EPANET input file". |
| `extensions` | Filename extensions the format uses | One or more, lowercase ASCII, no leading dot. |

This is deliberately the *only* file knowledge in the foundation layer.
It names formats; it says nothing about their contents, and nothing here
may be used to decide whether a given file is valid. **Validating that a
file really is a model of the named format is the owning engine's job** —
extensions are a picker filter and a first-pass hint, never a check. Two
engines legitimately share the `inp` extension (EPANET and SWMM both use
it) with entirely incompatible contents, so an application that trusted
the extension would hand a stormwater model to a water-distribution
solver.

### 2.3 Availability

A registered engine is either:

- **`available`** — implemented in this distribution and usable;
- **`planned`** — registered so applications can present it (and so its
  key is reserved), but carrying no implementation.

Planned engines are registered rather than hidden because a user choosing
a modelling domain deserves to see what Hydra covers and what is coming,
and because the key must be reserved before anything persists it.

Applications **must** present planned engines as explicitly unavailable
and **must** refuse to create projects, run simulations, or import models
for them. Refusing is a hard requirement, not a UI nicety: a persisted
project naming a planned engine would be indistinguishable from one whose
engine was removed.

Resolving a planned engine's key is **not** an error and must not be
conflated with the unknown-key case (§2.4) — the descriptor exists and
its identity fields are valid; only its implementation is absent.

### 2.4 Registry

The registry is the ordered collection of descriptors for every engine
compiled into a distribution. It supports:

- **Enumeration** in a stable, deliberate order (the order engines are
  presented to users), available and planned engines alike.
- **Lookup by key**, which either yields the descriptor or a typed
  "unknown engine" error.

Applications must treat an unknown key (e.g. a project created by a newer
Hydra carrying an engine this build lacks) as an explicit unsupported
state, never as a fallback to a default engine.

v1 ships three registered engines — `wds` (available), `uds` (planned),
and `och` (planned) — in that order.

---

## 3. Reportable-Output Contract

The contract by which an engine describes — and produces — the content
blocks a report can include. Presentation (layout, styling, output
formats, templates) is **not** part of this contract; it belongs to the
report layer, which consumes this contract and knows nothing
engine-specific.

### 3.1 Concepts

| Term | Meaning |
|---|---|
| **Block** | One self-contained unit of reportable content an engine can produce (e.g. a pressure summary, a pump energy table). |
| **Catalog** | The engine's complete list of block descriptors. Queryable statically — without any simulation having run. |
| **Fragment** | The materialized content of one block for one completed simulation. |

### 3.2 Block descriptor

| Field | Meaning | Constraints |
|---|---|---|
| `id` | Stable block identifier | Namespaced by engine key: `<engine>.<name>` (e.g. `wds.pressure-summary`). **Never changes once released** — report templates reference it. |
| `title` | Default human-facing heading | Plain text. |
| `summary` | What this block contains, for the template-builder UI | One or two sentences, plain text. |

The descriptor deliberately carries **no result-class or prerequisite
vocabulary** — what a block needs from a simulation is the producing
engine's internal concern, expressed only through the production error
contract (§3.4). Encoding result taxonomies (hydraulic vs. quality vs.
anything else) here would bake one engine family's domain into the
foundation layer.

Removing a block id, or changing the *meaning* of an existing id, is a
breaking change to every saved template that references it and must be
treated with the same gravity as a file-format break.

### 3.3 Fragment model

Fragments are neutral data — no colors, fonts, page geometry, or format
hints. A fragment is a titled sequence of items; each item is one of:

| Item | Shape | Notes |
|---|---|---|
| **Key-value list** | Ordered pairs of (label, value) | For scalar summaries ("Total demand", "Simulation duration"). |
| **Table** | Column descriptors + row-major values | Column descriptor: name, optional unit text, value kind. |
| **Note** | Plain text paragraph | For caveats and methodological remarks (e.g. "Convergence relaxed at 3 timesteps"). |
| **Chart** | Axis labels/units + chart data (below) | Declarative data only — engines describe *what* is charted, never colors, geometry, or layout. |

Chart data is one of:

- **Bar** — parallel category labels and values (distributions, rankings).
  Single-series in this revision.
- **Line** — one or more named series of (x, y) points over a continuous
  x axis (time series).

Every chart must be **table-derivable**: renderers without graphics
support present the chart as a data table derived mechanically from its
data (bar → category/value rows; line → x column plus one column per
series, absent where a series lacks that x). A chart therefore never
gates information behind a graphics-capable format.

Values are typed: number (with optional unit *text*), integer, boolean,
text, timestamp, or absent. Unit strings are display text in v1; a
structured unit system in `common` is an explicit non-goal (§1).
Nested sections and images are deferred to a later revision.

### 3.4 Production

An engine produces a fragment given:

- a block `id` from its catalog,
- the artifacts of one completed simulation (results and derived
  analytics — the engine defines internally what it needs), and
- an optional **options value**: JSON-shaped structured data whose
  meaning is defined entirely by the producing engine (thresholds,
  top-N counts, tolerances). The foundation layer and the report layer
  treat it as fully opaque — carrying it, never interpreting it. An
  absent options value means the engine's documented defaults; a
  malformed options value fails production with the `failed` error
  naming the problem. No option vocabulary may be defined in this layer.

Production is read-only and deterministic: the same simulation artifacts,
block id, and options always yield the same fragment. Production fails
with one of three neutral, typed errors:

- **unknown block** — the id is not in this engine's catalog;
- **unavailable** — the block does not apply to this run, with a
  human-readable reason supplied by the engine (e.g. "the run has no
  water-quality results"); an expected condition, not a fault;
- **failed** — reading or deriving from the simulation artifacts failed.

The report layer decides how an unavailable or failed block renders
(placeholder, omission) — the engine never does, and the contract carries
no engine vocabulary for *why* beyond the engine-authored reason text.

Block options arrived in v1.1 as production inputs only — the
*descriptor* still carries no options schema. How an engine's options are
discovered and edited by a template-builder UI is deferred; until then,
options are authored knowingly (documentation, or app-side knowledge of
specific ids).

### 3.5 Consumers and dependency rules

| Layer | May depend on | Must not depend on |
|---|---|---|
| `common` | nothing in the workspace | — |
| `engine-*` | `common` | the report layer, applications |
| report layer | `common` | any `engine-*`, applications |
| applications (CLI, GUI) | everything above, via the umbrella | — |

Applications are the composition root: they obtain catalogs and fragments
from engines and hand fragments to the report layer for rendering. The
report layer never invokes an engine; engines never render.

---

## 4. Evolution

- All v1 contracts evolve **additively**; fields are added, never
  repurposed.
- The deferred contracts (element schema, units, simulation surface) will
  arrive as *new* modules of this layer with their own spec sections,
  gated on a second engine implementation existing to validate them. Their
  arrival must not require changes to the v1 identity or report contracts.
- If a future revision must break a v1 contract, the break follows the
  library release track's semver discipline.
