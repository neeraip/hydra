# List available recipes (default)
default:
    @just --list

# ── Test ──────────────────────────────────────────────────────────────────────

# Run all tests
test:
    cargo test --workspace --all-targets --locked

# Run hydra-common tests only
test-common:
    cargo test -p hydra-common

# Run hydra-engine tests only
test-engine:
    cargo test -p hydra-engine

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

# Run criterion benchmarks
bench:
    cargo bench -p hydra-engine

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

# Bump the workspace version and sync it into tauri.conf.json, then commit and tag.
# Usage: just bump 1.2.3  |  just bump patch  |  just bump minor  |  just bump major
bump version:
    #!/usr/bin/env python3
    import json, pathlib, re, subprocess, sys
    arg = "{{version}}"
    cargo = pathlib.Path("Cargo.toml")
    m = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"', cargo.read_text(), re.MULTILINE)
    cur_major, cur_minor, cur_patch = int(m.group(1)), int(m.group(2)), int(m.group(3))
    if arg == "patch":
        version = f"{cur_major}.{cur_minor}.{cur_patch + 1}"
    elif arg == "minor":
        version = f"{cur_major}.{cur_minor + 1}.0"
    elif arg == "major":
        version = f"{cur_major + 1}.0.0"
    else:
        version = arg
    # Bump Cargo.toml workspace version
    cargo.write_text(re.sub(r'^version = ".*"', f'version = "{version}"', cargo.read_text(), count=1, flags=re.MULTILINE))
    # Sync cross-crate version pins
    for pin_path in ["crates/cli/Cargo.toml", "crates/sdk/Cargo.toml"]:
        p = pathlib.Path(pin_path)
        p.write_text(re.sub(r'version = "\d+\.\d+\.\d+"', f'version = "{version}"', p.read_text()))
    # Sync tauri.conf.json
    p = pathlib.Path("crates/gui/tauri.conf.json")
    d = json.loads(p.read_text())
    d["version"] = version
    p.write_text(json.dumps(d, indent=2) + "\n")
    # Regenerate Cargo.lock with the new workspace version
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    # Commit and tag
    subprocess.run(["git", "add", "Cargo.toml", "Cargo.lock", "crates/gui/tauri.conf.json",
                    "crates/cli/Cargo.toml", "crates/sdk/Cargo.toml"], check=True)
    subprocess.run(["git", "commit", "-m", f"chore: bump version to {version}"], check=True)
    subprocess.run(["git", "tag", "-a", f"v{version}", "-m", f"v{version}"], check=True)
    print(f"Tagged v{version}. Push with: git push && git push --tags")

# ── Clean ─────────────────────────────────────────────────────────────────────

# Remove all build artifacts
clean:
    cargo clean
    rm -rf crates/gui/frontend/dist docs/book
