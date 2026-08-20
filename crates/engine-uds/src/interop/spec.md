# Urban Drainage — Interoperability Specification

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

**A storage unit's seepage tail.** After a storage unit's geometry,
surcharge depth and evaporation fraction, its line may carry Green-Ampt
seepage parameters: suction head, saturated conductivity, initial moisture
deficit. A *single* trailing value is the conductivity alone, with the
other two zero. That is the predecessor's own shorthand, and it is the
form real files use to say "no seepage" by writing a conductivity of zero.
Two trailing values are not a form, and a line carrying two is refused.
Seepage is proportional to conductivity, so a zero conductivity is no
seepage whatever the other two parameters say.

**An outfall's two optional tails** are a flap gate and the parcel its
outflow is routed to, in that order, and either may be given without the
other. Both are read here whenever they are present.

> **DEVIATION from SWMM:** the predecessor tests the two tails with
> `ntoks == n` and `ntoks == n+1` rather than by length, so a line
> carrying *both* satisfies only the second: the parcel is read and the
> flap gate is silently discarded, leaving the outfall ungated. Its own
> documented format for the section lists `(flapGate) (routeTo)` as
> independently optional, so the gate is read here. A file giving only one
> of the two behaves identically in both.
>
> *Source: `node.c:outfall_readParams`, `:1392` and `:1399`.*

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

Rainfall, runoff, and RDII interface files are declared per the
predecessor's syntax, the hotstart type alone holding `USE` and `SAVE`
separately so one run may both load and save. The RDII
(§14.8.1), runoff (§14.8.2) and rainfall (§14.8.3) formats are all served,
each read and written. Routing interface files:
inflow files are read-only boundary inflows, outflow files written from
outlet vertices, one file never serving both roles in a run; values
interpolate between bracketing periods, unmatched pollutants read as zero,
and flows convert from the *file's* declared units. A file's span is served
inclusive of both its end instants, and an instant within a millisecond of
either end is inside it; beyond that the file contributes nothing rather than
its nearest record held flat, which would invent hours of inflow. That
tolerance is part of the definition and not a rounding convenience: the
instants being compared are absolute epoch seconds, whose representable
values are already spaced a fraction of a microsecond apart, so a tolerance
finer than that is no tolerance at all — and a run whose clock reaches the
file's last instant by accumulating steps rather than by the same arithmetic
would lose that period's inflow entirely. Declared counts are
bounded (100 constituents, 100 000 nodes) — each period allocates their
product, so an unbounded declaration would let a kilobyte-scale file
demand gigabytes. A run resumed from a
runoff interface file starts with cold antecedent state, the file replaying
results rather than state, and the engine says so at start-up (§14.8.2).

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

#### 14.8.1 RDII interface files

An RDII interface file replays a hydrograph of rainfall-derived infiltration
and inflow per vertex, so a model whose unit hydrographs and rainfall have
not changed need not recompute them. It is read (`USE`) in either of the
predecessor's two encodings, chosen by the first ten bytes: `SWMM5-RDII`
begins the binary form, and anything else is parsed as the text form. It is
written (`SAVE`) in the text form. The two usages are exclusive, as the
predecessor's single slot makes them: a file is read or written, never both
in one run.

**Binary.** The stamp, then a signed 32-bit step in seconds, a signed 32-bit
vertex count, and that many signed 32-bit values, followed by one record per
period: a date as the predecessor's decimal day, then one 32-bit float of
flow per vertex in declaration order. A non-positive step or count is
refused. Flows are in the units of the model that wrote the file, which the
format does not record.

**Text.** A first line whose first token is `SWMM5`, a title line, the step
in seconds, a constituent count (always 1), a line whose second token names
the flow units, a vertex count, that many lines each naming a vertex, and a
column-heading line. Then one line per vertex per period: the vertex's name,
year, month, day, hour, minute, second, and flow. Unlike the binary form the
text form declares its units, and flows convert from them.

