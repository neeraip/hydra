# Hydra Common — Foundation Contract

Status: **v1.6 — 2026-08-03** (v1.1 added opaque per-block options
to the production contract, §3.4; v1.2 added the chart fragment item,
§3.3; v1.3 added engine availability and import formats, §2.1–2.3; v1.4
added the recognition contract and its routing rules, §2.5; v1.5 — with a
second engine implemented and able to validate them — added the element
taxonomy contract (§4), the quantity contract (§5), the result-variable
contract (§6), and the run-dispatch layering rule (§2.6); v1.6 added the
optional compact symbol on variable descriptors, §6.1).
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

The layer defines five contracts:

1. **Engine identity** — what an engine *is*, including how it is
   recognised from a model's bytes (§2).
2. **Reportable output** — what an engine can contribute to a report (§3).
3. **Element taxonomy** — how an engine describes its model's element
   vocabulary so an application can present any engine's model (§4).
4. **Quantities** — how an engine declares the physical quantities its
   values carry, so applications can format and convert them (§5).
5. **Result variables** — how an engine describes the per-element result
   series a completed simulation carries (§6).

Contracts 3–5 were **explicit non-goals for v1.0** (ratified 2026-07-28):
abstracting them from a single implementation risked baking
water-distribution assumptions into the foundation, so they were deferred
until a second engine implementation existed to exercise them. The urban
drainage engine is that second implementation, and its model — in
particular the subcatchment, which is neither a node nor a link — is the
stress test these contracts were shaped against.

**Still deferred:** a cross-engine simulation contract (a neutral session
type every engine implements). Two engines with genuinely different run
shapes are not yet evidence of the right trait; §2.6 assigns *where* the
uniform run surface lives without defining it here. Nothing in this layer
may presuppose the shape that future contract will take.

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

The registry holds three registered engines — `wds` (available), `uds`
(available), and `och` (planned) — in that order. `uds` shipped planned
through v1 and became available with its engine implementation.

### 2.5 Recognition

§2.2 establishes that an extension cannot decide which engine owns a file.
Recognition is how that question *is* answered: given the bytes of a
candidate model, each engine reports whether the model is one of its own.

The foundation layer defines only the neutral verdict. It contains no
section names, no format grammar, and no engine vocabulary of any kind —
the judgement is authored entirely by the engine, and this layer merely
gives every engine the same three words to say it in:

| Verdict | Meaning |
|---|---|
| `definite` | The bytes carry a marker that belongs to this engine's format and to no other. |
| `plausible` | The bytes are shaped like this engine's format but carry nothing that distinguishes them from another engine claiming the same shape. |
| `no` | The bytes are not this engine's, either because the format is unrecognised or because they carry another format's marker. May carry engine-authored text saying what the engine believes the file is instead. |

The optional text on `no` is the same device the reportable-output contract
uses for an unavailable block (§3.4): the foundation layer holds no words of
its own, and an engine that can say *"this is a SWMM model, it declares a
`[SUBCATCHMENTS]` section"* gives an application something far more useful to
report than a bare refusal. It is advisory — an application must behave
identically whether or not it is present.

**Recognition is not validation.** It answers "whose is this?", not "can
this run?". It must be cheap enough to run against every registered
engine before any model is parsed, so it may inspect only as much of the
input as identification requires. A `definite` verdict is not a promise
that the model is well-formed or simulable — that remains the owning
engine's parse and validation step, which may still reject it.

**Recognition may be stricter than parsing.** An engine may decline to
claim a file it would nonetheless parse successfully when told to. This
is deliberate: automatic routing must not guess, whereas an explicit
instruction from the user carries information routing does not have.

#### 2.5.1 Routing

An application holding a model of unknown provenance resolves it by
asking every **available** engine (§2.3) and applying, in order:

1. Exactly one `definite` — that engine owns the model.
2. More than one `definite` — ambiguous. This indicates two engines
   claiming the same marker and is a defect in one of them; report it as
   ambiguity rather than choosing.
3. No `definite`, one or more `plausible` — ambiguous, however few
   engines answered that way.
4. Nothing but `no` — unrecognised.

