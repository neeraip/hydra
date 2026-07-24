#!/usr/bin/env python3
"""Show which tracks (library, CLI, GUI) have unreleased changes.

Release CANDIDATES are determined by the files changed since each track's last
release tag, which is reliable regardless of commit-message hygiene. Version
SEVERITY is left to the user's discretion: commit-message signals are surfaced
as hints only (positive evidence such as `feat:` or `!`/`BREAKING CHANGE`),
never as an authoritative bump level.

Commits that touch only `benches/` files or only the `[dev-dependencies]`
table of a Cargo.toml (e.g. a criterion version bump) have no effect on what
ships in the library/CLI/GUI, so they're excluded from candidate detection.
They're still listed (dimmed) for visibility; commits mixing dev-only and
release-affecting files are highlighted so nothing is silently hidden.

Usage:
    scripts/release-status.py [library|cli|gui]
"""

import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

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


def commit_shas_since(tag, paths):
    out = sh("git", "log", f"{tag}..HEAD", "--pretty=format:%H", "--", *paths)
    return [s for s in out.splitlines() if s.strip()] if out else []


def file_at_revision(rev, path):
    try:
        return subprocess.run(
            ["git", "show", f"{rev}:{path}"], check=True, capture_output=True, text=True
        ).stdout
    except subprocess.CalledProcessError:
        return None  # file didn't exist at that revision


def cargo_toml_change_is_dev_only(sha, path):
    """True if this commit's edit to a Cargo.toml only touches [dev-dependencies].

    Dev-dependency version bumps (e.g. criterion, used only for benchmarking)
    can't affect what ships in the CLI/GUI, so they shouldn't force a cascade.
    """
    old = file_at_revision(f"{sha}~1", path)
    new = file_at_revision(sha, path)
    if old is None or new is None:
        return False  # file added or removed — treat as impactful
    try:
        old_doc, new_doc = tomllib.loads(old), tomllib.loads(new)
    except tomllib.TOMLDecodeError:
        return False
    old_doc.pop("dev-dependencies", None)
    new_doc.pop("dev-dependencies", None)
    return old_doc == new_doc


def classify_commit(sha, paths):
    """Classify a commit's effect on the compiled library/binary.

    Returns "dev" if every file the commit touched (within `paths`) is a
    bench file or a dev-dependency-only Cargo.toml edit (no effect on what
    ships), "prod" if every touched file could affect the shipped artifact,
    or "mixed" if it's a combination of both.
    """
    out = sh("git", "diff-tree", "--no-commit-id", "--name-only", "-r", sha, "--", *paths)
    files = [f for f in out.splitlines() if f.strip()]
    if not files:
        return "prod"
    dev_files = 0
    for f in files:
        if "benches" in Path(f).parts:
            dev_files += 1
        elif Path(f).name == "Cargo.toml" and cargo_toml_change_is_dev_only(sha, f):
            dev_files += 1
    if dev_files == 0:
        return "prod"
    if dev_files == len(files):
        return "dev"
    return "mixed"


def has_impactful_change(tag, paths):
    """True if at least one commit in `tag..HEAD` touching `paths` could
    affect the compiled library/binary (classification "prod" or "mixed").
    """
    return any(classify_commit(sha, paths) != "dev" for sha in commit_shas_since(tag, paths))


def signal(messages):
    # Positive evidence only. Returns "major", "minor", or "none".
    result = "none"
    for msg in messages:
        subject = msg.splitlines()[0] if msg else ""
        # Conventional Commits: "BREAKING-CHANGE" is a synonym of "BREAKING CHANGE".
        if re.search(r"(^|\n)BREAKING[ -]CHANGE:\s", msg) or re.match(r"^[a-z]+(\([^)]*\))?!:", subject):
            return "major"
        if re.match(r"^feat(\([^)]*\))?:", subject):
            result = "minor"
    return result


HINT = {
    "major": ("1;31", "breaking-change commit(s) present → suggests MAJOR"),
    "minor": ("33", "feature commit(s) present → suggests at least MINOR"),
    "none": ("2", "no feat/breaking markers → likely PATCH (verify against commits)"),
}

