# Diagnostics & Errors (Drainage)

Import, validation and export diagnostics are Hydra's own — typed and exhaustive.

SWMM's numeric error catalogue is deliberately not reproduced. It is a property of that API, not of its files: nothing in a model file names an error code, so it is not an interoperability surface.

## What a diagnostic carries

Every substitution, mutation and interpretation decision made at import surfaces as a named, per-element notice. Nothing is rewritten silently. That includes:

- A section header the reader did not recognise, with the count of lines discarded after it.
- A line whose tokens ran past the fortieth.
- A later property line replacing an earlier one for the same object and slot.
- A relation whose coefficients were converted out of the file's unit system.
- A routing form or approximation switch mapped onto the model this engine solves.

## Repair by omission

A refusal is additionally marked **repairable by omission** when commenting out its line leaves a model SWMM accepts with identical meaning.

Exactly one refusal qualifies today: an unknown `[OPTIONS]` keyword. Every option has a default and SWMM refuses the keyword too, so omission is the only reading the two implementations share. This is what makes vendor dialects that write extra option keywords importable without admitting anything SWMM would run differently.

The marking is advisory. A consumer may comment the named line and re-read, and must surface the repair rather than apply it silently.

No other refusal qualifies: values, identifiers and structure all carry meaning that omission would change.

## Recognition

`.inp` belongs to both EPANET and SWMM, so the extension cannot decide which engine owns a file. Hydra decides from the contents — each engine judges the models it is shown, and names the other's exclusive sections as foreign markers, so any `.inp` both engines see gets complementary verdicts and routing never has to break a tie.

A file built only from sections both formats share is genuinely ambiguous, and `plausible` is the honest answer. Naming the engine explicitly — `--engine uds` on the CLI, or the engine choice in the desktop app's new-project wizard — supplies the evidence recognition lacked.

Recognition is stricter than parsing, deliberately. Parsing discards unrecognised sections rather than rejecting the file; recognition governs only *automatic routing*, where a wrong guess silently produces a confident wrong answer.
