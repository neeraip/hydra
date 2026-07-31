#!/usr/bin/env python3
"""Bump the workspace library version (hydra-engine-wds, hydra-sdk) and tag v{version}.

Run this first when bumping multiple tracks — it also updates the hydra-sdk dep
pin in hydra-cli and the hydra-engine-wds dep pin in hydra-sdk.

Usage: scripts/bump.py <patch|minor|major> [--push|--no-push]
"""

# Docs that tell users which hydra-sdk version to depend on. Cargo reads a bare
# `"1"` as `^1.0`, which EXCLUDES every later major — so a stale pin here does
# not merely look old, it silently resolves readers onto an ancient release.
# These went a whole major cycle stale precisely because nothing updated them,
# so the bump owns them now; `test_documented_sdk_pin_matches_workspace_major`
# fails the build if they ever drift again.
SDK_PIN_DOCS = ("README.md", "crates/sdk/README.md", "docs/src/sdk/overview.md")

import pathlib
import re
import sys

from _release import commit_and_tag, maybe_push, next_version, parse_level_arg, parse_push_pref, read_version, require_clean_main, set_version


def main():
    args, push_pref = parse_push_pref(sys.argv[1:])
    level = parse_level_arg(args)
    require_clean_main()

    cargo = pathlib.Path("Cargo.toml")
    version = next_version(read_version(cargo), level)
    set_version(cargo, version)

    # Update only the hydra-sdk dep pin in hydra-cli (not the cli package version).
    cli = pathlib.Path("crates/cli/Cargo.toml")
    cli.write_text(re.sub(r'(hydra-sdk[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', cli.read_text()))

    # Update every intra-workspace dep pin. Keyed off the crate name so a new
    # workspace dependency is picked up by adding it to one list, not by
    # remembering to add a bespoke regex.
    WORKSPACE_DEPS = (
        "hydra-common",
        "hydra-engine-wds",
        "hydra-engines",
        "hydra-report",
    )
    for crate in (
        "crates/sdk/Cargo.toml",
        "crates/engines/Cargo.toml",
        "crates/engine-wds/Cargo.toml",
        "crates/report/Cargo.toml",
    ):
        p = pathlib.Path(crate)
        text = p.read_text()
        for dep in WORKSPACE_DEPS:
            text = re.sub(rf'({dep}[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', text)
        p.write_text(text)

    # Only the MAJOR is documented: a caret pin already admits every
    # compatible minor and patch, so narrowing it would just create churn.
    major = version.split(".")[0]
    for doc in SDK_PIN_DOCS:
        p = pathlib.Path(doc)
        text, n = re.subn(r'(hydra-sdk = ")\d+(")', rf'\g<1>{major}\g<2>', p.read_text())
        if n != 1:
            raise SystemExit(f"error: expected exactly one hydra-sdk pin in {doc}, found {n}")
        p.write_text(text)

    commit_and_tag(
        [
            "Cargo.toml",
            "Cargo.lock",
            "crates/cli/Cargo.toml",
            "crates/sdk/Cargo.toml",
            "crates/engine-wds/Cargo.toml",
            "crates/report/Cargo.toml",
            *SDK_PIN_DOCS,
        ],
        f"chore: bump library version to {version}",
        f"v{version}",
    )
    print(f"Tagged v{version}.")
    maybe_push(push_pref)


if __name__ == "__main__":
    main()