> **DEVIATION from SWMM:** the binary form identifies its vertices by
> *position in the writing model's vertex array*, not by name, so a file is
> readable only against a model whose vertices are ordered exactly as the
> writer's were. The predecessor checks only that the vertex at each stored
> position happens to have RDII defined, which a reordered or edited model
> can satisfy while every hydrograph lands on the wrong vertex. This engine
> applies the same check and adds the one the format allows: a stored
> position outside the model's range, or one naming a vertex without RDII,
> is a refusal naming the file rather than a silent misassignment. The
> ambiguity is inherent to the format; what is removed is answering it
> silently.
>
> *Source: `rdii.c:1156` (writer stores `j`, the array index) against
> `:583–587` (reader indexes `Node[j]` and tests `rdiiInflow` alone).*

> **DEVIATION from SWMM:** the text form carries the vertex's name on every
> data row and the predecessor reads it into a variable its own comment marks
> "not used", matching rows to vertices by position instead. A file whose
> rows are ordered differently from its header is therefore read without
> complaint and every hydrograph is misassigned. This engine matches each row
> by the name the row carries, so such a file reads correctly; a row naming a
> vertex absent from the header is a refusal.
>
> *Source: `rdii.c:698` (`char s[MAXLINE+1]; // node ID label (not used)`)
> and `:705–708` (parsed, then discarded in favour of the loop index).*

**In time.** A record's flows apply from its own instant until the next
record's, and the file is read forward once rather than searched: before the
first record and after the last, the hydrograph is zero. Routing interface
files interpolate between bracketing periods and these do not, which is the
predecessor's behaviour and the right one for this file — an RDII hydrograph
is a volume already apportioned to a step, and interpolating it would move
water between steps that the unit hydrographs put where they did.

**Writing.** Export emits the text form. It is the encoding that survives a
model whose vertices have been reordered, it declares its own units, and it
is the one a modeller can read; the binary form's only advantage is size,
which the format's inability to identify its own vertices does not earn.
Both forms are read.

A record is written at each hydrology step, carrying the flow that step
convolved, and the declared step is the longer of the model's two hydrology
steps. That value bounds every gap between records, since a hydrology step is
one of the two and may only be shortened by a gage boundary, so the written
hydrograph is continuous: no instant of the run falls outside some record's
window. Where the run took the shorter step the windows overlap, which
costs nothing, because a reader takes the last record at or before the
instant it wants.

> **DEVIATION from SWMM:** the predecessor writes on a uniform grid, its
> RDII step being its wet step, because its convolution runs on that grid.
> This engine convolves on the hydrology clock, which alternates between the
> wet and dry steps, and writes what it computed rather than resampling it.
> A file written here therefore reproduces the run that wrote it exactly,
> where a uniform grid would have to interpolate the dry-weather recession
> or carry it at the wet step's density.
>
> *Source: `rdii.c:733` — `RdiiStep = WetStep`.*

#### 14.8.2 Runoff interface files

A runoff interface file replays a hydrology run so its routing can be redone
without recomputing the surface. It is the heavier of the two savings an
interface file offers, and the narrower: it replays *results*, not state.

Reading (`USE`) and writing (`SAVE`) are both served, and a model declares one
or the other, never both: a run either replays a hydrology or records the one
it computes. Supplying a file to replay to a run that is recording one is
refused for the same reason, rather than silently producing a file that merely
copies its input.

**Layout.** The stamp `SWMM5-RUNOFF`, then four signed 32-bit values: the
parcel count, the constituent count, the writing model's flow unit as the
predecessor's own enumeration, and the number of steps. Then one record per
step: a 32-bit float of the step's length in seconds, followed per parcel by
`8 + c` 32-bit floats, where `c` is the constituent count — rainfall
intensity, snow depth, evaporation loss, infiltration loss, runoff flow,
groundwater flow, groundwater elevation, soil moisture, and then one washoff
concentration per constituent, in that order.

**What it drives.** Everything the surface would have produced: runoff flow
and its washoff concentrations reach the network, groundwater flow, elevation
and soil moisture reach the aquifer, and snow depth and the evaporation and
infiltration losses stand in for what the compartment would have computed.
The hydrology is not run at all, which is the point.

**Units.** Values are in the units of the model that wrote the file, which is
why the flow unit is recorded. That word also fixes the unit *system*, since
each of the six belongs to one, so a file written by a US model cannot be
read into an SI one without the mismatch being seen.

**Identity.** The format holds no names. A file is matched to a model by
parcel count, constituent count and flow unit alone, and those three
agreeing is the whole of the check available.

