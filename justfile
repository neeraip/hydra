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
    # `just check-wasm` is part of `just ci`, so the target is not optional.
    rustup target add wasm32-unknown-unknown

# Install frontend (pnpm) dependencies.
setup-frontend:
    cd crates/gui/frontend && pnpm install

# Uses cargo-binstall for prebuilt binaries; skips tools already present, so
# re-runs (e.g. via `just start`) are fast and offline-friendly. Installs:
# tauri-cli, cargo-deny, cargo-audit, mdbook, cargo-llvm-cov, wasm-pack.
# Install the cargo subcommand tools this justfile relies on
setup-tools:
    @command -v cargo-binstall >/dev/null 2>&1 || cargo install cargo-binstall --locked
    @command -v cargo-tauri >/dev/null 2>&1 || cargo binstall tauri-cli --no-confirm
    @command -v cargo-deny >/dev/null 2>&1 || cargo binstall cargo-deny --no-confirm
    @command -v cargo-audit >/dev/null 2>&1 || cargo binstall cargo-audit --no-confirm
    # mdbook is pinned: docs/theme/ overrides its template (see site.yml).
    @command -v mdbook >/dev/null 2>&1 || cargo binstall mdbook@0.5.4 --no-confirm
    @command -v mdbook-katex >/dev/null 2>&1 || cargo binstall mdbook-katex@0.10.0 --no-confirm
    @command -v cargo-llvm-cov >/dev/null 2>&1 || cargo binstall cargo-llvm-cov --no-confirm
    @command -v wasm-pack >/dev/null 2>&1 || cargo binstall wasm-pack --no-confirm

# ── Test ──────────────────────────────────────────────────────────────────────

# Benches/examples compile too and the lockfile must be current.
# Run all tests with the same flags CI uses
test:
    cargo test --workspace --all-targets --locked
    # Workspace unification switches the drainage engine's `threads`
    # feature on (the apps enable it), so the line above tests the
    # threaded configuration. These two keep the other configurations
    # honest: the engine's serial-only compile, and the 6.4 width
    # contract even if the apps ever drop the feature.
    cargo test -p hydra-engine-uds --locked
    cargo test -p hydra-engine-uds --features threads --test threads --locked

# Run hydra-engine-wds tests only
test-engine:
    cargo test -p hydra-engine-wds

# Run hydra-engine-uds tests only
test-engine-uds:
    cargo test -p hydra-engine-uds

# Run hydra-sdk tests only
test-sdk:
    cargo test -p hydra-sdk

# Run hydra-cli tests only
test-cli:
    cargo test -p hydra-cli

# Run hydra-gui tests only
test-gui:
    cargo test -p hydra-gui

# Run frontend unit + component tests only
test-frontend:
    cd crates/gui/frontend && pnpm test

# Run the layout tests only — a real browser, because jsdom performs no
# layout and answers every question about a box with a zero. Needs the
# Chromium Playwright downloads (`just setup-layout-tests`).
test-layout:
    cd crates/gui/frontend && pnpm test:layout

# One-time: fetch the browser the layout tests drive.
setup-layout-tests:
    cd crates/gui/frontend && pnpm exec playwright install chromium

# The only check that executes engine code on wasm, and the only one that can
# see the failures that matter there: a host call or a dependency that
# compiles fine and then panics at runtime. `just check-wasm` compiles, which
# both of those survive.
#
# Uses the *system* Chrome, unlike the layout tests, which drive
# Playwright's own download. Left to itself wasm-pack fetches the *newest*
# chromedriver rather than one matching that Chrome, and a major-version
# mismatch fails as `http status: 404` and a SIGKILL naming neither. So the
# driver is resolved first — see scripts/wasm_chromedriver.py.
# Run the engines in a real browser
test-wasm:
    wasm-pack test --headless --chrome \
        --chromedriver "$(python3 scripts/wasm_chromedriver.py)" \
        crates/demo --test browser

# Ask which of this file's decisions no test would notice being changed.
#
# Coverage cannot answer that: every hollow test this repository has had
# executed the code it was hollow about. Mutation testing changes the code
# and reports which changes the suite still calls correct, which is the
# only mechanical check on whether an assertion asserts anything.
#
# Deliberately not part of `just ci`: one engine file is around eight
# minutes. Run it on what you changed, and on anything whose semantics a
# reader would have to take on trust.
#
# Every test target, not just `--lib`: much of this engine is covered by
# the integration tests in `crates/engine-uds/tests/`, and restricting the
# run to lib tests reported all of that as uncovered. The first run on
# `io/iface.rs` came back 185 missed of 407 that way, nearly all of them
# phantoms.
#
# The scratch trees go outside the repository, in a `.noindex` directory.
# cargo-mutants copies the whole source tree and relinks a fresh set of
# test binaries for every mutant, which is gigabytes of churn per run, and
# two things react to it: macOS wakes Spotlight, Gatekeeper and XProtect on
# every new binary, and anything watching the working tree — an editor, a
# file manager, `fseventsd` itself — wakes on every write. The `.noindex`
# suffix answers the first (macOS skips such directories) and staying out
# of the repository answers the second. Putting it *inside* the repository
# traded one for the other: `fseventsd` alone then ran at 200%.
#
# Usage: just mutants crates/engine-uds/src/io/rain.rs
#        just mutants crates/report/src/render.rs hydra-report
# Needs `cargo install cargo-mutants`.
mutants FILE PKG="hydra-engine-uds":
    mkdir -p "${TMPDIR:-/tmp}/hydra-mutants.noindex"
    TMPDIR="${TMPDIR:-/tmp}/hydra-mutants.noindex" cargo mutants -p {{PKG}} --file {{FILE}}

