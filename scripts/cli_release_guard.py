#!/usr/bin/env python3
"""Refuse a CLI release whose binaries would not match its crates.io package.

The CLI's release binaries are built from the workspace source at its tag,
engine and all. Its crates.io package is built against the `hydra-sdk`
version it pins. The two run the same engine only when the library crates
at the CLI tag are exactly the library crates of that pinned release.

They differ when a CLI release PR is merged while library changes are still
unreleased: `cargo install hydra-cli` would then run the last published
engine, and the downloaded binary the unreleased one, with nothing to say
so. This check stops the CLI build in that state and says which release to
merge first.

Usage (from the repository root, with tags fetched):
    python3 scripts/cli_release_guard.py cli-vX.Y.Z
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def workspace_members(root: pathlib.Path) -> list[str]:
    text = (root / "Cargo.toml").read_text()
    block = re.search(r"members\s*=\s*\[(.*?)\]", text, re.S)
    if block is None:
        raise SystemExit("error: no workspace members in Cargo.toml")
    return re.findall(r'"([^"]+)"', block.group(1))


def library_dirs(root: pathlib.Path = ROOT) -> list[str]:
    """The published library crates: every publishable member but the CLI.

    Read from the manifests, so a new library crate is covered the moment it
    is published, with nothing here to update.
    """
    dirs = []
    for member in workspace_members(root):
        text = (root / member / "Cargo.toml").read_text()
        name = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text)
        publish = re.search(r"(?m)^publish\s*=\s*(true|false)", text)
        if name is None or name.group(1) == "hydra-cli":
            continue
        if publish is None or publish.group(1) == "true":
            dirs.append(member)
    return dirs


def sdk_pin(cli_manifest: str) -> str:
    """The `hydra-sdk` version the CLI manifest pins."""
    match = re.search(r'package = "hydra-sdk"[^}\n]*version = "(\d+\.\d+\.\d+)"', cli_manifest)
    if match is None:
        raise SystemExit("error: no hydra-sdk version pin in crates/cli/Cargo.toml")
    return match.group(1)


def git(root: pathlib.Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *args], cwd=root, capture_output=True, text=True)


def unreleased_library_changes(root: pathlib.Path, library_tag: str, cli_tag: str) -> list[str]:
    """Library files that differ between the pinned release and the CLI tag."""
    if git(root, "rev-parse", "--verify", "--quiet", f"refs/tags/{library_tag}").returncode != 0:
        raise SystemExit(
            f"error: the CLI pins hydra-sdk {library_tag[1:]}, but there is no {library_tag} tag. "
            "Merge the library release PR first."
        )
    diff = git(root, "diff", "--name-only", library_tag, cli_tag, "--", *library_dirs(root))
    if diff.returncode != 0:
        raise SystemExit(f"error: git diff failed: {diff.stderr.strip()}")
    return diff.stdout.split()


def main(argv: list[str]) -> int:
    if len(argv) != 1 or not argv[0].startswith("cli-v"):
        raise SystemExit("usage: cli_release_guard.py cli-vX.Y.Z")
    cli_tag = argv[0]
    shown = git(ROOT, "show", f"{cli_tag}:crates/cli/Cargo.toml")
    if shown.returncode != 0:
        raise SystemExit(f"error: cannot read crates/cli/Cargo.toml at {cli_tag}: {shown.stderr.strip()}")
    library_tag = f"v{sdk_pin(shown.stdout)}"
    changed = unreleased_library_changes(ROOT, library_tag, cli_tag)
    if changed:
        listed = "\n".join(f"  {path}" for path in changed[:20])
        more = f"\n  ... and {len(changed) - 20} more" if len(changed) > 20 else ""
        print(
            f"::error::{cli_tag} carries library changes not in {library_tag}, the hydra-sdk it pins. "
            "Its binaries and its crates.io package would run different engines.\n"
            f"Changed since {library_tag}:\n{listed}{more}\n"
            "Merge and publish the library release PR, then release the CLI again."
        )
        return 1
    print(f"{cli_tag} builds the same library as {library_tag}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
