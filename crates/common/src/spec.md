# Hydra Common — Foundation Contract

Status: **v1.16 — 2026-08-17** (v1.1 added opaque per-block options
to the production contract, §3.4; v1.2 added the chart fragment item,
§3.3; v1.3 added engine availability and import formats, §2.1–2.3; v1.4
added the recognition contract and its routing rules, §2.5; v1.5 — with a
second engine implemented and able to validate them — added the element
taxonomy contract (§4), the quantity contract (§5), the result-variable
contract (§6), and the run-dispatch layering rule (§2.6); v1.6 added the
optional compact symbol on variable descriptors, §6.1; v1.7 let fragment
numbers, table columns, and chart axes reference quantity keys, §3.3,
joining the fragment model to the quantity contract so consumers format
tagged values in a chosen display family, §5; v1.8 added the
engine-authored category on block descriptors, §3.2; v1.9 — with a second
engine's criteria implemented to validate it — added the criteria
contract, §7, moving Evolution to §8; v1.10 joined the two: a `banded`
variable now names the criterion its thresholds come from, §6.1, and a
criterion says what each of its regions means, §7.2 — without which a
threshold scale could only be offered to variables an application
recognised by name; v1.11 completed the editing contract: a polyline's
two ends became editable state like a position, §4.5.2.1, a collection's
contents gained the section that had left every curve countable and
unopenable, §4.5.2.2, and `references` widened to a list of kinds so an
attribute naming more than one — a subcatchment's outlet — could be
described at all, §4.5.1.1; v1.12 added attached records, §4.5.2.3, for
the rows an element carries that have no identity of their own — a
junction's demand categories, a vertex's dry-weather inflows — which
attributes could only flatten and elements could only misname; v1.13 let
a kind say what heading it is listed under, §4.2.1, so a catalog of
two dozen is a list a reader can find anything in; v1.14 let empty
contents carry a note saying why they are empty, §4.5.2.2, separating an
element that has no contents from one whose contents are held outside the
model — which a consumer had been telling apart by guessing, and getting
wrong for six kinds; v1.15 let a record set say how many rows it may
hold, §4.5.2.3, so a set that is full stops offering a row it would
refuse; v1.16 withdrew the planned open-channel engine from the registry,
§2.4, when 2D overland flow was re-planned as future functionality of the
urban drainage engine — a breaking change, since the registry lost an entry
and its key now resolves as unknown).
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
   vocabulary so an application can present and edit any engine's model
   (§4).
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
| `key` | Stable machine identifier | Lowercase ASCII, short domain-umbrella abbreviation (`wds`, `uds`). **Never changes once released** — it is persisted in project metadata and report templates. |
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

The registry holds two registered engines, `wds` (available) and `uds`
(available), in that order. `uds` shipped planned through v1 and became
available with its engine implementation; `och` (open channel) was
registered as planned from v1 through v12 and was withdrawn when 2D
overland flow was re-planned as future functionality of `uds` instead of
a separate engine. Neither key may be reused for a different domain.

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
new contract, additively (§8).

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
| `category` | Engine-authored grouping heading | Plain text, short (one or two words). Blocks sharing the exact string belong together. |

`category` lets a consumer with many blocks on screen group them — as
tabs, section headings, or not at all; the choice is the consumer's.
Group order is catalog order: a category first appears where its first
block does. The string is display text with **no foundation-defined
vocabulary** — two engines using the same word ("Summary") are not
thereby related, exactly as with quantity keys (§5).

The descriptor otherwise deliberately carries **no result-class or
prerequisite vocabulary** — what a block needs from a simulation is the
producing engine's internal concern, expressed only through the
production error contract (§3.4). Encoding result taxonomies (hydraulic
vs. quality vs. anything else) here would bake one engine family's
domain into the foundation layer; a category is not that — it is an
opaque engine-authored label carrying no semantics the foundation or the
report layer can act on beyond equality.

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
text, timestamp, or absent. Unit strings are display text; a structured
unit system in `common` remains an explicit non-goal (§1).
Nested sections and images are deferred to a later revision.

**Quantity-tagged numbers** (v1.7). A number value, a table column, and a
chart axis may each additionally reference a **quantity key** from the
producing engine's quantity catalog (§5). The tag changes what the number
*is*:

