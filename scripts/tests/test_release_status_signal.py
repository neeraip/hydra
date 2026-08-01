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

    def test_signal_major_from_hyphenated_breaking_change_footer(self):
        # Conventional Commits: BREAKING-CHANGE is a synonym of BREAKING CHANGE.
        msgs = ["chore: cleanup\n\nBREAKING-CHANGE: incompatible output format"]
        self.assertEqual(release_status.signal(msgs), "major")

    def test_signal_minor_from_feat(self):
        msgs = ["feat(cli): add new switch"]
        self.assertEqual(release_status.signal(msgs), "minor")

    def test_signal_none_without_feat_or_breaking(self):
        msgs = ["fix: adjust timeout", "docs: update README"]
        self.assertEqual(release_status.signal(msgs), "none")


    def test_signal_ignores_bang_on_non_code_types(self):
        # A docs/style/test/ci commit cannot break a compiled API; its
        # breaking marker describes something else (a spec renumbering, a
        # test-contract change) and must not suggest MAJOR.
        for kind in ("docs", "style", "test", "ci"):
            self.assertEqual(
                release_status.signal([f"{kind}(uds)!: renumber the registry"]),
                "none",
            )

    def test_signal_ignores_breaking_footer_under_non_code_subject(self):
        # Squash merges carry constituent messages in the body; the subject's
        # type caps severity even when a footer survives inside.
        msg = (
            "docs(uds): write the urban drainage specification (#91)\n\n"
            "docs(uds)!: reframe the specification\n\n"
            "BREAKING CHANGE: the section registry is renumbered\n"
        )
        self.assertEqual(release_status.signal([msg]), "none")

    def test_signal_keeps_bang_on_chore_and_build(self):
        # Deliberately not exempt: either can carry a genuine break.
        self.assertEqual(release_status.signal(["chore!: raise MSRV to 1.85"]), "major")
        self.assertEqual(
            release_status.signal(["build!: drop the vendored-ssl feature"]), "major"
        )

    def test_signal_mixed_messages_still_finds_the_code_break(self):
        self.assertEqual(
            release_status.signal(["docs(uds)!: renumber", "feat(engine)!: remove API"]),
            "major",
        )


if __name__ == "__main__":
    unittest.main()
