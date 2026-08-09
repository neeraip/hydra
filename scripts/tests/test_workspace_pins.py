"""Every intra-workspace version pin must move with the workspace version.

A path dependency that also carries a version — `{ path = "../sdk",
version = "8.1.0" }` — is two claims, and only the first is checked by a
local build. Cargo resolves the path and ignores the version until the
requirement excludes what is there, so a stale pin sits harmless through
every patch and minor and refuses the next major.

That is exactly how it failed: hydra-wasm pinned `8.0.0` while the
workspace moved to 8.1.0, resolved by luck because a caret admits later
minors, and stopped the first major bump dead — halfway through, with the
manifests already rewritten.

Two claims are asserted here. Every pin equals the workspace version, and
every manifest carrying one is a manifest the bump script rewrites: the
first catches drift, the second catches a crate the bump has never heard
of, which is the shape the failure actually took.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

# A dependency on another crate *in this workspace* that also states a
# version. Deliberately not every `version = ` in the file: third-party
# pins are nobody's business here.
PIN_RE = re.compile(r'path = "\.\./[a-z0-9-]+",\s*version = "(\d+\.\d+\.\d+)"')


def workspace_version() -> str:
    m = re.search(r'(?m)^version = "(\d+\.\d+\.\d+)"', (ROOT / "Cargo.toml").read_text())
    assert m, "workspace version not found in Cargo.toml"
    return m.group(1)


def manifests_with_pins() -> dict[str, list[str]]:
    """Every crate manifest carrying an intra-workspace pin, and the pins."""
    found = {}
    for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        pins = PIN_RE.findall(manifest.read_text())
        if pins:
            found[manifest.relative_to(ROOT).as_posix()] = pins
    return found


class TestWorkspacePins(unittest.TestCase):
    def test_every_pin_matches_the_workspace_version(self):
        expected = workspace_version()
        for name, pins in manifests_with_pins().items():
            for pin in pins:
                self.assertEqual(
                    pin,
                    expected,
                    f"{name} pins an intra-workspace crate at {pin}, but the "
                    f"workspace is {expected}. A local build resolves the path "
                    f"and never notices; the next major bump refuses to.",
                )

    def test_the_bump_script_rewrites_every_manifest_that_has_one(self):
        # The failure was not a wrong pin but an unknown manifest: a crate
        # added after the bump script was written, carrying a pin the bump
        # had no idea existed.
        bump = (ROOT / "scripts" / "bump.py").read_text()
        for name in manifests_with_pins():
            self.assertIn(
                f'"{name}"',
                bump,
                f"{name} carries an intra-workspace pin that scripts/bump.py "
                f"does not rewrite — add it to CRATE_MANIFESTS, or drop the "
                f"version from the dependency if the crate is unpublished",
            )

    def test_an_unpublished_crate_states_no_version(self):
        # The cheaper half of the fix, and the reason gui never broke a
        # bump: nothing resolves an unpublished crate from a registry, so a
        # version requirement on its path deps can only ever go stale.
        for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
            text = manifest.read_text()
            if not re.search(r"(?m)^publish = false", text):
                continue
            self.assertEqual(
                PIN_RE.findall(text),
                [],
                f"{manifest.relative_to(ROOT).as_posix()} is unpublished but "
                f"pins a workspace crate by version; the pin serves nothing "
                f"and will go stale",
            )


if __name__ == "__main__":
    unittest.main()