> **DEVIATION from SWMM:** the counts agreeing is a weak test — two models
> with the same number of parcels and constituents accept each other's files
> and every hydrograph lands on the wrong parcel — and the format offers
> nothing better, since it stores no identity at all. The predecessor makes
> that check and says nothing further. This engine makes the same check,
> because there is no other, and additionally reports at start-up that the
> parcels were matched by position: a modeller who has reordered a parcel
> list since writing the file is told what the run assumed rather than left
> to discover it in the results. The check cannot be strengthened; what is
> removed is its silence.
>
> *Source: `runoff.c:379–383` — `nSubcatch`, `nPollut` and `flowUnits`
> compared, and nothing else.*

**Writing.** A record is written at the end of every hydrology step, from the
start of the run rather than from the reporting start, and each carries that
step's own length in seconds. Both follow from what the file is for: a run
replays it step by step in place of the surface, so a file written at the
reporting cadence, or beginning where reporting begins, could not replay the
run that produced it. Under a wet/dry split the step lengths differ from record
to record, which is why the length is stored per record and not once in the
header.

The values written are the same nine per parcel that the results file reports
for that parcel (§14.9), converted from the engine's own units to the writing
model's, which is the exact inverse of the conversion a reader applies. The
step count in the header is the number of records the run actually produced,
so a run that stops early still leaves a file that describes itself correctly.

> **DEVIATION from SWMM:** the predecessor's rainfall column holds whatever the
> gage last reported to the *results* file, not the intensity that drove the
> step being saved. In a three-hour run at a fifteen-minute step this leaves the
> column zero in eleven records of twelve, while the parcel is visibly producing
> runoff from rain throughout. This engine writes the intensity that drove each
> step, so the column means what its name says and a replayed run reports the
> rainfall that produced its flows.
>
> *Source: `subcatch.c:876` reads `Gage[k].reportRainfall`, which `gage.c:573`
> sets only when a reporting instant falls due; `runoff.c:290` saves at every
> runoff step.*

> **DEVIATION from SWMM:** the predecessor does not read its own file back
> faithfully. Its writer records evaporation as a depth per day and its reader
> divides by a depth per hour, so the value is recovered twenty-four times too
> large; its writer multiplies groundwater flow by the parcel's area and its
> reader does not divide by it, so that flow is recovered scaled by an area;
> and its writer stores the water-table elevation while its reader subtracts it
> from the aquifer bottom instead of the bottom from it, inverting the depth.
> This engine writes and reads one convention, the writer's, so a file it
> writes replays as the run that wrote it. The consequence is worth stating
> plainly: the predecessor reading a file this engine wrote will misread those
> three fields, and it misreads its own files the same way.
>
> *Source: writer at `subcatch.c:885` (evaporation), `:905` (groundwater flow,
> area-multiplied) and `:907` (water-table elevation); reader at `runoff.c:452`,
> `:461` and `:462–463`.*

**Balances.** A replayed run reports no surface or subsurface balance
(§11.1). The file carries the flows those compartments produced, not the
rainfall, storage and losses a balance is made of, and a balance built from
untouched accumulators would read as a compartment conserving perfectly
while never having run. The network balance is unaffected, since the
replayed flows reach it as any other lateral would.

**Running past the file.** A file covering less of the clock than the run
asks for is not an error: it is said once, and the surface contributes
nothing from there on. Refusing at that point would discard a run already
mostly done, and continuing in silence would lose the surface part way
through without saying so.

**Antecedent state.** A run reading one begins with cold antecedent state:
the file carries what the surface produced, not the moisture, depression
storage and buildup it produced them from. The engine says so at start-up
rather than presenting the run as a continuation.

#### 14.8.3 Rainfall interface files

A rainfall interface file is a cache of parsed external rain records
(§14.12). Reading archival records is the slow part of starting a run that
uses them, so the predecessor parses once, normalises what it read, and
keeps the result; a later run reads the cache instead of the records.
Reading (`USE`) and writing (`SAVE`) are both served. The scratch mode
builds the same file and discards it, so it is not written and the results
are identical either way.

**Layout.** The stamp `SWMM5-RAIN`, ten bytes with no terminator, then a
signed 32-bit gage count. Then one header per gage, each 1037 bytes: the
recording station's identifier in 1025 bytes, padded with zero bytes, then
three signed 32-bit values — the recording interval in seconds, the first
byte of that gage's readings, and the byte one past their last. The
readings follow, each a 64-bit decimal day (§14.1) and a 32-bit depth.

