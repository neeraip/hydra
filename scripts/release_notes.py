#!/usr/bin/env python3
"""Take a release's notes from a "# Release Notes" comment on its release PR.

release-please writes the changelog into the release PR's description and
copies it into the release when the PR merges. It also rewrites that
description whenever main moves, so a hand edit there rarely survives. A
comment does: if a maintainer comments on the release PR with a body that
starts with a `# Release Notes` heading, everything under that heading
becomes the release's notes. Otherwise the notes release-please wrote from
the description stay.

Only comments by people with write access count (the repository is public,
so anyone can comment). If several qualify, the newest wins.

Run on the push of the release PR's merge commit, while the release is
still a draft, before it is published.

Usage (needs GH_TOKEN with contents:write and pull-requests:read):
    python3 scripts/release_notes.py <tag> <merge-commit-sha>
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile

HEADING = re.compile(r"^#[ \t]+Release Notes[ \t]*$", re.IGNORECASE)

# GitHub's author_association values for people who can push here.
TRUSTED = {"OWNER", "MEMBER", "COLLABORATOR"}


def notes_of(body: str) -> str | None:
    """The text under a leading `# Release Notes` heading, or None.

    The heading must be the comment's first non-blank line. A heading with
    nothing under it is not notes.
    """
    lines = body.replace("\r\n", "\n").split("\n")
    while lines and not lines[0].strip():
        lines.pop(0)
    if not lines or not HEADING.match(lines[0]):
        return None
    notes = "\n".join(lines[1:]).strip()
    return notes or None


def chosen_notes(comments: list[dict]) -> str | None:
    """The notes from the newest trusted comment that has them, or None.

    Each comment is a dict with `body`, `author_association` and
    `created_at` (ISO 8601, which sorts chronologically as text).
    """
    candidates = [
        (c["created_at"], notes)
        for c in comments
        if c.get("author_association") in TRUSTED
        and (notes := notes_of(c.get("body") or "")) is not None
    ]
    if not candidates:
        return None
    return max(candidates, key=lambda pair: pair[0])[1]


def gh_json(*args: str):
    out = subprocess.run(["gh", "api", *args], check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    tag, sha = argv
    repo = os.environ.get("GITHUB_REPOSITORY") or subprocess.run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()

    prs = [p for p in gh_json(f"repos/{repo}/commits/{sha}/pulls") if p.get("merged_at")]
    if not prs:
        print(f"No merged pull request for {sha}; keeping release-please's notes.")
        return 0
    number = prs[0]["number"]

    pages = gh_json("--paginate", "--slurp", f"repos/{repo}/issues/{number}/comments")
    comments = [c for page in pages for c in page]
    notes = chosen_notes(comments)
    if notes is None:
        print(f"No '# Release Notes' comment on #{number}; keeping the notes from its description.")
        return 0

    with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as f:
        f.write(notes + "\n")
        path = f.name
    subprocess.run(
        ["gh", "release", "edit", tag, "--repo", repo, "--notes-file", path], check=True
    )
    print(f"Set the notes of {tag} from the '# Release Notes' comment on #{number}.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