# Version-bump level implied by a track's own commit-message signal.
LEVEL_FOR_SIGNAL = {"major": "major", "minor": "minor", "none": "patch"}

# Colour for the suggested level itself, shown next to each bump command.
# Kept separate from HINT's colours: a "patch" suggestion isn't inherently
# less important than major/minor (it could be a critical security fix), so
# it uses the default foreground instead of HINT's dim "none" colour.
LEVEL_COLOR = {"major": "1;31", "minor": "33", "none": None}

# Per-commit colour by classification. "prod" uses the terminal's default
# foreground (no wrapping) so it stands out against the dimmed/highlighted ones.
COMMIT_COLOR = {"dev": "2", "mixed": "33"}

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
        shas = commit_shas_since(tag, paths)
        classifications = [classify_commit(sha, paths) for sha in shas]
        info[name] = {
            "tag": tag,
            "cmd": cmd,
            "commits": list(zip(subjects, classifications)),
            "count": len(subjects),
            "impactful": any(c != "dev" for c in classifications),
            "signal": signal(messages_since(tag, paths)),
        }

    if missing:
        print(f"error: no release tags found matching: {', '.join(missing)}", file=sys.stderr)
        return 1

    lib_changed = info["Library"]["impactful"]
    # Definitive candidate determination from changed files:
    #   - a library change cascades a required release onto CLI and GUI
    #   - otherwise CLI and GUI are independent, candidates only if they changed
    #   - dev-only changes (bench files, [dev-dependencies] bumps) don't count
    candidate = {
        "Library": lib_changed,
        "CLI": lib_changed or info["CLI"]["impactful"],
        "GUI": lib_changed or info["GUI"]["impactful"],
    }

    print()
    print(paint("1", "Hydra — Release Readiness"))
    print(paint("2", "─" * 60))
    print(paint("2", "Candidates come from changed files (reliable). Severity is your"))
    print(paint("2", "call — commit-message signals below are hints, not decisions."))
    print(
        "  "
        + paint("2", "■ dev-only")
        + "   "
        + paint("33", "■ mixed (dev + release-affecting)")
        + "   "
        + "■ affects release"
    )
    print()

    shown = [t for t in ("Library", "CLI", "GUI") if focus in ("", t.lower())]
    list_cap = None if focus else 10

    def print_commits(commits):
        capped = commits if list_cap is None else commits[:list_cap]
        for subject, cls in capped:
            color = COMMIT_COLOR.get(cls)
            text = paint(color, subject) if color else subject
            print(f"    {paint('2', '•')} {text}")
        if list_cap is not None and len(commits) > list_cap:
            print(paint("2", f"    … and {len(commits) - list_cap} more (pass a track name to see all)"))

    for name in shown:
        i = info[name]
        commits = i["commits"]
        if not candidate[name]:
            if i["count"] > 0:
                plural = "" if i["count"] == 1 else "s"
                status = paint("2", f"up to date · {i['count']} dev-only commit{plural} (no release needed)")
            else:
                status = paint("2", "up to date · no changes since tag")
            print(f"{paint('1', name)}  {i['tag']}   {status}")
            print_commits(commits)
            print()
            continue
        reason = "own changes" if i["impactful"] else "library cascade (no own changes)"
        status = paint("32", "release candidate")
        plural = "" if i["count"] == 1 else "s"
        print(f"{paint('1', name)}  {i['tag']}   {status} · {i['count']} commit{plural} · {reason}")
        print_commits(commits)
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
        own_signal = info[name]["signal"]
        level_text = LEVEL_FOR_SIGNAL[own_signal]
        color = LEVEL_COLOR[own_signal]
        suggested_level = paint(color, level_text) if color else level_text
        note = paint("2", "(signal suggests ") + suggested_level
        if not info[name]["impactful"]:
            note += paint("2", ", cascade only — no own changes")
        note += paint("2", ")")
        print(f"  {info[name]['cmd']} <level>   {note}")
    print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