Rule 3 holds **even when exactly one engine answered `plausible`**, and
even when only one engine is available at all. A `plausible` verdict means
precisely "I cannot distinguish this from another engine's model", so
acting on it is the guess this contract exists to prevent — the model may
belong to an engine that is registered but planned (§2.3), or to one a
later release adds. An engine that can genuinely identify its own models
returns `definite`; if it cannot, the shortfall is in its recognition, not
something routing should paper over.

The two failures are therefore distinguishable and should be reported
differently: ambiguity means "narrow it down for me" and is answered by
naming the engine explicitly, whereas unrecognised means no engine here
reads this format at all.

Routing **must never fall back to a default engine.** Ambiguous and
unrecognised are terminal outcomes that the application reports, offering
the user the means to name the engine explicitly. Choosing arbitrarily
would hand a model to a solver that models different physics and return a
confident, wrong answer — the failure §2.2 exists to prevent.

Planned engines (§2.3) are not consulted, having no implementation to
consult. An application that can otherwise identify the model as a
planned engine's — for example because the owning engine returned `no`
and named the foreign format — should say so rather than reporting a
generic failure: "this is a SWMM model, and that engine is not yet
implemented" is actionable where "unrecognised" is not.

#### 2.5.2 Layering

The registry (§2.4) is inert data and cannot invoke engines: this layer
depends on nothing, and an engine's recognition lives in the engine. The
dispatch that consults each engine and applies §2.5.1 therefore belongs
to a layer that sees both this contract and every engine — never to an
individual application, which would duplicate the routing policy in every
interface and let them drift apart.

### 2.6 Run dispatch

Running a model is engine-owned, and each engine's run has its own shape:
one engine solves in phases and streams results as they become final;
another steps a single cascade and writes results when it completes.
Knowing those shapes — "how do I drive engine X from bytes to a results
file?" — is per-engine knowledge of exactly the kind §2.5.2 forbids
applications from holding, and for the same reason: an application that
encodes it duplicates it in every interface, and the copies drift.

The **uniform run surface** — open a model for its engine, advance it,
observe progress, persist its results, collect its warnings — therefore
belongs to the same both-seeing dispatch layer as routing (§2.5.2), and
every application drives every engine through that one implementation.

This section deliberately assigns *where* that surface lives and no more.
Its concrete shape is the dispatch layer's own, documented with its
implementation, because a neutral session contract in this layer remains
an explicit non-goal (§1): it would have to be abstracted from two run
shapes that genuinely differ, and this layer must not freeze a guess. When
a later engine proves the common shape, the surface graduates here as a
new contract, additively (§7).

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

### 3.2.1 Option descriptors

A block may accept options (§3.4). So that a template-builder UI can offer
them without knowing any engine, an engine can **describe** the options one
of its blocks accepts. A description is a list of option descriptors:

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Field name in the options object | Stable per block; renaming one is a break, like a block id. |
| `label` | Human-facing control label | Plain text, engine-authored. |
| `help` | One or two sentences explaining the option | Plain text, engine-authored. |
| `kind` | What shape the value takes, and its bounds | Below. |
| `unit` | Display unit text, or absent | Display text only — never a unit system (§3.3). |

`kind` is one of: **number** (optional default, optional inclusive
minimum and maximum), **integer** (same), **boolean** (optional default),
**text** (optional default), **number list** (optional default, optional
minimum length, and a flag requiring strict ascent — threshold edges),
**choice** (one of a supplied list of items), or **multi-choice** (any
subset of one). A choice item is an opaque `value` plus a `label` for
display.

**Descriptions are resolved against a model, not fixed by the catalog.**
The catalog (§3.2) is static and model-free, because listing blocks must
not require a loaded model. Options are the opposite: their permissible
values and their correct defaults are frequently properties of the model
in hand — which constituents exist, which land uses, and what unit system
the file declares. An engine therefore describes a block's options given
that block's id **and the model**, exactly as it produces a fragment given
the model (§3.4). Only the description vocabulary lives in this layer; the
model type is the engine's own and is never named here.

