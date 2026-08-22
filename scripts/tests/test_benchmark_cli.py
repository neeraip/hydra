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

    def test_the_drainage_benchmark_runs_in_the_predecessor_too(self) -> None:
        # It did not, for its whole life. Every discharging structure at
        # the basin pointed at one outfall, which the predecessor refuses
        # ("more than 1 inlet link"), and it refuses by writing ERROR into
        # its report and exiting zero. So a comparison harness that
        # trusted the exit code timed a refusal against a real run and
        # read it as a 400x win.
        gen = (ROOT / "scripts" / "make_uds_benchmark.py").read_text()
        for link in ("W_OVER", "O_LOW", "P_LIFT"):
            m = re.search(rf'add\("{link}\s+\S+\s+(\S+)', gen)
            self.assertIsNotNone(m, f"{link} missing from the generator")
            self.assertTrue(
                m.group(1).startswith("OUT_"),
                f"{link} discharges to {m.group(1)}, which is shared",
            )

    def test_the_performance_baseline_names_models_that_exist(self) -> None:
        # The baseline is only a gate while the models it names are the
        # ones the generator writes.
        import json

        models = json.loads((ROOT / "tests" / "benchmarks" / "uds" / "models.json").read_text())
        base = json.loads((ROOT / "tests" / "benchmarks" / "uds" / "baseline.json").read_text())
        self.assertEqual(sorted(models), sorted(base), "baseline and model list disagree")
        gen = (ROOT / "scripts" / "make_uds_benchmark.py").read_text()
        sizes = set(re.findall(r'^\s*"(\w)": \(', gen, re.M))
        for name in models:
            self.assertIn(name.split("_")[-1], sizes, f"{name} is not a size the generator writes")
            self.assertIn("hydra", base[name], f"{name} has no recorded time")

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
