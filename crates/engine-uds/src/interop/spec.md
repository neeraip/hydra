# hydra-engine-uds — Interoperability Specification

This document holds §14 of the urban drainage specification: the predecessor
file formats, what importing them means, and what exporting to them promises.
It is the Tier 1 boundary of §1.4 — everything in it binds syntax and
interpretation, and nothing in it constrains how results are computed.

---

## 14. Interoperability

### 14.1 Stance

Import produces a §2 model from a predecessor input file; export writes
both the model (§14.13) and its results in forms the predecessor's readers
accept. The physics of §3–§12 is defined without reference to any of them.
Where the predecessor's file semantics presuppose behaviours this engine
does not have — reduced routing forms, approximation switches — import maps
them onto the model this engine does solve and **says so**: every
substitution, mutation, and interpretation decision surfaces as a named,
per-element notice, never a silent rewrite.

### 14.2 Input Grammar

The lexical layer is reproduced exactly. A line is at most 1024 characters,
the length re-measured up to the first `;` so overflow entirely within a
comment is legal; everything from `;` to end of line is cut before
tokenising; at most 40 tokens per line are read, separated by spaces, tabs,
carriage returns and newlines — tokens beyond the fortieth are ignored as
the predecessor ignores them, with a warning where it says nothing; a token
opening with a double quote runs to the next quote or newline — the only
way to carry a separator inside a value. Section
headers are tokens beginning `[`, matched case-insensitively. `[TITLE]`
accumulates up to three lines.

An **unrecognised section header** leaves the reader sectionless: the
header is reported, and every subsequent line is discarded until the next
recognised header — the predecessor's accept-set, reproduced. The report
carries the count of lines discarded, which the predecessor never states.

The file is read in two passes: the first registers identifiers and counts
objects, the second parses data. **Forward references are legal** — every
identifier exists before any parameter is read — and duplicate identifiers
within a type are rejected. **Identifier matching is case-insensitive**, as
the predecessor's hash table is: `Node1` and `NODE1` are one identifier,
references resolve regardless of case, and a case-only re-declaration is
the duplicate it is everywhere else in the ecosystem. Objects keep their
as-written spelling for output. `[TITLE]` retains its first three lines; further
title lines are ignored, as the predecessor ignores them.

A numeric token must parse to a **finite** value: the spellings `nan` and
`inf` (any casing or sign) and magnitudes overflowing the double range are
refused as bad values, never admitted. This is a deviation from the
predecessor, whose `strtod`-based reader can accept them: a non-finite
parameter poisons every downstream computation while the continuity
statistics — NaN-blind by construction — still report zero error, which is
a confident wrong answer of exactly the kind this engine refuses to
produce.

### 14.3 Keyword Matching

One rule performs every keyword lookup — section names, option names, and
enumerated option values: the first table entry that is a **prefix** of the
token matches. Trailing characters are ignored; truncations are not
accepted. Two orderings are load-bearing and normative: `[INLET_USAGE]`
precedes `[INLET]`, so the longer name wins; `NODE` precedes `NODESTATS` in
`[REPORT]`, so the shorter wins. The `ALL` and `NONE` values of `[REPORT]`
lists compare by full string, so `ALLNODES` is an identifier there while a
keyword with trailing garbage matches elsewhere.

**Every token matched by prefix rather than equality raises a warning** —
the accept-set is the predecessor's, but a typo it would swallow is visible
here.

`NODESTATS` is honoured in its intended meaning: recognised, ignored, and
warned as deprecated.

> **CORRESPONDENCE:** the predecessor's own tolerate-and-ignore branch for
> `NODESTATS` is unreachable — the prefix rule parses the line as `NODES`, so
> `NODESTATS YES` aborts its run on an undefined node and `NODESTATS ALL`
> silently enlarges its output file. The intended branch is present in its
> source and cannot execute; this engine executes it. No working predecessor
> file is affected: the `YES` form never ran there at all.
>
> *Source: `input.c:798`; `keywords.c:111–113`; `report.c:106–123`.*

> **DEVIATION from SWMM:** the predecessor's grate-type table lists
> `P_BAR-50` before `P_BAR-50x100`, so its prefix matching resolves the
> token `P_BAR-50x100` to the *wrong* grate family. The token names a
> grate; this reader resolves it to the grate it names.