- A tagged value is expressed in that quantity's **SI display unit** —
  the same convention §5 already fixes for every quantity-bearing value
  crossing an engine boundary. Its unit text, when present, is the
  quantity's SI label. The producer performs no display formatting
  beyond this; choosing a display family is the consumer's decision,
  which is the reason the tag exists.
- A consumer holding the producing engine's quantity catalog may
  re-express the value in either display family using only the
  descriptor: convert by the affine map, label from the family's unit
  text, round by the family's advisory decimals. The catalog reaches
  such a consumer from the application, which is the composition root
  (§3.5) — this layer still never resolves a key to a descriptor itself.
- A consumer holding no catalog renders the value and its unit text
  as written. A fragment therefore remains self-describing: the tag
  refines presentation, it never gates content.
- An untagged number is what every number was before v1.7:
  engine-authored display text, rendered as given. Tags are per-value
  facts, not a fragment-wide mode — one table may carry tagged and
  untagged columns side by side (a flow column beside an
  engine-spelling text column, a count beside a pressure).

A column's tag applies to every number in that column; a chart axis's tag
applies to that axis's coordinates in the chart data. Keys are opaque and
engine-scoped exactly as in §5; a tag naming a key the engine's catalog
does not declare is a producer defect, and consumers treat the value as
untagged rather than failing the fragment.

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
block id, and options always yield the same fragment. Production
deliberately takes **no display-family input**: a producer emits
quantity-tagged values in SI display units (§3.3) and untagged values as
engine-authored text, and which family a reader sees is decided where the
fragment is presented, not where it is produced. Production fails
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
so that an application can enumerate, render, inspect and **edit** *any*
engine's model without knowing what a junction or a subcatchment is. It
follows the
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
| `group` | What this kind is listed under (§4.2.1) | Plain text, engine-authored; absent for a kind the engine does not place in one. |

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

#### 4.2.1 Groups

A catalog of twenty-four kinds is a list nobody reads. `group` is the
engine's own name for the heading a kind belongs under, so an application
listing them can break the list up without knowing what any of them are.

**It is the engine's word, not a class.** §4.1's classes say how an
application must *draw* a thing — a marker, a line, a polygon — and that
is not what a modeller calls it. A drainage engineer says "nodes" and
"links"; presenting them as "points" and "polylines" would put a
rendering vocabulary in front of a reader who has no reason to hold it.
So a group is plain engine-authored text, exactly as `label` is, and an
engine names its groups in its own domain's terms.

It also lets two engines differ where they genuinely do. One engine's
non-spatial kinds may divide into patterns and controls; another's into
rainfall, water quality, ground conditions and street drainage. Neither
has to adopt the other's shape, and this layer never learns either.

**Grouping is presentation, not structure.** Nothing here may depend on
it: it carries no meaning for simulation, validation, or what an element
*is*. An application free to ignore it renders a flat list and is
correct, which is why the field is optional and why a kind may belong to
no group at all.

The catalog's order stands. A group is a name attached to kinds that are
already adjacent in it, not an instruction to gather kinds that are not —
so an application draws a heading wherever the group changes, and never
reorders to bring a group together. An engine that wants its kinds
grouped puts them in that order.

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
remains free to hold data no schema advertises.

### 4.5 Editing contract

The contract by which an engine describes and accepts *changes* to its
model, so that one editing surface serves any engine.

It was deferred from the revision that defined §4.1–4.4, on the grounds
that describing editability before a second engine's editor existed would
repeat the mistake §1 warns against. That editor now exists, and it
exposed the failure this section prevents: with no contract, the two
engines were edited through two different sets of operations, and the
difference reached the screen. One editor staged its changes behind a
Save while the other wrote them through. One showed a position as two
ordinary columns while the other could not show it at all, because
position was not in this contract and the engine that stores it outside
its model had no way to say it had one. Neither difference is a fact
about drainage or distribution; both are facts about which engine's
editor was written first.

The rule this section exists to enforce: **a difference between file
formats is the engine's to absorb, never the application's to display.**
An application asks to move an element, name it, change a value, add one
or remove one. What that costs underneath — a field assignment, a
rewritten line in a section the engine otherwise preserves verbatim, a
cascade through a dozen index spaces — is invisible above this line.

#### 4.5.1 Editable attributes