The offsets are absolute positions in the file, so a gage's readings can be
found without reading any other gage's. They are also signed 32-bit, which
is the format's own limit: it cannot describe a file of two gigabytes or
more, and this engine refuses to write one rather than emit an offset that
has wrapped.

**Identity.** A gage is matched to the file by its recording *station*
identifier, not by the gage's own name — one station's record may feed
gages named differently in different models, which is the point of caching
it. A gage whose station does not appear, or whose byte range is empty, is
refused by name: the file is an input its results depend on, and a gage
silently reading nothing is a dry model.

> This is the opposite of the runoff format (§14.8.2), which stores no
> identity at all and can only count. The difference is the formats', not a
> choice available here.

**Units and meaning.** Every reading is a depth in inches, accumulated over
the gage's recording interval, whatever the gage declared. The predecessor
normalises on the way in: a record read as an intensity is multiplied by
the interval, a cumulative record is differenced against the running total,
and a record in millimetres is divided by 25.4. A gage's declared form
therefore describes its *record*, not this file, and a run reading one
reads interval depths whichever form it declares. This engine writes and
reads the same normalisation, so a model's own unit system converts on the
way out of the file and never appears in it.

> *Source: `rain.c:1028–1043` normalises by `RainType` and then by
> `UnitsFactor`; `gage.c:293` converts back for metric models, its comment
> stating that depths on the interface file are in inches; `rain.c:407`
> reads every gage as interval depths regardless of its declared form.*

**Zero readings.** A reading of zero is written like any other and skipped
when read: a gage advances to its next non-zero depth, so the periods
between are dry rather than absent. Writing them costs twelve bytes each
and keeps a record's own spacing visible in the file it was cached from.

> **CORRESPONDENCE:** the predecessor's own description of this format says
> it stores each period with non-zero rain. Its writer stores every reading
> it parsed, zero or not, and its reader skips the zeros while advancing.
> The behaviour is consistent; only the description is wrong. This engine
> follows the behaviour, since that is what the files hold.
>
> *Source: `rain.c:27–38` (the description) against `rain.c:1044–1046`
> (the writer, which excludes only missing readings) and `gage.c:668–693`
> (the reader, which loops while the converted value is zero).*

> **DEVIATION from SWMM:** the predecessor writes the station field from an
> uninitialised buffer, so the bytes after the identifier's terminator are
> whatever was on its stack — two files it writes from the same record
> differ, and neither is reproducible. This engine writes zeros there. Both
> readers stop at the terminator, so nothing reads differently; what
> changes is that a file this engine writes can be compared with another.
>
> *Source: `rain.c:216` declares `char staID[MAXMSG+1]` as a local,
> `:255–257` copies the identifier into it and writes all 1025 bytes.*

**Precision.** Depths are stored at single precision, so a run reading a
cache is not bit-identical to the run that wrote it: a depth of 0.4 inches
returns as 0.40000000596, and the runoff it drives differs in its last bit.
The cache is a cache of a record, not of a result, and this is the record's
own resolution being rounded rather than a result being approximated.

**Which gages appear.** Only those sourcing an external record. A gage
reading an inline series has nothing to cache and is absent, so a model
whose gages all read series writes a file declaring no gages. Reading such
a file back is not an error in itself; a gage looking for a station in it
is the thing that fails.

**Writing.** The predecessor writes the file in two passes, laying down
placeholder headers and returning to overwrite each with the byte range its
gage's readings turned out to occupy. This engine knows every reading
before it writes anything, so it computes the ranges first and writes the
file once. The bytes are the same.

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

**A reported precipitation is a lookup, not a leftover.** The subcatchment
record's precipitation is the gage's rate for the recording interval that
contains the reporting instant, carrying the monthly adjustment in force
there. It is not the rate the last completed hydrology step ran on: the
hydrology and reporting clocks are independent (§10.1), so a step's rate
belongs to a window that need not contain the instant being stamped, and on
a run where the two happen to align it lands one interval early. Every other
field of the record is live state read at the instant. The runoff interface
file of §14.8.2 keeps the step's own rate instead, because its rows are
steps rather than reporting instants.

