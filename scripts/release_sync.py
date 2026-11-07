#!/usr/bin/env python3
"""Complete a release-please pull request with what release-please cannot do.

release-please moves the version numbers and writes the changelog (see
release-please-config.json). Three things it does not know how to do, and a
release cannot go out without, are done here, on the release branch, by the
`complete release PRs` job in .github/workflows/release.yml:

1. Move every intra-workspace dependency pin to the workspace version, and
   the documented `hydra-sdk = "N"` pin to its major.
2. Refresh Cargo.lock, which records every workspace crate's version.
3. Regenerate the third-party licence notices, so the release ships notices
   that describe the binary it ships (Settings -> About in the app).

Idempotent: on a CLI or GUI release branch the pins are already current and
only the lock file and notices can move. Run with no arguments from the
repository root.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

SDK_PIN_DOCS = ("README.md", "crates/sdk/README.md", "docs/src/sdk/overview.md")

# Every manifest carrying intra-workspace dep pins.
#
# Unpublished crates (hydra-gui, hydra-demo) are absent on purpose: nothing
# resolves them from a registry, so their path deps carry no version at all
# and there is nothing here to rewrite.
CRATE_MANIFESTS = (
    "crates/sdk/Cargo.toml",
    "crates/engines/Cargo.toml",
    "crates/engine-wds/Cargo.toml",
    "crates/engine-uds/Cargo.toml",
    "crates/interop-swmm/Cargo.toml",
    "crates/interop-epanet/Cargo.toml",
    "crates/report/Cargo.toml",
    # Published, so its hydra-sdk pin is real and must move with the
    # workspace. Its own package version is a separate release track.
    "crates/cli/Cargo.toml",
)

WORKSPACE_DEPS = (
    "hydra-common",
    "hydra-engine-wds",
    "hydra-engine-uds",
    "hydra-interop-swmm",
    "hydra-interop-epanet",
    "hydra-engines",
    "hydra-report",
    "hydra-sdk",
)


def workspace_version(root: pathlib.Path = ROOT) -> str:
    """The `[workspace.package]` version, which release-please has set."""
    text = (root / "Cargo.toml").read_text()
    section = text.split("[workspace.package]", 1)
    if len(section) != 2:
        raise SystemExit("error: no [workspace.package] table in Cargo.toml")
    match = re.search(r'(?m)^version = "(\d+\.\d+\.\d+)"', section[1])
    if not match:
        raise SystemExit("error: no version in [workspace.package]")
    return match.group(1)


def rewrite_pins(text: str, version: str) -> str:
    """Point every intra-workspace dependency pin in a manifest at `version`.

    Keyed off the crate name, so a new workspace dependency is picked up by
    adding it to one list, not by remembering to add a bespoke pattern. The
    CLI depends on the SDK under the alias `hydra`, so the pattern matches
    the package name wherever it sits on the line.
    """
    for dep in WORKSPACE_DEPS:
        text = re.sub(rf'({dep}[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', text)
    return text


def rewrite_doc_pin(text: str, version: str, doc: str) -> str:
    """Point the documented `hydra-sdk = "N"` pin at the major of `version`.

    Only the major is documented: a caret pin already admits every compatible
    minor and patch, so narrowing it would just create churn.
    """
    major = version.split(".")[0]
    text, n = re.subn(r'(hydra-sdk = ")\d+(")', rf'\g<1>{major}\g<2>', text)
    if n != 1:
        raise SystemExit(f"error: expected exactly one hydra-sdk pin in {doc}, found {n}")
    return text


def main() -> int:
    version = workspace_version()
    for manifest in CRATE_MANIFESTS:
        path = ROOT / manifest
        path.write_text(rewrite_pins(path.read_text(), version))
    for doc in SDK_PIN_DOCS:
        path = ROOT / doc
        path.write_text(rewrite_doc_pin(path.read_text(), version, doc))

    # --workspace touches only the workspace's own entries, so a release
    # moves no third-party dependency.
    subprocess.run(["cargo", "update", "--workspace"], cwd=ROOT, check=True)
    # The notices read every platform's packages from the registry, not just
    # this machine's, so all of them must be fetched first.
    subprocess.run(["cargo", "fetch", "--locked"], cwd=ROOT, check=True)
    subprocess.run([sys.executable, "scripts/licenses.py"], cwd=ROOT, check=True)
    print(f"Release branch completed for workspace version {version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
