# List available recipes (default)
default:
    @just --list

# ── Test ──────────────────────────────────────────────────────────────────────

# Run all tests
test:
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

# ── Lint & Format ─────────────────────────────────────────────────────────────

# Format all Rust source files
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Run all Rust lints (format check + clippy)
lint: fmt-check clippy

# Format frontend source files
fmt-frontend:
    cd crates/gui/frontend && pnpm format

# Check frontend linting and formatting
lint-frontend:
    cd crates/gui/frontend && pnpm lint

# Type-check frontend source files
type-check-frontend:
    cd crates/gui/frontend && pnpm exec tsc --noEmit

# ── Security ──────────────────────────────────────────────────────────────────

# Check dependency licenses and bans
deny:
    cargo deny check

# Audit dependencies for known vulnerabilities
audit:
    cargo audit

# ── Build ─────────────────────────────────────────────────────────────────────

# Run cargo check (fast compile verification)
check:
    cargo check

# Build debug binaries
build:
    cargo build

# Build frontend
build-frontend:
    cd crates/gui/frontend && pnpm build

# Build optimised release binaries (fat LTO)
release:
    cargo build --release

# Build release binaries tuned for the local CPU
release-native:
    RUSTFLAGS="-C target-cpu=native" cargo build --release

# ── Docs ──────────────────────────────────────────────────────────────────────

# Check Rust API documentation compiles without warnings
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Build the mdbook docs
docs-build:
    mdbook build docs

# Serve the mdbook docs locally with live reload
docs:
    mdbook serve docs --open

# ── CI ────────────────────────────────────────────────────────────────────────

# Run all checks that CI runs (mirrors cargo-ci + pnpm-ci workflows)
ci: deny fmt-check clippy doc test type-check-frontend lint-frontend build-frontend test-frontend

# ── Release ───────────────────────────────────────────────────────────────────

# Bump the workspace library version (hydra-engine-wds, hydra-sdk) and tag v{version}.
# When bumping multiple tracks, always run this first — it updates the hydra-sdk dep pin in hydra-cli.
# Usage: just bump patch  |  just bump minor  |  just bump major
bump version:
    @python3 scripts/bump.py {{version}}

# Bump the CLI application version independently and tag cli-v{version}.
# Usage: just bump-cli patch  |  just bump-cli minor  |  just bump-cli major
bump-cli version:
    @python3 scripts/bump-cli.py {{version}}

# Bump the GUI application version independently and tag gui-v{version}.
# Usage: just bump-gui patch  |  just bump-gui minor  |  just bump-gui major
bump-gui version:
    @python3 scripts/bump-gui.py {{version}}

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
