"""The benchmark script drives the CLI, so it rots when the CLI moves.

It did. `scripts/benchmark.py` was written against the pre-3.0 grammar,
where a bare model path ran a model and the report was a positional. The
CLI grew a `run` subcommand and named outputs, and every invocation in the
script began failing with a usage error. Nothing noticed, because nothing
runs the script but a person asking for a performance table, and the
number it produces is not one anybody sees often enough to miss.

So both sides are asserted here: the grammar the script builds, and the
grammar the CLI declares. Neither test alone would have caught the drift.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "benchmark.py"
CLI_MAIN = ROOT / "crates" / "cli" / "src" / "main.rs"


class TestBenchmarkCli(unittest.TestCase):
    def setUp(self) -> None:
        self.script = SCRIPT.read_text()
        self.cli = CLI_MAIN.read_text()

    def test_script_runs_models_through_the_run_subcommand(self) -> None:
        # Every subprocess invocation of the binary must name `run` first.
        calls = re.findall(r"\[hydra,\s*([^\]]*)\]", self.script, re.S)
        self.assertTrue(calls, "no CLI invocations found in the script")
        for call in calls:
            self.assertIn(
                '"run"',
                call,
                f"invocation does not use the run subcommand: {call.strip()}",
            )

    def test_script_names_its_outputs(self) -> None:
        # The pre-3.0 grammar took the report as a positional; it is now a
        # named argument, and passing a bare path silently means nothing.
        self.assertIn('"--summary"', self.script)
        self.assertNotRegex(
            self.script,
            r"\[hydra,\s*str\(inp\),\s*str\(out\)\]",
            "the report is passed positionally, which the CLI no longer accepts",
        )

    def test_cli_still_declares_that_grammar(self) -> None:
        # The other half of the invariant: if the CLI drops `run` or the
        # summary path, this fails here rather than in a table nobody
        # regenerates.
        self.assertRegex(
            self.cli,
            r"enum Command\b[\s\S]*?\bRun\(",
            "the CLI no longer declares a Run subcommand",
        )
        self.assertRegex(
            self.cli,
            r"summary:\s*Option<String>",
            "the CLI no longer takes a summary path",
        )

    def test_drainage_models_are_generated_not_vendored(self) -> None:
        # The drainage corpus is a generator plus a gitignore, because this
        # repository has no SWMM benchmark suite it can redistribute.
        gen = ROOT / "scripts" / "make_uds_benchmark.py"
        self.assertTrue(gen.exists(), "the drainage benchmark generator is missing")
        ignore = ROOT / "tests" / "benchmarks" / "uds" / ".gitignore"
        self.assertTrue(ignore.exists(), "generated drainage models are not ignored")
        self.assertIn("*.inp", ignore.read_text())


if __name__ == "__main__":
    unittest.main()