An attribute descriptor (§4.4) carries whether it may be **written**.

The flag is advisory in the §3.2.1 sense and the write is the authority:
an engine refuses a write it will not accept whether or not any schema
advertised it, and a consumer that never reads the flag is wrong about
what to offer, never about what happens. The flag exists so a surface can
offer an input rather than offering one and being refused, which teaches
the user the same thing one interaction later.

| Field | Meaning | Constraints |
|---|---|---|
| `editable` | Whether a write to this attribute may be offered | Advisory; the write is the authority. Absent reads as not editable, so a schema written before this revision offers nothing rather than everything. |

Editability is a property of the *attribute*, not of the element. Whether
a **particular** element accepts a write is answered by whether it
carries that attribute at all: a kind's schema describes every attribute
elements of that kind may have, and an element that has none for a given
key has no value to change. A consumer needs both answers, and confusing
them offers an input that would create a value the model never held.

#### 4.5.1.1 References

An attribute whose value names *another element* says which kind it
names.

| Field | Meaning | Constraints |
|---|---|---|
| `references` | Kind ids (§4.2) whose elements this attribute may name | Empty for a value that is not a reference. |

Without it a reference is indistinguishable from free text, and an
application can only offer a box to type a name into — where the names
are the model's own, frequently numerous, and a typo produces a
reference to nothing. With it, the same generic surface can offer the
ids that exist, and can say that a value naming no element is wrong
before the engine has to.

**It is a list because a reference is not always to one kind.** A
drainage subcatchment's outlet may name a conveyance node *or* another
subcatchment, and an earlier revision of this section held a single kind
id and said so — that such an attribute "declares nothing", and that
widening the field was an additive change to make when an engine's
editor needed one. It did: the attribute stayed unwritable, and
re-routing a catchment was the one topological edit no surface offered.

The list is the complete set of kinds an application may offer for the
value, in the engine's own presentation order. Naming a subset would be
worse than naming none, because a list that looks complete is read as
complete — so an engine that cannot enumerate its targets leaves this
empty and gets a box to type into, which is honest.

It is advisory like the rest of the schema: an engine validates
references itself, and remains free to accept one this field did not
describe. It is also not a foreign key — this layer defines no
referential integrity, does not require the named element to exist, and
says nothing about what happens to the reference when it stops
existing. Those are the engine's own rules, expressed through its
validation and its removal (§4.5.4), which is where a reference that
cannot be repaired refuses.

#### 4.5.2 Position

Elements of the spatial classes (§4.1: `point`, `polyline`, `region`)
have a **position**, and it is editable through this contract
independently of any attribute schema.

Position is deliberately *not* an attribute. It is implied by the class —
this layer already requires an application to render a point as a marker,
so it already presumes the point has somewhere to be — and making it a
schema entry would let an engine omit it, which would mean an element the
application must draw and cannot move. It is also the place where the two
engines' storage differs most and matters least: one holds coordinates as
model fields, the other as lines in a section it preserves verbatim and
never interprets. An application says where; the engine decides what that
means in its own file.

A position is expressed in the model's own coordinate system. This layer
defines no projection, no unit and no datum for it — those belong to the
application's handling of the model as a whole, and a contract that
guessed here would be wrong for every model that is a drawing rather than
a map.

#### 4.5.2.1 Ends

Elements of the `polyline` class have two **ends**, each naming another
element, and both are editable through this contract.

An end is not an attribute, for the reason position is not one: it is
implied by the class. §4.1 already requires an application to draw a
polyline as a line between two elements, so it already presumes the
polyline has two of them. Leaving them to the schema would let an engine
publish a line an application must draw and cannot reconnect — and would
let two engines call the same two ends by different names, which is the
difference this section exists to keep off the screen.

The two ends are **ordered**, and the order is meaning rather than
storage. It is the sign convention for whatever the polyline carries: a
result reported as positive flows from the first end to the second. So
they are addressed as *first* and *second* and never as an unordered
pair, and an application that offers to swap them is offering to reverse
the element.

**What an end may name is the engine's judgement, not a declared kind.**
§4.5.1.1 carries one kind id for a reference and says that a value which
may name several kinds declares nothing — and an end is exactly that
case, since a line in any real model may run to several kinds of thing.
An application therefore offers the elements whose class can be an end,
which it already knows from §4.1, and the engine refuses what it will not
accept. As everywhere else here, the write is the authority.

