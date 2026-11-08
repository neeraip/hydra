"""The check that a CLI release builds the library it pins.

The failure it prevents is silent: a CLI whose downloaded binary and
crates.io package run different engines. So the decision is exercised on a
real git history, both ways, not just on its parsing helpers.
"""

import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "cli_release_guard.py"
spec = importlib.util.spec_from_file_location("cli_release_guard", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def write(root, path, text):
    p = root / path
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)


def git(root, *args):
    subprocess.run(
        ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false",
         "-c", "tag.gpgsign=false", *args],
        cwd=root, check=True, capture_output=True,
    )


class Workspace:
    """A minimal workspace: one library crate, the CLI, and the unpublished GUI."""

    def __init__(self, tmp):
        self.root = pathlib.Path(tmp)
        write(self.root, "Cargo.toml",
              '[workspace]\nmembers = [\n  "crates/sdk",\n  "crates/cli",\n  "crates/gui",\n]\n')
        write(self.root, "crates/sdk/Cargo.toml", '[package]\nname = "hydra-sdk"\n')
        write(self.root, "crates/sdk/src/lib.rs", "// v1\n")
        write(self.root, "crates/cli/Cargo.toml",
              '[package]\nname = "hydra-cli"\n\n[dependencies]\n'
              'hydra = { package = "hydra-sdk", path = "../sdk", version = "1.0.0" }\n')
        write(self.root, "crates/gui/Cargo.toml", '[package]\nname = "hydra-gui"\npublish = false\n')
        git(self.root, "init", "-q")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", "init")
        git(self.root, "tag", "v1.0.0")

    def commit(self, path, text):
        write(self.root, path, text)
        git(self.root, "add", "-A")
        git(self.root, "commit", "-qm", f"touch {path}")


class LibraryDirs(unittest.TestCase):
    def test_takes_publishable_members_but_not_the_cli_or_the_gui(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(guard.library_dirs(Workspace(tmp).root), ["crates/sdk"])

    def test_the_real_workspace_covers_the_sdk_and_engines_only(self):
        dirs = guard.library_dirs()
        self.assertIn("crates/sdk", dirs)
        self.assertIn("crates/engine-wds", dirs)
        self.assertNotIn("crates/cli", dirs)
        self.assertNotIn("crates/gui", dirs)


class SdkPin(unittest.TestCase):
    def test_reads_the_aliased_pin(self):
        text = 'hydra = { package = "hydra-sdk", path = "../sdk", version = "17.0.2", features = ["threads"] }\n'
        self.assertEqual(guard.sdk_pin(text), "17.0.2")

    def test_the_real_cli_has_a_pin(self):
        text = (guard.ROOT / "crates/cli/Cargo.toml").read_text()
        self.assertRegex(guard.sdk_pin(text), r"^\d+\.\d+\.\d+$")


class UnreleasedLibraryChanges(unittest.TestCase):
    def test_passes_when_only_the_cli_and_gui_moved(self):
        with tempfile.TemporaryDirectory() as tmp:
            ws = Workspace(tmp)
            ws.commit("crates/cli/src/main.rs", "fn main() {}\n")
            ws.commit("crates/gui/src/lib.rs", "// gui\n")
            git(ws.root, "tag", "cli-v1.1.0")
            self.assertEqual(guard.unreleased_library_changes(ws.root, "v1.0.0", "cli-v1.1.0"), [])

    def test_refuses_when_library_source_moved_after_the_pinned_release(self):
        with tempfile.TemporaryDirectory() as tmp:
            ws = Workspace(tmp)
            ws.commit("crates/sdk/src/lib.rs", "// v1, plus an unreleased fix\n")
            git(ws.root, "tag", "cli-v1.1.0")
            self.assertEqual(
                guard.unreleased_library_changes(ws.root, "v1.0.0", "cli-v1.1.0"),
                ["crates/sdk/src/lib.rs"],
            )

    def test_refuses_a_pin_with_no_release_tag(self):
        with tempfile.TemporaryDirectory() as tmp:
            ws = Workspace(tmp)
            git(ws.root, "tag", "cli-v1.1.0")
            with self.assertRaises(SystemExit):
                guard.unreleased_library_changes(ws.root, "v2.0.0", "cli-v1.1.0")


if __name__ == "__main__":
    unittest.main()