> **CORRESPONDENCE:** the predecessor keeps two rainfall values per gage for
> exactly this reason: the one its runoff step consumes, and a separate
> reported one re-evaluated at every reporting instant.
>
> *Source: `gage.c:535` — `gage_setReportRainfall`, called from
> `output.c:593` before each period's subcatchment records are written.*

> **CORRESPONDENCE:** the surface *loss* rates are stamped the other way
> round, and this engine does not follow. The predecessor's runoff clock
> runs ahead of its routing clock, so when a period is written the
> hydrology step *opening* at that instant has already been taken, and the
> infiltration and evaporation rates written are that step's. Here the
> surface stands exactly at the instant, and the rates written are those of
> the step that closed there. On a run whose hydrology and reporting clocks
> align, the two series are the same values one period apart; the §11.1
> volumes are identical either way, because nothing about the accounting
> depends on which period carries a rate. Matching the predecessor would
> mean stamping an instant with a step not yet taken, which is a property
> of its loop order rather than a definition of the format.
>
> *Source: `swmm5.c:589` — `while (NewRunoffTime < nextRoutingTime)
> runoff_execute();` before `routing_execute`; `subcatch.c:885` takes
> `infilLoss` and `evapLoss` unweighted while runoff is interpolated.*

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
Instants print as the predecessor's elapsed `days hr:min`, measured from
the **report start** and never negative; control actions print as absolute
dates.

> **CORRESPONDENCE:** the origin is the report start in both engines, and
> the choice is easy to get wrong. Per-object statistics begin at the report
> start (§11.2), so measuring their instants from the simulation start
> would print an origin the numbers themselves never see. A model that
> reports four hours into a two-day run has every instant column shifted by
> those four hours between the two conventions, with nothing on the page to
> say which is meant.
>
> *Source: `swmm5.c:1567` — `x = aDate - ReportStart`, and
> `project.c:151` — `ReportStart = MAX(ReportStart, StartDateTime)`.*

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
(foundation contract §2.5) — "are these bytes yours?" — so an application
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

#### 14.12.1 Archival station records

The predecessor also reads the layouts national weather services publish.
All of them are served here: the US National Weather Service's fixed-field
tape layout, its space- and comma-delimited DSI exports, its online
retrieval exports, and the Environment-Canada hourly and quarter-hourly
layouts.

An archival record parses into exactly what a rainfall interface file
holds (§14.8.3): depths in inches over a recording interval the file
itself declares. That is not a convenience — it is what the archives are,
and it is why the interface file has that shape at all. A caller therefore
reads an archive and a cache of one through the same path.

**A gage declares nothing.** Which layout a file is written in is
recognised from the file, not from the model: a modeller who swaps a
station export for an archive of the same weather changes the file and
nothing else. A file that is neither is refused naming both reasons, since
either could be the one the reader meant.

**Which layout, and at what interval.** The layout is recognised from the
first five lines, and the recording interval from the element code the
line carries: `HPCP` is hourly, `QPCP` and `QGAG` are quarter-hourly. A
file whose element code is none of those is not one of these layouts, and
is refused rather than read at a guessed interval.

**The stamp is the end of its interval.** A reading marked 01:00 in an
hourly record is the hour that *ended* at 01:00, so every reading is
shifted back one interval to the instant it began. Reading them as
starting instants shifts a whole record forward by an hour, which no
single value reveals.

**Values are hundredths of an inch.** A reading of 9999 or more is
missing, as is one flagged `M`, and so is every reading inside a deleted
or missing period. A missing reading is not a dry one: it is absent from
the record this parse produces, and the gage's own treatment of a gap
applies.

**Accumulation periods.** A flag `a` opens a period whose readings were
not measured separately, and a later `A` closes it carrying the total that
fell across the whole of it. That total is divided evenly among the
intervals from the opening instant to the closing one inclusive, and each
receives its share. An accumulation whose total is missing contributes
nothing but is still counted as that many missing periods.

> **DEVIATION from SWMM:** the predecessor divides an accumulated total
> evenly and says nothing about having done so. A modeller reading the
> results sees a uniform hour of rain where the record only ever claimed a
> total. This engine divides it the same way, because any other division
> would be invented, and reports each accumulation period it spread, with
> its span and total, so the uniformity is known to be an artefact of the
> record rather than a measurement.
>
> *Source: `rain.c:saveAccumRainfall`, which spreads `v/n` and writes each
> interval.*

