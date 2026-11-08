"""Which CI a change runs.

A wrong "skip" lets a broken change through with green checks, so every
cross-area dependency is asserted in the direction that matters: a change
on one side runs the other side's checks.
"""

import importlib.util
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci_changes.py"
spec = importlib.util.spec_from_file_location("ci_changes", SCRIPT)
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)


def needs(*paths):
    return ci.classify(list(paths))


class CrossAreaDependencies(unittest.TestCase):
    def test_a_frontend_file_the_rust_tests_read_runs_the_gui_tests(self):
        for path in ci.RUST_READS_FRONTEND:
            self.assertEqual(needs(path), {"rust": "gui", "frontend": "true"}, path)

    def test_the_list_of_frontend_files_rust_reads_is_complete(self):
        # Every frontend path named in the gui crate's Rust source.
        named = set()
        for rs in (ROOT / "crates" / "gui" / "src").rglob("*.rs"):
            for m in re.finditer(r"frontend/([A-Za-z0-9_./-]+\.(?:ts|tsx|json|css))", rs.read_text()):
                named.add("crates/gui/frontend/" + m.group(1))
        self.assertEqual(named, ci.RUST_READS_FRONTEND)

    def test_fixtures_the_frontend_reads_run_both(self):
        self.assertEqual(needs("tests/fixtures/period-results.bin"), {"rust": "full", "frontend": "true"})

    def test_the_crs_catalog_runs_both(self):
        self.assertEqual(needs("crates/gui/resources/crs-catalog.json"), {"rust": "full", "frontend": "true"})

    def test_the_embedded_licence_texts_run_rust(self):
        for path in ("LICENSE", "COMMERCIAL_LICENSE.md"):
            self.assertEqual(needs(path)["rust"], "full", path)


class Skips(unittest.TestCase):
    def test_a_frontend_only_change_skips_rust(self):
        self.assertEqual(needs("crates/gui/frontend/src/App.tsx", "crates/gui/frontend/package.json"),
                         {"rust": "none", "frontend": "true"})

    def test_a_rust_only_change_skips_the_frontend(self):
        self.assertEqual(needs("crates/engine-wds/src/lib.rs", "Cargo.lock", "tests/fixtures/wds/a.inp"),
                         {"rust": "full", "frontend": "false"})

    def test_docs_site_and_other_workflows_skip_both(self):
        self.assertEqual(needs("docs/src/intro.md", "site/index.html", "RELEASING.md",
                               ".github/workflows/release.yml", "scripts/release_sync.py"),
                         {"rust": "none", "frontend": "false"})


class NeverSkipWhatMatters(unittest.TestCase):
    def test_a_readme_inside_a_crate_is_rust(self):
        # Crate READMEs can be compiled into docs and doctested.
        self.assertEqual(needs("crates/sdk/README.md")["rust"], "full")

    def test_the_ci_workflows_run_their_own_checks(self):
        self.assertEqual(needs(".github/workflows/cargo-ci.yml")["rust"], "full")
        self.assertEqual(needs(".github/workflows/pnpm-ci.yml")["frontend"], "true")
        self.assertEqual(needs(".github/actions/tauri-deps/action.yml")["rust"], "full")

    def test_the_decision_itself_runs_everything(self):
        for path in ("scripts/ci_changes.py", ".github/actions/ci-changes/action.yml"):
            self.assertEqual(needs(path), {"rust": "full", "frontend": "true"}, path)

    def test_the_crs_catalog_generator_runs_the_frontend(self):
        self.assertEqual(needs("scripts/update-crs-catalog.mjs")["frontend"], "true")

    def test_every_script_a_ci_workflow_runs_is_classified(self):
        # A script CI runs must not fall into the inert scripts/* bucket,
        # or changing it would skip the very workflow that runs it.
        for wf in ("cargo-ci.yml", "pnpm-ci.yml"):
            text = (ROOT / ".github" / "workflows" / wf).read_text()
            for m in set(re.findall(r"scripts/[A-Za-z0-9_.-]+\.(?:py|mjs|sh)", text)):
                self.assertNotEqual(ci.area(m), "inert", f"{m} is run by {wf}")

    def test_the_browser_test_helper_runs_rust(self):
        self.assertEqual(needs("scripts/wasm_chromedriver.py")["rust"], "full")

    def test_an_unrecognised_path_runs_everything(self):
        self.assertEqual(needs("somewhere/new.txt"), {"rust": "full", "frontend": "true"})

    def test_unknown_paths_run_everything(self):
        self.assertEqual(ci.classify(None), {"rust": "full", "frontend": "true"})

    def test_one_rust_file_among_docs_runs_rust(self):
        self.assertEqual(needs("docs/a.md", "crates/report/src/lib.rs")["rust"], "full")


if __name__ == "__main__":
    unittest.main()
