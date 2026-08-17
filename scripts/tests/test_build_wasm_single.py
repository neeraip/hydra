"""The portable single-file build.

Two things make this build wrong in ways nobody notices until the file is on
somebody else's machine: a script that truncates because its text contained
`</script>`, and a reference to a file that no longer travels with the
page. Both render *something*, which is why they are worth a test rather
than an eyeball.
"""

import base64
import gzip
import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

# The script is `build-wasm-single.py` — a hyphen, so it cannot be imported
# by name. Load it by path instead of renaming a file whose name matches its
# `just` recipe.
_spec = importlib.util.spec_from_file_location(
    "build_wasm_single", ROOT / "scripts" / "build-wasm-single.py"
)
build_wasm_single = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(build_wasm_single)

guard_script_end = build_wasm_single.guard_script_end
external_references = build_wasm_single.external_references
embed_payload = build_wasm_single.embed_payload


class GuardScriptEnd(unittest.TestCase):
    """An HTML parser ends a script at the first `</script`, whatever the
    JavaScript around it means."""

    def test_a_closing_tag_in_a_string_is_hidden_from_the_parser(self):
        js = 'const t = "</script>";'
        self.assertNotIn("</script", guard_script_end(js))

    def test_the_escape_leaves_the_string_unchanged_for_javascript(self):
        # `\/` is just `/` in a JS string literal, so the value the program
        # sees is the same — that is the whole reason this escape is safe.
        self.assertEqual(guard_script_end('"</script>"'), '"<\\/script>"')

    def test_it_matches_regardless_of_what_follows(self):
        # The parser ends the element at `</script` plus any of space, tab,
        # newline, `/` or `>`, so matching only the exact `</script>` would
        # miss `</script >` and `</script\n>`.
        for tail in (">", " >", "\n>", "/>"):
            with self.subTest(tail=tail):
                self.assertNotIn("</script", guard_script_end(f"//</script{tail}"))

    def test_an_html_comment_opener_is_hidden_too(self):
        self.assertNotIn("<!--", guard_script_end("// <!-- not a comment here"))

    def test_ordinary_source_is_untouched(self):
        js = "const a = 1 < 2;\n// </div> is fine\n"
        self.assertEqual(guard_script_end(js), js)


class ExternalReferences(unittest.TestCase):
    """What "portable" means, and the claim most easily broken by adding an
    asset to the served page."""

    def test_a_stylesheet_link_is_reported(self):
        html = '<link rel="stylesheet" href="app.css" />'
        self.assertEqual(external_references(html), ["app.css"])

    def test_a_script_src_is_reported(self):
        self.assertEqual(external_references('<script src="app.js"></script>'), ["app.js"])

    def test_an_absolute_url_is_reported(self):
        html = '<link href="https://fonts.example/x.css">'
        self.assertEqual(external_references(html), ["https://fonts.example/x.css"])

    def test_a_data_uri_is_inside_the_file(self):
        self.assertEqual(external_references('<img src="data:image/png;base64,AAA">'), [])

    def test_a_fragment_is_inside_the_file(self):
        self.assertEqual(external_references('<a href="#top">top</a>'), [])

    def test_an_absolute_nav_link_is_navigation_not_an_asset(self):
        html = '<a class="x" href="https://neeraip.github.io/hydra/">Home</a>'
        self.assertEqual(external_references(html), [])

    def test_a_relative_nav_link_is_still_reported(self):
        # From file:// a relative link points at nothing.
        self.assertEqual(external_references('<a href="docs/">Docs</a>'), ["docs/"])

    def test_an_inlined_page_has_none(self):
        html = "<style>body{}</style><script>const a = 1;</script>"
        self.assertEqual(external_references(html), [])


class EmbedPayload(unittest.TestCase):
    def test_the_payload_round_trips_to_the_original_bytes(self):
        wasm = b"\x00asm\x01\x00\x00\x00" + bytes(range(256)) * 40
        packed = embed_payload(wasm)
        self.assertEqual(gzip.decompress(base64.b64decode(packed)), wasm)

    def test_it_is_smaller_than_plain_base64(self):
        # The reason for gzipping at all: base64 alone adds a third to a
        # bundle that is already over a megabyte.
        wasm = b"\x00asm\x01\x00\x00\x00" + b"repetitive" * 5000
        self.assertLess(len(embed_payload(wasm)), len(base64.b64encode(wasm)))

    def test_the_same_bundle_always_produces_the_same_payload(self):
        # gzip stamps the time by default, so two builds of one bundle would
        # differ byte-for-byte and could not be compared against a published
        # file.
        wasm = b"\x00asm\x01\x00\x00\x00 deterministic"
        self.assertEqual(embed_payload(wasm), embed_payload(wasm))

    def test_the_payload_is_ascii_and_safe_to_inline(self):
        packed = embed_payload(b"\x00asm\x01\x00\x00\x00" + bytes(range(256)))
        self.assertTrue(packed.isascii())
        self.assertNotIn("<", packed)


class BuiltArtifact(unittest.TestCase):
    """Assertions against the real file, when one has been built.

    Skipped rather than failed where it has not: `just demo-single` is not
    part of the CI gate, and a demo artifact missing from a fresh checkout
    is the normal case rather than a fault.
    """

    def setUp(self):
        self.page = ROOT / "crates" / "wasm" / "www" / "hydra.html"
        if not self.page.exists():
            self.skipTest("run `just demo-single` to build the portable page")

    def test_it_references_nothing_outside_itself(self):
        self.assertEqual(external_references(self.page.read_text()), [])

    def test_it_uses_no_es_modules(self):
        # `file://` refuses them, which is the whole reason for the
        # no-modules build.
        self.assertNotIn('type="module"', self.page.read_text())

    def test_it_carries_both_the_demo_and_the_engine(self):
        text = self.page.read_text()
        self.assertIn("startHydraDemo", text)
        self.assertIn("HYDRA_WASM_GZ_BASE64", text)
        self.assertIn("wasm_bindgen", text)


if __name__ == "__main__":
    unittest.main()
