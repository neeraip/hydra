#!/usr/bin/env python3
"""Ask Dependabot to refresh every open PR that is behind its base branch.

Dependabot rebases on its own schedule, or when a PR develops conflicts.
After a burst of pushes to main its PRs all go stale at once, and the
only lever GitHub offers is commenting on each one. This script does
that, but only where the branch is actually behind its base, so PRs that
are already current are not churned, and never while a PR still has
checks running, since the refresh would throw that run away.

The command posted depends on the branch: `@dependabot rebase` on a
clean one, `@dependabot recreate` where CI has pushed a commit onto it
(the licences workflow completes GUI bumps that way), because Dependabot
refuses to rebase a branch holding a commit it did not author and says
so instead of acting.

Usage: scripts/rebase-dependabot.py [--dry-run] [--force]

Lists the PRs it would comment on and asks before doing so; `--force`
skips the prompt (and is what a non-interactive caller must pass, since
without a terminal the prompt reads as a refusal). `--dry-run` only
prints the plan.

Needs an authenticated `gh` CLI.
"""

import json
import subprocess
import sys


def sh(args: list[str]) -> str:
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"{args[0]} failed")
    return result.stdout


def open_dependabot_prs() -> list[dict]:
    out = sh(
        [
            "gh", "pr", "list",
            "--author", "app/dependabot",
            "--state", "open",
            "--json", "number,title,headRefName,baseRefName,statusCheckRollup,commits",
        ]
    )
    return json.loads(out)


def command_for(pr: dict) -> str:
    """The refresh command this PR will actually obey.

    Dependabot refuses to rebase a branch holding a commit it did not
    author (the licences workflow pushes one onto GUI bumps), and only
    `recreate` works from then on. Missing commit data reads as clean:
    the worst case is a refusal comment from Dependabot naming the fix.
    """
    for commit in pr.get("commits") or []:
        for author in commit.get("authors") or []:
            login = author.get("login") or ""
            if login not in ("dependabot", "dependabot[bot]"):
                return "@dependabot recreate"
    return "@dependabot rebase"


def behind_by(base: str, head: str) -> int | None:
    """Commits the head ref is behind its base, or None if the comparison
    failed (branch vanished mid-run, API unavailable)."""
    try:
        out = sh(
            [
                "gh", "api",
                f"repos/{{owner}}/{{repo}}/compare/{base}...{head}",
                "--jq", ".behind_by",
            ]
        )
        return int(out.strip())
    except (RuntimeError, ValueError):
        return None


def has_running_checks(rollup: list[dict] | None) -> bool:
    """True while any check on the PR is still queued or running.

    The rollup mixes two shapes: check runs carry `status`
    (QUEUED/IN_PROGRESS/COMPLETED), commit statuses carry `state`
    (PENDING/SUCCESS/...). Rebasing under either would discard the run.
    """
    for check in rollup or []:
        status = check.get("status")
        if status and status != "COMPLETED":
            return True
        if check.get("state") == "PENDING":
            return True
    return False


def partition(prs: list[dict], behind_of: dict[str, int | None]):
    """Split PRs into (outdated, current, unknown) by how far behind their
    base branch they are.

    `behind_of` maps a head ref to a commit count, or None where the
    comparison failed. A failed comparison lands in `unknown` rather than
    being guessed either way: commenting on it might churn a current PR,
    and skipping it silently would hide that it was never checked.
    Outdated entries are (pr, behind) pairs; input order is kept.
    """
    outdated, current, unknown = [], [], []
    for pr in prs:
        behind = behind_of.get(pr["headRefName"])
        if behind is None:
            unknown.append(pr)
        elif behind > 0:
            outdated.append((pr, behind))
        else:
            current.append(pr)
    return outdated, current, unknown


def confirmed(outdated: list, ask=input) -> bool:
    """Show what a run would touch and ask for a yes.

    Anything except an explicit y/yes (a plain Enter, a closed stdin, an
    interrupt) declines: the safe reading of an ambiguous answer to "may
    I comment on N PRs" is no.
    """
    print(f"\nWould request a rebase on {len(outdated)} PR(s):")
    for pr, behind in outdated:
        print(f"  #{pr['number']}: behind by {behind} — {pr['title']}")
    try:
        answer = ask("Proceed? [y/N] ")
    except (EOFError, KeyboardInterrupt):
        print()
        return False
    return answer.strip().lower() in ("y", "yes")


def main() -> int:
    dry_run = "--dry-run" in sys.argv[1:]
    force = "--force" in sys.argv[1:]

    prs = open_dependabot_prs()
    if not prs:
        print("No open Dependabot PRs.")
        return 0

    busy = [pr for pr in prs if has_running_checks(pr.get("statusCheckRollup"))]
    idle = [pr for pr in prs if pr not in busy]

    behind_of = {pr["headRefName"]: behind_by(pr["baseRefName"], pr["headRefName"]) for pr in idle}
    outdated, current, unknown = partition(idle, behind_of)

    for pr in busy:
        print(f"#{pr['number']}: checks still running — skipped")
    for pr in current:
        print(f"#{pr['number']}: up to date — {pr['title']}")
    for pr in unknown:
        print(f"#{pr['number']}: could not compare against {pr['baseRefName']} — skipped")

    if dry_run:
        for pr, behind in outdated:
            verb = command_for(pr).split()[-1]
            print(f"#{pr['number']}: would request {verb} (behind by {behind}) — {pr['title']}")
        return 1 if unknown else 0

    if outdated and not force and not confirmed(outdated):
        print("Nothing done.")
        return 1

    failed = False
    for pr, behind in outdated:
        command = command_for(pr)
        try:
            sh(["gh", "pr", "comment", str(pr["number"]), "--body", command])
            print(
                f"#{pr['number']}: {command.split()[-1]} requested (behind by {behind})"
                f" — {pr['title']}"
            )
        except RuntimeError as e:
            print(f"#{pr['number']}: comment failed: {e}", file=sys.stderr)
            failed = True

    return 1 if failed or unknown else 0


if __name__ == "__main__":
    sys.exit(main())
