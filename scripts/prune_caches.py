#!/usr/bin/env python3
"""Delete the Actions caches that a newer save on main has superseded.

rust-cache keys end in two hashes, one of the toolchain and environment and
one of the lock files: `v0-rust-test-Linux-x64-2acfcdfa-0fbe4f12`. Every
change to either saves a fresh cache under a new key, and the old one stays
until GitHub evicts it. Each Cargo cache is 0.6 to 0.9 GB against a 10 GB
budget, so three generations of every job filled the budget, and the
eviction that followed took whichever cache was least recently used: the
weekly and release-only ones, which then ran cold.

Only the newest cache of each job survives here. Caches on other refs are
left to GitHub: a pull request's cache is scoped to it and small.

Usage (needs GH_TOKEN with actions:write):
    python3 scripts/prune_caches.py [--dry-run]
"""

from __future__ import annotations

import json
import re
import subprocess
import sys

# rust-cache's key: a prefix naming the job, then two 8-hex-digit hashes.
RUST_KEY = re.compile(r"^(v\d+-rust-.+)-[0-9a-f]{8}-[0-9a-f]{8}$")


def job_of(key: str) -> str | None:
    """The job a rust-cache key belongs to, or None for any other cache."""
    match = RUST_KEY.match(key)
    return match.group(1) if match else None


def superseded(caches: list[dict]) -> list[dict]:
    """Caches on main that a newer cache of the same job has replaced.

    Each cache is a dict with `id`, `key`, `ref` and `createdAt` (ISO 8601,
    which sorts chronologically as text).
    """
    newest: dict[str, dict] = {}
    for cache in caches:
        job = job_of(cache["key"])
        if job is None or cache["ref"] != "refs/heads/main":
            continue
        kept = newest.get(job)
        if kept is None or cache["createdAt"] > kept["createdAt"]:
            newest[job] = cache
    keep = {id(c) for c in newest.values()}
    return [
        c
        for c in caches
        if job_of(c["key"]) is not None and c["ref"] == "refs/heads/main" and id(c) not in keep
    ]


def main(argv: list[str]) -> int:
    dry_run = argv == ["--dry-run"]
    if argv and not dry_run:
        raise SystemExit("usage: prune_caches.py [--dry-run]")
    listed = subprocess.run(
        ["gh", "cache", "list", "--ref", "refs/heads/main", "--limit", "500",
         "--json", "id,key,ref,createdAt,sizeInBytes"],
        check=True, capture_output=True, text=True,
    )
    stale = superseded(json.loads(listed.stdout))
    freed = 0
    for cache in stale:
        freed += cache.get("sizeInBytes", 0)
        print(f"{'would delete' if dry_run else 'deleting'} {cache['key']} ({cache['createdAt']})")
        if not dry_run:
            subprocess.run(["gh", "cache", "delete", str(cache["id"])], check=True)
    print(f"{len(stale)} superseded cache(s), {freed / 1e9:.2f} GB.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
