import importlib.util
import pathlib
import tempfile
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


release = load_module("_release.py", "_release")


class TestReleaseHelpers(unittest.TestCase):
    def test_parse_level_valid(self):
        self.assertEqual(release.parse_level("patch"), "patch")
        self.assertEqual(release.parse_level("minor"), "minor")
        self.assertEqual(release.parse_level("major"), "major")

    def test_parse_level_invalid_raises(self):
        with self.assertRaises(SystemExit) as ctx:
            release.parse_level("bogus")
        self.assertEqual(ctx.exception.code, 1)

    def test_next_version(self):
        self.assertEqual(release.next_version("1.2.3", "patch"), "1.2.4")
        self.assertEqual(release.next_version("1.2.3", "minor"), "1.3.0")
        self.assertEqual(release.next_version("1.2.3", "major"), "2.0.0")

    def test_read_and_set_version_roundtrip(self):
        content = '[package]\nname = "foo"\nversion = "1.2.3"\n'
        with tempfile.NamedTemporaryFile("w+", suffix=".toml", delete=True) as tmp:
            tmp.write(content)
            tmp.flush()
            path = pathlib.Path(tmp.name)

            self.assertEqual(release.read_version(path), "1.2.3")
            release.set_version(path, "1.3.0")
            self.assertEqual(release.read_version(path), "1.3.0")

    def test_read_version_missing_raises(self):
        content = '[package]\nname = "foo"\n'
        with tempfile.NamedTemporaryFile("w+", suffix=".toml", delete=True) as tmp:
            tmp.write(content)
            tmp.flush()
            path = pathlib.Path(tmp.name)

            with self.assertRaises(SystemExit) as ctx:
                release.read_version(path)
            self.assertEqual(ctx.exception.code, 1)


if __name__ == "__main__":
    unittest.main()