### 14.4 Options

The full option vocabulary is accepted; every default of the predecessor is
a default here, because an omitted keyword is part of what a file means.
Options fall into four classes:

**Mapped.** Time steps, dates (month/day/year with `-` or `/` separators,
three-letter month names, decimal-hour or h:m:s times), reporting controls,
process switches (`IGNORE_*`, with an object-free subsystem ignored
automatically), quality and infiltration selections, and the numerical
options that survive as this engine's own (`MIN_SURFAREA`,
`MAX_TRIALS`, `HEAD_TOLERANCE`, `VARIABLE_STEP` — whose value is the Courant
factor of §6.5 — `MINIMUM_STEP`, `MIN_SLOPE`, …) convert units and
carry over. Time-step interlocks
apply at validation as the predecessor's do: a report step below the routing
step is fatal, the dry hydrology step is raised to the wet, the routing step
is clamped to the wet.

**Substituted, with notice.** `FLOW_ROUTING STEADY` and `KINWAVE` (and the
legacy aliases `NF`, `KW`, `EKW`) select the one solver of §6; the run
notice names the requested form. `FLOW_ROUTING NONE` means ignore-routing,
as it does in the predecessor. `SURCHARGE_METHOD` in either value maps to
the §6.2 closure. `INERTIAL_DAMPING` in any value maps to the §6.3 taper.
`LENGTHENING_STEP` is accepted and ignored with a warning naming §6.5's
retirement. `NORMAL_FLOW_LIMITED` maps directly (§6.6 keeps its semantics).

**Accepted and inert, as in the predecessor.** `SLOPE_WEIGHTING` and
`COMPATIBILITY` parse and do nothing there and here.

**Timestamp convention.** Reported timestamps are exact.

> **CORRESPONDENCE:** the predecessor adds one millisecond to every
> conversion of elapsed time to a calendar date, so each reporting timestamp
> and date-driven lookup sits 1 ms past nominal. The offset is an epoch-
> arithmetic guard, not a semantic; timestamps here are exact, and a reader
> comparing timestamps across engines sees the millisecond.
>
> *Source: `swmm5.c:1591` — `datetime_addSeconds(StartDateTime, (elapsedMsec+1)/1000.0)`.*

### 14.5 Sections

The recognised section vocabulary is the predecessor's, each mapping to the
owning specification section for its semantics. The nine display-metadata
sections (`[MAP]`, `[COORDINATES]`, `[VERTICES]`, `[POLYGONS]`, `[SYMBOLS]`,
`[LABELS]`, `[BACKDROP]`, `[TAGS]`, `[PROFILES]`) carry no engine semantics:
they are parsed for well-formedness only and preserved verbatim for writers,
so applications may consume them and a load-and-save cycle keeps them.

Within the per-object property sections — external inflows, sanitary
inflows, sewer inflows, treatment, land cover, and initial loadings — a
later line for the same object and slot **replaces** the earlier one, as it
does throughout the predecessor's ecosystem, and each override is reported.
Accumulating them would silently change what a legal file means.

`[REPORT]`'s dual grammar is reproduced: six yes/no directives (a seventh,
`NODESTATS`, is the deprecated form §14.3 honours) and three list-valued
ones (`SUBCATCHMENTS`, `NODES`, `LINKS`) whose `ALL`/`NONE`/
identifier lists select which objects a **predecessor-format export**
carries. They do not restrict this engine's own results access (§12.2).

### 14.6 Unit-Dependent Relations

Seven relations are defined by the predecessor in the *user's* unit system,
so their file coefficients change meaning with the flow-units selection.
This list is authoritative — it is longer than the predecessor's manual
acknowledges — and each entry names its conversion treatment:

