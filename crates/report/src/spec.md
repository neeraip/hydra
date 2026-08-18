# Hydra — Report Generation

Status: **v1.1 — 2026-08-08** (v1.1 added display-family formatting of
quantity-tagged values, §4.0, following the foundation contract v1.7).
This document follows the spec-first workflow: implementation changes flow
from changes here, never the reverse.

---

## 1. Purpose and Scope

The report layer turns neutral content fragments (foundation contract §3)
into presentable documents. Its entire job is presentation: given blocks
of data, produce txt / csv / html output. It knows **nothing about
engines or the results they produce** — no engine names, block semantics,
result classes, or domain vocabulary may appear in this layer. It depends
only on the foundation contracts.

Applications are the composition root: they obtain fragments from an
engine and hand them to this layer. This layer never invokes an engine.

**v1 non-goals:** charts and styling themes. Both must arrive additively.

---

## 2. Report Template

A template is the user's saved answer to "what goes in my report": an
ordered list of block references plus a document title. Templates are
JSON documents, shared verbatim between the GUI's template builder and
headless CLI generation.

```json
{
  "version": 1,
  "title": "Quarterly hydraulic report",
  "blocks": [
    { "id": "wds.run-summary" },
    { "id": "wds.pump-energy", "title": "Pumping cost" }
  ]
}
```

| Field | Meaning | Constraints |
|---|---|---|
| `version` | Template format version | Must be `1`. Readers reject other values with a typed error. |
| `title` | Document title | Plain text, non-empty. |
| `blocks[].id` | Block reference | Opaque to this layer — validated only by the producing engine at assembly time. |
| `blocks[].title` | Optional heading override | Plain text; replaces the block's default heading. |
| `blocks[].options` | Optional per-block options | A JSON value passed to the producing engine verbatim (foundation contract §3.4); fully opaque to this layer. |
| `removed` | Optional list of block ids the author deliberately excluded | Ignored by assembly and rendering. Template-editing consumers use it to distinguish "removed by the author" from "did not exist when this template was saved", so a block an engine adds later can join existing reports by default while deliberate removals stay removed. Absent (any template written before the field existed) means nothing was deliberately removed. |

Unknown fields are ignored on read (additive evolution); a breaking
change to the format requires a version bump. An empty `blocks` list is
valid and yields a document with no sections.

---

## 3. Document Model

Assembly pairs a template with a **producer** — a function the
application supplies that maps a block id and the block's optional
options value to a fragment or a block error (the engine's
`produce_report_block` behind the scenes) — and with the **block
catalog** the template's ids refer to (foundation contract §3.1). The
result is a render-ready document:

- **Title** — from the template.
- **Provenance** — caller-supplied: an optional generation timestamp
  (RFC 3339 text) and an ordered list of (label, value) source pairs
  (project name, scenario, file, …). The layer never reads the clock:
  identical inputs must yield byte-identical output.
- **Sections**, one per template block, in template order:
  - **Content** — the produced fragment, with the heading override
    applied when present;
  - **Unavailable** — the block does not apply to this run; carries the
    engine-authored reason and renders as an explicit placeholder;
  - **Failed** — production failed (including unknown block ids); carries
    the error text and renders as an explicit placeholder.

Placeholders are never silently omitted — a report a reader believes is
complete must not be hiding sections that could not be produced.

**Section headings** resolve in a fixed order, identically for all three
variants:

1. the template block's heading override, when set;
2. otherwise the catalog descriptor's default heading for that block id;
3. otherwise the raw block id.

A produced fragment carries its own heading, which the catalog entry
matches by construction, so step 2 changes nothing for content sections.
It exists for placeholders: a block that is unavailable or failed has no
fragment to take a heading from, and falling straight to the raw id
surfaces an internal identifier (`wds.quality-summary`) in a document
whose every other heading is prose. Step 3 remains only for an id absent
from the catalog — which is exactly the unknown-block case, where showing
the offending id is the useful thing to do.

---

## 4. Renderers

Each renderer is a pure function from a document to a string. Output is
deterministic byte-for-byte for identical documents and identical display
settings (§4.0).

### 4.0 Display settings

Rendering takes optional **display settings**: a display family (`si` or
`us`) and the producing engine's quantity catalog (foundation contract §5),
supplied by the application alongside the document. They govern
quantity-tagged values only (foundation contract §3.3):

- A tagged number is converted from its SI display unit to the chosen
  family by the descriptor's affine map, and its unit text (value unit,
  column header unit, chart axis unit) is replaced by the family's label.
  The descriptor's advisory decimals are **not** applied here — each
  format keeps its own number style (§4.1), so choosing a family never
  changes a format's precision rules.
