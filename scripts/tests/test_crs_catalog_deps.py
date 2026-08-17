"""update-crs-catalog.mjs runs against the frontend's node_modules, but
lives outside the frontend source tree — so no frontend-scoped analysis
sees it as a consumer. An unused-dependency sweep (knip) deleted its
package once on exactly that reasoning, and every gui-v2.18.1 release
build then died at the Regenerate CRS catalog step, the first place a
fresh install meets the script."""

import json
import pathlib
import unittest

REPO = pathlib.Path(__file__).resolve().parent.parent.parent


class CrsCatalogDependencyTests(unittest.TestCase):
    def test_the_catalog_scripts_package_is_declared_by_the_frontend(self):
        script = (REPO / "scripts" / "update-crs-catalog.mjs").read_text()
        self.assertIn("@esri/proj-codes", script, "script no longer uses the package; retire this test")
        pkg = json.loads((REPO / "crates" / "gui" / "frontend" / "package.json").read_text())
        declared = {**pkg.get("dependencies", {}), **pkg.get("devDependencies", {})}
        self.assertIn(
            "@esri/proj-codes",
            declared,
            "update-crs-catalog.mjs resolves this from crates/gui/frontend/node_modules; "
            "declaring it there is what installs it in CI",
        )


if __name__ == "__main__":
    unittest.main()
