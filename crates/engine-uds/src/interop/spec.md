# hydra-engine-uds — Interoperability Specification

This document holds §14 of the urban drainage specification: the predecessor
file formats, what importing them means, and what exporting to them promises.
It is the Tier 1 boundary of §1.4 — everything in it binds syntax and
interpretation, and nothing in it constrains how results are computed.

---

## 14. Interoperability

### 14.1 Stance

Import produces a §2 model from a predecessor input file; export writes
results in forms the predecessor's readers accept. The physics of §3–§12 is
defined without reference to either. Where the predecessor's file semantics
presuppose behaviours this engine does not have — reduced routing forms,
approximation switches — import maps them onto the model this engine does
solve and **says so**: every substitution, mutation, and interpretation
decision surfaces as a named, per-element notice, never a silent rewrite.

### 14.2 Input Grammar

The lexical layer is reproduced exactly. A line is at most 1024 characters,
the length re-measured up to the first `;` so overflow entirely within a
comment is legal; everything from `;` to end of line is cut before
tokenising; at most 40 tokens per line, separated by spaces, tabs, carriage
returns and newlines; a token opening with a double quote runs to the next
quote or newline — the only way to carry a separator inside a value. Section
headers are tokens beginning `[`, matched case-insensitively. `[TITLE]`
accumulates up to three lines.

An **unrecognised section header** leaves the reader sectionless: the
header is reported, and every subsequent line is discarded until the next
recognised header — the predecessor's accept-set, reproduced. The report
carries the count of lines discarded, which the predecessor never states.

The file is read in two passes: the first registers identifiers and counts
objects, the second parses data. **Forward references are legal** — every
identifier exists before any parameter is read — and duplicate identifiers
within a type are rejected. `[TITLE]` retains its first three lines; further
title lines are ignored, as the predecessor ignores them.

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

### 14.5 Sections

The recognised section vocabulary is the predecessor's, each mapping to the
owning specification section for its semantics. The nine display-metadata
sections (`[MAP]`, `[COORDINATES]`, `[VERTICES]`, `[POLYGONS]`, `[SYMBOLS]`,
`[LABELS]`, `[BACKDROP]`, `[TAGS]`, `[PROFILES]`) carry no engine semantics:
they are parsed for well-formedness only and preserved verbatim for writers,
so applications may consume them and a load-and-save cycle keeps them.

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

### 14.8 Interface Files

Rainfall, runoff, and RDII interface files are read and written per the
predecessor's formats, the hotstart type alone holding `USE` and `SAVE`
separately so one run may both load and save. Routing interface files:
inflow files are read-only boundary inflows, outflow files written from
outlet vertices, one file never serving both roles in a run; values
interpolate between bracketing periods, unmatched pollutants read as zero,
and flows convert from the *file's* declared units. A run resumed from a
runoff interface file starts with cold antecedent state — the file replays
results, not state — and the engine says so at start-up.

**Predecessor hotstart files** (`SWMM5-HOTSTART` versions 1–4) are an import
and export format for the checkpoint contract of §12.3. Import recovers what
the format carries and names what it cannot: control-measure layer state is
absent from the format entirely, and surface buildup is recoverable only for
single-pollutant models — the predecessor's writer and reader disagree
beyond that, its files never round-tripping multi-pollutant buildup. Export
writes version 4, complete for single-pollutant models, with the same named
omissions otherwise; this engine's own checkpoints (§12.3) are the lossless
form.

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

**The text report** follows the predecessor's structure — banner, title,
optional input echo, options summary, rainfall and RDII summaries, the
control-actions log, the continuity balances of §11, the numerical-
performance block, and the per-object summary tables with the predecessor's
grouping and gating. Three content differences are inherent and carried
openly: the flow-classification table's adjusted/actual length ratio is
identically 1 (§6.5 retired the transform, the column stays for layout);
the pumping table's off-curve columns are both live for every pump type
(§11.2); and the step statistics report rejections and degraded-accuracy
tallies in place of steady-state-skip time (§10.3).

### 14.10 Diagnostics

Import, validation, and export diagnostics are this engine's own, typed and
exhaustive. The predecessor's numeric error catalogue is a property of its
API, not of its files, and is not an interoperability surface — nothing in a
model file names an error code.

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