# Run Python script unit tests
test-scripts:
    python3 -m unittest discover -s scripts/tests -p "test_*.py" -v

# Run criterion benchmarks
bench:
    cargo bench -p hydra-engine-wds

# Set HYDRA_UDS_CORPUS to a directory of .inp files to time real drainage
# models as well as the generated ones.
# Regenerate the performance-page numbers for both engines (Markdown tables)
bench-report:
    cargo build --release -p hydra-cli
    python3 scripts/benchmark.py

# Write the generated drainage benchmark models without timing them
bench-models:
    python3 scripts/make_uds_benchmark.py

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

# Find em dashes in user-facing copy: GUI text, CLI and demo diagnostics,
# report text, the docs site, the marketing site, the READMEs. Comments,
# specs and tests are exempt, and so is the lone placeholder glyph. See
# the User-Facing Copy section of AGENTS.md.
em-dashes:
    python3 scripts/em_dashes.py

# Run every static check, Rust and frontend — no tests
lint: fmt-check clippy check-wasm typecheck-frontend lint-frontend em-dashes

# ── Security ──────────────────────────────────────────────────────────────────

# Check dependency licenses and bans
deny:
    cargo deny check

# The notices the app shows under Settings → About, generated from the GUI's
# own dependency graph. Run after any dependency change, Rust or frontend,
# and commit the result — `just ci` fails on a stale file, because notices
# that no longer describe the binary are not notices.
# Regenerate the bundled third-party licence notices
licenses:
    python3 scripts/licenses.py

# Fail when the committed third-party notices no longer match the dependencies
licenses-check:
    python3 scripts/licenses.py --check

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

# Catches a dependency that will not build for wasm at all — a build script
# needing a host, a crate with no wasm support. That is a real class and this
# is the only thing that sees it.
#
# It does NOT catch either way the browser build has actually broken so far,
# and the comment says so because the recipe name suggests otherwise:
#
#   * A host call that compiles and panics at runtime. `SystemTime::now()`
#     is one. Guarded by the engines' `clippy.toml` files.
#   * A dependency that compiles and panics at runtime. `chrono` without
#     `wasmbind` is one, and removing that feature still passes this recipe.
#     Guarded by nothing yet — only running the engine in a real browser
#     would see it.
#
# Checks the SDK rather than crates/demo because the SDK is the whole engine
# surface, and it is the layer a third party would compile for wasm too.
# Check that the engines still compile for WebAssembly
check-wasm:
    cargo check -p hydra-sdk --target wasm32-unknown-unknown --locked

# NOTE: this is a compile check — it does NOT build the frontend and does NOT
# enable hydra-gui/custom-protocol, so `target/debug/hydra-gui` from here shows
# a white window (it tries to load the dev-server URL). Use `just dev` to run
# the GUI, `just release` for an optimised binary, or `just bundle` for the
# distributable app.
# Build debug binaries (compile check; the GUI binary is not runnable)
build:
    cargo build

# Build frontend
build-frontend: regen-crs-catalog
    cd crates/gui/frontend && pnpm build

# Run the GUI in development mode (Tauri hot-reload for frontend + backend)
dev:
    cd crates/gui && cargo tauri dev

# Stage a network as a project and open the app on it, landed on a view, for
# marketing screenshots. The engine is recognised from the file (--engine
# overrules). Stages into the real profile so basemap tokens and prefs apply;
# --isolate uses a scratch HOME instead (no keychain there, so no tokened
# basemaps). Only marker-bearing bundles are ever listed or reset.
# Usage: just screenshots <model.inp> [--view overview|canvas|editor|analysis|report]
#        just screenshots            (list staged)  |  --reset <model.inp|all>
screenshots *args:
    @python3 scripts/screenshot-stage.py {{args}}

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

# ── Browser demo ──────────────────────────────────────────────────────────────

# Deliberately outside `just ci`: the wasm bundle is a demo artifact, and CI
# already covers the crate's logic through the ordinary workspace test run
# (`crates/demo` is a member, and every decision in it is plain Rust). What CI
# does not cover is that the module loads in a browser — run `just demo-serve`
# for that.

# Build the demo's WebAssembly bundle into crates/demo/www/pkg
demo:
    wasm-pack build crates/demo --target web --out-dir www/pkg --out-name hydra