**The Environment-Canada layouts.** One line per station per day, its
readings in fixed seven-character groups: twenty-four of them for an
hourly record, ninety-six for a quarter-hourly one. Values are tenths of a
millimetre, and −99999 is missing. The stamp is the end of its interval
here too, so the first group of a day is the interval that ended at that
day's midnight and belongs to the day before.

A line declares which quantity it carries, and a line that is not rainfall
is skipped rather than read as though it were: 123 is rainfall in the
hourly layouts, 159 in the quarter-hourly one.

> **DEVIATION from SWMM:** the hourly Environment-Canada layout writes its
> year in three digits as the year less 1900, so 120 is 2020. The
> predecessor adds 1000 to any such value and 2000 to a value below 100,
> putting a 2020 record in the year 1120 and a 1995 one in 2095. Neither
> is a year a weather record refers to, and the effect is silent: the
> readings land nine centuries from the simulation and the gage receives
> nothing at all, with no message. This engine reads the field as the
> convention defines it.
>
> *Source: `rain.c:937–938`, and a record whose year field is 120 lands in
> 1120 when the reference implementation reads it.*

**The online retrieval exports.** One reading per line, and neither the
date nor the value is at a column the layout fixes. The quantity's own
name in the header line marks the column its values are written in, and
the date's column is eleven characters before the last colon of the first
line that names a station, which is the colon inside that line's clock.
Both are read from the file rather than assumed, because a file that
carries an extra column moves them.

Midnight belongs to the day before: a reading marked 00:00 is the interval
that ended at that midnight. Values are decimal inches where the field
carries a decimal point and hundredths where it does not, which is how a
newer export and an older one are told apart.

**Condition codes.** `{` and `}` mark a deleted period, `[` and `]` a
missing one, and a reading carrying any of the four is absent. They mark
their own readings and no others: an unflagged reading between an opening
bracket and its closing one is an ordinary measurement.

> **CORRESPONDENCE:** the brackets read as though they opened and closed a
> span, and the predecessor's own variable is named for a period. It
> recomputes that variable from each reading's flag, so the span never
> outlives the reading that opened it, and an unflagged reading inside one
> is kept. This engine keeps it too: these records are written by the
> agency that publishes them, and what matters is reading the file the way
> its writer's tool does.
>
> *Source: `rain.c:828–833`, where `setCondition` runs on every reading
> and an unflagged one sets the condition back to none. A record bracketed
> at 02:00 and 04:00 keeps its 03:00 reading when the reference
> implementation reads it.*

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

   *Identical* is exact wherever a quantity's conversion is a
   multiplication, which is almost everywhere. It is exact **to
   floating-point rounding** where export must invert a relation that
   import compiled, because such an inversion passes through a root:
   §14.13.5's storage shapes are the case that exists today. The
   distinction is worth drawing rather than blurring into a general
   tolerance — a value that comes back differing in its last digit is
   reporting the arithmetic it travelled through, and a value that comes
   back differing in its third is reporting a defect.
2. **Writer idempotence.** The second export is byte-identical to the
   first. A writer that is semantically correct but not idempotent hides a
   value that survives one cycle and drifts on the next.
3. **Mutation quiescence.** Re-importing an exported file raises none of
   §14.7's mutation warnings. The mutations were applied before the file
   was written, so a second import finds nothing left to rewrite; a warning
   on re-import means export wrote something import did not mean.

These are properties of the writer, so they are established against
models rather than argued: a fixture exercising one feature at a time
proves each in isolation, and a body of real models proves that nothing
was assumed about a column whose meaning differs from its neighbours'.
Both are needed, and neither substitutes for the other — a fixture
contains only what its author thought to include, while a real model
exercises a hundred sections and localises nothing.

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

**Objects are therefore grouped by kind, and a file that interleaved them
does not come back interleaved.** Each kind has one section, so a model
whose author wrote weirs above conduits returns with the same links
registered in a different order. Every reference between objects resolves
by identifier, so nothing in the model changes meaning — but registration
order is the order §14.9's results file lists elements in, so a
save-and-reload can renumber the records a consumer reads positionally.
That is a property of grouping the sections, which the format requires;
it is stated here because it is invisible in the model and visible in the
results.

