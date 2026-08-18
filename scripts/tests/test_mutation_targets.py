"""Which edit commands cannot check the model they are editing.

The GUI keeps one loaded network at a time. The frontend sets
`activeProjectId` before that project's model has finished loading, so
there is a window in which an edit carries the new project's id while the
cache still holds the previous project's network. `save_project` has
always refused to write across that gap, so nothing wrong reaches disk.
The edit itself was not refused: it changed the wrong model in memory and
returned as though it had worked.

`mutate_wds` and `mutate_uds` now take the project the edit is for, and
the check runs under the same lock the mutation does. Five commands
cannot be given a project id without changing their argument lists on
both sides of the IPC boundary, and one of them, `delete_element`, is
replayed by the undo stack. Those pass `None` and go unchecked.

That set is a decision, not an oversight, so it is written down here. A
sixth command joining it silently is the thing this catches.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
MUTATIONS = ROOT / "crates" / "gui" / "src" / "commands" / "mutations.rs"

# A mutation with no project to check itself against.
UNTARGETED = re.compile(r"\b(mutate_structural|mutate_uds_inner)\(\s*&?app,\s*&?state,\s*None\b")

# Edit commands that are given no project id, so pass `None`.
KNOWN_UNTARGETED = {
    "create_node",
    "create_link",
    "delete_element",
    "rename_element",
    "update_network_title",
}


def enclosing_fn(text: str, offset: int) -> str | None:
    """Name of the item this offset sits in."""
    best = None
    for m in re.finditer(r"^(?:pub(?:\(crate\))? )?(?:async )?fn ([a-z_0-9]+)", text, re.M):
        if m.start() > offset:
            break
        best = m.group(1)
    return best


class MutationTargetTests(unittest.TestCase):
    def test_only_the_known_commands_edit_without_a_target(self):
        text = MUTATIONS.read_text()
        # Test modules build states directly and check them, not commands.
        cut = text.find("#[cfg(test)]")
        if cut >= 0:
            text = text[:cut]
        callers = {enclosing_fn(text, m.start()) for m in UNTARGETED.finditer(text)}
        callers.discard(None)
        self.assertEqual(
            KNOWN_UNTARGETED & callers,
            callers,
            "this command edits the loaded network without saying which "
            "project it is for, so nothing can check it is the right one; "
            "route it through mutate_wds/mutate_uds with a project id, or "
            "add it here and say why it cannot have one",
        )

    def test_the_targeted_wrappers_still_check(self):
        text = MUTATIONS.read_text()
        for wrapper in ("mutate_wds", "mutate_uds"):
            m = re.search(rf"pub\(crate\) fn {wrapper}<F>\((.|\n)*?\n{{0}}", text)
            self.assertIsNotNone(m, wrapper)
            start = text.index(f"pub(crate) fn {wrapper}<F>")
            body = text[start : start + 900]
            self.assertIn("project_id: &str", body, f"{wrapper} takes no target")
            self.assertIn("Some(project_id)", body, f"{wrapper} does not pass it on")

    def test_the_check_runs_under_the_lock_that_mutates(self):
        # Checking under one lock and mutating under the next reopens the
        # window the check exists to close: commands run on a thread pool.
        text = MUTATIONS.read_text()
        for wrapper, apply in (
            ("mutate_structural", "apply_structural_mutation"),
            ("mutate_uds_inner", "apply_uds_mutation"),
        ):
            body = text[text.index(f"fn {wrapper}<F>") :][:1200]
            lock = body.index("state.0.lock()")
            self.assertLess(lock, body.index("check_owner("), wrapper)
            self.assertLess(body.index("check_owner("), body.index(apply), wrapper)
            self.assertNotIn("drop(guard)", body[lock : body.index(apply)])

    def test_the_scan_finds_the_calls_that_exist(self):
        # A regex that stopped matching would pass every assertion above.
        # Seven call sites across five commands: the two that serve both
        # engines reach the path once per engine.
        text = MUTATIONS.read_text()
        self.assertEqual(len(UNTARGETED.findall(text)), 7)


if __name__ == "__main__":
    unittest.main()
