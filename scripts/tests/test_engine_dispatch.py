"""Every engine dispatch names its engines and refuses the rest.

A project's engine key lives in its own metadata, so a build can open a
project written by a build that knew one more engine than it does. What
the GUI did with that key fell into two habits, and both were wrong in a
way nothing reported:

  * the reads answered an unknown engine with an empty model, which looks
    exactly like a project someone emptied, and
  * every report command treated "not uds" as "wds", handing a model this
    build cannot read to the water engine's reader.

The write paths always refused it outright. This makes the rest match, and
keeps them matching: a bare `_` arm on an engine key is the shape of both
habits, so no dispatch may have one.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
COMMANDS = ROOT / "crates" / "gui" / "src" / "commands"

DISPATCH = re.compile(r"project_engine_key\([^;]*?\)\s*\.as_str\(\)\s*\{")
# A local already read from `project_engine_key`, matched on further down.
BOUND = re.compile(r"let\s+engine\s*=[^;]*?project_engine_key")
LOCAL_DISPATCH = re.compile(r"match\s+engine(?:\.as_str\(\))?\s*\{")

# Dispatches on an engine key the *caller* supplied, rather than one read
# from a project, are catalog lookups: `list_element_kinds` asked for a
# key it does not know has no project to be wrong about, and answers with
# an empty catalog. Only a key read from a project's own metadata is in
# scope here.


def match_block(text: str, open_brace: int) -> tuple[str, int]:
    """The source between a match's braces, and where it ends."""
    depth, i = 1, open_brace
    while i < len(text) and depth:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
        i += 1
    return text[open_brace:i], i


def dispatches() -> list[tuple[pathlib.Path, int, str]]:
    """Every match on an engine key read from a project, outside tests."""
    found = []
    for path in sorted(COMMANDS.glob("*.rs")):
        text = path.read_text()
        # Test modules build both engines' fixtures on purpose.
        cut = text.find("#[cfg(test)]")
        if cut >= 0:
            text = text[:cut]
        starts = [m.end() for m in DISPATCH.finditer(text)]
        for m in LOCAL_DISPATCH.finditer(text):
            bound = [b.end() for b in BOUND.finditer(text) if b.end() < m.start()]
            if bound and text.count("\nfn ", bound[-1], m.start()) == 0:
                starts.append(m.end())
        for start in sorted(starts):
            body, _end = match_block(text, start)
            found.append((path, text[:start].count("\n") + 1, body))
    return found


def wildcard_is_explained(body: str) -> bool:
    """A `_` arm that refuses, or that says in a comment why it does not."""
    for i, raw in enumerate(body.splitlines()):
        if not re.match(r"\s*_\s*=>", raw):
            continue
        arm = body.splitlines()[i:]
        if any("Err(" in line for line in arm[:4]):
            return True
        before = body.splitlines()[:i]
        return bool(before and before[-1].strip().startswith("//"))
    return True


def top_level_arms(body: str) -> list[str]:
    """The arm patterns of this match, ignoring any match nested inside one."""
    # `body` starts just inside the match's brace, so its own arms sit at
    # depth zero and anything deeper belongs to a match nested in one.
    arms, depth = [], 0
    for raw in body.splitlines():
        stripped = raw.strip()
        if depth == 0:
            m = re.match(r'(_|other|"[a-z]+"(?:\s*\|\s*"[a-z]+")*)\s*=>', stripped)
            if m:
                arms.append(m.group(1))
        depth += raw.count("{") - raw.count("}")
    return arms


class EngineDispatchTests(unittest.TestCase):
    def test_no_dispatch_falls_through_on_a_bare_wildcard(self):
        offences = []
        for path, line, body in dispatches():
            if "_" in top_level_arms(body) and not wildcard_is_explained(body):
                offences.append(f"{path.relative_to(ROOT)}:{line}")
        self.assertEqual(
            [],
            offences,
            "a bare `_` arm on an engine key either serves an unknown engine "
            "an empty model or hands it to whichever engine the arm happens "
            'to implement; name each engine and refuse the rest with `other =>`, '
            "or say in a comment on the arm why an unknown engine is handled "
            "the way it is",
        )

    def test_every_dispatch_names_both_engines(self):
        offences = []
        for path, line, body in dispatches():
            arms = set(top_level_arms(body))
            if not {'"wds"', '"uds"'} <= arms:
                offences.append(f"{path.relative_to(ROOT)}:{line}: {sorted(arms)}")
        self.assertEqual(
            [],
            offences,
            "an engine dispatch that names only one engine is deciding the "
            "other's behaviour by omission",
        )

    def test_the_refusal_is_worded_in_exactly_one_place(self):
        # Six sites used to say "no editing surface for engine 'x'", which
        # reads as though the engine exists and merely cannot be edited,
        # while the reads said something else again. One condition, one
        # sentence, so the two cannot come to disagree.
        sources = list(COMMANDS.glob("*.rs")) + [COMMANDS.parent / "main.rs"]
        defs = sum(s.read_text().count("pub(crate) fn unknown_engine") for s in sources)
        self.assertEqual(1, defs, "unknown_engine should be defined once")
        for s in sources:
            self.assertNotIn(
                "no editing surface for engine",
                s.read_text(),
                f"{s.name} words this refusal for itself; call unknown_engine",
            )

    def test_the_scan_finds_the_dispatches_that_exist(self):
        # A regex that stopped matching would pass every assertion above.
        self.assertGreater(len(dispatches()), 15)


if __name__ == "__main__":
    unittest.main()
