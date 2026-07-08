#!/usr/bin/env python3
"""Bump the workspace library version (hydra-engine-wds, hydra-sdk) and tag v{version}.

Run this first when bumping multiple tracks — it also updates the hydra-sdk dep
pin in hydra-cli and the hydra-engine-wds dep pin in hydra-sdk.

Usage: scripts/bump.py <patch|minor|major> [--push|--no-push]
"""

import pathlib
import re
import sys

from _release import commit_and_tag, maybe_push, next_version, parse_level, parse_push_pref, read_version, require_clean_main, set_version


def main():
    args, push_pref = parse_push_pref(sys.argv[1:])
    level = parse_level(args[0] if args else "")
    require_clean_main()

    cargo = pathlib.Path("Cargo.toml")
    version = next_version(read_version(cargo), level)
    set_version(cargo, version)

    # Update only the hydra-sdk dep pin in hydra-cli (not the cli package version).
    cli = pathlib.Path("crates/cli/Cargo.toml")
    cli.write_text(re.sub(r'(hydra-sdk[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', cli.read_text()))

    # Update only the hydra-engine-wds dep pin in hydra-sdk.
    sdk = pathlib.Path("crates/sdk/Cargo.toml")
    sdk.write_text(re.sub(r'(hydra-engine-wds[^\n]+version = ")\d+\.\d+\.\d+"', rf'\g<1>{version}"', sdk.read_text()))

    commit_and_tag(
        ["Cargo.toml", "Cargo.lock", "crates/cli/Cargo.toml", "crates/sdk/Cargo.toml"],
        f"chore: bump library version to {version}",
        f"v{version}",
    )
    print(f"Tagged v{version}.")
    maybe_push(push_pref)


if __name__ == "__main__":
    main()
