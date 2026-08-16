# Migrating from SWMM

Your `.inp` file is the input. There is nothing to convert and nothing to annotate.

```sh
hydra run model.inp
```

The model's own sections identify the engine, so no flag is needed unless the file is genuinely ambiguous — see [Diagnostics](diagnostics.md#recognition).

## What Hydra owes SWMM, and what it does not

Compatibility here is three obligations of decreasing strength, kept apart deliberately. Conflating them is what leads an engine to inherit defects in the name of fidelity.

**Interoperability is binding.** A model in SWMM's input format is read and understood to mean what its author meant, and results are written in formats SWMM's readers accept. This is not negotiable — it is the entire reason the formats are supported. It binds syntax and interpretation: that a field is accepted, and that its value denotes the quantity its author intended.

**Result correspondence is bounded.** Results are at least as accurate as SWMM's, judged against measurement or analytical solution rather than against SWMM's output. Where a result differs in a way you would notice, the difference is attributable to a stated improvement and recorded at the point it arises. Agreement with SWMM is evidence, not the objective.

**Reproducing SWMM's arithmetic is not an obligation at all.** There is no predecessor-faithful mode, and there will not be one.

Every claim Hydra's specifications make about SWMM is made against SWMM 5.2.4, at a named commit, and cites the file and line it rests on — so a reader can check the claim rather than take it.

## What to expect on your first run

- **Numbers will not match SWMM digit for digit.** Where they differ materially, the reason is documented in the specification section the quantity belongs to.
- **The report will look familiar.** The `.rpt` follows SWMM's layout section for section, so it diffs side by side against a SWMM run.
- **The `.out` file is readable by your existing tools.** It is written to SWMM 5.2.4's binary layout.
- **You will see notices you did not see before.** Where SWMM silently reinterprets something, Hydra names it: a discarded section, a property line replacing an earlier one, a coefficient converted out of the file's unit system, a routing form mapped onto the one this engine solves.

## Things Hydra refuses that SWMM accepts

- **Non-finite numbers.** `nan`, `inf` and magnitudes that overflow a double are refused. SWMM's reader can accept them, and a non-finite parameter poisons every downstream computation while the continuity statistics still report zero error.
- **Unknown `[OPTIONS]` keywords**, which SWMM also refuses. This one is marked *repairable by omission*: commenting the line out leaves a model both engines read identically, which is what makes vendor dialects importable.

## Where to go next

- [INP Format Support](inp-format.md) — how the file is read, and what is preserved
- [Output Files](output-files.md) — the `.out` and `.rpt` layouts
- [Diagnostics & Errors](diagnostics.md) — what the notices mean