# The no-modules target rather than the web one: a file:// document has an
# opaque origin, so it can neither import an ES module nor fetch the wasm.
# See scripts/build-wasm-single.py for the rest of that story.
# Build the whole demo as one portable HTML file that runs from file://
demo-single:
    wasm-pack build crates/demo --target no-modules --out-dir www/pkg-nomodules --out-name hydra
    python3 scripts/build-wasm-single.py

# Needs a server rather than a file:// open — ES modules and WebAssembly
# streaming instantiation both require an http origin.
# Build the wasm bundle and serve the demo page at http://localhost:8000
demo-serve: demo
    cp site/hydra-theme.css crates/demo/www/hydra-theme.css
    @echo "Hydra in the browser: http://localhost:8000"
    cd crates/demo/www && python3 -m http.server 8000

# ── Docs ──────────────────────────────────────────────────────────────────────

# Build the Rust API docs, failing on rustdoc warnings (the CI docs check)
docs-api:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# The copy step first: the book links the shared web theme
# (site/hydra-theme.css is the source of truth; docs/hydra-theme.css is
# gitignored build input).
# Build the mdbook docs
docs-build:
    cp site/hydra-theme.css docs/hydra-theme.css
    mdbook build docs

# Serve the mdbook docs locally with live reload
docs:
    cp site/hydra-theme.css docs/hydra-theme.css
    mdbook serve docs --open

# ── Site ──────────────────────────────────────────────────────────────────────

# The assembled layout is what the Site workflow deploys to GitHub Pages:
# the marketing page at /, the mdbook at /docs, the browser demo at /try.
# One deliberate difference: CI builds /try from the latest library
# release tag (visitors try released engines), while this recipe builds
# it from the working tree for local preview. Keep the assembly steps in
# agreement with .github/workflows/site.yml.
# Assemble the whole Pages site into target/site
site: docs-build demo
    rm -rf target/site
    mkdir -p target/site/try
    cp -R site/. target/site/
    cp -R docs/book target/site/docs
    cp crates/demo/www/index.html crates/demo/www/app.js crates/demo/www/app.css \
       site/hydra-theme.css target/site/try/
    cp -R crates/demo/www/pkg target/site/try/pkg

# Assemble the site and serve it at http://localhost:8000
site-serve: site
    @echo "Site: http://localhost:8000  (docs at /docs, demo at /try)"
    cd target/site && python3 -m http.server 8000

# ── CI ────────────────────────────────────────────────────────────────────────

# Skips the slower CI-only steps (deny, docs-api, catalog drift, lockfile
# check, python scripts); run `just ci` for the full set.
# Fast local gate: every static check plus the Rust and frontend test suites
verify: lint test test-wasm test-frontend test-layout

# Fails when package.json and pnpm-lock.yaml have drifted (e.g. a hand-edited
# dependency without a corresponding install); fast no-op when in sync.
# Mirror CI's `pnpm install --frozen-lockfile` consistency check
check-frontend-lockfile:
    cd crates/gui/frontend && pnpm install --frozen-lockfile

# `test` already covers every workspace crate with CI's exact flags, so the
# per-crate test recipes are not repeated here.
# Run all checks that CI runs (mirrors cargo-ci + pnpm-ci + scripts-ci)
ci: deny check-frontend-lockfile lint docs-api test test-wasm check-crs-catalog licenses-check build-frontend test-frontend test-layout test-scripts

# ── Release ───────────────────────────────────────────────────────────────────

# When bumping multiple tracks, always run this first — it updates the hydra-sdk dep pin in hydra-cli.
# Usage: just bump patch|minor|major [--push|--no-push] (omit flag to be prompted)
# Bump the workspace library version (common, engines, report, sdk) and tag v{version}
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

# Ask Dependabot to refresh its open PRs that are behind main — `rebase` on
# clean branches, `recreate` where CI pushed a commit onto one (Dependabot
# refuses to rebase those). Skips PRs with checks still running, lists the
# rest and asks before commenting.
# Usage: just rebase-dependabot [--dry-run] [--force]
rebase-dependabot *flags:
    @python3 scripts/rebase-dependabot.py {{flags}}

# ── Clean ─────────────────────────────────────────────────────────────────────

# Check the drainage engine has not got slower (needs a release build).
#
# Every performance win in `uds` came from finding work that should not
# have been happening, and the same work can be reintroduced without any
# test noticing: the results stay byte-identical and the run takes twice
# as long. This compares against a recorded baseline and fails on a
# doubling, not a drift. Not in `ci` — it wants a release build and a
# quiet machine.
#
# Set HYDRA_SWMM to a build of the predecessor for the ratio column.
perf-check:
    python3 scripts/make_uds_benchmark.py
    python3 scripts/perf_baseline.py check tests/benchmarks/uds/baseline.json

# Re-record the baseline after a deliberate change. Read the diff.
perf-record:
    python3 scripts/make_uds_benchmark.py
    python3 scripts/perf_baseline.py record tests/benchmarks/uds/models.json \
        > tests/benchmarks/uds/baseline.json

# Remove all build artifacts
clean:
    cargo clean
    rm -rf crates/gui/frontend/dist docs/book
