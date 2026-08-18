"""The em-dash guard has to be right in both directions.

A guard that misses a violation is worse than none, because it turns a
manual sweep nobody now performs into a green check. So the cases below
are mostly *positive*: text that must be caught, in each of the shapes
user-facing copy actually takes here. The negatives pin the exemptions
the rule grants, each of which is somewhere a false alarm would have
made the guard annoying enough to switch off.
"""

import pathlib
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import em_dashes  # noqa: E402


def scan_text(body: str, suffix: str) -> list[str]:
    with tempfile.TemporaryDirectory() as d:
        p = pathlib.Path(d) / f"sample{suffix}"
        p.write_text(body, encoding="utf-8")
        return [line for _, line in em_dashes.scan(p)]


class CaughtTests(unittest.TestCase):
    """Copy a user reads. Every one of these must be reported."""

    def test_markdown_prose(self):
        self.assertTrue(scan_text("Hydra reads a model — and solves it.\n", ".md"))

    def test_markdown_table_cell_with_words(self):
        self.assertTrue(scan_text("| a | fast — usually | b |\n", ".md"))

    def test_rust_string_literal(self):
        self.assertTrue(scan_text('fn f() { say("no file — ignored"); }\n', ".rs"))

    def test_rust_string_continued_across_lines(self):
        body = 'fn f() {\n    say("the file was not supplied — \\\n         the run goes on");\n}\n'
        self.assertTrue(scan_text(body, ".rs"))

    def test_rust_raw_string(self):
        self.assertTrue(scan_text('fn f() { say(r#"a — b"#); }\n', ".rs"))

    def test_typescript_string(self):
        self.assertTrue(scan_text('const m = "saved — nothing to do";\n', ".ts"))

    def test_jsx_text(self):
        self.assertTrue(scan_text("const C = () => <p>Run finished — 3 warnings</p>;\n", ".tsx"))

    def test_template_literal_prose(self):
        self.assertTrue(scan_text("const m = `imported ${n} nodes — none moved`;\n", ".ts"))

    def test_html_body_text(self):
        self.assertTrue(scan_text("<p>Runs where you work — and nowhere else.</p>\n", ".html"))

    def test_html_script_string(self):
        body = '<script>\nconst m = "no engine — pick one";\n</script>\n'
        self.assertTrue(scan_text(body, ".html"))

    def test_jsx_text_following_a_bare_url(self):
        # The `//` of a scheme used to open a line comment, blanking the
        # rest of the line. A string is safe because the quote handling
        # swallows it first; JSX text is unquoted and was not.
        body = "const C = () => <p>See https://x.example — now</p>;\n"
        self.assertTrue(scan_text(body, ".tsx"))

    def test_rust_code_after_a_braceless_cfg_test_item(self):
        # `#[cfg(test)] const X` has no braces, so matching them ran to the
        # next `{` anywhere and blanked everything in between: 294 of
        # pdf.rs's 637 lines were invisible to this scan.
        body = (
            "#[cfg(test)]\nconst MARGIN: f64 = 2.0;\n\n"
            'pub fn render() { say("nothing to render — the document is empty"); }\n'
        )
        self.assertTrue(scan_text(body, ".rs"))


