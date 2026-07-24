# ── Quickstart ─────────────────────────────────────────────────────────────────

# List available recipes (default)
default:
    @just --list

# One-shot quickstart: install everything, then launch the GUI in dev mode.
start: setup dev

# ── Setup ─────────────────────────────────────────────────────────────────────

# Linux only: Tauri also needs system packages, installed separately —
# see https://tauri.app/start/prerequisites/
# Safe to re-run — every step skips work already done.
# Install everything needed for local development (Cargo, frontend, CLI tools)
setup: setup-tools setup-rust setup-frontend
    @echo "Setup complete. Try 'just build' or 'just dev' next."

# Fetch Cargo dependencies for the whole workspace.
setup-rust:
    cargo fetch

# Install frontend (pnpm) dependencies.
setup-frontend:
    cd crates/gui/frontend && pnpm install

# Uses cargo-binstall for prebuilt binaries; skips tools already present, so
# re-runs (e.g. via `just start`) are fast and offline-friendly. Installs:
# tauri-cli, cargo-deny, cargo-audit, mdbook, cargo-llvm-cov.
# Install the cargo subcommand tools this justfile relies on
setup-tools:
    @command -v cargo-binstall >/dev/null 2>&1 || cargo install cargo-binstall --locked
    @command -v cargo-tauri >/dev/null 2>&1 || cargo binstall tauri-cli --no-confirm
    @command -v cargo-deny >/dev/null 2>&1 || cargo binstall cargo-deny --no-confirm
    @command -v cargo-audit >/dev/null 2>&1 || cargo binstall cargo-audit --no-confirm
    @command -v mdbook >/dev/null 2>&1 || cargo binstall mdbook --no-confirm
    @command -v cargo-llvm-cov >/dev/null 2>&1 || cargo binstall cargo-llvm-cov --no-confirm

# ── Test ──────────────────────────────────────────────────────────────────────

# Benches/examples compile too and the lockfile must be current.
# Run all tests with the same flags CI uses
test:
    cargo test --workspace --all-targets --locked

# Run hydra-engine-wds tests only
test-engine:
    cargo test -p hydra-engine-wds

# Run hydra-sdk tests only
test-sdk:
    cargo test -p hydra-sdk

# Run hydra-cli tests only
test-cli:
    cargo test -p hydra-cli

# Run hydra-gui tests only
test-gui:
    cargo test -p hydra-gui

# Run frontend tests only
test-frontend:
    cd crates/gui/frontend && pnpm test

# Run Python script unit tests
test-scripts:
    python3 -m unittest discover -s scripts/tests -p "test_*.py" -v

# Run criterion benchmarks
bench:
    cargo bench -p hydra-engine-wds

# cargo-llvm-cov is installed by `just setup-tools`.
# Generate an HTML test-coverage report (target/llvm-cov/html/index.html).
coverage:
    cargo llvm-cov --workspace --html

# ── Lint & Format ─────────────────────────────────────────────────────────────

# Format everything (Rust + frontend)
fmt: fmt-rust fmt-frontend

# Format Rust source files
fmt-rust:
    cargo fmt --all

# Format frontend source files
fmt-frontend:
    cd crates/gui/frontend && pnpm format

# Check Rust formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Check frontend linting and formatting (Biome)
lint-frontend:
    cd crates/gui/frontend && pnpm lint

# Type-check frontend source files
typecheck-frontend:
    cd crates/gui/frontend && pnpm exec tsc --noEmit

# Run every static check, Rust and frontend — no tests
lint: fmt-check clippy typecheck-frontend lint-frontend

# ── Security ──────────────────────────────────────────────────────────────────

# Check dependency licenses and bans
deny:
    cargo deny check

# Audit Rust dependencies for known vulnerabilities
audit:
    cargo audit

# Audit frontend (pnpm) dependencies for known vulnerabilities
audit-frontend:
    cd crates/gui/frontend && pnpm audit --audit-level=high

# Run all dependency audits (Rust + frontend)
audit-all: audit audit-frontend

# ── Build ─────────────────────────────────────────────────────────────────────

# All three recipes below wrap the same script (scripts/update-crs-catalog.mjs)
# for three different call sites:
#   regen-crs-catalog  — regenerate from whatever @esri/proj-codes is installed
#                        now. Silent/non-failing; used as a normal build step.
#   update-crs-catalog — bump @esri/proj-codes to latest, then regenerate. The
#                        only one that changes package.json/the lockfile; run
#                        deliberately before a release.
#   check-crs-catalog  — regenerate, then fail if it differs from what's
#                        committed. CI-only drift check — never run as part of
#                        a normal local build, since that would fail on any
#                        version skew instead of just fixing it.

