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

## PR Descriptions

**Summary**: what changed and why, in plain prose.

**Spec**: which spec file and section this implements or updates (e.g. `crates/engine-wds/src/hydraulics/spec.md §3.2`). Write "N/A" for non-engine changes.

**Testing**: how the change was verified: new fixtures, unit tests, manual checks, or regression runs.

**Version Bump**: analyze the diff and recommend whether a semver bump is warranted and at what severity. Use the following rules:

- **none** — no public API or observable behaviour change (e.g. docs, tests, internal refactors, comments, CI changes)
- **patch** — bug fix or internal change that corrects incorrect behaviour without adding new capabilities
- **minor** — new feature or capability added in a backward-compatible way
- **major** — breaking change to a public API or behaviour that existing callers depend on

Determine which version track(s) are affected:
- **Library** (`just bump patch|minor|major`) — changes to `hydra-engine-wds` or `hydra-sdk`
- **CLI** (`just bump-cli patch|minor|major`) — changes to `hydra-cli` or its user-facing behaviour
- **GUI** (`just bump-gui patch|minor|major`) — changes to `hydra-gui` or the frontend

If multiple tracks are affected, list each one. If no bump is needed, write "none".

Example outputs:
- `just bump-cli patch` — fixed a crash in the CLI argument parser
- `just bump minor` + `just bump-cli minor` — added a new solver option exposed through both the SDK and CLI
- none — updated a comment in the engine crate
