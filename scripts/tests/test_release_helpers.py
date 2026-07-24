import contextlib
import importlib.util
import io
import pathlib
import sys
import tempfile
import unittest
import urllib.error
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPTS_DIR = ROOT / "scripts"

# bump-cli.py does `from _release import …` at import time.
sys.path.insert(0, str(SCRIPTS_DIR))


def load_module(filename: str, module_name: str):
    path = SCRIPTS_DIR / filename
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release = load_module("_release.py", "_release")
bump_cli = load_module("bump-cli.py", "bump_cli")


class TestReleaseHelpers(unittest.TestCase):
    def test_parse_level_valid(self):
        self.assertEqual(release.parse_level("patch"), "patch")
        self.assertEqual(release.parse_level("minor"), "minor")
        self.assertEqual(release.parse_level("major"), "major")

    def test_parse_level_invalid_raises(self):
        with self.assertRaises(SystemExit) as ctx:
            release.parse_level("bogus")
        self.assertEqual(ctx.exception.code, 1)

    def test_parse_level_arg_single(self):
        self.assertEqual(release.parse_level_arg(["minor"]), "minor")

    def test_parse_level_arg_empty_raises(self):
        with self.assertRaises(SystemExit) as ctx:
            release.parse_level_arg([])
        self.assertEqual(ctx.exception.code, 1)

    def test_parse_level_arg_extras_raise(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            with self.assertRaises(SystemExit) as ctx:
                release.parse_level_arg(["patch", "stray"])
        self.assertEqual(ctx.exception.code, 1)
        self.assertIn("unexpected extra argument(s): stray", err.getvalue())

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

    def test_parse_push_pref_no_flag(self):
        args, push_pref = release.parse_push_pref(["patch"])
        self.assertEqual(args, ["patch"])
        self.assertIsNone(push_pref)

    def test_parse_push_pref_push(self):
        args, push_pref = release.parse_push_pref(["minor", "--push"])
        self.assertEqual(args, ["minor"])
        self.assertTrue(push_pref)

    def test_parse_push_pref_no_push(self):
        args, push_pref = release.parse_push_pref(["--no-push", "major"])
        self.assertEqual(args, ["major"])
        self.assertFalse(push_pref)

    def test_parse_push_pref_conflicting_flags_raises(self):
        with self.assertRaises(SystemExit) as ctx:
            release.parse_push_pref(["patch", "--push", "--no-push"])
        self.assertEqual(ctx.exception.code, 1)

    def test_maybe_push_yes(self):
        calls = []

        def fake_sh(*args, **kwargs):
            calls.append((args, kwargs))

            class R:
                stdout = ""

            return R()

        with mock.patch.object(release, "sh", new=fake_sh):
            with contextlib.redirect_stdout(io.StringIO()):
                release.maybe_push(True)

        self.assertEqual(calls[0][0], ("git", "push"))
        self.assertEqual(calls[1][0], ("git", "push", "--tags"))

    def test_maybe_push_no(self):
        calls = []

        def fake_sh(*args, **kwargs):
            calls.append((args, kwargs))

            class R:
                stdout = ""

            return R()

        with mock.patch.object(release, "sh", new=fake_sh):
            with contextlib.redirect_stdout(io.StringIO()):
                release.maybe_push(False)

        self.assertEqual(calls, [])

    def test_maybe_push_prompt_yes(self):
        calls = []

        def fake_sh(*args, **kwargs):
            calls.append((args, kwargs))

            class R:
                stdout = ""

            return R()

        with mock.patch.object(release, "sh", new=fake_sh):
            with mock.patch("builtins.input", return_value="y"):
                with contextlib.redirect_stdout(io.StringIO()):
                    release.maybe_push(None)

        self.assertEqual(calls[0][0], ("git", "push"))
        self.assertEqual(calls[1][0], ("git", "push", "--tags"))


class TestEnsureSdkPublished(unittest.TestCase):
    CLI_TOML = 'hydra = { package = "hydra-sdk", path = "../sdk", version = "1.2.3" }\n'

    def write_toml(self, content):
        tmp = tempfile.NamedTemporaryFile("w+", suffix=".toml", delete=False)
        self.addCleanup(pathlib.Path(tmp.name).unlink)
        tmp.write(content)
        tmp.flush()
        return pathlib.Path(tmp.name)

    def test_published_version_passes(self):
        path = self.write_toml(self.CLI_TOML)
        with mock.patch("urllib.request.urlopen", return_value=io.BytesIO(b"{}")):
            bump_cli.ensure_sdk_published(path)  # must not raise

    def test_missing_pin_is_a_no_op(self):
        path = self.write_toml('[package]\nname = "hydra-cli"\n')
        with mock.patch("urllib.request.urlopen", side_effect=AssertionError("must not be called")):
            bump_cli.ensure_sdk_published(path)

    def test_unpublished_version_fails_with_guidance(self):
        path = self.write_toml(self.CLI_TOML)
        err404 = urllib.error.HTTPError("url", 404, "Not Found", None, None)
        err = io.StringIO()
        with mock.patch("urllib.request.urlopen", side_effect=err404):
            with contextlib.redirect_stderr(err):
                with self.assertRaises(SystemExit) as ctx:
                    bump_cli.ensure_sdk_published(path)
        self.assertEqual(ctx.exception.code, 1)
        self.assertIn("hydra-sdk 1.2.3 is not yet on crates.io", err.getvalue())

    def test_server_error_fails_without_claiming_unpublished(self):
        path = self.write_toml(self.CLI_TOML)
        err500 = urllib.error.HTTPError("url", 500, "Server Error", None, None)
        err = io.StringIO()
        with mock.patch("urllib.request.urlopen", side_effect=err500):
            with contextlib.redirect_stderr(err):
                with self.assertRaises(SystemExit) as ctx:
                    bump_cli.ensure_sdk_published(path)
        self.assertEqual(ctx.exception.code, 1)
        self.assertIn("could not verify hydra-sdk 1.2.3", err.getvalue())
        self.assertNotIn("not yet on crates.io", err.getvalue())

    def test_network_failure_fails_cleanly(self):
        path = self.write_toml(self.CLI_TOML)
        neterr = urllib.error.URLError("connection refused")
        err = io.StringIO()
        with mock.patch("urllib.request.urlopen", side_effect=neterr):
            with contextlib.redirect_stderr(err):
                with self.assertRaises(SystemExit) as ctx:
                    bump_cli.ensure_sdk_published(path)
        self.assertEqual(ctx.exception.code, 1)
        self.assertIn("could not reach crates.io", err.getvalue())


if __name__ == "__main__":
    unittest.main()
