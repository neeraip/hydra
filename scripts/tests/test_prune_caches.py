"""Which Actions caches the pruner deletes.

Deleting the wrong cache costs a cold build; deleting none lets the budget
fill and GitHub evict the rarely used ones. Both directions are asserted.
"""

import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "prune_caches.py"
spec = importlib.util.spec_from_file_location("prune_caches", SCRIPT)
prune = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prune)

MAIN = "refs/heads/main"


def cache(key, created, ref=MAIN, id_=None):
    return {"id": id_ or hash((key, created)), "key": key, "ref": ref, "createdAt": created}


class JobOf(unittest.TestCase):
    def test_strips_the_two_hashes(self):
        self.assertEqual(prune.job_of("v0-rust-test-Linux-x64-2acfcdfa-0fbe4f12"), "v0-rust-test-Linux-x64")

    def test_ignores_caches_that_are_not_rust_caches(self):
        for key in ("node-cache-Linux-x64-pnpm-ab7dae84", "tauri-cli-2.11.4-Linux",
                    "codeql-overlay-base-database-1-8ad4c9f449c267ec"):
            self.assertIsNone(prune.job_of(key), key)


class Superseded(unittest.TestCase):
    def test_keeps_the_newest_of_each_job_and_deletes_the_rest(self):
        old_linux = cache("v0-rust-test-Linux-x64-2acfcdfa-0fbe4f12", "2026-11-01T00:00:00Z")
        new_linux = cache("v0-rust-test-Linux-x64-b7a72f56-1e7685ae", "2026-11-07T00:00:00Z")
        older_linux = cache("v0-rust-test-Linux-x64-2acfcdfa-a1dde2a1", "2026-10-20T00:00:00Z")
        only_mac = cache("v0-rust-test-Darwin-arm64-7dfe23d7-0fbe4f12", "2026-10-01T00:00:00Z")
        stale = prune.superseded([old_linux, new_linux, older_linux, only_mac])
        self.assertCountEqual([c["key"] for c in stale], [old_linux["key"], older_linux["key"]])

    def test_jobs_are_told_apart_by_their_whole_prefix(self):
        # The lint job and the test job share an OS; neither replaces the other.
        lint = cache("v0-rust-lint-Linux-x64-d07fae92-0fbe4f12", "2026-11-01T00:00:00Z")
        test = cache("v0-rust-test-Linux-x64-2acfcdfa-0fbe4f12", "2026-11-07T00:00:00Z")
        self.assertEqual(prune.superseded([lint, test]), [])

    def test_leaves_other_refs_and_other_caches_alone(self):
        pr = cache("v0-rust-test-Linux-x64-2acfcdfa-0fbe4f12", "2026-10-01T00:00:00Z", ref="refs/pull/154/merge")
        node_old = cache("node-cache-Linux-x64-pnpm-aaaa", "2026-10-01T00:00:00Z")
        node_new = cache("node-cache-Linux-x64-pnpm-bbbb", "2026-11-01T00:00:00Z")
        main_rust = cache("v0-rust-test-Linux-x64-b7a72f56-1e7685ae", "2026-11-07T00:00:00Z")
        self.assertEqual(prune.superseded([pr, node_old, node_new, main_rust]), [])


if __name__ == "__main__":
    unittest.main()
