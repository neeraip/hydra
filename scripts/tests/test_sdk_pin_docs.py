"""The documented `hydra-sdk = "N"` pin must track the workspace major.

Cargo reads a bare `"1"` as `^1.0`, which excludes every later major. A stale
pin therefore does not merely look out of date — it silently resolves readers
onto an ancient release. This went unnoticed for the whole 2.x line, so it is
asserted rather than remembered.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

# Kept in step with SDK_PIN_DOCS in scripts/bump.py; the last test below
# fails if that list gains an entry this one does not know about.
PIN_DOCS = ("README.md", "crates/sdk/README.md", "docs/src/sdk/overview.md")

PIN_RE = re.compile(r'hydra-sdk = "(\d+)"')


def workspace_major() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    m = re.search(r'(?m)^version = "(\d+)\.\d+\.\d+"', text)
    assert m, "workspace version not found in Cargo.toml"
    return m.group(1)


class TestSdkPinDocs(unittest.TestCase):
    def test_documented_sdk_pin_matches_workspace_major(self):
        expected = workspace_major()
        for doc in PIN_DOCS:
            path = ROOT / doc
            pins = PIN_RE.findall(path.read_text())
            self.assertEqual(
                len(pins), 1, f"{doc}: expected exactly one hydra-sdk pin, got {pins}"
            )
            self.assertEqual(
                pins[0],
                expected,
                f"{doc} tells users to depend on hydra-sdk {pins[0]!r}, but the "
                f"workspace is {expected}.x — a bare major pin excludes every "
                f"later major, so readers would resolve to an old release",
            )

    def test_bump_script_owns_every_documented_pin(self):
        # A doc added to one list and not the other is exactly how the pin
        # went stale before: the bump would stop updating it silently.
        bump = (ROOT / "scripts" / "bump.py").read_text()
        for doc in PIN_DOCS:
            self.assertIn(
                doc,
                bump,
                f"{doc} carries an sdk pin but bump.py does not update it",
            )

    def test_no_undocumented_pin_escapes_the_list(self):
        # Any other tracked markdown carrying a pin would drift unnoticed.
        missed = []
        for path in ROOT.glob("**/*.md"):
            rel = path.relative_to(ROOT).as_posix()
            if any(part in rel for part in ("node_modules", "target/", ".git/")):
                continue
            if rel in PIN_DOCS:
                continue
            if PIN_RE.search(path.read_text(errors="ignore")):
                missed.append(rel)
        self.assertEqual(
            missed, [], f"these files pin hydra-sdk but are not maintained: {missed}"
        )


if __name__ == "__main__":
    unittest.main()