- A tag whose key the supplied catalog does not declare, and every tagged
  value when no settings are supplied, renders as written: the SI value
  with its SI text. Untagged values are never touched.

Settings default to family `si` with no catalog, which renders every
fragment exactly as its producer wrote it. The report layer still knows
nothing beyond the foundation contract: the catalog is neutral descriptor
data handed in by the composition root, never looked up here.

**Fidelity tiering (ratified 2026-07-28).** pdf is the flagship deliverable
format: new visual features (charts, future branding/layout work) land
there first. html tracks pdf where cheap — it is also the GUI's live
preview. txt and csv are stable utility formats — diffable regression
artifacts and data interchange respectively — frozen at tabular fidelity:
new fragment kinds degrade to their tabular derivation there and never
require visual support.

**Charts (foundation contract §3.3).** pdf and html render charts as static
vector graphics sharing one SVG generator; txt and csv render the chart's
derived data table. Chart graphics follow fixed marks: bars ≤ 24 units
thick with rounded data-ends and square baselines, 2-unit surface gaps
between touching bars, 2-unit lines, hairline solid gridlines, one axis,
value labels on bar caps, line series direct-labeled at their ends when
they fit plus a legend whenever there are two or more series (never a
legend for one). Series colors come from a fixed validated categorical
order (colorblind-checked against the white document surface); color
follows the series, never its rank; text always wears ink colors, never
series colors. Documents are light-surface artifacts — charts have no
dark variant.

### 4.1 Number formatting

- **Data formats (csv):** numbers render in shortest round-trip form
  (full precision) — data fidelity over readability.
- **Human formats (txt, html):** numbers render with up to 3 decimal
  places, trailing zeros and trailing decimal point trimmed
  (`1234.5`, `0.042`, `7`). Unit text follows the number, space-separated.
- Absent values render as an em dash (txt/html) or an empty field (csv).

### 4.2 txt

Plain text, human-readable, diffable. Title underlined with `=`;
provenance as `Label: value` lines; section headings underlined with `-`;
key-value lists as aligned `Label: value` lines; tables as fixed-width
columns computed per table, header row separated by dashes, with column
units in the header (`Name (unit)`); notes as plain paragraphs;
placeholders as `[not available: reason]` / `[failed: message]` lines.

### 4.3 csv

RFC 4180 quoting (fields containing `"`, `,`, CR, or LF are quoted;
quotes doubled), `\n` line endings, UTF-8. The document opens with a
`#`-prefixed title row and `label,value` provenance rows. Sections are
separated by one blank line and introduced by a `#`-prefixed title row. Key-value lists
emit `label,value,unit` rows; tables emit a header row (`Name (unit)`)
then data rows; notes and placeholders emit a single quoted row. The `#`
convention is a pragmatic section delimiter, documented here as part of
the format.

### 4.4 html

A single self-contained document: semantic markup (`h1`/`h2`, `dl`,
`table`, `p`), inline CSS only, no external resources, no scripts.
Neutral styling that prints acceptably (the GUI's print-to-PDF path).
All text content is HTML-escaped. This rendering doubles as the GUI's
live preview.

### 4.5 pdf

Available behind an opt-in build feature (`pdf`) — it embeds a typesetting
engine and fonts, a heavyweight dependency the text formats never pay
for. The document model is typeset (via Typst) to an A4 portrait page
with 2 cm margins and embedded fonts: title and provenance header,
numbered-free section headings, key-value grids, ruled tables with
right-aligned numeric columns, and the same placeholder treatment as the
other renderers. All user text passes through string context — content
can never inject markup. No creation timestamp is embedded, preserving
byte-determinism for identical documents.

**Pagination.** Every page carries its number and the total in the bottom
margin ("3 / 12"), so a page separated from the report still says where it
belongs and whether any are missing. Every page after the first carries a
running header naming the document and what it was produced from, since a
default title identifies nothing on its own. The first page is exempt: it
already states both, and repeating them immediately above themselves reads
as an error rather than a header. A section heading is never the last
thing on a page: it is kept with the content that follows it. A table that continues onto a
further page repeats its column header there.

Neither guarantee stops a section spanning pages. A table longer than a
page must break somewhere, and it breaks where it lands — the guarantees
are only that a break never strands a heading from its content, nor a row
from its column names. Starting each section on a fresh page is a
presentation choice this format does not currently offer.

Unlike the text renderers, PDF rendering **may fail** (typesetting
diagnostics); it returns the document bytes or a typed error carrying the
diagnostic text.

---

## 5. Errors

Template parsing fails with a typed error naming the problem (bad JSON,
unsupported version, empty title). Assembly itself cannot fail — block
production failures become placeholder sections (§3). The txt/csv/html
renderers cannot fail; the pdf renderer fails only with a typed
typesetting error (§4.5).
