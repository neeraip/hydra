#!/usr/bin/env python3
"""Bump the workspace library version (common, engines, report, sdk) and tag v{version}.

Run this first when bumping multiple tracks — it also updates the hydra-sdk dep
pin in hydra-cli and every intra-workspace dep pin (see CRATE_MANIFESTS).

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

from _release import commit_and_tag, require_green_ci, maybe_push, next_version, parse_level_arg, parse_push_pref, read_version, require_clean_main, set_version


# Every manifest carrying intra-workspace dep pins. Used both to rewrite the
# pins and to stage the result — see commit_and_tag below.
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
    # workspace. Its own package version is a separate track (bump-cli).
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


def main():
    args, push_pref = parse_push_pref(sys.argv[1:])
    level = parse_level_arg(args)
    require_clean_main()
    require_green_ci("--no-verify-ci" in sys.argv[1:])

    cargo = pathlib.Path("Cargo.toml")
    version = next_version(read_version(cargo), level)
    set_version(cargo, version)

    # Update every intra-workspace dep pin. Keyed off the crate name so a new
    # workspace dependency is picked up by adding it to one list, not by
    # remembering to add a bespoke regex.
    for crate in CRATE_MANIFESTS:
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

    # One list, used both to rewrite and to stage. Keeping them separate cost
    # a release once: crates/engines was rewritten but never committed, so the
    # tag carried pins nothing had updated.
    commit_and_tag(
        [
            "Cargo.toml",
            "Cargo.lock",
            *CRATE_MANIFESTS,
            *SDK_PIN_DOCS,
        ],
        f"chore: bump library version to {version}",
        f"v{version}",
    )
    print(f"Tagged v{version}.")
    maybe_push(push_pref, f"v{version}")


if __name__ == "__main__":
    main()
