"""Which comment, if any, supplies a release's notes.

Picking up a stranger's comment would publish it as Hydra's release notes;
missing a maintainer's would publish the raw changelog. Both directions are
asserted.
"""

import importlib.util
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "release_notes.py"
spec = importlib.util.spec_from_file_location("release_notes", SCRIPT)
rn = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rn)


def comment(body, created="2026-11-08T10:00:00Z", who="OWNER"):
    return {"body": body, "created_at": created, "author_association": who}


class NotesOf(unittest.TestCase):
    def test_takes_everything_under_the_heading(self):
        self.assertEqual(rn.notes_of("# Release Notes\n\n- Faster runs\n\n## Fixes\n- One"),
                         "- Faster runs\n\n## Fixes\n- One")

    def test_heading_must_come_first(self):
        self.assertIsNone(rn.notes_of("Draft below\n# Release Notes\n- Faster runs"))

    def test_leading_blank_lines_and_crlf_are_fine(self):
        self.assertEqual(rn.notes_of("\r\n\r\n# Release Notes\r\n- Faster runs\r\n"), "- Faster runs")

    def test_heading_case_and_trailing_space_are_ignored(self):
        self.assertEqual(rn.notes_of("# release notes  \n- Faster runs"), "- Faster runs")

    def test_other_headings_do_not_count(self):
        for body in ("## Release Notes\n- x", "# Release Notes draft\n- x", "#Release Notes\n- x",
                     "# Notes\n- x", "Release Notes\n- x"):
            self.assertIsNone(rn.notes_of(body), body)

    def test_a_heading_with_nothing_under_it_is_not_notes(self):
        self.assertIsNone(rn.notes_of("# Release Notes\n\n   \n"))


class ChosenNotes(unittest.TestCase):
    def test_none_without_a_notes_comment(self):
        self.assertIsNone(rn.chosen_notes([comment("LGTM"), comment(":robot: Release is at ...")]))

    def test_ignores_people_without_write_access(self):
        for who in ("CONTRIBUTOR", "FIRST_TIME_CONTRIBUTOR", "NONE", None):
            self.assertIsNone(rn.chosen_notes([comment("# Release Notes\n- spam", who=who)]), who)

    def test_trusts_owners_members_and_collaborators(self):
        for who in ("OWNER", "MEMBER", "COLLABORATOR"):
            self.assertEqual(rn.chosen_notes([comment("# Release Notes\n- x", who=who)]), "- x", who)

    def test_newest_qualifying_comment_wins(self):
        comments = [
            comment("# Release Notes\n- second", created="2026-11-08T12:00:00Z"),
            comment("# Release Notes\n- first", created="2026-11-08T09:00:00Z"),
            comment("# Release Notes\n- stranger", created="2026-11-08T13:00:00Z", who="NONE"),
            comment("thanks", created="2026-11-08T14:00:00Z"),
        ]
        self.assertEqual(rn.chosen_notes(comments), "- second")

    def test_a_missing_body_is_skipped(self):
        self.assertIsNone(rn.chosen_notes([{"body": None, "created_at": "x", "author_association": "OWNER"}]))


if __name__ == "__main__":
    unittest.main()
