"""The pin rewrites release_sync.py applies to a release branch.

release-please moves the version numbers; these rewrites move everything that
must follow them. A pin left behind fails at publish time, after earlier
crates are already live, so each rewrite is asserted on the shapes the
manifests actually use.
"""

import importlib.util
import pathlib
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "release_sync.py"
spec = importlib.util.spec_from_file_location("release_sync", SCRIPT)
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


class RewritePins(unittest.TestCase):
    def test_moves_a_plain_path_pin(self):
        text = 'hydra-common = { path = "../common", version = "17.0.1" }\n'
        self.assertEqual(
            sync.rewrite_pins(text, "18.0.0"),
            'hydra-common = { path = "../common", version = "18.0.0" }\n',
        )

    def test_moves_the_cli_pin_held_under_an_alias(self):
        # The CLI names the SDK `hydra`; the package name is what identifies it.
        text = 'hydra = { package = "hydra-sdk", path = "../sdk", version = "17.0.1", features = ["threads"] }\n'
        self.assertIn('version = "17.1.0"', sync.rewrite_pins(text, "17.1.0"))

    def test_leaves_third_party_versions_alone(self):
        text = 'serde = { version = "1.0.200" }\nhydra-report = { path = "../report", version = "17.0.1" }\n'
        out = sync.rewrite_pins(text, "17.0.2")
        self.assertIn('serde = { version = "1.0.200" }', out)
        self.assertIn('hydra-report = { path = "../report", version = "17.0.2" }', out)


class RewriteDocPin(unittest.TestCase):
    def test_documents_only_the_major(self):
        out = sync.rewrite_doc_pin('[dependencies]\nhydra-sdk = "17"\n', "18.2.3", "README.md")
        self.assertIn('hydra-sdk = "18"', out)

    def test_refuses_a_doc_without_exactly_one_pin(self):
        for text in ("no pin here\n", 'hydra-sdk = "17"\nhydra-sdk = "17"\n'):
            with self.assertRaises(SystemExit):
                sync.rewrite_doc_pin(text, "18.0.0", "README.md")


class WorkspaceVersion(unittest.TestCase):
    def test_reads_the_workspace_package_version_not_a_dependency(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "Cargo.toml").write_text(
                '[workspace]\nmembers = []\n\n[workspace.dependencies]\nfoo = { version = "9.9.9" }\n\n'
                '[workspace.package]\nedition = "2021"\nversion = "17.4.0"\n'
            )
            self.assertEqual(sync.workspace_version(root), "17.4.0")

    def test_the_real_workspace_has_a_version(self):
        self.assertRegex(sync.workspace_version(), r"^\d+\.\d+\.\d+$")


if __name__ == "__main__":
    unittest.main()