A list spanning kinds — cross-section assignments are the case today —
follows that same grouping rather than registration order. Following
registration order would make the first export and the second disagree,
since the first is what reordered the objects: the model would be right
both times and neither file would be canonical.

**Analytical relations are written as some member of the family that
reproduces them, not as the parameters their author wrote.** Import
compiles the analytical storage shapes to $A = a_0 + a_1y + a_2y^2$,
keeping the shape's name and discarding its axes. The engine solves
$A(y)$ and re-import recompiles whatever is written, so export solves for
parameters reproducing the stored coefficients. Where that system is
underdetermined — a cone's coefficients satisfy $a_1^2 = 4a_0a_2$
identically, leaving two independent equations in three unknowns — export
takes the symmetric member, the circular cone. A pyramid's parameters are
uniquely recoverable and come back as written. Every member of such a
family is the same storage unit as far as the engine is concerned, which
is what makes the choice free rather than lossy.

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

### 14.14 External Climate Records

A model may source daily temperature, evaporation and wind from a climate
file rather than from the input (§3.1 of the hydrology specification). The
engine performs no file I/O — the caller reads the file and supplies its
text at load (§12.1); this section defines what that text means.

The served format is the predecessor's **user-prepared** climate format:
one record per line,

```text
station  year  month  day  tmax  tmin  (evap)  (wind)
```

whitespace-separated, with `*` in any value position meaning the day has
no reading for that quantity and the trailing two optional. Blank lines
and lines opening `;` are ignored. A day absent from the file, or absent a
quantity, inherits the most recent recorded value rather than a default,
so a file may list only the days on which something changed.

**Column units.** Temperatures are in the model's own temperature unit,
°F for a model in US units and °C for one in SI. Evaporation is the
model's depth unit per day, inches or millimetres. Wind is **miles per
hour in both unit systems**, which is not the model's speed unit and not
what the same quantity means when declared as monthly averages.

> **CORRESPONDENCE:** the predecessor converts a monthly wind declaration
> out of the user's unit system and reads the climate file's wind column
> without converting it, so the two ways of declaring the same quantity
> disagree about units for a model in SI: `120` means 120 km/h as a
> monthly average and 120 mph read from a file. This engine reproduces the
> file semantics, because the column's meaning is part of the format and a
> file must read here as it reads there.
>
> *Source: `climate.c:setTemp`, `Wind.ws = Wind.aws[mon-1] /
> UCF(WINDSPEED);` against `Wind.ws = FileValue[WIND];`.*

#### 14.14.1 Archival climate records

The predecessor also reads the layouts national climate services publish,
alongside the user-prepared climate file of §14.12. Three are served here:
the US National Climatic Data Center's TD-3200 fixed-field layout, the
Environment-Canada DLY02 and DLY04 daily layouts, and the NCDC Global
Historical Climatology Network Daily exports.

A climate record carries four daily quantities, any subset of which a file
may hold: maximum air temperature, minimum air temperature, pan
evaporation, and wind speed. Everything a climate file supplies is daily,
and the engine's own sub-daily temperature curve (§3.1) is built from the
day's two extremes rather than read.

**A declaration names a file, not a format.** As with rain (§14.12.1),
which layout a file is written in is recognised from the file itself. A
file matching none of them is refused naming every layout it was tested
against, since any of them could be the one the modeller meant.

**Which layout.** The first line decides, and the tests are applied in a
fixed order because they are not mutually exclusive by construction:

1. **TD-3200** if the line begins `DLY` and carries `9999` at columns
   23–27.
2. **DLY02/DLY04** if the line is at least 233 characters and the element
   code at columns 13–16 is 1, 2 or 151.
3. **User-prepared** (§14.12) if the line reads as a station name followed
   by a year, a month, a day and at least one value.
4. **GHCN-Daily** if the line is a header naming a `DATE` field and at
   least one of `TMIN`, `TMAX`, `EVAP`, `WDMV` or `AWND`.

**A file is read a month at a time.** The reader positions itself at the
first line whose year and month are the ones wanted, then consumes lines
until it meets one belonging to a later month. That line is kept rather
than discarded: it is the first line of the next month's read. A day
absent from the file holds the last value read for that quantity rather
than reverting to a default, which is what makes a file of monthly
extremes usable at all.

