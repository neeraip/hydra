# Contributing to Hydra

Thank you for your interest in contributing. This document explains how to set up a development environment and how changes flow through the project.

## Contributor License Agreement

**All contributors must sign a Contributor License Agreement (CLA) before any pull request can be merged.**

Hydra is dual-licensed: publicly under AGPL v3, and commercially under a separate proprietary license. The CLA grants NEER AI the right to include your contribution under both licenses. Without it, we cannot accept your code.

A CLA process will be linked here before the first external PR is merged. If you want to contribute before that process is live, reach out at [matthew@neer.ai](mailto:matthew@neer.ai) first.

## Table of Contents

- [Development Setup](#development-setup)
- [Workflow: Spec First](#workflow-spec-first)
- [Commit Messages](#commit-messages)
- [Pull Requests](#pull-requests)
- [Running Checks Locally](#running-checks-locally)

---

## Development Setup

### Prerequisites

| Tool | Purpose | Install |
|---|---|---|
| Rust ≥ 1.95 | Compiler | [rustup.rs](https://rustup.rs) |
| `just` | Task runner | `cargo install just` or `brew install just` |
| Node.js 22 | GUI frontend | [nodejs.org](https://nodejs.org) |
| pnpm 10 | Frontend package manager | `npm install -g pnpm` |
| Tauri CLI | GUI builds | `cargo install tauri-cli` |
| Tauri system deps | GUI builds on Linux | [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/) |

Node.js, pnpm, and Tauri are only required if you are working on the GUI (`crates/gui`).

### Clone and build

```sh
git clone https://github.com/neeraip/hydra.git
cd hydra
just build
just test
```

---

## Workflow: Spec First

Hydra's solver is specification-driven. **The spec is the source of truth — not the code.** All changes to solver algorithms, the data model, or post-simulation analytics must update the relevant spec before touching implementation code.

### Change flows

```
Analysis → Spec → Implementation
```

| Type of change | Required steps |
|---|---|
| Solver algorithm (hydraulics, quality, simulation) | 1. Update the relevant `crates/engine-wds/src/*/spec.md`. 2. Write/change implementation. |
| Data model or parser | 1. Update `crates/engine-wds/src/model/spec.md`. 2. Write/change implementation. |
| Post-simulation analytics | 1. Update `crates/engine-wds/src/analysis/spec.md`. 2. Write/change implementation. |
| CLI or GUI | Make changes directly — no spec document needed. |
| Public API (`hydra` facade) | Update re-exports and `README.md` examples. No spec document needed. |

### Spec files

| File | Covers |
|---|---|
| `crates/engine-wds/src/model/spec.md` | Data model, unit system, INP/OUT/RPT formats |
| `crates/engine-wds/src/hydraulics/spec.md` | GGA Newton-Raphson solver, valve models, demand models |
| `crates/engine-wds/src/quality/spec.md` | Lagrangian transport, mixing, reactions, source tracing |
| `crates/engine-wds/src/simulation/spec.md` | Session API, controls, timestep, accounting |
| `crates/engine-wds/src/analysis/spec.md` | Post-simulation analytics |

### Implementation rules

- Implement exactly the algorithm in the spec — same equations, same convergence criteria, same defaults.
- Mark gaps with `// TODO: spec section missing for <subsystem>`.
- Mark intentional deviations with `// SPEC-DEVIATION: <reason>`.
- All hydraulic and quality quantities use `f64`. Never narrow to `f32` for intermediate values.
- Only parallelise operations explicitly marked **∥** in the owning spec.
- No `unwrap()` or `expect()` outside test code.

---

## Commit Messages

Hydra uses [Conventional Commits](https://www.conventionalcommits.org). PR titles are enforced by CI.

```
<type>: <short description>

[optional body]
```

| Type | When to use |
|---|---|
| `feat` | New capability visible to users |
| `fix` | Bug fix |
| `chore` | Maintenance, dependency bumps |
| `docs` | Documentation or spec changes only |
| `refactor` | Code restructuring with no behaviour change |
| `test` | Adding or improving tests |
| `ci` | Workflow and tooling changes |
| `perf` | Performance improvement |
| `revert` | Reverting a previous commit |

Breaking changes: append `!` after the type — e.g. `feat!: rename Simulation::run`.

---

## Pull Requests

1. **Open an issue first** for non-trivial changes so the approach can be discussed before you invest time implementing it.
2. **One logical change per PR.** Spec + implementation for a single subsystem is fine; mixing unrelated subsystems is not.
3. **Reference the spec section** in the PR description — e.g. *"implements §4.3 of the hydraulics spec"*.
4. All CI checks must pass before a PR is merged.

---

## Running Checks Locally

```sh
just fmt      # format everything (Rust + frontend)
just lint     # every static check: rustfmt, clippy, tsc, Biome — no tests
just verify   # lint + the full Rust and frontend test suites
just ci       # everything CI runs (adds deny, docs, catalog drift, scripts)
```

Run `just` with no arguments to see all available recipes.