| Relation | Treatment at import |
|---|---|
| Control-measure underdrain equation (§3) | coefficient converted to SI-dimensional form; the optional multiplier curve stays raw and is looked up with the offset-relative head expressed in the file's rain-depth unit — the same boundary-moves-to-the-edges rule as user-written expressions |
| Groundwater lateral-flow power function (§3) | coefficients converted per their exponents |
| Weir discharge coefficients, including coefficient curves (§7.3) | converted to the SI dimension of their relation; the embankment weir's published-chart coefficient likewise — the predecessor's $1/0.552$ SI rescale *is* this conversion |
| Outlet power-function coefficient and rating curves (§7.4) | converted per the exponent; ratings pointwise |
| Storage area/volume/depth relations (§2.6) | functional coefficients converted per their exponents; tabular curves pointwise |
| Divider rules (§7.5) | reduced-form semantics; not evaluated by this engine |
| User-written expressions: groundwater flow, deep percolation, treatment (§9.3) | **evaluated in the file's unit system**: inputs are presented to the expression in the units its author wrote it for, and the result is converted — a formula cannot be dimensionally converted, so the boundary moves to its edges |

### 14.7 Validation and Mutation

A behaviourally faithful reading of a predecessor file is the
**post-validation** model. Validation both refuses and rewrites, and both
halves are part of what a file means.

**Fatal rules** — the predecessor's cross-object refusals, adopted:
ambiguous parcel outlet; ground elevation below the initial water table;
initial depth above maximum; negative integrated storage volume; ambiguous
gage station; inconsistent co-gage formats; a gage series shared with
another consumer; a recording interval coarser than its series; a flat
transect; a negative unit-hydrograph time-to-peak or monthly $R$ sum above
1.01; cyclic treatment dependencies; non-increasing curve abscissae or
series timestamps. Diagnostics are reported exhaustively — the
predecessor's 100-error reporting cap is not carried.

**Mutations** — each applied as the predecessor applies it, and each
**warning with the element's name**, because a silent rewrite is a model the
user did not author:

- a vertex's maximum depth raised to the crown of its highest connecting
  link, with the predecessor's exemptions (pumps and bottom orifices
  exempt; downstream vertices raised only by channels; storage vertices
  skipped unless carrying a surcharge allowance);
- a regulator crest below its downstream vertex's invert raised to that
  invert — **unconditionally**, where the predecessor applies it under
  dynamic wave only, since this engine has no other mode; a file whose
  meaning depended on the reduced-form escape differs, with the notice
  naming each regulator;
- channel slope floored at the minimum-slope option, and the degenerate
  drop-exceeds-length geometry falling back to $\Delta z / L$;
- an adverse-slope channel reversed internally, all reported flows carrying
  the direction multiplier so output keeps the user's orientation;
- a negative invert offset zeroed; offsets converted between the height and
  elevation conventions per the option;
- parcel imperviousness above 100 % capped at 100 %; the five parcel
  geometry parameters rejected only for being negative;
- infeasible cross-section radii enlarged to their geometric minimum;
- street sections compiled to transects (§5.6), inlet placements
  shape-checked, invalid placements removed;
- equivalent lengths and surface areas computed for orifices and weirs
  (§7.2–§7.3).

**Advisories.** Import additionally flags, without mutating: stub channels
short enough to Courant-limit the run (§6.5); rules mixing `AND` and `OR`
whose firing depends on the precedence correction (§9.1); a user-dimensioned
ellipse cross-section, which the predecessor evaluated at fixed proportions
regardless of the entered width (§5.4); a pattern whose
declared type does not match the slot it occupies in a sanitary-inflow line,
which contributes its own type's multiplier from whatever slot it sits in —
reproduced exactly, warned because the one silent case yields a constant
factor of 1.

**Tidal boundary curves** are indexed by clock time.

> **CORRESPONDENCE:** the predecessor indexes a tidal stage curve by elapsed
> routing time from the curve's first hour, coinciding with clock time only
> for midnight starts — a 06:00 start applies the tide six hours out of
> phase. Nothing about a tide is relative to when a simulation began; this
> engine indexes by clock time, and import notices any non-midnight start
> whose tidal results differ accordingly.
>
> *Source: `node.c:1446–1452` against `:1454–1458`.*

### 14.8 Interface Files

Rainfall, runoff, and RDII interface files are read and written per the
predecessor's formats, the hotstart type alone holding `USE` and `SAVE`
separately so one run may both load and save. Routing interface files:
inflow files are read-only boundary inflows, outflow files written from
outlet vertices, one file never serving both roles in a run; values
interpolate between bracketing periods, unmatched pollutants read as zero,
and flows convert from the *file's* declared units. Declared counts are
bounded (100 constituents, 100 000 nodes) — each period allocates their
product, so an unbounded declaration would let a kilobyte-scale file
demand gigabytes. A run resumed from a
runoff interface file starts with cold antecedent state — the file replays
results, not state — and the engine says so at start-up.

