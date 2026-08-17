# INP Format Support (Drainage)

Hydra's urban drainage engine reads SWMM `.inp` files directly. There is no conversion step and no Hydra-specific dialect: the file you have is the file it reads.

## Reading the file

The lexical layer reproduces SWMM's exactly, so a file that parses there parses here:

- A line is at most 1024 characters, re-measured up to the first `;`, so an overflow that lies entirely inside a comment is legal.
- Everything from `;` to the end of the line is cut before tokenising.
- At most 40 tokens are read per line. Tokens beyond the fortieth are ignored, as SWMM ignores them, but Hydra warns where SWMM says nothing.
- A token opening with a double quote runs to the next quote or newline. That is the only way to carry a separator inside a value.
- `[TITLE]` keeps its first three lines and ignores the rest.

Section headers and identifiers are matched case-insensitively (`Node1` and `NODE1` are one identifier), and objects keep their as-written spelling for output.

The file is read in two passes: the first registers identifiers, the second parses data. **Forward references are legal**, because every identifier exists before any parameter is read. A duplicate identifier within a type is rejected, and a case-only re-declaration counts as a duplicate.

## Unrecognised sections

An unrecognised section header leaves the reader sectionless: the header is reported and every following line is discarded until the next recognised header. This is SWMM's own accept-set behaviour, with one addition: the report carries the count of lines discarded, which SWMM never states.

## Display sections are preserved

Nine sections carry no engine semantics: `[MAP]`, `[COORDINATES]`, `[VERTICES]`, `[POLYGONS]`, `[SYMBOLS]`, `[LABELS]`, `[BACKDROP]`, `[TAGS]` and `[PROFILES]`. They are parsed for well-formedness only and preserved verbatim, so a load-and-save cycle keeps them intact and applications can consume them.

## Later lines replace earlier ones

Within the per-object property sections (external inflows, sanitary inflows, sewer inflows, treatment, land cover and initial loadings), a later line for the same object and slot **replaces** the earlier one, as it does throughout the SWMM ecosystem. Each override is reported. Accumulating them would quietly change what a legal file means.

## Numbers must be finite

A numeric token must parse to a finite value. The spellings `nan` and `inf`, in any casing or sign, and magnitudes that overflow the double range, are refused as bad values.

> **Deviation from SWMM.** SWMM's `strtod`-based reader can accept them. A non-finite parameter poisons every downstream computation while the continuity statistics (NaN-blind by construction) still report zero error, which is a confident wrong answer.

## Unit-dependent relations

Seven relations are defined by SWMM in the *user's* unit system, so their file coefficients change meaning with the flow-units selection. Hydra converts each at import, and the list is longer than SWMM's own manual acknowledges:

| Relation | Treatment at import |
|---|---|
| Control-measure underdrain equation | Coefficient converted to SI-dimensional form; the optional multiplier curve stays raw |
| Groundwater lateral-flow power function | Coefficients converted per their exponents |
| Weir discharge coefficients, including coefficient curves | Converted to the SI dimension of their relation |
| Outlet power-function coefficient and rating curves | Converted per the exponent; ratings pointwise |
| Storage area/volume/depth relations | Functional coefficients converted per their exponents; tabular curves pointwise |
| Divider rules | Reduced-form semantics; not evaluated by this engine |
| User-written expressions (groundwater flow, deep percolation, treatment) | **Evaluated in the file's unit system** |

The last row is the interesting one: a formula cannot be dimensionally converted, so the boundary moves to its edges. Inputs are presented to the expression in the units its author wrote it for, and the result is converted.

## Auxiliary files

A model that names external files is read with them:

- **External rain records**: a gage naming `FILE "rain.dat" STA1 MM` is read directly, one read per distinct file, resolved relative to the model.
- **Climate files**, **hotstart state** and **routing interface files** are likewise resolved beside the model.

Paths split on either separator, so a model authored on Windows opens on macOS and Linux. A missing or malformed auxiliary file is a named error rather than a silent dry run.

## What import will not do quietly

Where SWMM's file semantics presuppose behaviours this engine does not have (reduced routing forms, approximation switches), import maps them onto the model Hydra does solve and **says so**. Every substitution, mutation and interpretation decision surfaces as a named, per-element notice. There are no silent rewrites.

See [Diagnostics & Errors](diagnostics.md) for how those notices are reported.
