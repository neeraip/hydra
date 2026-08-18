"""The rehearsal has to build the app the way the release does.

A rehearsal that has drifted from the thing it rehearses is worse than
none: it goes green every week while the release it claims to stand in
for would fail. This is not hypothetical. The rehearsal's first run
failed because its `pnpm install` step had lost the `working-directory`
the release job carries, which is exactly the class of mistake it exists
to catch, made while writing it.

So the steps the two share are compared here, by name, on the fields that
decide what actually runs. Steps unique to either are fine and expected:
the release uploads its artifacts, and the rehearsal generates a
throwaway signing key and inspects what the bundler produced.
"""

import pathlib
import unittest

try:
    import yaml
except ImportError:  # pragma: no cover
    yaml = None

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"

# What decides what a step does. `name` is the key, not a field.
COMPARED = ("uses", "run", "working-directory", "with")


def steps_by_name(workflow: str, job: str) -> dict[str, dict]:
    spec = yaml.safe_load((WORKFLOWS / workflow).read_text())["jobs"][job]
    out = {}
    for step in spec["steps"]:
        name = step.get("name") or step.get("uses", "")
        out[name] = step
    return out


@unittest.skipIf(yaml is None, "PyYAML not available")
class ReleaseRehearsalTests(unittest.TestCase):
    def setUp(self):
        self.release = steps_by_name("draft-release.yml", "gui")
        self.rehearsal = steps_by_name("release-rehearsal.yml", "gui")

    def test_shared_steps_run_the_same_thing(self):
        shared = set(self.release) & set(self.rehearsal)
        self.assertGreater(len(shared), 6, f"barely any shared steps: {shared}")
        for name in sorted(shared):
            a, b = self.release[name], self.rehearsal[name]
            for field in COMPARED:
                self.assertEqual(
                    a.get(field),
                    b.get(field),
                    f"step {name!r} differs on {field!r}: the rehearsal would "
                    "not exercise what the release does",
                )

    def test_the_rehearsal_covers_the_build_itself(self):
        # Without these it is not rehearsing a release, it is rehearsing a
        # checkout.
        for needed in (
            "Install frontend dependencies",
            "Regenerate CRS catalog",
            "Install Tauri CLI",
        ):
            self.assertIn(needed, self.rehearsal)

    def test_the_rehearsal_publishes_nothing(self):
        joined = " ".join(
            str(s.get("run", "")) + " " + str(s.get("uses", ""))
            for s in self.rehearsal.values()
        )
        for forbidden in ("gh release", "upload", "softprops/action-gh-release"):
            self.assertNotIn(forbidden, joined, "a rehearsal must not publish")


if __name__ == "__main__":
    unittest.main()
