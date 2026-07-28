#!/usr/bin/env python3
"""Bump the workspace library version (hydra-engine-wds, hydra-sdk) and tag v{version}.

Run this first when bumping multiple tracks — it also updates the hydra-sdk dep
pin in hydra-cli and the hydra-engine-wds dep pin in hydra-sdk.

Usage: scripts/bump.py <patch|minor|major> [--push|--no-push]
"""

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

    # Update the hydra-engine-wds, hydra-common, and hydra-report dep pins
    # in hydra-sdk.
    sdk = pathlib.Path("crates/sdk/Cargo.toml")
    sdk_text = sdk.read_text()
    for dep in ("hydra-engine-wds", "hydra-common", "hydra-report"):
        sdk_text = re.sub(rf'({dep}[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', sdk_text)
    sdk.write_text(sdk_text)

    # Update the hydra-common dep pins in hydra-engine-wds and hydra-report.
    for crate in ("crates/engine-wds/Cargo.toml", "crates/report/Cargo.toml"):
        p = pathlib.Path(crate)
        p.write_text(re.sub(r'(hydra-common[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', p.read_text()))

    commit_and_tag(
        [
            "Cargo.toml",
            "Cargo.lock",
            "crates/cli/Cargo.toml",
            "crates/sdk/Cargo.toml",
            "crates/engine-wds/Cargo.toml",
            "crates/report/Cargo.toml",
        ],
        f"chore: bump library version to {version}",
        f"v{version}",
    )
    print(f"Tagged v{version}.")
    maybe_push(push_pref)


if __name__ == "__main__":
    main()