> **DEVIATION from SWMM:** the predecessor decides a line belongs to a
> later month with `year > current || month > current`, which is not a
> date comparison. A line dated an earlier year but a later month — which
> is every January-to-December wrap in a file whose months run backwards,
> and any out-of-order line — ends the month's read early, and the days
> after it silently hold the previous month's values. This engine compares
> the year and month as one date.
>
> *Source: `climate.c:readFileValues`, `if ( y > FileYear || m >
> FileMonth ) return;`.*

**The TD-3200 layout.** One line per station per quantity per month. The
year is at columns 17–21 and the month at 21–23. The quantity is a
four-character element name at columns 11–15: `TMAX`, `TMIN`, `EVAP` or
`WDMV`. A count of days present is at columns 27–30, and that many
twelve-character groups follow from column 30. Within a group the day of
the month is at offset 0, a sign at offset 4, a five-character value at
offset 5, and a flag at offset 11. A value of `99999` is missing, as is
any reading whose flag is neither `0` nor `1`. Evaporation is hundredths
of an inch; wind is miles per day; temperatures are degrees Fahrenheit.

**The Environment-Canada daily layouts.** One line per station per
quantity per month, at least 233 characters. The year is at columns 7–11
and the month at 11–13. The element code at columns 13–16 is 1 for
maximum temperature, 2 for minimum, and 151 for evaporation. Thirty-one
seven-character groups follow from column 16, one per day of the month
whether or not the month has that many: a sign at offset 0, a
five-character value at offset 1, and a condition code at offset 6. A
value of `99999` or of five blanks is missing. Temperatures are tenths of
a degree Celsius; evaporation is tenths of a millimetre.

> **CORRESPONDENCE:** the predecessor reads each day's condition code and
> discards it, so an estimated or accumulated reading is used as though it
> were measured. This engine reads these layouts the same way. The codes
> distinguish estimates from measurements rather than marking absence, the
> archive documents no code as meaning "do not use", and refusing them
> would drop readings the predecessor keeps and the record intends.
>
> *Source: `climate.c:parseDLY0204FileLine`, which fills `code` and never
> reads it, against `setTD3200FileValues`, which tests its flag.*

**The GHCN-Daily exports.** The first line is a header naming its columns.
Each quantity's data is read from the column at which its name begins in
that header, and the date from the column at which `DATE` begins, as
`YYYYMMDD`. Wind is `WDMV` (daily movement) if the header names it and
`AWND` (average speed) otherwise. A value of 9999 or more in magnitude,
in either direction, is missing.

This is the fixed-column form. The delimited exports the same service
publishes are a different format that this layout's column arithmetic
cannot read, and a file recognised as GHCN-Daily whose fields do not sit
at their header's columns is refused rather than read at the wrong
offsets.

**Declared units.** A file declaration may carry a units word, `C10`, `C`
or `F`, defaulting to `F` for a model in US units and `C` for one in SI.
It governs the GHCN-Daily exports alone, where it selects the unit family
for all three of temperature, evaporation and wind together: `C10` reads
tenths of a degree Celsius, tenths of a millimetre, and either kilometres
per day or tenths of a metre per second; `C` reads degrees Celsius,
millimetres, and kilometres per day or metres per second; `F` reads
degrees Fahrenheit, inches, and miles per day or miles per hour. TD-3200
and the Canadian layouts carry their own units, stated above, and a units
word declared beside one of them is reported as having no effect rather
than silently ignored.

> **DEVIATION from SWMM:** a climate file that is *recognised* but whose
> values do not sit where its layout puts them yields no readings at all,
> and the predecessor reports nothing. Every day is missing, so every day
> holds what the run began with: a flat 70 °F, no evaporation and no wind,
> for the whole simulation. A misaligned file is therefore indistinguishable
> from a working one by looking at the results, which is the same class of
> defect as a rain file read as dry. This engine refuses a climate file
> that is recognised as a layout and yields no readings at all, naming the
> layout it was read as.
>
> *Source: `climate.c:climate_openFile` seeding `FileValue[TMIN]` and
> `[TMAX]` from `Temp.ta`, `project.c:915` setting that to 70.0, and
> `updateFileValues`, `if ( FileData[i][FileDay] == MISSING ) continue;`.*