This is why descriptors carry values rather than pre-rendered text: an
engine resolving `minPressure` for a US-customary model returns a default
of 20 with unit `psi`, and for an SI model 14 with unit `m`. A consumer
displays what it is given and computes nothing.

A description is advisory. It tells a UI what to offer; it is **not** the
validation authority. Production (§3.4) validates independently and remains
the sole judge of a malformed options value, so an engine is free to accept
values no description advertised, and a consumer that skips the description
entirely — as a template authored by hand does — is unaffected. Describing
no options for a block means a UI offers none, not that none are accepted.

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
  human-readable reason supplied by the engine, written as a complete
  sentence because a consumer may show it standing alone rather than after
  a label (e.g. "The run has no water-quality results."); an expected
  condition, not a fault;
- **failed** — reading or deriving from the simulation artifacts failed.

The report layer decides how an unavailable or failed block renders
(placeholder, omission) — the engine never does, and the contract carries
no engine vocabulary for *why* beyond the engine-authored reason text.

Block options arrived in v1.1 as production inputs only. Since v1.3 an
engine can additionally **describe** the options a block accepts (§3.2.1),
so a template-builder UI can offer them generically. Production is
unchanged by this: it validates the options value it is given regardless of
what was described, and a hand-authored template that never consults a
description behaves exactly as before.

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

## 4. Element Taxonomy Contract

The contract by which an engine describes its model's element vocabulary,
so that an application can enumerate, render, and inspect *any* engine's
model without knowing what a junction or a subcatchment is. It follows the
same discipline as recognition (§2.5) and reportable output (§3):
**engine-specific meaning travels only through opaque ids and
engine-authored text**; this layer contributes structure, never domain
vocabulary.

### 4.1 Element classes

The single piece of structural vocabulary this layer owns is the **element
class** — the geometric and referential nature of an element, which an
application must know to render and organise it, and which is genuinely
engine-independent:

| Class | Nature | Application obligations |
|---|---|---|
| `point` | A located element: one coordinate. | Render as a marker; selectable; may anchor `polyline` ends and `region` outlets. |
| `polyline` | A connecting element: references a from-`point` and a to-`point`, with optional intermediate vertices. | Render as a line/path between its endpoints; selectable. |
| `region` | An areal element: a polygon boundary, with an optional reference to a `point` element it discharges to. | Render as a filled polygon; selectable; the discharge reference may be visualised as a connector. |
| `collection` | A non-spatial named object (a curve, a pattern, a time series, a control). | Enumerable and countable; presentation is application-defined and may be engine-specific. |

The class list is closed **in this revision**; extending it is an additive
spec change here, not an engine decision. A subcatchment is the proof case
for `region`: it is neither a node nor a link, and any taxonomy that
offered only those two classes would have baked one engine family's shape
into the foundation.

### 4.2 Element kinds

Within those classes, an engine describes its **kinds** — junction, tank,
conduit, subcatchment, rain gage — as an ordered catalog of descriptors:

| Field | Meaning | Constraints |
|---|---|---|
| `id` | Stable kind identifier | Opaque to this layer; stable per engine — persisted data and application preferences may reference it. |
| `label` | Human-facing singular name | Plain text, engine-authored. |
| `label_plural` | Human-facing plural name | Plain text, engine-authored. |
| `class` | The kind's element class (§4.1) | One of the four classes. |
| `role` | What the kind does in the network (§4.3) | One of the three roles, or absent. |
| `badge` | Short glyph for dense UI (markers, chips) | One or two characters, engine-authored. |

The catalog is static and model-free, like the block catalog (§3.2): an
application must be able to build its chrome — tables, filters, layer
toggles, legends — before any model is loaded. Kind ids follow the block-id
stability rule: removing one, or changing the *meaning* of one, is a break
on the order of a file-format break.

