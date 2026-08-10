"""The notices generator's decisions, on input it cannot get from a machine.

Everything asserted here is a judgement `scripts/licenses.py` makes about
what belongs in the notices and what a package's licence *is* — which
dependency edges to follow, which files count, when two texts are one. Run
against the real dependency graph these would be assertions about the
crates.io ecosystem on the day they ran; run against a fixture they are
assertions about the script.

The dev-dependency case is the one worth stating out loud: notices are a
distribution obligation, and a file listing packages nobody receives is
both wrong and impossible to check, since the test-only tree changes
whenever a test does.
"""

import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

spec = importlib.util.spec_from_file_location("licenses", ROOT / "scripts" / "licenses.py")
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)


def pkg(name, version="1.0.0", **over):
    """A `cargo metadata` package entry, cut down to what the script reads."""
    return {
        "id": f"{name} {version}",
        "name": name,
        "version": version,
        "license": over.get("license", "MIT"),
        "repository": over.get("repository", f"https://example.com/{name}"),
        "manifest_path": f"/registry/{name}-{version}/Cargo.toml",
        "source": over.get("source", "registry+https://github.com/rust-lang/crates.io-index"),
    }


def node(name, version="1.0.0", deps=()):
    """A resolve node. `deps` is a list of (id, kind) — None being normal."""
    return {
        "id": f"{name} {version}",
        "deps": [{"pkg": dep, "dep_kinds": [{"kind": kind}]} for dep, kind in deps],
    }


class TestRustComponents(unittest.TestCase):
    def metadata(self):
        return {
            "packages": [
                # The workspace member everything hangs off, and a sibling
                # workspace crate it depends on.
                pkg("hydra-gui", source=None),
                pkg("hydra-sdk", source=None),
                pkg("serde"),
                pkg("proptest"),
                pkg("cc"),
                pkg("indirect"),
            ],
            "resolve": {
                "nodes": [
                    node(
                        "hydra-gui",
                        deps=[
                            ("serde 1.0.0", None),
                            ("proptest 1.0.0", "dev"),
                            ("cc 1.0.0", "build"),
                            ("hydra-sdk 1.0.0", None),
                        ],
                    ),
                    node("hydra-sdk", deps=[("indirect 1.0.0", None)]),
                    node("serde"),
                    node("proptest"),
                    node("cc"),
                    node("indirect"),
                ]
            },
        }

    def names(self):
        return {c["name"] for c in licenses.rust_components(self.metadata())}

    def test_a_dependency_of_a_dependency_is_shipped_too(self):
        self.assertIn("indirect", self.names())

    def test_test_only_and_build_only_crates_are_not_shipped(self):
        # Neither is inside the binary anyone receives, and including them
        # would make the file churn every time a test gains a helper.
        self.assertNotIn("proptest", self.names())
        self.assertNotIn("cc", self.names())

    def test_hydras_own_crates_are_not_third_party(self):
        # They are covered by the licence the About panel shows separately;
        # listing them here would say Hydra is built on Hydra.
        self.assertNotIn("hydra-gui", self.names())
        self.assertNotIn("hydra-sdk", self.names())

    def test_a_missing_root_crate_is_an_error_not_an_empty_file(self):
        metadata = self.metadata()
        metadata["packages"] = [p for p in metadata["packages"] if p["name"] != "hydra-gui"]
        with self.assertRaises(SystemExit):
            licenses.rust_components(metadata)


