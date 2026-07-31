"""Guards that the release machinery keeps up with the workspace.

A new publishable crate that nobody adds to the publish workflow does not
fail loudly: `cargo release publish -p hydra-sdk` fails on an unindexed
dependency midway through a release, after earlier crates are already live
and unrepublishable. These tests turn that into a red build instead.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


def workspace_members():
    text = (ROOT / "Cargo.toml").read_text()
    block = re.search(r"members\s*=\s*\[(.*?)\]", text, re.S).group(1)
    return [m.group(1) for m in re.finditer(r'"([^"]+)"', block)]


def publishable_crates():
    """Workspace members with `publish = true` (or no explicit opt-out)."""
    out = []
    for member in workspace_members():
        toml = ROOT / member / "Cargo.toml"
        text = toml.read_text()
        name = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text).group(1)
        publish = re.search(r"(?m)^publish\s*=\s*(true|false)", text)
        if publish is None or publish.group(1) == "true":
            out.append(name)
    return out


class PublishWorkflowCoversEveryCrate(unittest.TestCase):
    def test_every_publishable_crate_is_published(self):
        workflow = (ROOT / ".github/workflows/publish-crates.yml").read_text()
        published = set(re.findall(r"-p\s+(hydra-[a-z-]+)", workflow))
        missing = [c for c in publishable_crates() if c not in published]
        self.assertEqual(
            missing,
            [],
            "publishable crate(s) absent from publish-crates.yml — the release "
            "will fail partway through, once earlier crates are already live",
        )

    def test_publish_order_puts_dependencies_first(self):
        """crates.io rejects a crate whose deps are not yet indexed."""
        workflow = (ROOT / ".github/workflows/publish-crates.yml").read_text()
        line = next(
            l for l in workflow.splitlines() if "cargo release publish" in l and "hydra-sdk" in l
        )
        order = re.findall(r"-p\s+(hydra-[a-z-]+)", line)
        position = {name: i for i, name in enumerate(order)}
        for member in workspace_members():
            toml = ROOT / member / "Cargo.toml"
            text = toml.read_text()
            name = re.search(r'(?m)^name\s*=\s*"([^"]+)"', text).group(1)
            if name not in position:
                continue
            for dep in re.findall(r"(?m)^(hydra-[a-z-]+)\s*=\s*\{", text):
                if dep in position and dep != name:
                    self.assertLess(
                        position[dep],
                        position[name],
                        f"{name} is published before its dependency {dep}",
                    )


class BumpUpdatesEveryPin(unittest.TestCase):
    def test_bump_rewrites_every_intra_workspace_pin(self):
        """A pin bump.py does not know about is left at the old version, and
        the publish then pins a version that no longer exists."""
        bump = (ROOT / "scripts/bump.py").read_text()
        known = set(re.findall(r'"(hydra-[a-z-]+)",', bump))
        touched = set(re.findall(r'"(crates/[a-z-]+)/Cargo\.toml"', bump))

        for member in workspace_members():
            toml = ROOT / member / "Cargo.toml"
            text = toml.read_text()
            deps = [
                d
                for d in re.findall(r"(?m)^(hydra-[a-z-]+)\s*=\s*\{[^\n]*version", text)
            ]
            if not deps:
                continue
            # crates/cli is handled by its own dedicated rewrite in bump.py.
            if member == "crates/cli":
                continue
            self.assertIn(
                member,
                touched,
                f"{member} has workspace dep pins but bump.py never rewrites it",
            )
            for dep in deps:
                self.assertIn(
                    dep,
                    known,
                    f"{member} pins {dep}, which bump.py does not rewrite",
                )


if __name__ == "__main__":
    unittest.main()