Two rules are the contract's rather than any engine's, because they
follow from what a polyline *is*:

- An end must name an element that exists. Setting one to a name the
  model does not hold refuses; it does not create the element, and it
  does not leave the line attached to nothing.
- The two ends must differ. A polyline from a thing to itself is not a
  short line, it is not a line.

Everything else is the engine's: which kinds may be an end, whether a
particular one may be reconnected at all, and what a reconnection costs
in its file.

#### 4.5.2.2 Contents

An element of the `collection` class (§4.1) has **contents**, and they are
editable through this contract.

A curve is its points, a pattern its multipliers, a time series its
values, a control its language. None of that is an attribute: an
attribute is one value under one label, and these are a table or a block
of text whose *length* is part of what the modeller is authoring. §4.5.1
cannot describe them, and an engine with no way to say what they are is
an engine whose curves an application can count and never open.

**Contents take one of two shapes**, and an engine declares which by
which one it serves:

| Shape | What it is | Example |
|---|---|---|
| Rows | A table of numbers under engine-authored column headings, each column declaring its §5 quantity or none | A curve's points, a pattern's multipliers |
| Lines | Text, kept as written | A control rule |

Two shapes rather than one per kind, because a consumer that must know
which kind it asked for is a consumer with a list of kinds in it — and a
list of kinds is what §1 warns against. It renders whichever shape it
was given.

**Column headings are the engine's and are fixed.** What a curve's two
columns *are* depends on what the curve is for: a storage curve relates
depth to surface area, a rating curve head to discharge. Only the engine
knows, so only the engine names them — and an application may not add,
remove or reorder columns, because a table whose shape the reader can
change is no longer the thing the engine described.

**A write replaces the whole contents.** Not a row inserted, a row
removed, a cell set:

- The rows are *ordered and interdependent*. A curve's abscissae must
  ascend, a pattern's multipliers are a cycle whose length is its period.
  A per-row operation would have to be valid mid-sequence, and half of
  the useful edits are not — inserting a point before sorting it into
  place makes a curve that is briefly illegal.
- One write is one validation. An engine judges the contents it is being
  given, as a whole, once, and either takes them or refuses them.
- Its inverse is the contents that were there, which is what an
  application building undo (§4.5.5) needs and cannot assemble from a
  sequence of row operations that each individually refused.

Numbers cross this boundary in the units their column declares, the same
rule §4.5.1 applies to an attribute — the engine states the quantity, the
application converts for display and converts back, and no engine learns
what the reader chose to see.

**Serving contents does not promise to accept them.** The write is the
authority here as everywhere in §4.5, and the two shapes are not equally
writable in practice: rows are numbers under headings the engine already
named, while lines are a *language*, and taking them back means parsing
that language into whatever the engine holds. That parser is the
engine's, it is the same one that reads the model file, and an engine
that has not exposed it refuses — serving the text so it can be read is
worth doing on its own, and a surface that shows the rule and declines to
rewrite it is telling the truth.

An application therefore offers an edit where the write accepts one, and
learns which by the same means it learns anything else here: the engine
says so, or the write refuses.

**Empty is two different answers, and the engine says which.** Contents
that come back empty may mean the element has none — a pollutant is its
attributes and nothing further — or that they are real and held somewhere
this contract cannot reach, as a time series read from a file beside the
model. The two are identical on the wire and read very differently to a
person: one is a kind with nothing below its row, the other is a thing
whose values exist and are elsewhere.

So empty contents may carry a **note**: one sentence, in the engine's own
words, saying why there is nothing. A note is served only where there is
something to say. Empty contents *without* one mean the element simply
has no contents, and a consumer draws nothing at all rather than an empty
frame — a bordered panel holding a heading and no content reads as a
surface that failed to load, which is the opposite of what it means.

The note is engine-authored text, like the reason a kind cannot be
created (§4.5.3), and for the same reason: the alternative is a consumer
that works out what an empty result means from which kind it asked for,
and a consumer holding a list of kinds is what §1 exists to prevent. A
note is never served alongside contents — it explains an absence, and an
absence that has content is not one.

Everything *else* about a collection element is already described: it is
named, created and removed like any other element (§4.5.1, §4.5.3,
§4.5.4). Only its contents needed a section.