class TestNpmComponents(unittest.TestCase):
    def test_each_version_keeps_its_own_package_directory(self):
        # pnpm reports one entry with parallel arrays when two versions of a
        # package are installed. Pairing them by position is the only thing
        # that keeps each version's notice attached to that version.
        listing = {
            "MIT": [
                {
                    "name": "react",
                    "versions": ["18.3.1", "19.0.0"],
                    "paths": ["/n/react@18", "/n/react@19"],
                    "license": "MIT",
                    "homepage": "https://react.dev",
                }
            ]
        }
        out = licenses.npm_components(listing)
        self.assertEqual(
            [(c["version"], c["dir"]) for c in out],
            [("18.3.1", "/n/react@18"), ("19.0.0", "/n/react@19")],
        )

    def test_a_package_pnpm_gave_no_path_for_is_dropped(self):
        # Zipping stops at the shorter array. The alternative — inventing a
        # path — would read a licence out of whatever happens to be there.
        listing = {
            "MIT": [{"name": "ghost", "versions": ["1.0.0"], "paths": [], "license": "MIT"}]
        }
        self.assertEqual(licenses.npm_components(listing), [])


class TestFileSelection(unittest.TestCase):
    def test_picks_licence_files_by_any_of_their_usual_names(self):
        names = [
            "LICENSE",
            "LICENSE-APACHE",
            "LICENCE.md",
            "COPYING",
            "NOTICE",
            "UNLICENSE",
            "Cargo.toml",
            "README.md",
            "src",
            "license_key.rs",
        ]
        self.assertEqual(
            licenses.pick_license_files(names),
            ["COPYING", "LICENCE.md", "LICENSE", "LICENSE-APACHE", "NOTICE", "UNLICENSE"],
        )

    def test_the_order_is_the_name_order_not_the_directory_order(self):
        # Same package, same output, on every machine — otherwise `--check`
        # fails for whoever's filesystem enumerates differently.
        shuffled = ["NOTICE", "LICENSE-MIT", "LICENSE-APACHE"]
        self.assertEqual(
            licenses.pick_license_files(shuffled),
            ["LICENSE-APACHE", "LICENSE-MIT", "NOTICE"],
        )


class TestTextHandling(unittest.TestCase):
    def test_line_endings_and_trailing_space_do_not_make_a_new_licence(self):
        a = licenses.normalise_text("MIT License\r\nCopyright   \r\n")
        b = licenses.normalise_text("MIT License\nCopyright\n\n\n")
        self.assertEqual(a, b)

    def test_identical_texts_are_stored_once_and_referenced(self):
        components = [
            {"name": "a", "files": [("LICENSE", "same")]},
            {"name": "b", "files": [("LICENSE", "same")]},
        ]
        texts, out = licenses.dedupe(components)
        self.assertEqual(texts, ["same"])
        self.assertEqual(out[0]["files"], [["LICENSE", 0]])
        self.assertEqual(out[1]["files"], [["LICENSE", 0]])

    def test_a_dual_licence_keeps_its_halves_apart(self):
        # The Apache half is identical in every dual-licensed crate and the
        # MIT half is not. Joined into one text per package, the shared half
        # would be stored once per package instead of once.
        components = [
            {"name": "a", "files": [("LICENSE-APACHE", "apache"), ("LICENSE-MIT", "mit-a")]},
            {"name": "b", "files": [("LICENSE-APACHE", "apache"), ("LICENSE-MIT", "mit-b")]},
        ]
        texts, out = licenses.dedupe(components)
        self.assertEqual(texts, ["apache", "mit-a", "mit-b"])
        self.assertEqual(out[1]["files"], [["LICENSE-APACHE", 0], ["LICENSE-MIT", 2]])

    def test_a_package_with_no_licence_file_still_gets_an_entry(self):
        texts, out = licenses.dedupe([{"name": "a", "files": []}])
        self.assertEqual(texts, [])
        self.assertEqual(out[0]["files"], [])


class TestCommittedFile(unittest.TestCase):
    def test_the_generated_file_is_present_and_whole(self):
        # The app embeds this file at compile time; the Rust tests check its
        # shape. This checks the thing they cannot — that it was committed.
        import json

        doc = json.loads(licenses.OUT.read_text(encoding="utf-8"))
        self.assertGreater(len(doc["components"]), 100)
        self.assertGreater(len(doc["texts"]), 10)


if __name__ == "__main__":
    unittest.main()