# No network access required — safe to call in CI and build pipelines.
# Regenerate the bundled CRS catalog from the installed @esri/proj-codes
regen-crs-catalog:
    node scripts/update-crs-catalog.mjs

# Run deliberately before a release to pull in new CRS definitions.
# Update @esri/proj-codes to its latest version and regenerate the catalog
update-crs-catalog:
    cd crates/gui/frontend && pnpm update @esri/proj-codes
    node scripts/update-crs-catalog.mjs

# Mirrors the "Check CRS catalog is up to date" step in pnpm-ci.yml — catches
# a stale catalog in CI instead of only discovering it after merge.
# Regenerate the CRS catalog and fail if it doesn't match what's committed
check-crs-catalog: regen-crs-catalog
    git diff --exit-code -- crates/gui/resources/crs-catalog.json

# Run cargo check (fast compile verification)
check:
    cargo check --workspace --all-targets

# Build debug binaries
build:
    cargo build

# Build frontend
build-frontend: regen-crs-catalog
    cd crates/gui/frontend && pnpm build

# Run the GUI in development mode (Tauri hot-reload for frontend + backend)
dev:
    cd crates/gui && cargo tauri dev

# Depends on build-frontend so the GUI embeds a current dist, and enables
# hydra-gui/custom-protocol — without that feature a release binary loads the
# dev-server URL and shows a white window (tauri: `dev = !custom-protocol`).
# Build optimised release binaries (fat LTO) with embedded GUI assets
release: build-frontend
    cargo build --release --features hydra-gui/custom-protocol

# Build release binaries tuned for the local CPU
release-native: build-frontend
    RUSTFLAGS="-C target-cpu=native" cargo build --release --features hydra-gui/custom-protocol

# Runs the frontend build itself (beforeBuildCommand), enables custom-protocol
# automatically, and drops output under target/release/bundle/.
# Build the distributable GUI app bundle (.app/.dmg) via tauri-cli
bundle:
    cd crates/gui && cargo tauri build

# ── Docs ──────────────────────────────────────────────────────────────────────

# Build the Rust API docs, failing on rustdoc warnings (the CI docs check)
docs-api:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Build the mdbook docs
docs-build:
    mdbook build docs

# Serve the mdbook docs locally with live reload
docs:
    mdbook serve docs --open

# ── CI ────────────────────────────────────────────────────────────────────────

# Skips the slower CI-only steps (deny, docs-api, catalog drift, python
# scripts); run `just ci` for the full set.
# Fast local gate: every static check plus the Rust and frontend test suites
verify: lint test test-frontend

# `test` already covers every workspace crate with CI's exact flags, so the
# per-crate test recipes are not repeated here.
# Run all checks that CI runs (mirrors cargo-ci + pnpm-ci + scripts-ci)
ci: deny lint docs-api test check-crs-catalog build-frontend test-frontend test-scripts

# ── Release ───────────────────────────────────────────────────────────────────

# When bumping multiple tracks, always run this first — it updates the hydra-sdk dep pin in hydra-cli.
# Usage: just bump patch|minor|major [--push|--no-push] (omit flag to be prompted)
# Bump the workspace library version (hydra-engine-wds, hydra-sdk) and tag v{version}
bump version push_flag="":
    @python3 scripts/bump.py {{version}} {{push_flag}}

# Usage: just bump-cli patch|minor|major [--push|--no-push] (omit flag to be prompted)
# Bump the CLI application version independently and tag cli-v{version}
bump-cli version push_flag="":
    @python3 scripts/bump-cli.py {{version}} {{push_flag}}

# Usage: just bump-gui patch|minor|major [--push|--no-push] (omit flag to be prompted)
# Bump the GUI application version independently and tag gui-v{version}
bump-gui version push_flag="":
    @python3 scripts/bump-gui.py {{version}} {{push_flag}}

# Release CANDIDATES are determined by changed files (reliable). Version SEVERITY
# is left to your discretion — commit-message signals are shown as hints only,
# never as an authoritative bump. Optionally focus on one track: e.g.
#   just release-status gui
# Show which tracks have unreleased changes; you choose the semver bump.
release-status track="":
    @python3 scripts/release-status.py {{track}}

# ── Clean ─────────────────────────────────────────────────────────────────────

# Remove all build artifacts
clean:
    cargo clean
    rm -rf crates/gui/frontend/dist docs/book
