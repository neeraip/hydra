"""The selection decision in rebase-dependabot: which PRs get a rebase
comment, which are left alone, and which are surfaced as uncheckable."""

import contextlib
import importlib.util
import io
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "rebase-dependabot.py"
spec = importlib.util.spec_from_file_location("rebase_dependabot", SCRIPT)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def pr(number, head):
    return {"number": number, "title": f"bump {head}", "headRefName": head, "baseRefName": "main"}


class RunningChecksTests(unittest.TestCase):
    # The rollup mixes check runs (`status`) and commit statuses
    # (`state`); a rebase under either in flight throws the run away.

    def test_an_in_progress_check_run_blocks(self):
        rollup = [{"status": "COMPLETED", "conclusion": "SUCCESS"}, {"status": "IN_PROGRESS"}]
        self.assertTrue(mod.has_running_checks(rollup))

    def test_a_queued_check_run_blocks(self):
        self.assertTrue(mod.has_running_checks([{"status": "QUEUED"}]))

    def test_a_pending_commit_status_blocks(self):
        self.assertTrue(mod.has_running_checks([{"state": "PENDING"}]))

    def test_settled_checks_do_not_block(self):
        rollup = [
            {"status": "COMPLETED", "conclusion": "FAILURE"},
            {"state": "SUCCESS"},
        ]
        self.assertFalse(mod.has_running_checks(rollup))

    def test_no_checks_do_not_block(self):
        self.assertFalse(mod.has_running_checks([]))
        self.assertFalse(mod.has_running_checks(None))


class CommandTests(unittest.TestCase):
    # Dependabot refuses to rebase a branch holding a commit it did not
    # author (the licences workflow pushes one onto GUI bumps); only
    # `recreate` works from then on, and posting `rebase` there earns a
    # refusal comment instead of a refresh.

    def test_a_clean_branch_gets_rebase(self):
        p = pr(1, "clean")
        p["commits"] = [{"authors": [{"login": "dependabot[bot]"}]}]
        self.assertEqual(mod.command_for(p), "@dependabot rebase")

    def test_a_ci_completed_branch_gets_recreate(self):
        p = pr(2, "gui-bump")
        p["commits"] = [
            {"authors": [{"login": "dependabot[bot]"}]},
            {"authors": [{"login": "github-actions[bot]"}]},
        ]
        self.assertEqual(mod.command_for(p), "@dependabot recreate")

    def test_missing_commit_data_reads_as_clean(self):
        # The worst case of guessing wrong here is Dependabot's refusal
        # comment, which itself names the fix.
        self.assertEqual(mod.command_for(pr(3, "unknown")), "@dependabot rebase")


class ConfirmationTests(unittest.TestCase):
    OUTDATED = [({"number": 7, "title": "bump x", "headRefName": "x", "baseRefName": "main"}, 4)]

    def confirmed(self, ask):
        # confirmed() prints the plan it is asking about; keep that off
        # the test run's output.
        with contextlib.redirect_stdout(io.StringIO()):
            return mod.confirmed(self.OUTDATED, ask=ask)

    def test_yes_confirms(self):
        for answer in ("y", "Y", "yes", " YES "):
            self.assertTrue(self.confirmed(lambda _: answer))

    def test_anything_else_declines(self):
        # A plain Enter must decline: the prompt promises [y/N].
        for answer in ("", "n", "no", "q", "sure"):
            self.assertFalse(self.confirmed(lambda _: answer))

    def test_closed_stdin_declines(self):
        # A non-interactive caller without --force gets a refusal, not a
        # hang and not a silent yes.
        def closed(_):
            raise EOFError

        self.assertFalse(self.confirmed(closed))


class PartitionTests(unittest.TestCase):
    def test_behind_prs_are_selected_with_their_distance(self):
        prs = [pr(1, "a"), pr(2, "b")]
        outdated, current, unknown = mod.partition(prs, {"a": 92, "b": 3})
        self.assertEqual([(p["number"], n) for p, n in outdated], [(1, 92), (2, 3)])
        self.assertEqual(current, [])
        self.assertEqual(unknown, [])

    def test_current_prs_are_not_selected(self):
        outdated, current, unknown = mod.partition([pr(1, "a")], {"a": 0})
        self.assertEqual(outdated, [])
        self.assertEqual([p["number"] for p in current], [1])
        self.assertEqual(unknown, [])

    def test_a_failed_comparison_is_reported_not_guessed(self):
        # None means the compare API call failed. The PR must land in
        # `unknown` — neither commented on (might churn a current PR) nor
        # silently dropped (would read as "checked and current").
        outdated, current, unknown = mod.partition([pr(1, "a")], {"a": None})
        self.assertEqual(outdated, [])
        self.assertEqual(current, [])
        self.assertEqual([p["number"] for p in unknown], [1])

    def test_input_order_is_kept(self):
        prs = [pr(3, "c"), pr(1, "a"), pr(2, "b")]
        outdated, _, _ = mod.partition(prs, {"c": 5, "a": 1, "b": 2})
        self.assertEqual([p["number"] for p, _ in outdated], [3, 1, 2])


if __name__ == "__main__":
    unittest.main()
