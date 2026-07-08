#!/usr/bin/env python3
"""Bump the GUI application version independently and tag gui-v{version}.

Updates crates/gui/Cargo.toml, tauri.conf.json, and the frontend package.json.

Usage: scripts/bump-gui.py <patch|minor|major>
"""

import json
import pathlib
import sys

from _release import commit_and_tag, next_version, parse_level, read_version, require_clean_main, set_version


def set_json_version(path: pathlib.Path, version):
    data = json.loads(path.read_text())
    data["version"] = version
    path.write_text(json.dumps(data, indent=2) + "\n")


def main():
    level = parse_level(sys.argv[1] if len(sys.argv) > 1 else "")
    require_clean_main()

    gui = pathlib.Path("crates/gui/Cargo.toml")
    version = next_version(read_version(gui), level)
    set_version(gui, version)

    set_json_version(pathlib.Path("crates/gui/tauri.conf.json"), version)
    set_json_version(pathlib.Path("crates/gui/frontend/package.json"), version)

    commit_and_tag(
        ["crates/gui/Cargo.toml", "crates/gui/tauri.conf.json", "crates/gui/frontend/package.json", "Cargo.lock"],
        f"chore(gui): bump version to {version}",
        f"gui-v{version}",
    )
    print(f"Tagged gui-v{version}. Push with: git push && git push --tags")


if __name__ == "__main__":
    main()