#### 4.5.2.3 Attached records

An element may carry **records**: rows of engine-described values that
belong to it and have no identity of their own.

A water-distribution junction's demand categories are this. So are a
drainage vertex's dry-weather inflows, its external inflows, its
treatment expressions, and a subcatchment's land-cover fractions. Each is
a row keyed by what it is *about* — a constituent, a land use, a demand
category's name — rather than by an id, and each element may carry
several.

Nothing else in this contract can hold them, and the two attempts to
make something else hold them both fail in the same way:

- **As attributes (§4.5.1).** An attribute is one value under one label,
  so several rows must be flattened into one. That is not hypothetical:
  an engine publishing a junction's demand as a single number is
  publishing the *sum* of its categories, and one pattern reference is
  the *first* category's. Reading is then lossy and writing is
  impossible — a total cannot be distributed back over categories nobody
  described — so the write refuses, and an element with more than one
  record becomes uneditable while looking ordinary.
- **As elements (§4.1).** An element is identified by an id, unique
  within its class. These rows have no name. Giving them a synthetic one
  makes an identifier that means two things — the row's position and the
  thing it is about — which drift apart the moment a row is removed.

##### The shape

An element carries zero or more **record sets**, each a small table:

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Stable machine identifier for the set | Engine's own; persisted by applications, so it never changes meaning once released. |
| `label` | What the set is called | Engine-authored; an application never names it. |
| `columns` | What each row holds | Described exactly as a kind's attributes are (§4.4, §4.5.1, §4.5.1.1): a label, a value shape, a quantity where numeric, referenced kinds where the value names another element. |
| `rows` | The records themselves, in the engine's order | Each row one value per column, in column order. |
| `capacity` | How many rows the set may hold | Optional. Absent means the engine knows no limit, which is the ordinary case. |

**A set that is full says so, rather than refusing.** Most sets are
open-ended — a junction may have any number of demand categories — but
some are not, and their bound is a fact about the model rather than a
policy: a control measure has one surface layer or none, a snow pack has
one of each of its three surfaces. Without a published bound an
application can only offer a row and let the write refuse it, which
turns a fixed set into a button that never works.

The bound is on the set, not on the write. An engine still validates:
`capacity` says how many rows may exist, never which ones, so a set of
three named surfaces that is not yet full can still refuse a second
row named the same as the first.

**Columns are described the way attributes are, deliberately.** A
record's cells are the same kinds of value an attribute holds — a number
with a unit, a choice, a reference to a pattern — so describing them
twice would let the two descriptions disagree, and an application would
need two renderers for one thing. This is the reuse the section is built
on: a surface that can already draw an attribute row can draw a record
table without learning anything new.

It is also what §4.5.2.2 could not do. Contents are numbers under
headings, and a record's cells are not all numbers: a dry-weather
inflow's four pattern slots are references, and a treatment expression
is text. The two sections stay separate because they describe different
relationships, not merely different types — contents are what an element
*is* (a curve is its points), records are what an element *has* (a
junction has demands).

##### Writing

A write replaces a whole set, for the reasons §4.5.2.2 gives and one
more of its own: a set is validated together. Two dry-weather inflows
for the same constituent are not two records but a contradiction, and
only the whole set can be judged for that. Adding and removing a record
is therefore writing the set with a row more or a row fewer, and the
inverse of any of it is the set that was there.

Rows are addressed by position within their set and by nothing else.
Position is not an identity — it is where the row currently sits — so a
write that reorders rows is legitimate, and an engine is free to return
them in a different order than it was given if its own storage has one.

**An engine serves what it can describe and accepts what it can take.**
A set served read-only is not a failure: showing a modeller the four
treatment expressions attached to a node is worth doing whether or not
this contract can yet rewrite them.

#### 4.5.3 Creation

A kind descriptor (§4.2) carries whether elements of it may be
**created**, and, when they may not, engine-authored text saying what is
missing.

| Field | Meaning | Constraints |
|---|---|---|
| `creatable` | Whether elements of this kind may be created | Advisory; creation is the authority. Absent reads as not creatable. |
| `not_creatable_because` | What a new one would need that cannot be defaulted | Plain text, engine-authored. Present only when `creatable` is false, and required then — a refusal without a reason is a dead end. |

