#!/usr/bin/env python3
"""Show which tracks (library, CLI, GUI) have unreleased changes.

Release CANDIDATES are determined by the files changed since each track's last
release tag, which is reliable regardless of commit-message hygiene. Version
SEVERITY is left to the user's discretion: commit-message signals are surfaced
as hints only (positive evidence such as `feat:` or `!`/`BREAKING CHANGE`),
never as an authoritative bump level.

Usage:
    scripts/release-status.py [library|cli|gui]
"""

import os
import re
import subprocess
import sys

SEMVER_RE = r"\d+\.\d+\.\d+"

USE_COLOR = sys.stdout.isatty() and os.environ.get("NO_COLOR") is None


def paint(code, text):
    return f"\033[{code}m{text}\033[0m" if USE_COLOR else text


def sh(*args):
    return subprocess.run(list(args), check=True, capture_output=True, text=True).stdout.strip()


def latest_tag(pattern):
    tags = [t for t in sh("git", "tag", "--list", pattern).splitlines() if t]
    if not tags:
        return None

    def key(tag):
        m = re.search(rf"({SEMVER_RE})$", tag)
        return tuple(int(p) for p in m.group(1).split(".")) if m else (0, 0, 0)

    return sorted(tags, key=key)[-1]


def messages_since(tag, paths):
    out = sh("git", "log", f"{tag}..HEAD", "--pretty=format:%s%n%b%x00", "--", *paths)
    return [c.strip() for c in out.split("\x00") if c.strip()] if out else []


def subjects_since(tag, paths):
    out = sh("git", "log", f"{tag}..HEAD", "--pretty=format:%s", "--", *paths)
    return [s for s in out.splitlines() if s.strip()] if out else []


def signal(messages):
    # Positive evidence only. Returns "major", "minor", or "none".
    result = "none"
    for msg in messages:
        subject = msg.splitlines()[0] if msg else ""
        if re.search(r"(^|\n)BREAKING CHANGE:\s", msg) or re.match(r"^[a-z]+(\([^)]*\))?!:", subject):
            return "major"
        if re.match(r"^feat(\([^)]*\))?:", subject):
            result = "minor"
    return result


HINT = {
    "major": ("1;31", "breaking-change commit(s) present → suggests MAJOR"),
    "minor": ("33", "feature commit(s) present → suggests at least MINOR"),
    "none": ("2", "no feat/breaking markers → likely PATCH (verify against commits)"),
}

TRACKS = [
    ("Library", "v[0-9]*.[0-9]*.[0-9]*", ["Cargo.toml", "crates/engine-wds", "crates/sdk"], "just bump"),
    ("CLI", "cli-v[0-9]*.[0-9]*.[0-9]*", ["crates/cli"], "just bump-cli"),
    ("GUI", "gui-v[0-9]*.[0-9]*.[0-9]*", ["crates/gui"], "just bump-gui"),
]


def main():
    focus = (sys.argv[1] if len(sys.argv) > 1 else "").strip().lower()
    valid = {"", "library", "cli", "gui"}
    if focus not in valid:
        print(f"error: unknown track '{focus}' — choose one of: library, cli, gui", file=sys.stderr)
        return 1

    info = {}
    missing = []
    for name, pattern, paths, cmd in TRACKS:
        tag = latest_tag(pattern)
        if tag is None:
            missing.append(pattern)
            continue
        subjects = subjects_since(tag, paths)
        info[name] = {
            "tag": tag,
            "cmd": cmd,
            "subjects": subjects,
            "count": len(subjects),
            "signal": signal(messages_since(tag, paths)),
        }

    if missing:
        print(f"error: no release tags found matching: {', '.join(missing)}", file=sys.stderr)
        return 1

    lib_changed = info["Library"]["count"] > 0
    # Definitive candidate determination from changed files:
    #   - a library change cascades a required release onto CLI and GUI
    #   - otherwise CLI and GUI are independent, candidates only if they changed
    candidate = {
        "Library": lib_changed,
        "CLI": lib_changed or info["CLI"]["count"] > 0,
        "GUI": lib_changed or info["GUI"]["count"] > 0,
    }

    print()
    print(paint("1", "Hydra — Release Readiness"))
    print(paint("2", "─" * 60))
    print(paint("2", "Candidates come from changed files (reliable). Severity is your"))
    print(paint("2", "call — commit-message signals below are hints, not decisions."))
    print()

    shown = [t for t in ("Library", "CLI", "GUI") if focus in ("", t.lower())]
    list_cap = None if focus else 10

    for name in shown:
        i = info[name]
        if not candidate[name]:
            status = paint("2", "up to date · no changes since tag")
            print(f"{paint('1', name)}  {i['tag']}   {status}")
            print()
            continue
        reason = "own changes" if i["count"] > 0 else "library cascade (no own changes)"
        status = paint("32", "release candidate")
        plural = "" if i["count"] == 1 else "s"
        print(f"{paint('1', name)}  {i['tag']}   {status} · {i['count']} commit{plural} · {reason}")
        subjects = i["subjects"]
        capped = subjects if list_cap is None else subjects[:list_cap]
        for s in capped:
            print(f"    {paint('2', '•')} {s}")
        if list_cap is not None and len(subjects) > list_cap:
            print(paint("2", f"    … and {len(subjects) - list_cap} more (pass a track name to see all)"))
        if i["count"] > 0:
            code, text = HINT[i["signal"]]
            print(f"    {paint(code, 'hint: ' + text)}")
        print()

    if lib_changed:
        print(paint("1", "Cascade"))
        print("  " + paint("2", "Library changed → CLI and GUI must be released from the library bump too."))
        print()

    print(paint("1", "Release plan"))
    plan = [n for n in ("Library", "CLI", "GUI") if candidate[n]]
    if not plan:
        print("  " + paint("2", "Nothing to release — no changes since the last tags."))
        print()
        return 0

    print(paint("2", "  You choose the level: <patch|minor|major>"))
    note_shown = False
    for name in plan:
        if name != "Library" and lib_changed and not note_shown:
            print("  " + paint("2", "# after publish-crates completes for the library:"))
            note_shown = True
        print(f"  {info[name]['cmd']} <level>")
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
