# List available recipes (default)
default:
    @just --list

# ── Release ───────────────────────────────────────────────────────────────────

# Bump the workspace version and sync it into tauri.conf.json, then commit and tag.
# Usage: just bump 1.2.3
bump version:
    #!/usr/bin/env python3
    import json, pathlib, re, subprocess, sys
    version = "{{version}}"
    # Bump Cargo.toml workspace version
    cargo = pathlib.Path("Cargo.toml")
    cargo.write_text(re.sub(r'^version = ".*"', f'version = "{version}"', cargo.read_text(), count=1, flags=re.MULTILINE))
    # Sync tauri.conf.json
    p = pathlib.Path("crates/gui/tauri.conf.json")
    d = json.loads(p.read_text())
    d["version"] = version
    p.write_text(json.dumps(d, indent=2) + "\n")
    # Regenerate Cargo.lock with the new workspace version
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    # Commit and tag
    subprocess.run(["git", "add", "Cargo.toml", "Cargo.lock", "crates/gui/tauri.conf.json"], check=True)
    subprocess.run(["git", "commit", "-m", f"chore: bump version to {version}"], check=True)
    subprocess.run(["git", "tag", "-a", f"v{version}", "-m", f"v{version}"], check=True)
    print(f"Tagged v{version}. Push with: git push && git push --tags")



# Build debug binaries
build:
    cargo build

# Build optimised release binaries (fat LTO)
release:
    cargo build --release

# Build release binaries tuned for the local CPU
release-native:
    RUSTFLAGS="-C target-cpu=native" cargo build --release

# ── Check & Test ──────────────────────────────────────────────────────────────

# Run cargo check (fast compile verification)
check:
    cargo check

# Run all tests
test:
    cargo test --workspace --all-targets --locked

# Run hydra-common tests only
test-common:
    cargo test -p hydra-common

# Run hydra-engine tests only
test-engine:
    cargo test -p hydra-engine

# Run hydra-cli tests only
test-cli:
    cargo test -p hydra-cli

# Run hydra-gui tests only
test-gui:
    cargo test -p hydra-gui

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

# Run all lints (format check + clippy)
lint: fmt-check clippy

# Check frontend linting and formatting
lint-frontend:
    cd crates/gui/frontend && pnpm lint

# Format frontend source files
fmt-frontend:
    cd crates/gui/frontend && pnpm format

# ── Benchmarks ────────────────────────────────────────────────────────────────

# Benchmark Hydra vs EPANET on synthetic networks
bench: release
    python3 ref/benchmarks/synthetic.py

# Benchmark with CPU-native release build
bench-native: release-native
    python3 ref/benchmarks/synthetic.py

# ── Clean ─────────────────────────────────────────────────────────────────────

# Remove build artifacts (target/)
clean:
    cargo clean