**Predecessor hotstart files** (`SWMM5-HOTSTART` versions 3 and 4; the
1–2 layouts are refused with a typed error, their node-record tails and
groundwater prefixes being formats this engine does not read) are an import
and export format for the checkpoint contract of §12.3. The version-4
storage residence time is read only from version-4 files. Import recovers
what the format carries and names what it cannot: control-measure layer
state is absent from the format entirely. Surface buildup is read in the
layout the predecessor's writer actually emits — $P$ doubles per land-use ×
pollutant slot, of which the leading one is the value — so multi-pollutant
files restore completely and the stream stays aligned; this engine's export
emits the identical layout. Export writes version 4, complete but for the
named control-measure omission; this engine's own checkpoints (§12.3) are
the lossless form.

> **CORRESPONDENCE:** the predecessor's hotstart writer emits
> `Nobjects[POLLUT]` doubles per buildup slot while its reader consumes
> one, so its own reader misreads every multi-pollutant file it writes —
> the stream misaligns at the first land-use block and everything after
> restores as garbage. This engine reads the writer's true layout instead:
> multi-pollutant hotstart files restore correctly here and cannot restore
> correctly in the tool that wrote them.
>
> *Source: `hotstart.c:414–423` (writer, `fwrite(x, sizeof(double),
> Nobjects[POLLUT], f)` inside the per-pollutant loop) against `:483–491`
> (reader, one `readDouble` per slot).*

### 14.9 Output

**The binary results file** is written to the predecessor's layout: magic
number 516114522, version 52004 (the pinned predecessor's 5.2.4 encoded as
major·10⁴ + minor·10³ + patch), flow-units code, object counts, identifier
tables, pollutant unit codes, static property tables, result-variable code
lists, the reporting clock, fixed-size per-period records (subcatchment,
node, link, and the fifteen system series, in user units), and the six-int
epilog readers locate by seeking back from end of file. Per-object records
appear only for objects the `[REPORT]` selection flagged, and the stored
start date is backdated one period when reporting starts after the
simulation, both as the predecessor's readers expect. Node and link values
are period-interpolated; the period-averaged variant is served as the
predecessor defines it, settings exempted.

