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

# Bump the workspace library version (hydra-common, hydra-engine, hydra-sdk) and tag v{version}.
# When bumping multiple tracks, always run this first — it updates the hydra-sdk dep pin in hydra-cli.
# Usage: just bump 1.2.3  |  just bump patch  |  just bump minor  |  just bump major
bump version:
    #!/usr/bin/env python3
    import pathlib, re, subprocess, sys
    result = subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True)
    if result.stdout.strip():
        print("error: working tree is dirty — commit or stash changes before bumping", file=sys.stderr)
        sys.exit(1)
    branch = subprocess.run(["git", "branch", "--show-current"], capture_output=True, text=True).stdout.strip()
    if branch != "main":
        print(f"error: must be on main branch to bump (currently on '{branch}')", file=sys.stderr)
        sys.exit(1)
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
    # Bump workspace version
    cargo.write_text(re.sub(r'^version = ".*"', f'version = "{version}"', cargo.read_text(), count=1, flags=re.MULTILINE))
    # Sync the hydra-sdk dep pin in hydra-cli, and the hydra-engine dep pin in hydra-sdk
    for pin_path in ["crates/cli/Cargo.toml", "crates/sdk/Cargo.toml"]:
        p = pathlib.Path(pin_path)
        p.write_text(re.sub(r'version = "\d+\.\d+\.\d+"', f'version = "{version}"', p.read_text()))
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    subprocess.run(["git", "add", "Cargo.toml", "Cargo.lock", "crates/cli/Cargo.toml", "crates/sdk/Cargo.toml"], check=True)
    subprocess.run(["git", "commit", "-m", f"chore: bump library version to {version}"], check=True)
    subprocess.run(["git", "tag", "-a", f"v{version}", "-m", f"v{version}"], check=True)
    print(f"Tagged v{version}. Push with: git push && git push --tags")

# Bump the CLI application version independently and tag cli-v{version}.
# Usage: just bump-cli 1.2.3  |  just bump-cli patch  |  just bump-cli minor  |  just bump-cli major
bump-cli version:
    #!/usr/bin/env python3
    import pathlib, re, subprocess, sys
    result = subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True)
    if result.stdout.strip():
        print("error: working tree is dirty — commit or stash changes before bumping", file=sys.stderr)
        sys.exit(1)
    branch = subprocess.run(["git", "branch", "--show-current"], capture_output=True, text=True).stdout.strip()
    if branch != "main":
        print(f"error: must be on main branch to bump (currently on '{branch}')", file=sys.stderr)
        sys.exit(1)
    # Check that the pinned hydra-sdk version is already on crates.io.
    import urllib.request, json as _json
    sdk_pin = re.search(r'hydra-sdk[^}]+version = "([^"]+)"', pathlib.Path("crates/cli/Cargo.toml").read_text())
    if sdk_pin:
        sdk_version = sdk_pin.group(1)
        try:
            url = f"https://crates.io/api/v1/crates/hydra-sdk/{sdk_version}"
            req = urllib.request.Request(url, headers={"User-Agent": "hydra-justfile"})
            urllib.request.urlopen(req, timeout=10)
        except urllib.error.HTTPError:
            print(f"error: hydra-sdk {sdk_version} is not yet on crates.io.", file=sys.stderr)
            print("       Wait for the publish-crates workflow to finish before bumping the CLI.", file=sys.stderr)
            sys.exit(1)
    arg = "{{version}}"
    cli = pathlib.Path("crates/cli/Cargo.toml")
    m = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"', cli.read_text(), re.MULTILINE)
    cur_major, cur_minor, cur_patch = int(m.group(1)), int(m.group(2)), int(m.group(3))
    if arg == "patch":
        version = f"{cur_major}.{cur_minor}.{cur_patch + 1}"
    elif arg == "minor":
        version = f"{cur_major}.{cur_minor + 1}.0"
    elif arg == "major":
        version = f"{cur_major + 1}.0.0"
    else:
        version = arg
    cli.write_text(re.sub(r'^version = ".*"', f'version = "{version}"', cli.read_text(), count=1, flags=re.MULTILINE))
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    subprocess.run(["git", "add", "crates/cli/Cargo.toml", "Cargo.lock"], check=True)
    subprocess.run(["git", "commit", "-m", f"chore(cli): bump version to {version}"], check=True)
    subprocess.run(["git", "tag", "-a", f"cli-v{version}", "-m", f"cli-v{version}"], check=True)
    print(f"Tagged cli-v{version}. Push with: git push && git push --tags")

# Bump the GUI application version independently and tag gui-v{version}.
# Usage: just bump-gui 1.2.3  |  just bump-gui patch  |  just bump-gui minor  |  just bump-gui major
bump-gui version:
    #!/usr/bin/env python3
    import json, pathlib, re, subprocess, sys
    result = subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True)
    if result.stdout.strip():
        print("error: working tree is dirty — commit or stash changes before bumping", file=sys.stderr)
        sys.exit(1)
    branch = subprocess.run(["git", "branch", "--show-current"], capture_output=True, text=True).stdout.strip()
    if branch != "main":
        print(f"error: must be on main branch to bump (currently on '{branch}')", file=sys.stderr)
        sys.exit(1)
    arg = "{{version}}"
    gui = pathlib.Path("crates/gui/Cargo.toml")
    m = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"', gui.read_text(), re.MULTILINE)
    cur_major, cur_minor, cur_patch = int(m.group(1)), int(m.group(2)), int(m.group(3))
    if arg == "patch":
        version = f"{cur_major}.{cur_minor}.{cur_patch + 1}"
    elif arg == "minor":
        version = f"{cur_major}.{cur_minor + 1}.0"
    elif arg == "major":
        version = f"{cur_major + 1}.0.0"
    else:
        version = arg
    gui.write_text(re.sub(r'^version = ".*"', f'version = "{version}"', gui.read_text(), count=1, flags=re.MULTILINE))
    p = pathlib.Path("crates/gui/tauri.conf.json")
    d = json.loads(p.read_text())
    d["version"] = version
    p.write_text(json.dumps(d, indent=2) + "\n")
    pkg = pathlib.Path("crates/gui/frontend/package.json")
    d = json.loads(pkg.read_text())
    d["version"] = version
    pkg.write_text(json.dumps(d, indent=2) + "\n")
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    subprocess.run(["git", "add", "crates/gui/Cargo.toml", "crates/gui/tauri.conf.json",
                    "crates/gui/frontend/package.json", "Cargo.lock"], check=True)
    subprocess.run(["git", "commit", "-m", f"chore(gui): bump version to {version}"], check=True)
    subprocess.run(["git", "tag", "-a", f"gui-v{version}", "-m", f"gui-v{version}"], check=True)
    print(f"Tagged gui-v{version}. Push with: git push && git push --tags")

# ── Clean ─────────────────────────────────────────────────────────────────────

# Remove all build artifacts
clean:
    cargo clean
    rm -rf crates/gui/frontend/dist docs/book
