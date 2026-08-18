"""Shared helpers for the release bump scripts (bump, bump-cli, bump-gui).

Not a standalone command — imported by the sibling scripts.
"""

import json
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
        # Consumed here so it never reaches parse_level_arg, which rejects
        # any positional it does not recognise.
        if arg == "--no-verify-ci":
            continue
        positionals.append(arg)
    return positionals, push_pref


def require_clean_main():
    if sh("git", "status", "--porcelain").stdout.strip():
        fail("working tree is dirty — commit or stash changes before bumping")
    branch = sh("git", "branch", "--show-current").stdout.strip()
    if branch != "main":
        fail(f"must be on main branch to bump (currently on '{branch}')")
    require_up_to_date_with_origin()


def ci_verdict(runs):
    """What CI says about a commit: ("green"|"failing"|"pending"|"unknown", names).

    A tag is cut from a commit, and the release build starts from the tag —
    so if that commit's CI is red, the release ships whatever was broken.
    gui-v2.18.0 and gui-v2.18.1 were both tagged over a red main and both
    failed; gui-v2.19.0 was tagged before its own licence check reported and
    shipped notices that did not match its dependencies.

    `runs` is one dict per workflow run, as `gh run list --json` returns
    them. "unknown" means nothing could be determined and the caller should
    say so rather than pretend either way.
    """
    if not runs:
        return "unknown", []
    failing = sorted(
        {r.get("name", "?") for r in runs if r.get("conclusion") in ("failure", "timed_out")}
    )
    if failing:
        return "failing", failing
    pending = sorted(
        {r.get("name", "?") for r in runs if r.get("status") not in ("completed",)}
    )
    if pending:
        return "pending", pending
    return "green", []


def ci_runs_for(sha):
    """Workflow runs for one commit, or None when they cannot be fetched."""
    try:
        out = subprocess.run(
            ["gh", "run", "list", "--commit", sha, "--limit", "50",
             "--json", "name,status,conclusion"],
            check=True,
            capture_output=True,
            text=True,
            timeout=20,
        ).stdout
        return json.loads(out)
    except Exception:
        return None


def require_green_ci(skip):
    """Refuse to tag a commit whose CI is red, or still deciding."""
    if skip:
        print("Skipping the CI check (--no-verify-ci).")
        return
    sha = sh("git", "rev-parse", "HEAD").stdout.strip()
    runs = ci_runs_for(sha)
    if runs is None:
        print(f"Could not reach GitHub to check CI for {sha[:8]}; continuing.")
        return
    verdict, names = ci_verdict(runs)
    if verdict == "green":
        return
    if verdict == "unknown":
        print(f"No CI runs found for {sha[:8]} yet; continuing.")
        return
    joined = ", ".join(names)
    fail(
        f"CI is {verdict} for {sha[:8]} ({joined}).\n"
        "       A tag is cut from this commit and the release builds from the\n"
        "       tag, so releasing now ships whatever is broken. Wait for CI, or\n"
        "       pass --no-verify-ci if you know better."
    )


def upstream_state(local, remote):
    """How this branch stands against its remote, from two rev-lists.

    `local` is the commits here and not there, `remote` the reverse — the
    two halves of `git rev-list --left-right --count`. Returned as a word
    so the caller's decision reads as a sentence and can be tested without
    a repository:

    - `"ahead"` — the normal state for a release. The commits being tagged
      are exactly the ones not yet pushed.
    - `"synced"` — also fine; a bump of work that is already pushed.
    - `"behind"` / `"diverged"` — refused. The tag would name a tree
      missing commits that are already on the remote, and nothing
      downstream would say so: the release builds from the tag, so the
      omission is silent and permanent.
    """
    if remote and local:
        return "diverged"
    if remote:
        return "behind"
    if local:
        return "ahead"
    return "synced"


def require_up_to_date_with_origin():
    """Refuse to bump a branch that is missing commits from origin/main.

    A release tag is immutable in this project (see RELEASING.md), so a
    tag cut from a stale checkout cannot be moved afterwards — it can only
    be superseded by another version. Fetching first is the one check that
    prevents it, and it costs a second.

    A clone with no origin is not an error: the check simply does not
    apply, and a local-only repository must still be bumpable. An origin
    that cannot be reached *is* an error, because "I could not look" and
    "there is nothing new" are the two answers this must not confuse.
    """
    remotes = sh("git", "remote", check=False).stdout.split()
    if "origin" not in remotes:
        return
    fetched = sh("git", "fetch", "--quiet", "origin", "main", check=False)
    if fetched.returncode != 0:
        fail(
            "could not reach origin to check for newer commits — a tag cut "
            "from a stale checkout cannot be moved later. Connect and retry, "
            "or fetch by hand if you are certain this branch is current."
        )
    counts = sh(
        "git",
        "rev-list",
        "--left-right",
        "--count",
        "HEAD...origin/main",
        check=False,
    )
    if counts.returncode != 0:
        return
    local, remote = (int(n) for n in counts.stdout.split())
    state = upstream_state(local, remote)
    if state == "behind":
        fail(
            f"main is {remote} commit(s) behind origin/main — pull before "
            f"bumping, or the tag will name a tree missing them"
        )
    if state == "diverged":
        fail(
            f"main has diverged from origin/main ({local} here, {remote} "
            f"there) — reconcile before bumping"
        )


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


def maybe_push(push_pref, tag):
    if push_pref is None:
        # Without a terminal there is nobody to ask, and by this point the
        # commit and the tag already exist. Raising here left the release
        # half-made and printed a traceback over it; not pushing is the
        # safe half of the question, and the line below says how to finish.
        if not sys.stdin.isatty():
            print("Not a terminal, so not pushing. Pass --push to push.")
            push_pref = False
        else:
            answer = input("Push commit and tags now? [y/N]: ").strip().lower()
            push_pref = answer in {"y", "yes"}

    if push_pref:
        sh("git", "push", capture=False)
        # This tag by name, not --tags: that pushes every local tag, including
        # any left over from an abandoned release.
        sh("git", "push", "origin", f"refs/tags/{tag}", capture=False)
        print(f"Pushed branch and {tag}.")
        return

    print(f"Not pushed. Push with: git push && git push origin refs/tags/{tag}")