**Reading binary results** is the same format's other half, and the one
filesystem carve-out inside this engine: results files can dwarf the model
that produced them, so the reader operates on an explicitly supplied path
and seeks — metadata, one period, one element's series, or a sequential
scan visiting every period once — rather than requiring the whole file in
memory (the same carve-out, for the same
reason, as the water-distribution engine's results reader). Opening
validates before serving: the leading and trailing magic numbers, the
version, the epilog's section positions against the actual file length
(header, identifier tables, property tables, and the fixed-size period
records must tile the file exactly), and the stored error code — a file
whose writer recorded an error, or whose geometry does not reconcile, is
refused with a message naming what failed rather than misread. Values are
served as stored — in the file's declared unit system, per-object records
gated by the `[REPORT]` selection — with the metadata carrying everything
a consumer needs to interpret them: unit codes, object identifiers in
record order, pollutant names and concentration units, and the reporting
clock (including undoing the backdated start, so served times are true
record instants).

**The text report** reproduces the predecessor's layout, not merely its
content. The report is a compatibility surface: it is read by people
diffing this engine against the predecessor and by tools that parse the
predecessor's reports, and both fail on a report that carries the right
numbers in a different shape. So block order, block titles and their
asterisk rules, column headings and their unit rows, dashed table rules,
field widths, and decimal places are the predecessor's, and a block with
nothing to report prints the predecessor's sentence saying so rather than
an empty table.

The blocks, in order: the banner and title; the analysis-options summary,
including the process-model checklist; the runoff quantity and quality
continuity balances; the flow routing and quality routing continuity
balances; the control-actions log; the time-step critical elements, flow
instability indexes, and non-converging vertices; the routing time-step
summary; then the per-object summary tables — subcatchment runoff,
subcatchment washoff, node depth, node inflow, node surcharge, node
flooding, storage volume, outfall loading, link flow, flow classification,
conduit surcharge, pumping, and link pollutant load — each gated as the
predecessor gates it, and each drawn from the §11.2 catalogue. Continuity
blocks whose subject is absent from the model do not print; the runoff
blocks require a surface, the quality blocks require a constituent.

Volumes print in acre-feet and 10⁶ gallons under US flow units,
hectare-metres and 10⁶ litres under SI; depths in inches or millimetres.
Pollutant loads — the quality continuity balances, the subcatchment washoff
summary, the outfall loading summary, and the link pollutant load summary —
print in the predecessor's load units: pounds under US flow units,
kilograms under SI, and log₁₀ of the count (zero when the count is zero)
for count-type constituents, each column labelled with its unit word.
Instants print as the predecessor's elapsed `days hr:min`; control actions
print as absolute dates.

Four content differences are inherent and carried openly: the
flow-classification table's adjusted/actual length ratio is identically 1
(§6.5 retired the transform, the column stays for layout); the pumping
table's off-curve columns are both live for every pump type (§11.2); the
step statistics report rejections and degraded-accuracy tallies in place of
steady-state-skip time (§10.3), so the predecessor's steady-state row is
absent rather than zero; and the banner names this engine and its version,
never the predecessor's — a report is evidence of what produced it, and a
reader who cannot tell the two apart cannot use it as evidence.

### 14.10 Diagnostics

Import, validation, and export diagnostics are this engine's own, typed and
exhaustive. The predecessor's numeric error catalogue is a property of its
API, not of its files, and is not an interoperability surface — nothing in a
model file names an error code.

**Repair by omission.** A refusal is additionally marked *repairable by
omission* when neutralising its line — commenting it out — leaves a model
the predecessor accepts with identical meaning. Exactly one refusal
qualifies today: the unknown-`[OPTIONS]`-keyword refusal. Every option has
a default, and the predecessor refuses the keyword too, so omission is the
only reading the two implementations share; vendor dialects that write
extra option keywords become importable without admitting anything the
predecessor would run differently. The marking is advisory: a consumer may
comment the named line and re-read, and must surface the repair rather
than apply it silently. No other refusal qualifies — values, identifiers,
and structure all carry meaning that omission would change.

### 14.11 Recognition

This engine answers the foundation layer's recognition question
(hydra-common spec §2.5) — "are these bytes yours?" — so an application
holding a model of unknown provenance can route it without guessing. The
verdict is derived from section names alone, requiring no field parsing:

| Condition | Verdict |
|---|---|
| The first non-whitespace byte is neither `[` nor `;` (not INP-shaped) | `no` |
| Any **EPANET-exclusive** section is present | `no`, with a reason naming the tool and the giveaway section |
| At least one **SWMM-exclusive** section is present | `definite` |
| Otherwise (INP-shaped, nothing foreign, nothing exclusive) | `plausible` |

The SWMM-exclusive sections are those §14.5 defines that EPANET's input
format does not also define:

`[ADJUSTMENTS]`, `[AQUIFERS]`, `[CONDUITS]`, `[COVERAGES]`, `[DIVIDERS]`,
`[DWF]`, `[EVAPORATION]`, `[GWF]`, `[HYDROGRAPHS]`, `[INFILTRATION]`,
`[INFLOWS]`, `[LANDUSES]`, `[LID_CONTROLS]`, `[LID_USAGE]`, `[LOADINGS]`,
`[LOSSES]`, `[ORIFICES]`, `[OUTFALLS]`, `[OUTLETS]`, `[POLLUTANTS]`,
`[POLYGONS]`, `[PROFILES]`, `[RAINGAGES]`, `[SNOWPACKS]`, `[STORAGE]`,
`[SUBAREAS]`, `[SUBCATCHMENTS]`, `[TEMPERATURE]`, `[TRANSECTS]`,
`[TREATMENT]`, `[WEIRS]`, `[XSECTIONS]`.

The EPANET-exclusive sections — foreign markers that settle the question
against this engine, outranking any shared section — are:

`[DEMANDS]`, `[EMITTERS]`, `[ENERGY]`, `[LEAKAGE]`, `[MIXING]`, `[PIPES]`,
`[QUALITY]`, `[REACTIONS]`, `[RESERVOIRS]`, `[ROUGHNESS]`, `[SOURCES]`,
`[STATUS]`, `[TANKS]`, `[TIMES]`, `[VALVES]`.

Sections both formats declare — `[TITLE]`, `[OPTIONS]`, `[JUNCTIONS]`,
`[PUMPS]`, `[CURVES]`, `[PATTERNS]`, `[CONTROLS]`, `[REPORT]`, `[TAGS]`,
`[COORDINATES]`, `[VERTICES]`, `[LABELS]`, `[BACKDROP]` — carry no evidence
either way. A model built only from these is genuinely indistinguishable
from a water-distribution model by section vocabulary, and `plausible` is
the honest answer.

These two marker lists are the mirror image of the water-distribution
engine's (its model spec §4.1.3): each engine names the other's exclusive
sections as its own foreign markers, so any INP file both engines see gets
complementary verdicts and routing never has to break a tie between them.

**Recognition is stricter than parsing, deliberately.** §14.2's grammar
discards unrecognised sections rather than rejecting the file, and that
remains true when this engine is asked to parse by name. Recognition
governs only *automatic routing*, where a wrong guess silently produces a
confident wrong answer. Naming the engine explicitly supplies the evidence
recognition lacked.

### 14.12 External Rain Records

A precipitation gage may source its record from an external file (§2.4 of
the model specification): the gage declares a file name, a station
identifier, and the record's depth unit. The engine performs no file I/O —
the caller reads the file and supplies its text at load (§12.1); this
section defines what that text means.

The served format is the predecessor's **user-prepared** station format:
one reading per line,

```text
station  year  month  day  hour  minute  value
```

seven whitespace-separated fields, with blank lines and lines opening `;`
ignored. Readings for stations other than the gage's are skipped — one file
may interleave many stations — and intervals with no reading are dry, so a
record lists wet minutes only. Values are read in the record's own declared
unit (`IN` or `MM`, defaulting to the model unit system's depth unit) and
converted to the model's; their meaning — intensity, volume, or cumulative
volume over the gage's recording interval, stamped at interval start — is
the gage's declaration, exactly as for a supplied series (§3.1 of the
hydrology specification). A malformed line is a parse error naming its line
number, never skipped.

The predecessor's archival formats (NWS and Environment-Canada tape and
DSI layouts) are deferred (§1): a file in one of those layouts fails this
format's parse and is refused with the parse's own reason.