Not every kind can be created from a generic surface, and the reason is
never that the engine cannot be bothered. Some kinds require a referent
that has to be chosen rather than defaulted — a relation curve, a rating,
an opening geometry — and there is no defensible value for one. Inventing
it is worse than refusing: it produces a model that runs and is wrong,
which is the failure mode this whole layer is built to avoid.

So the contract carries the refusal *and its reason*, as text the engine
writes and the application shows. An application that lists creatable
kinds and says why the others are absent is telling the user something
true; one that offers every kind and fails on submit is teaching them the
same thing later and less kindly.

What a new element needs is exactly what this contract already describes:
an identifier, somewhere to be — a position (§4.5.2), or two ends
(§4.5.2.1) for a polyline, which is placed by what it joins rather than
by a coordinate — and values for its editable attributes (§4.5.1).
Everything else is the engine's
default, and defaults are the engine's judgement — a zero maximum depth
that means "raise it to the crown of the highest connecting conduit" is a
sensible default in one engine and meaningless in another.

#### 4.5.4 Removal

Removing an element is rarely removing one thing. The contract's answer to
a removal therefore reports what actually went: the element, any elements
removed with it because they cannot exist without it, and any records that
described only it.

A removal an engine cannot perform safely **refuses, naming what
prevents it**, and changes nothing. Two outcomes are acceptable — it
happened and here is everything that went, or it did not happen and here
is why — and the third, a partial removal reported as a success, is the
one this shape exists to make unrepresentable. Which references cascade
and which refuse is the engine's judgement: a reference with exactly one
correct repair may be repaired, and one that needs a choice belongs to
whoever can make it.

#### 4.5.5 When an edit exists

An accepted edit is part of the model when the operation returns. This
contract has no staged, pending, or uncommitted state.

That is a deliberate narrowing rather than an omission. An application is
free to offer staging on top — collecting changes and applying them
together is an application's affair — but it may not require it, and it
may not present two engines differently because one of them was built
around a Save button. A contract that admitted both would make "has this
change happened yet" an engine-dependent question, and that question
reaches the user faster than any other.

Since an edit cannot be un-made by discarding a draft, an application
offering undo must implement it above this contract, in terms of the
operations here — a position restored is a move, a removed element
restored is a creation. An undo built from one engine's operations is not
undo; it is that engine's editor with a longer reach.

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
display families applications offer. Since v1.7 it reaches into the
fragment model: a fragment number, table column, or chart axis may
reference a quantity key (§3.3), in which case the value is in the
quantity's SI display unit and formatting for a display family belongs to
whichever consumer presents it — the report layer at render time, a live
application at display time. "Engines never format" thereby holds for
fragments too: a producer that tags a value stops choosing its display
family, and the engine-side conversion code that used to make that choice
is deleted, not parameterised.

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
| `banded` | Values classed against a criterion's threshold bands (§7); the hint carries the `criterion` key whose valuation supplies them. |
| `categorical` | A closed set of discrete states; the descriptor carries the engine-authored items described below, as a §3.2.1 choice does. |

An application chooses palettes and legend styling; the engine says only
which shape is truthful for the data — and, for `banded`, which criterion
the thresholds come from.

That last part is not decoration. A banded variable without it is
uninterpretable: an application holding a valuation of several criteria
cannot tell which of them bands *this* variable, and matching them by
quantity is a guess — two criteria may share a quantity, and two engines
may publish a variable of the same name meaning different things. The
consequence of guessing was observed: a drainage map was offered a
threshold scale annotated with water-distribution numbers. The criterion
named here must exist in the same engine's criteria catalog (§7.1) and
must carry severities (§7.2); an application that cannot resolve it
renders the variable as a plain magnitude rather than inventing bands.

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

## 7. Criteria Contract

The contract by which an engine describes — and consumes — the
**assessment criteria** a user asserts over a model's simulated
behaviour: minimum service pressure, a self-cleansing velocity, a
freeboard allowance. Criteria are engineering judgements about the
network, not display settings and not part of the model; they belong to
the person assessing, travel with a project, and outlive any single run.

### 7.1 Concepts

| Term | Meaning |
|---|---|
| **Criterion** | One field of the assessment standard, described by the engine. |
| **Criteria catalog** | The engine's complete, static, model-free list of criterion descriptors. |
| **Valuation** | A caller-held assignment of values to criterion keys. |

