#!/usr/bin/env python3
"""Bump the CLI application version independently and tag cli-v{version}.

Verifies the pinned hydra-sdk version is already on crates.io before proceeding,
since the CLI publish will fail otherwise.

Usage: scripts/bump-cli.py <patch|minor|major>
"""

import pathlib
import re
import sys
import urllib.error
import urllib.request

from _release import commit_and_tag, fail, next_version, parse_level, read_version, require_clean_main, set_version


def ensure_sdk_published(cli_toml: pathlib.Path):
    pin = re.search(r'hydra-sdk[^}]+version = "([^"]+)"', cli_toml.read_text())
    if not pin:
        return
    sdk_version = pin.group(1)
    url = f"https://crates.io/api/v1/crates/hydra-sdk/{sdk_version}"
    req = urllib.request.Request(url, headers={"User-Agent": "hydra-release"})
    try:
        urllib.request.urlopen(req, timeout=10)
    except urllib.error.HTTPError:
        fail(
            f"hydra-sdk {sdk_version} is not yet on crates.io.\n"
            "       Wait for the publish-crates workflow to finish before bumping the CLI."
        )


def main():
    level = parse_level(sys.argv[1] if len(sys.argv) > 1 else "")
    require_clean_main()

    cli = pathlib.Path("crates/cli/Cargo.toml")
    ensure_sdk_published(cli)

    version = next_version(read_version(cli), level)
    set_version(cli, version)

    commit_and_tag(
        ["crates/cli/Cargo.toml", "Cargo.lock"],
        f"chore(cli): bump version to {version}",
        f"cli-v{version}",
    )
    print(f"Tagged cli-v{version}. Push with: git push && git push --tags")


if __name__ == "__main__":
    main()