### 14.13 Model Export

Export writes a §2 model as predecessor input text. It is the other half of
§14.2's grammar and §14.5's section vocabulary: the columns export writes
are the columns import reads, so this section defines only what the
direction itself decides, never the layouts again.

#### 14.13.1 What Is Written

**The exported model is the post-validation model (§14.7), not the file as
authored.** Import refuses, rewrites, and derives; a model in memory has
already been through all three. Export therefore writes maximum depths
raised to their crowns, regulator crests lifted to their downstream
inverts, slopes floored, capped imperviousness, and enlarged infeasible
radii — each as a plain value, with nothing marking it as a rewrite.

This is the honest reading and it has a consequence worth stating plainly:
**export is not a round trip through the original file.** A file that
imports with mutation warnings, exported and compared against its source,
differs everywhere a warning was raised. The engine holds a model, not a
document; a consumer wanting the author's text must keep the author's text.

Two mutations are *presentation* rather than substance, and export undoes
them so the written file reads as its author wrote it:

- an adverse-slope channel, reversed internally, is written in the user's
  orientation with its original end order and sign conventions;
- invert offsets are written in the convention the model's own option
  declares, height or elevation, not the internal one.

Quantities import *derives* are not written: the equivalent lengths and
surface areas computed for orifices and weirs (§7.2–§7.3), and the
transects compiled from street sections (§5.6). A street is written as a
street, because that is what its author wrote and what its inlet placements
name; writing its compiled transect instead would re-import as a model with
no streets in it.

#### 14.13.2 The Round-Trip Contract

Three properties define correctness, in ascending strength:

1. **Semantic round trip.** Import, export, and import again yields a model
   identical to the first — every value, every identifier, every
   relationship. This is the property that matters, and it is the one the
   corpus is tested against.
