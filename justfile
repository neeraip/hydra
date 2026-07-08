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

# Regenerate the bundled CRS catalog from the currently-installed @esri/proj-codes.
# No network access required — safe to call in CI and build pipelines.
regen-crs-catalog:
    node scripts/update-crs-catalog.mjs

# Update @esri/proj-codes to its latest version and regenerate the catalog.
# Run deliberately before a release to pull in new CRS definitions.
update-crs-catalog: regen-crs-catalog
    cd crates/gui/frontend && pnpm update @esri/proj-codes
    node scripts/update-crs-catalog.mjs

# Run cargo check (fast compile verification)
check:
    cargo check

# Build debug binaries
build:
    cargo build

# Build frontend
build-frontend: regen-crs-catalog
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
# Usage: just bump patch [push_flag]  |  just bump minor [push_flag]  |  just bump major [push_flag]
# push_flag: --push or --no-push (or omit to be prompted)
bump version push_flag="":
    @python3 scripts/bump.py {{version}} {{push_flag}}

# Bump the CLI application version independently and tag cli-v{version}.
# Usage: just bump-cli patch [push_flag]  |  just bump-cli minor [push_flag]  |  just bump-cli major [push_flag]
# push_flag: --push or --no-push (or omit to be prompted)
bump-cli version push_flag="":
    @python3 scripts/bump-cli.py {{version}} {{push_flag}}

# Bump the GUI application version independently and tag gui-v{version}.
# Usage: just bump-gui patch [push_flag]  |  just bump-gui minor [push_flag]  |  just bump-gui major [push_flag]
# push_flag: --push or --no-push (or omit to be prompted)
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
