"""The driver-resolution decisions, and that both callers still use them.

The bug this guards against is silent in every other way: a mismatched
driver produces a 404 and a SIGKILL, naming neither version, and the only
check that runs engine code on wasm goes quietly red.
"""

import json
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import wasm_chromedriver as wc  # noqa: E402


CATALOG = {
    "builds": {
        "150.0.7800": {
            "downloads": {"chromedriver": [
                {"platform": "mac-arm64", "url": "d150-mac"},
                {"platform": "linux64", "url": "d150-linux"},
            ]}
        },
        "151.0.7922": {
            "downloads": {"chromedriver": [{"platform": "mac-arm64", "url": "d151"}]}
        },
        "151.0.7100": {
            "downloads": {"chromedriver": [{"platform": "mac-arm64", "url": "d151-old"}]}
        },
    }
}


class TestVersionParts(unittest.TestCase):
    def test_build_is_the_catalog_key(self):
        self.assertEqual(wc.build_of("151.0.7922.172"), "151.0.7922")

    def test_major_is_what_a_driver_must_agree_on(self):
        self.assertEqual(wc.major_of("151.0.7922.172"), "151")

    def test_versions_order_numerically_not_lexically(self):
        # As strings "151.0.7922.9" sorts above "151.0.7922.10", which would
        # pick the older driver of the two.
        self.assertLess(wc.version_tuple("151.0.7922.9"),
                        wc.version_tuple("151.0.7922.10"))
        self.assertLess(wc.version_tuple("151.0.900.0"),
                        wc.version_tuple("151.0.7922.0"))


class TestPickDriver(unittest.TestCase):
    def test_a_driver_of_the_wrong_major_is_not_offered(self):
        # The whole defect: this one is present, runnable, and useless.
        self.assertIsNone(wc.pick_driver([("/a", "152.0.7977.54")], "151"))

    def test_the_matching_major_is_chosen_over_a_newer_driver(self):
        self.assertEqual(
            wc.pick_driver([("/new", "152.0.7977.54"), ("/ok", "151.0.7922.138")], "151"),
            "/ok",
        )

    def test_the_newest_patch_of_the_right_major_wins(self):
        self.assertEqual(
            wc.pick_driver([("/old", "151.0.7922.77"), ("/new", "151.0.7922.138")], "151"),
            "/new",
        )

    def test_nothing_on_the_machine_is_not_an_error(self):
        self.assertIsNone(wc.pick_driver([], "151"))


class TestDriverUrl(unittest.TestCase):
    def test_the_build_is_used_when_the_catalog_has_it(self):
        self.assertEqual(wc.driver_url(CATALOG, "151.0.7922", "mac-arm64"), "d151")

    def test_a_browser_newer_than_the_catalog_falls_back_within_its_major(self):
        self.assertEqual(wc.driver_url(CATALOG, "151.0.9999", "mac-arm64"), "d151")

    def test_the_fallback_takes_the_newest_build_of_that_major(self):
        # Not merely "some build with the right major": 151.0.7100 also
        # matches, and picking it would drive a browser with a driver two
        # builds behind.
        self.assertNotEqual(wc.driver_url(CATALOG, "151.0.9999", "mac-arm64"), "d151-old")

    def test_a_major_the_catalog_does_not_carry_has_no_url(self):
        self.assertIsNone(wc.driver_url(CATALOG, "9.0.1", "mac-arm64"))

    def test_a_platform_the_build_does_not_publish_has_no_url(self):
        self.assertIsNone(wc.driver_url(CATALOG, "151.0.7922", "linux64"))


class TestPlatformKey(unittest.TestCase):
    def test_the_hosts_that_run_this_suite(self):
        self.assertEqual(wc.platform_key("Darwin", "arm64"), "mac-arm64")
        self.assertEqual(wc.platform_key("Darwin", "x86_64"), "mac-x64")
        self.assertEqual(wc.platform_key("Linux", "x86_64"), "linux64")
        self.assertEqual(wc.platform_key("Windows", "AMD64"), "win64")

    def test_a_host_with_no_published_driver_says_so(self):
        self.assertIsNone(wc.platform_key("Linux", "aarch64"))


class TestBothCallersUseIt(unittest.TestCase):
    """The recipe and the CI step are hand-mirrored, so assert each side.

    Either one reverting to a bare `wasm-pack test --chrome` restores the
    defect, and nothing else in the suite would notice.
    """

    def test_the_justfile_recipe_resolves_a_driver_first(self):
        text = (ROOT / "justfile").read_text()
        recipe = text.split("\ntest-wasm:", 1)[1].split("\n\n", 1)[0]
        self.assertIn("wasm_chromedriver.py", recipe)
        self.assertIn("--chromedriver", recipe)

    def test_the_ci_step_resolves_a_driver_first(self):
        text = (ROOT / ".github/workflows/cargo-ci.yml").read_text()
        self.assertIn("wasm_chromedriver.py", text)
        self.assertIn("--chromedriver", text)


if __name__ == "__main__":
    unittest.main()
