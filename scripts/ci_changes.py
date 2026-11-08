#!/usr/bin/env python3
"""Decide which CI a change needs, from the paths it touches.

Two answers, one per expensive workflow:

- `rust`: `full` runs Cargo CI as usual; `gui` runs only the hydra-gui
  tests, once, on Linux; `none` skips it.
- `frontend`: `true` runs PNPM CI; `false` skips it.

The areas are almost independent, and the few places they are not are
listed here, each with the file that creates the dependency:

- hydra-gui's Rust tests read three frontend files to check that hand-kept
  Rust and TypeScript lists agree (`RUST_READS_FRONTEND`).
- The frontend tests read binary fixtures the Rust side writes
  (`tests/fixtures/*.bin`).
- PNPM CI regenerates the CRS catalog, which the GUI embeds.
- The GUI embeds the licence texts (`LICENSE`, `COMMERCIAL_LICENSE.md`).

A path this file does not recognise runs everything. A new directory can
therefore cost CI time until it is classified, but it can never skip a
check that would have caught something.

Usage: pass changed paths on stdin, one per line, or `--unknown` when they
cannot be listed (run everything). Prints `rust=...` and `frontend=...`
lines for $GITHUB_OUTPUT.
"""

from __future__ import annotations

import fnmatch
import sys

FRONTEND = "crates/gui/frontend/"

# Frontend files the Rust test suite reads (grep the gui crate for
# "frontend/" to find them). A change to one of these needs the gui tests.
RUST_READS_FRONTEND = {
    "crates/gui/frontend/src/hooks/engines.ts",
    "crates/gui/frontend/src/pages/project/ElementsView/railGroups.ts",
    "crates/gui/frontend/src/types/elementTypes.ts",
}

# Neither build nor test reads these. Scripts CI, which always runs, covers
# the workflows and scripts themselves.
INERT = (
    "docs/*",
    "site/*",
    "*.md",  # root-level only; see `classify`
    ".github/*",
    "scripts/*",
    "release-please-config.json",
    ".release-please-manifest.json",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".vscode/*",
)

# Inert-looking paths that are not: they feed one of the two CI workflows,
# or this decision itself.
NOT_INERT = {
    ".github/workflows/cargo-ci.yml": "rust",
    ".github/workflows/pnpm-ci.yml": "frontend",
    ".github/actions/tauri-deps/action.yml": "rust",
    ".github/actions/ci-changes/action.yml": "both",
    "scripts/ci_changes.py": "both",
    "scripts/wasm_chromedriver.py": "rust",  # just test-wasm
    "scripts/update-crs-catalog.mjs": "frontend",  # PNPM Lint
    "LICENSE": "rust",
    "COMMERCIAL_LICENSE.md": "rust",
}

# Read by both sides.
BOTH = (
    "tests/fixtures/*.bin",
    "crates/gui/resources/crs-catalog.json",
)

# Rust-only: the workspace, its configuration, and the fixtures its tests
# read. Everything else that is not inert falls through to "both".
RUST = (
    "crates/*",
    "tests/*",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "deny.toml",
    "clippy.toml",
    "rustfmt.toml",
    ".cargo/*",
    "justfile",
)


def area(path: str) -> str:
    """One of `inert`, `rust`, `gui`, `frontend` or `both` for a path."""
    if path in NOT_INERT:
        return NOT_INERT[path]
    if path.startswith(FRONTEND):
        return "gui" if path in RUST_READS_FRONTEND else "frontend"
    if any(fnmatch.fnmatchcase(path, p) for p in BOTH):
        return "both"
    if any(fnmatch.fnmatchcase(path, p) for p in RUST):
        return "rust"
    for pattern in INERT:
        # `*.md` means the repository root: a README inside a crate can be
        # compiled into its documentation and doctested.
        if pattern == "*.md":
            if "/" not in path and path.endswith(".md"):
                return "inert"
        elif fnmatch.fnmatchcase(path, pattern):
            return "inert"
    return "both"


def classify(paths: list[str] | None) -> dict[str, str]:
    """The CI a change needs. `None` means the paths are unknown."""
    if paths is None:
        return {"rust": "full", "frontend": "true"}
    areas = {area(p) for p in paths if p}
    if areas & {"both", "rust"}:
        rust = "full"
    elif "gui" in areas:
        rust = "gui"
    else:
        rust = "none"
    frontend = bool(areas & {"both", "frontend", "gui"})
    return {"rust": rust, "frontend": "true" if frontend else "false"}


def main(argv: list[str]) -> int:
    if argv == ["--unknown"]:
        paths = None
    elif argv:
        raise SystemExit("usage: ci_changes.py [--unknown] < paths")
    else:
        paths = [line.strip() for line in sys.stdin if line.strip()]
    for key, value in classify(paths).items():
        print(f"{key}={value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
