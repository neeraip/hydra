"""Shared helpers for the release bump scripts (bump, bump-cli, bump-gui).

Not a standalone command — imported by the sibling scripts.
"""

import pathlib
import re
import subprocess
import sys

LEVELS = ("patch", "minor", "major")


def sh(*args, check=True, capture=True):
    return subprocess.run(list(args), check=check, capture_output=capture, text=True)


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def parse_level(arg):
    if arg not in LEVELS:
        fail(f"invalid bump level '{arg}' — must be patch, minor, or major")
    return arg


def parse_level_arg(positionals):
    """Parse the single <patch|minor|major> positional, rejecting extras."""
    if len(positionals) > 1:
        fail(f"unexpected extra argument(s): {' '.join(positionals[1:])}")
    return parse_level(positionals[0] if positionals else "")


def parse_push_pref(args):
    push_pref = None
    positionals = []
    for arg in args:
        if arg == "--push":
            if push_pref is False:
                fail("cannot pass both --push and --no-push")
            push_pref = True
            continue
        if arg == "--no-push":
            if push_pref is True:
                fail("cannot pass both --push and --no-push")
            push_pref = False
            continue
        positionals.append(arg)
    return positionals, push_pref


def require_clean_main():
    if sh("git", "status", "--porcelain").stdout.strip():
        fail("working tree is dirty — commit or stash changes before bumping")
    branch = sh("git", "branch", "--show-current").stdout.strip()
    if branch != "main":
        fail(f"must be on main branch to bump (currently on '{branch}')")


def next_version(current, level):
    major, minor, patch = (int(p) for p in current.split("."))
    if level == "patch":
        return f"{major}.{minor}.{patch + 1}"
    if level == "minor":
        return f"{major}.{minor + 1}.0"
    return f"{major + 1}.0.0"


def read_version(path: pathlib.Path):
    m = re.search(r'^version = "(\d+\.\d+\.\d+)"', path.read_text(), re.MULTILINE)
    if not m:
        fail(f"could not find a version field in {path}")
    return m.group(1)


def set_version(path: pathlib.Path, version):
    path.write_text(
        re.sub(r'^version = ".*"', f'version = "{version}"', path.read_text(), count=1, flags=re.MULTILINE)
    )


def commit_and_tag(files, message, tag):
    sh("cargo", "update", "--workspace", capture=False)
    sh("git", "add", *files)
    sh("git", "commit", "-m", message)
    sh("git", "tag", "-a", tag, "-m", tag)


def maybe_push(push_pref):
    if push_pref is None:
        answer = input("Push commit and tags now? [y/N]: ").strip().lower()
        push_pref = answer in {"y", "yes"}

    if push_pref:
        sh("git", "push", capture=False)
        sh("git", "push", "--tags", capture=False)
        print("Pushed branch and tags.")
        return

    print("Not pushed. Push with: git push && git push --tags")