class ExemptTests(unittest.TestCase):
    """The rule's own carve-outs. A false alarm here kills the guard."""

    def test_rust_line_comment(self):
        self.assertFalse(scan_text("fn f() {} // the model is opened — then stepped\n", ".rs"))

    def test_rust_block_comment(self):
        self.assertFalse(scan_text("/* opened — then stepped */\nfn f() {}\n", ".rs"))

    def test_rust_doc_comment(self):
        self.assertFalse(scan_text("/// Opens a model — and steps it.\nfn f() {}\n", ".rs"))

    def test_rust_test_module(self):
        body = '#[cfg(test)]\nmod tests {\n    fn t() { assert!(x, "bad — very"); }\n}\n'
        self.assertFalse(scan_text(body, ".rs"))

    def test_a_real_test_module_after_a_braceless_item_is_still_skipped(self):
        body = (
            "#[cfg(test)]\nconst MARGIN: f64 = 2.0;\n\n"
            "pub fn render() {}\n\n"
            '#[cfg(test)]\nmod tests {\n    fn t() { assert!(x, "bad — very"); }\n}\n'
        )
        self.assertFalse(scan_text(body, ".rs"))

    def test_rust_test_function(self):
        body = '#[test]\nfn t() {\n    assert!(x, "the fixture is wrong — regenerate it");\n}\n'
        self.assertFalse(scan_text(body, ".rs"))

    def test_jsx_block_comment(self):
        self.assertFalse(scan_text("const C = () => <p>{/* a — b */}ok</p>;\n", ".tsx"))

    def test_shader_comment_inside_a_template_literal(self):
        # FlowPathLayer holds its GLSL this way, comments and all.
        body = "const s = `\n  // was always 1.0 — a constant that cost a float\n  float x = 1.0;\n`;\n"
        self.assertFalse(scan_text(body, ".ts"))

    def test_url_in_a_template_literal_is_not_a_comment(self):
        body = "const m = `see https://x.example — for the rest`;\n"
        self.assertTrue(scan_text(body, ".ts"))

    def test_markdown_fenced_code(self):
        self.assertFalse(scan_text("Text.\n\n```sh\n# run it — fast\nhydra run\n```\n", ".md"))

    def test_markdown_inline_code(self):
        self.assertFalse(scan_text("Set `a — b` in the file.\n", ".md"))

    def test_html_comment(self):
        self.assertFalse(scan_text("<!-- laid out — then styled -->\n<p>ok</p>\n", ".html"))

    def test_css_comment_in_a_style_block(self):
        self.assertFalse(scan_text("<style>\n/* dark — fixed columns */\np { color: red }\n</style>\n", ".html"))


class PlaceholderTests(unittest.TestCase):
    """A glyph standing for a value the app does not have is not prose."""

    def test_lone_string(self):
        self.assertFalse(scan_text('fn f() { m.insert("openings", "—"); }\n', ".rs"))

    def test_lone_jsx_text(self):
        self.assertFalse(scan_text("const C = () => <span>—</span>;\n", ".tsx"))

    def test_symbol_beside_the_glyph(self):
        self.assertFalse(scan_text('const d = ok ? size : "Ø —";\n', ".ts"))

    def test_empty_table_cell(self):
        self.assertFalse(scan_text("| SDK | yes | — |\n", ".md"))

    def test_a_placeholder_does_not_excuse_prose_on_the_same_line(self):
        body = 'const m = missing ? "—" : "loaded — from disk";\n'
        self.assertTrue(scan_text(body, ".ts"))


class RepositoryTests(unittest.TestCase):
    def test_no_user_facing_file_carries_an_em_dash(self):
        offences = [
            f"{p.relative_to(ROOT)}:{n}: {line}"
            for p in em_dashes.user_facing_files()
            for n, line in em_dashes.scan(p)
        ]
        self.assertEqual(
            [],
            offences,
            "AGENTS.md forbids the em dash in user-facing copy; "
            "use a full stop, a comma, a colon, or parentheses",
        )

    def test_the_guard_looks_at_the_surfaces_that_have_regressed(self):
        looked = {p.relative_to(ROOT).as_posix() for p in em_dashes.user_facing_files()}
        for expected in (
            "COMMERCIAL_LICENSE.md",
            "README.md",
            "crates/demo/src/run.rs",
            "docs/src/getting-started/installation.md",
            "site/index.html",
        ):
            self.assertIn(expected, looked)

    def test_the_pinned_theory_snapshots_are_left_alone(self):
        looked = {p.relative_to(ROOT).as_posix() for p in em_dashes.user_facing_files()}
        self.assertFalse([p for p in looked if p.startswith("docs/src/theory/")])


if __name__ == "__main__":
    unittest.main()
