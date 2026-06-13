# Copilot Instructions

<!-- This file is read by GitHub Copilot in the web UI (PR generation, editor suggestions).
     For agentic coding workflows, see AGENTS.md at the repo root. -->

## PR Titles

PR titles must follow the Conventional Commits format:

```
<type>(<optional scope>): <description>
```

Valid types: `feat`, `fix`, `chore`, `docs`, `style`, `refactor`, `test`, `ci`, `perf`, `build`, `revert`

Examples:
- `feat(engine): add FAVAD leakage model`
- `fix(solver): correct head-loss exponent for Chezy-Manning`
- `docs: update INP format support table`
- `chore: bump version to 1.2.0`
- `refactor(io): split inp_reader into section parsers`
- `test(hydraulics): add regression fixture for KY10`

## Commit Messages

Commits follow the same Conventional Commits format as PR titles:

```
<type>(<optional scope>): <description>
```

Use the imperative mood in the description ("add", "fix", "remove", not "added", "fixes", "removed"). Keep the subject line under 72 characters. Add a body if the change needs more context.

## PR Descriptions

**Summary**: what changed and why, in plain prose.

**Spec**: which spec file and section this implements or updates (e.g. `crates/engine/src/hydraulics/spec.md §3.2`). Write "N/A" for non-engine changes.

**Testing**: how the change was verified: new fixtures, unit tests, manual checks, or regression runs.
