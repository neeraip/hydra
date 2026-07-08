import importlib.util
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


class TestReleaseStatusSignal(unittest.TestCase):
    def test_signal_major_from_bang(self):
        msgs = ["feat(api)!: break a public API"]
        self.assertEqual(release_status.signal(msgs), "major")

    def test_signal_major_from_breaking_change_footer(self):
        msgs = ["chore: cleanup\n\nBREAKING CHANGE: incompatible output format"]
        self.assertEqual(release_status.signal(msgs), "major")

    def test_signal_minor_from_feat(self):
        msgs = ["feat(cli): add new switch"]
        self.assertEqual(release_status.signal(msgs), "minor")

    def test_signal_none_without_feat_or_breaking(self):
        msgs = ["fix: adjust timeout", "docs: update README"]
        self.assertEqual(release_status.signal(msgs), "none")


if __name__ == "__main__":
    unittest.main()