2. **Writer idempotence.** The second export is byte-identical to the
   first. A writer that is semantically correct but not idempotent hides a
   value that survives one cycle and drifts on the next.
3. **Mutation quiescence.** Re-importing an exported file raises none of
   §14.7's mutation warnings. The mutations were applied before the file
   was written, so a second import finds nothing left to rewrite; a warning
   on re-import means export wrote something import did not mean.

The third is the sharpest test of the first. A depth already at its crown
cannot be raised again, a floored slope cannot be floored further — so any
mutation warning on re-import localises the defect to the section that
raised it.

Advisories (§14.7) are exempt: they flag properties of the model itself — a
stub channel, an ambiguous pattern slot — and survive export because they
describe something export faithfully preserved.

#### 14.13.3 Units and Numeric Form

Every stored quantity is written in the unit system the model's own
flow-units selection declares, by the **exact inverse** of the conversion
import applied (§14.6). The seven unit-dependent relations invert by their
own entries in that table: coefficients converted per their exponents,
curves pointwise, and user-written expressions written unchanged — an
expression was never converted, only its inputs and result were, so there
is nothing to invert.

Numbers are written in **shortest round-trip decimal form**: the fewest
digits that re-read as the same value. Fixed precision is not sufficient
and the failure is not hypothetical — a tolerance or a decay constant set
programmatically can sit below any fixed decimal precision, serialise as
zero, and re-read as a value no test can satisfy. Where the predecessor's
own readers require a fixed form for a field, that field's entry in §14.5
governs and the constraint is noted there.

Time-valued fields are written in the clock forms §14.4 defines, never as
bare numbers. A bare number re-reads as decimal hours, so a duration
written plainly would multiply by 3600 on every save-and-load cycle.

Identifiers are written quoted when they contain any separator §14.2's
lexer would split on, and unquoted otherwise, so that an identifier
survives a cycle whatever it contains.

#### 14.13.4 Defaults and Omission

A field is written when it differs from the value import would assume in
its absence, and omitted when it matches. The exception is a field whose
import default is *derived* from another field rather than fixed: those are
always written, because omitting them makes the re-read value depend on a
neighbour that may itself have changed.

Omission is never used to express meaning. A value that is absent because
it is the default and a value that is absent because it is unset must not
be the same file, so any model state with no defaulted spelling is written
explicitly.

#### 14.13.5 Sections and Ordering

Sections are written in a fixed order — the predecessor's own, so a reader
comparing an exported file against a hand-authored one finds them where it
expects — and a section with nothing to write is omitted entirely rather
than written empty.

The nine display-metadata sections (§14.5) are written from what import
preserved, verbatim, in their original order. They carry no engine
semantics, so export neither validates nor normalises them; a consumer that
put them there gets them back.

`[REPORT]`'s selection lists round-trip as authored, including the
`ALL`/`NONE` spellings, because they select what a results export carries
(§14.9) and rewriting `ALL` as an enumeration would silently freeze a
selection that was meant to follow the model.

Auxiliary files are written **by name only**: the declarations naming
climate records, rain records, hotstart state, and interface files are
preserved, and their contents are not written. The engine performs no file
I/O (§12.1), and a declaration is part of the model while the file it names
is not.

#### 14.13.6 What Export Loses, and What It Refuses

Three things do not survive, all of them document rather than model, and
all of them stated here so no consumer discovers them by comparison:

- **Comments.** Every `;` comment, including the descriptive ones the
  predecessor's own interface writes above object definitions.
- **Original section order and whitespace.** §14.13.5's fixed order and
  column layout replace whatever the source used.
- **Unrecognised sections.** §14.2 discards them at import for forward
  compatibility, so there is nothing left to write.

> **CORRESPONDENCE:** the predecessor's interface preserves a file's
> comments and its own object-description comments across a save. It can,
> because it holds the document; this engine holds the model. An
> application that must keep a user's comments has to keep the source text
> alongside the model, and the interface layer is where that belongs.

Export **refuses**, rather than writing something that will not re-read,
when the model holds state with no spelling in the format: an identifier
containing a line break, a quantity that is not finite, or a construct
built programmatically that the grammar has no form for. A refusal names
the element and what about it cannot be written. Silently dropping such
state would produce a file that imports cleanly and means something else,
which is the one outcome worse than failing to write at all.