**Identity:** every element carries an engine-scoped string identifier.
This layer requires only that the pair (kind id, element id) is unique
within a model; whether identifiers are additionally unique across kinds
(as they are within one engine's node family) is the engine's own rule,
expressed through its validation, not through this contract.

### 4.3 Element roles

A class says what an element *is* geometrically. A **role** says what it
does in the network:

| Role | Meaning |
|---|---|
| `conveyance` | Carries flow without imposing a boundary or a control on it — a junction, a pipe, a conduit. The bulk of any model. |
| `boundary` | Where the model meets what it does not simulate: a fixed head or stage, a storage volume, an outfall. Flow enters or leaves the modelled system here. |
| `control` | Acts on the flow rather than merely passing it — a pump, a valve, a weir, an orifice, a flow divider. |

Role exists because it is the distinction an application must draw to
present an *unsimulated* model at all. Before any results exist there is
nothing to colour by, and a network drawn in one uniform tone tells a
reader nothing; what they need to see is where the system is fed and
drained, and where something acts on the flow. Class cannot answer that —
a pump and a pipe are both `polyline`, a reservoir and a junction both
`point` — and kind cannot either without the application naming kinds it
should not know.

**A kind may have no role at all.** A rain gage is located but conveys
nothing; a curve, a pattern and a control rule are not in the flow network
to begin with. Those declare no role, and an application draws them by
whatever means suits — the absence is information, not an omission to be
defaulted away.

The role list is closed in this revision, and extending it is an additive
spec change here rather than an engine decision, exactly as the class list
is. Roles carry no presentation: an application decides what a boundary
looks like, and this layer decides only which kinds are boundaries.

> **Assignment is the engine's judgement, not a lookup.** A storage unit is
> a boundary in drainage because it is where volume leaves the routed
> network, while a tank is a boundary in distribution for the same reason
> expressed differently. Where a kind is arguably two roles, the engine
> picks the one an application should draw it as.

### 4.4 Attribute schemas

For each kind, an engine describes the attributes an application may
display for elements of that kind — an ordered list of attribute
descriptors reusing the option-descriptor vocabulary of §3.2.1:

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Field name in the element's attribute data | Stable per kind; renaming one is a break, like a block id. |
| `label` | Human-facing name | Plain text, engine-authored. |
| `kind` | Value shape and bounds | The §3.2.1 kinds, unchanged. |
| `quantity` | Key of the physical quantity the value carries (§5), or absent | Absent means dimensionless or textual. |

An attribute schema is advisory in exactly the §3.2.1 sense: it tells a
generic UI what to show; it is not the validation authority, and an engine
remains free to hold data no schema advertises. This revision defines
attribute schemas for **display**. Editability, defaults, and creation
flows are a later additive revision — describing them before a second
engine's editor exists would repeat the mistake §1 warns against.

---

## 5. Quantity Contract

Fragments carry unit strings as display text (§3.3) because a rendered
report needs no arithmetic. Live applications do: they let the user choose
a display unit system, format values in it, and accept input in it. The
quantity contract is how an engine declares the physical quantities its
values carry so that applications can do that generically.

An engine publishes a static catalog of **quantity descriptors**:

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Stable quantity identifier | Opaque to this layer; referenced by attribute schemas (§4.3) and result variables (§6). |
| `si_label` | Unit text in the SI display system | Plain text, e.g. "m", "L/s", "mm/hr". |
| `us_label` | Unit text in the US-customary display system | Plain text, e.g. "ft", "gpm", "in/hr". |
| `si_to_us` | Affine conversion from SI display value to US display value | A scale factor and an offset (offset 0 for all but temperature-like quantities). |
| `si_decimals` / `us_decimals` | Suggested display precision per system | Advisory formatting hints. |

Values crossing an engine boundary for a quantity-bearing field are **in
that quantity's SI display unit**; the application converts for display
and converts back on input, using only the descriptor. Engines never
format, and applications never hardcode a conversion — the descriptor is
the single authority, so a quantity this layer has never heard of (a
rainfall intensity, an infiltration rate) costs an application nothing to
support.

Quantity keys are engine-scoped: two engines may both declare a `flow`
quantity, and nothing requires their descriptors to agree, because no
value ever crosses between engines. The catalog is static; which
attributes and variables *reference* which quantities is declared where
those are declared (§4.3, §6).

This contract deliberately does not model unit *systems* beyond the two
display families applications offer, and it does not touch §3.3: fragment
unit strings remain engine-authored display text, unchanged.

---

## 6. Result-Variable Contract

The contract by which an engine describes the per-element time-series
variables a completed simulation carries — pressure, flow, depth,
runoff — so an application can offer result exploration (map colouring,
legends, per-element series, period scrubbing) for any engine.

### 6.1 Variable descriptors

For each element class it produces results for (§4.1), an engine publishes
an ordered catalog of variable descriptors:

| Field | Meaning | Constraints |
|---|---|---|
| `id` | Stable variable identifier | Opaque to this layer; application preferences and saved views may reference it. |
| `label` | Human-facing name | Plain text, engine-authored. |
| `symbol` | Compact notation for space-starved surfaces (column headers, chips), or absent | Engine-authored, at most three characters, ideally the domain's standard notation (Q for discharge, y for depth, Ø for diameter). Absent means the application derives its own fallback, e.g. the label's initial. |
| `quantity` | Key of the quantity the values carry (§5), or absent | Absent means dimensionless. |
| `ramp` | How values are meaningfully mapped to a colour scale | One of the ramp hints below. |

Ramp hints are the only presentation vocabulary this layer contributes,
and they are shape statements, never colours:

| Hint | Meaning |
|---|---|
| `sequential` | Magnitude on a continuous low→high scale. |
| `diverging` | Signed values around a meaningful zero (e.g. flow direction). |
| `banded` | Values classed into user-configurable threshold bands. |
| `categorical` | A closed set of discrete states; the descriptor carries the engine-authored items described below, as a §3.2.1 choice does. |

An application chooses palettes, band edges, and legend styling; the
engine says only which shape is truthful for the data.

#### Categorical items

Each item of a `categorical` variable carries:

| Field | Meaning | Constraints |
|---|---|---|
| `value` | The number the result series stores for this state | Engine-authored; unique within the variable. |
| `label` | Human-facing name for the state | Plain text, engine-authored. |
| `severity` | Whether the state is unremarkable, worth attention, or wrong, or absent when the states carry no such judgement | One of `nominal`, `caution`, `alarm`. |

Severity is a statement about the *domain*, not about presentation: a
closed pipe is an abnormal condition in a pressurised network whoever is
looking at it, and only the engine knows that. Without it an application
can order states but cannot rank them, so it must colour a closed pipe and
an open one as merely *different* — losing a distinction the engine
already held. It stays optional because it is a real claim: a state set
that is genuinely just a partition (a land-use class, a material) must not
be forced to invent a judgement, and absent means exactly that.

As with every hint here, this fixes no colours. An application decides
what caution and alarm look like, and remains free to ignore severity
entirely.

### 6.2 Presence

Not every catalog variable exists in every run — a quality variable is
absent from a run with quality disabled. An engine therefore reports,
**for a given completed simulation's results**, which of its catalog
variables are present, resolved the way block options are resolved against
a model (§3.2.1): the catalog stays static, presence is per-run. An
application offers only present variables and treats an absent one the way
the report layer treats an unavailable block — an expected state, not an
error.

### 6.3 Addressing

Consumers address results by (element class, variable id, reporting
period), and per-variable minimum/maximum envelopes are addressed by
(element class, variable id). Wire encodings, caching, and file formats
are the consumer's own concern and are not part of this contract — but
they must be derived from the catalog rather than fixing a variable list,
or they re-create the closed-set coupling this contract exists to remove.

---

## 7. Evolution

- All contracts evolve **additively**; fields are added, never
  repurposed.
- The element, quantity, and result-variable contracts (§4–§6) arrived in
  v1.5 exactly this way: as new sections, gated on a second engine
  implementation existing to validate them, requiring no change to the
  identity or report contracts. The one remaining deferred contract — a
  neutral simulation session — follows the same path when a further engine
  proves its shape (§2.6); until then only its dispatch home is assigned.
- Known additive follow-ups already anticipated: editability, defaults,
  and creation flows on attribute schemas (§4.3), and additional element
  classes (§4.1) should an engine need one.
- If a future revision must break a contract, the break follows the
  library release track's semver discipline.
