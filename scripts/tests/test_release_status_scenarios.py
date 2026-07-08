import contextlib
import importlib.util
import io
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS_DIR = ROOT / "scripts"


def load_module(filename: str, module_name: str):
    path = SCRIPTS_DIR / filename
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release_status = load_module("release-status.py", "release_status")


class TestReleaseStatusScenarios(unittest.TestCase):
    def run_with_fake_history(self, subjects_by_track, messages_by_track=None, focus=""):
        if messages_by_track is None:
            messages_by_track = subjects_by_track

        patterns = {name: pattern for name, pattern, _, _ in release_status.TRACKS}
        paths_to_name = {tuple(paths): name for name, _, paths, _ in release_status.TRACKS}

        old_latest_tag = release_status.latest_tag
        old_subjects_since = release_status.subjects_since
        old_messages_since = release_status.messages_since
        old_argv = release_status.sys.argv
        old_use_color = release_status.USE_COLOR

        def fake_latest_tag(pattern):
            for name, p in patterns.items():
                if p == pattern:
                    return {
                        "Library": "v1.0.0",
                        "CLI": "cli-v1.0.0",
                        "GUI": "gui-v1.0.0",
                    }[name]
            return None

        def fake_subjects_since(_tag, paths):
            name = paths_to_name[tuple(paths)]
            return subjects_by_track[name]

        def fake_messages_since(_tag, paths):
            name = paths_to_name[tuple(paths)]
            return messages_by_track[name]

        try:
            release_status.latest_tag = fake_latest_tag
            release_status.subjects_since = fake_subjects_since
            release_status.messages_since = fake_messages_since
            release_status.USE_COLOR = False
            release_status.sys.argv = ["release-status.py"] + ([focus] if focus else [])

            out = io.StringIO()
            err = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = release_status.main()
            return code, out.getvalue(), err.getvalue()
        finally:
            release_status.latest_tag = old_latest_tag
            release_status.subjects_since = old_subjects_since
            release_status.messages_since = old_messages_since
            release_status.sys.argv = old_argv
            release_status.USE_COLOR = old_use_color

    def test_library_change_cascades_cli_and_gui(self):
        code, out, err = self.run_with_fake_history(
            {
                "Library": ["fix(engine): adjust matrix assembly"],
                "CLI": [],
                "GUI": [],
            }
        )

        self.assertEqual(code, 0)
        self.assertEqual(err, "")
        self.assertIn("Library  v1.0.0", out)
        self.assertIn("release candidate · 1 commit · own changes", out)
        self.assertIn("CLI  cli-v1.0.0", out)
        self.assertIn("library cascade (no own changes)", out)
        self.assertIn("GUI  gui-v1.0.0", out)
        self.assertIn("Cascade", out)
        self.assertIn("just bump <level>", out)
        self.assertIn("just bump-cli <level>", out)
        self.assertIn("just bump-gui <level>", out)

    def test_standalone_cli_only(self):
        code, out, err = self.run_with_fake_history(
            {
                "Library": [],
                "CLI": ["fix(cli): better error message"],
                "GUI": [],
            }
        )

        self.assertEqual(code, 0)
        self.assertEqual(err, "")
        self.assertIn("Library  v1.0.0", out)
        self.assertIn("up to date", out)
        self.assertIn("CLI  cli-v1.0.0", out)
        self.assertIn("release candidate · 1 commit · own changes", out)
        self.assertIn("GUI  gui-v1.0.0", out)
        self.assertNotIn("Cascade", out)
        self.assertNotIn("just bump <level>", out)
        self.assertIn("just bump-cli <level>", out)
        self.assertNotIn("just bump-gui <level>", out)

    def test_standalone_gui_only(self):
        code, out, err = self.run_with_fake_history(
            {
                "Library": [],
                "CLI": [],
                "GUI": ["fix(gui): tighten validation"],
            }
        )

        self.assertEqual(code, 0)
        self.assertEqual(err, "")
        self.assertIn("Library  v1.0.0", out)
        self.assertIn("CLI  cli-v1.0.0", out)
        self.assertIn("GUI  gui-v1.0.0", out)
        self.assertIn("release candidate · 1 commit · own changes", out)
        self.assertNotIn("Cascade", out)
        self.assertNotIn("just bump <level>", out)
        self.assertNotIn("just bump-cli <level>", out)
        self.assertIn("just bump-gui <level>", out)

    def test_standalone_cli_and_gui(self):
        code, out, err = self.run_with_fake_history(
            {
                "Library": [],
                "CLI": ["feat(cli): add diagnostics flag"],
                "GUI": ["fix(gui): keep panel state"],
            }
        )

        self.assertEqual(code, 0)
        self.assertEqual(err, "")
        self.assertIn("CLI  cli-v1.0.0", out)
        self.assertIn("GUI  gui-v1.0.0", out)
        self.assertNotIn("Cascade", out)
        self.assertNotIn("just bump <level>", out)
        self.assertIn("just bump-cli <level>", out)
        self.assertIn("just bump-gui <level>", out)

    def test_no_changes_anywhere(self):
        code, out, err = self.run_with_fake_history(
            {
                "Library": [],
                "CLI": [],
                "GUI": [],
            }
        )

        self.assertEqual(code, 0)
        self.assertEqual(err, "")
        self.assertIn("Nothing to release", out)
        self.assertNotIn("just bump <level>", out)
        self.assertNotIn("just bump-cli <level>", out)
        self.assertNotIn("just bump-gui <level>", out)

    def test_missing_tag_errors(self):
        patterns = {name: pattern for name, pattern, _, _ in release_status.TRACKS}

        old_latest_tag = release_status.latest_tag
        old_subjects_since = release_status.subjects_since
        old_messages_since = release_status.messages_since
        old_argv = release_status.sys.argv
        old_use_color = release_status.USE_COLOR

        def fake_latest_tag(pattern):
            if pattern == patterns["GUI"]:
                return None
            if pattern == patterns["Library"]:
                return "v1.0.0"
            if pattern == patterns["CLI"]:
                return "cli-v1.0.0"
            return None

        def fake_subjects_since(_tag, _paths):
            return []

        def fake_messages_since(_tag, _paths):
            return []

        try:
            release_status.latest_tag = fake_latest_tag
            release_status.subjects_since = fake_subjects_since
            release_status.messages_since = fake_messages_since
            release_status.USE_COLOR = False
            release_status.sys.argv = ["release-status.py"]

            out = io.StringIO()
            err = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = release_status.main()

            self.assertEqual(code, 1)
            self.assertEqual(out.getvalue(), "")
            self.assertIn("error: no release tags found matching", err.getvalue())
            self.assertIn("gui-v[0-9]*.[0-9]*.[0-9]*", err.getvalue())
        finally:
            release_status.latest_tag = old_latest_tag
            release_status.subjects_since = old_subjects_since
            release_status.messages_since = old_messages_since
            release_status.sys.argv = old_argv
            release_status.USE_COLOR = old_use_color

    def test_focus_filters_output(self):
        history = {
            "Library": ["fix(engine): deterministic solver order"],
            "CLI": ["fix(cli): improve help text"],
            "GUI": ["fix(gui): avoid duplicate rows"],
        }

        code_lib, out_lib, err_lib = self.run_with_fake_history(history, focus="library")
        self.assertEqual(code_lib, 0)
        self.assertEqual(err_lib, "")
        self.assertIn("Library  v1.0.0", out_lib)
        self.assertNotIn("CLI  cli-v1.0.0", out_lib)
        self.assertNotIn("GUI  gui-v1.0.0", out_lib)

        code_cli, out_cli, err_cli = self.run_with_fake_history(history, focus="cli")
        self.assertEqual(code_cli, 0)
        self.assertEqual(err_cli, "")
        self.assertNotIn("Library  v1.0.0", out_cli)
        self.assertIn("CLI  cli-v1.0.0", out_cli)
        self.assertNotIn("GUI  gui-v1.0.0", out_cli)

        code_gui, out_gui, err_gui = self.run_with_fake_history(history, focus="gui")
        self.assertEqual(code_gui, 0)
        self.assertEqual(err_gui, "")
        self.assertNotIn("Library  v1.0.0", out_gui)
        self.assertNotIn("CLI  cli-v1.0.0", out_gui)
        self.assertIn("GUI  gui-v1.0.0", out_gui)


if __name__ == "__main__":
    unittest.main()
