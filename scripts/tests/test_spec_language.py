"""Specifications describe behaviour, not the program that implements it.

AGENTS.md: specs are language- and platform-agnostic, with no references
to Rust, crates, or file layouts. The rule had drifted a long way. Every
spec but two opened by naming its own crate in the title, and the bodies
carried twenty-two more references: the analysis sub-spec alone had
thirteen, and several sentences described which crate owns which module,
which is a fact about the source tree rather than about the domain.

The distinction that matters is between citing another *specification*
and naming the *crate* that happens to carry it. "the foundation contract
§7.4" is a citation; "hydra-common §7.4" is the same citation written in
terms of the build. The first survives a reorganisation of the workspace,
which is the whole point of the rule.

Relative links between spec documents are left alone. Their text names
the document and only the href is a path, so they read as citations and
stay navigable in the rendered docs.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

CRATE = re.compile(r"\bhydra-(?:common|report|sdk|cli|gui|demo|engines|engine-[a-z]+)\b")
LANGUAGE = re.compile(r"\bRust\b|\bcargo\b|\brustdoc\b")
# `struct` inside `construct` is not a type declaration.
DECLARATION = re.compile(r"(?<![a-z])(?:pub fn|impl |#\[derive)")


def specs() -> list[pathlib.Path]:
    return sorted(ROOT.glob("crates/*/src/**/spec.md")) + sorted(
        ROOT.glob("crates/*/src/spec.md")
    )


def offences(pattern: re.Pattern[str]) -> list[str]:
    found = []
    for path in specs():
        for n, line in enumerate(path.read_text().splitlines(), start=1):
            if pattern.search(line):
                rel = path.relative_to(ROOT)
                found.append(f"{rel}:{n}: {line.strip()[:90]}")
    return found


class SpecLanguageTests(unittest.TestCase):
    def test_no_spec_names_a_crate(self):
        self.assertEqual(
            [],
            offences(CRATE),
            "a spec is language- and platform-agnostic; cite the document "
            "(\"the foundation contract §7.4\") rather than the crate",
        )

    def test_no_spec_names_the_implementation_language(self):
        self.assertEqual([], offences(LANGUAGE))

    def test_no_spec_declares_a_type_or_function(self):
        self.assertEqual([], offences(DECLARATION))

    def test_every_spec_title_names_a_domain(self):
        # The heading is where this drifted furthest: thirteen of fifteen
        # opened with their own crate name.
        for path in specs():
            first = path.read_text().splitlines()[0]
            self.assertTrue(first.startswith("# "), f"{path}: no H1 heading")
            self.assertNotRegex(first, CRATE, f"{path}: title names a crate")

    def test_the_scan_reads_every_spec(self):
        found = {p.relative_to(ROOT).as_posix() for p in specs()}
        self.assertEqual(16, len(found), sorted(found))
        for expected in (
            "crates/common/src/spec.md",
            "crates/engine-uds/src/hydraulics/spec.md",
            "crates/engine-wds/src/analysis/spec.md",
            "crates/report/src/spec.md",
        ):
            self.assertIn(expected, found)


if __name__ == "__main__":
    unittest.main()
