import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS_DIR = ROOT / "scripts"


class TestScriptCliGuards(unittest.TestCase):
    def run_script(self, script_name: str, *args: str):
        script = SCRIPTS_DIR / script_name
        return subprocess.run(
            [sys.executable, str(script), *args],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_bump_rejects_invalid_level(self):
        cp = self.run_script("bump.py", "bogus")
        self.assertNotEqual(cp.returncode, 0)
        self.assertIn("invalid bump level", cp.stderr)

    def test_bump_cli_rejects_invalid_level(self):
        cp = self.run_script("bump-cli.py", "bogus")
        self.assertNotEqual(cp.returncode, 0)
        self.assertIn("invalid bump level", cp.stderr)

    def test_bump_gui_rejects_invalid_level(self):
        cp = self.run_script("bump-gui.py", "bogus")
        self.assertNotEqual(cp.returncode, 0)
        self.assertIn("invalid bump level", cp.stderr)

    def test_release_status_rejects_unknown_track(self):
        cp = self.run_script("release-status.py", "not-a-track")
        self.assertNotEqual(cp.returncode, 0)
        self.assertIn("unknown track", cp.stderr)


if __name__ == "__main__":
    unittest.main()