The foundation stays engine-blind, exactly as in §4–§6: criterion keys
are opaque, meaning travels only through engine-authored text, and no
criterion vocabulary (pressure, freeboard, anything else) enters this
layer.

### 7.2 Criterion descriptor

| Field | Meaning | Constraints |
|---|---|---|
| `key` | Stable criterion identifier | Unique within the engine; persisted by applications, so renaming one is a break. |
| `label` | Human-facing name | Plain text, engine-authored. |
| `help` | One or two sentences on what the criterion judges | Plain text, engine-authored. |
| `quantity` | §5 quantity key, or absent | Values of this criterion are expressed in the quantity's **SI display unit**; absent means dimensionless. |
| `kind` | Shape of the value | Below. |
| `severities` | What each region between the cut points means, ascending, or empty | Empty means the criterion is judged in reports but never drawn. When present, exactly one more entry than the criterion has cut points: a **value** criterion has one cut and so two regions, a **band** of *n* cuts has *n+1*. Each entry is `nominal`, `caution` or `alarm` — the §6.1 vocabulary, so a compliance verdict and a categorical state read alike. |

`kind` is one of:

- **value** — a single number, with a required `default`;
- **band** — an ordered list of named cut points, each `{key, label,
  default}`, defaults strictly ascending. A band's value is a same-length
  list of numbers, ascending in the same order.

Severities are a claim about the domain, and the reason they are the
engine's to make is that compliance is rarely monotonic. Service pressure
is worst when too low, acceptable in a middle, and worth attention again
when too high; conduit velocity is worth attention when too slow and wrong
when too fast. An application given only the numbers would have to decide
which end is bad, and it has no basis for that in either direction.

Defaults are the engine's judgement of a conventional standard; they are
advisory for editors and binding for consumption (§7.4).

### 7.3 Valuation

A valuation is a JSON object: criterion key → number (value kind) or
array of numbers (band kind), every number in the criterion's SI display
unit. A key absent from the valuation means the criterion's defaults; a
key the catalog does not declare is ignored, so a persisted valuation
survives catalog growth. A value of the wrong shape or holding a
non-finite number is **malformed**, and consumption refuses it with a
message naming the criterion. A band value out of ascending order is
well-formed but **degenerate** — an editor mid-edit produces one
transiently, so it must not poison the whole valuation; consumption
handles it per §7.4.

### 7.4 Consumption

An engine derives **per-block options** from a valuation: given a
valuation and a model, it answers with an options object (§3.2.1 shapes)
for each of its criteria-shaped blocks. This mapping is the engine's
own — which blocks a criterion drives, and in what units their options
are expressed, is engine knowledge that never leaks to the caller. A
criterion no block consumes may still be cataloged: applications judge
with criteria in more places than block production (a map colour scale),
and the catalog is the single description of the standard.

An engine omits a block from its answer when the valuation cannot shape
it (a degenerate band, §7.3); the block then runs on its documented
option defaults. Consumption of a well-formed valuation never fails.

### 7.5 Persistence and dependency rules

Persistence is the application's concern: where a valuation lives, and
per what scope (a project, a scenario), is not this contract's business.
The layering of §3.5 applies unchanged: engines depend on this crate,
applications compose catalogs, valuations, and production, and this
crate depends on nothing.

## 8. Evolution

- All contracts evolve **additively**; fields are added, never
  repurposed.
- The element, quantity, and result-variable contracts (§4–§6) arrived in
  v1.5 exactly this way: as new sections, gated on a second engine
  implementation existing to validate them, requiring no change to the
  identity or report contracts. The one remaining deferred contract — a
  neutral simulation session — follows the same path when a further engine
  proves its shape (§2.6); until then only its dispatch home is assigned.
- The editing contract (§4.5) arrived the same way in a later revision,
  on the same gate: it was named as a follow-up when §4 landed and held
  until a second engine's editor existed to shape it. It adds fields to
  the kind and attribute descriptors and no others, and an engine that
  sets none of them is exactly as editable as it was before — which is
  to say, not.
- Known additive follow-ups already anticipated: additional element
  classes (§4.1) should an engine need one.
- If a future revision must break a contract, the break follows the
  library release track's semver discipline.
