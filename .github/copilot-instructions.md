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

**Release**: say whether the change releases, and which tracks. Versions are bumped by release-please, not by hand (see RELEASING.md). It reads the PR title:

- **fix** / **perf** — a patch release of each track the PR touches
- **feat** — a minor release
- **`!`** after the type, or a `BREAKING CHANGE:` footer — a major release
- any other type (`chore`, `docs`, `refactor`, `test`, `ci`, `build`, `style`) — no release

The track is chosen by the files changed, not the scope:
- **Library** — any workspace-versioned crate (`hydra-common`, `hydra-engine-wds`, `hydra-engine-uds`, `hydra-interop-swmm`, `hydra-interop-epanet`, `hydra-engines`, `hydra-report`, `hydra-sdk`)
- **CLI** — `crates/cli`
- **GUI** — `crates/gui`, including the frontend

So pick the type that describes the change for its users, and keep a breaking change to the track that actually breaks.
