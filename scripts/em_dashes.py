"""Find em dashes in user-facing copy.

AGENTS.md forbids the em dash everywhere a person using Hydra reads:
GUI text, CLI output and diagnostics, report text, the marketing site,
the docs site, and the READMEs. It permits it in code comments, specs,
commit messages and chat. That split is the whole difficulty, because
both live in the same files, and three manual sweeps have each missed
some: at the time this was written the demo's own diagnostics carried
five, an example note carried one, and five docs pages carried more.

The detection rests on one property of both languages: after comments
are removed, an em dash can only be in a string literal or in JSX text,
and both of those are read by a user. So the scanners only have to find
comments accurately, not classify prose.

The one em dash that stays is the placeholder standing for "no value
here", recognised by the company it keeps: a prose em dash joins two
phrases, so a run of text holding an em dash and no word at all is not
prose. The rule's own remedies (a full stop, a comma, a colon,
parentheses) are sentence punctuation, and none of them can replace a
glyph in an empty table cell.
"""

import pathlib
import re

EM = "—"
REPO = pathlib.Path(__file__).resolve().parent.parent

# Everything a person using Hydra reads. Excluded on purpose: AGENTS.md
# and CLAUDE.md (agent instructions), RELEASING.md (a maintainer's
# runbook), specs and code comments (exempted by the rule itself),
# tests, and docs/src/theory (pinned snapshots that must stay faithful
# to the tag they describe, defects included).
PROSE_GLOBS = [
    "README.md",
    "COMMERCIAL_LICENSE.md",
    "crates/*/README.md",
    "docs/src/**/*.md",
    "site/**/*.html",
    "crates/demo/www/*.html",
]
SOURCE_GLOBS = [
    "crates/*/src/**/*.rs",
    "crates/gui/frontend/src/**/*.ts",
    "crates/gui/frontend/src/**/*.tsx",
    "crates/demo/www/app.js",
]
EXCLUDED = ("docs/src/theory/",)


# String quotes, JSX tag brackets and table pipes all bound a run of
# text that is read as one thing. Splitting on them is enough to tell a
# sentence from a lone glyph without parsing either language.
UNIT_BOUNDARY = re.compile(r"""["'`|<>]""")


def is_placeholder(unit: str) -> bool:
    """True when this run of text is a glyph, not a sentence.

    A prose em dash joins two phrases, so it always has something
    substantial either side. A placeholder stands alone, at most beside
    one symbol naming the quantity it has no value for (`Ø —`). Both
    forms in the app today are covered by allowing a single character
    of company, and prose can never be that short.
    """
    return len("".join(unit.split()).replace(EM, "")) <= 1


def blank(text: str) -> str:
    """Same length, no content — keeps every later line number true."""
    return re.sub(r"[^\n]", " ", text)


def strip_prose_code(text: str) -> str:
    """Blank fenced blocks, inline code, and HTML comments.

    A fence's contents are code, and a code sample's own strings
    illustrate someone else's program rather than saying anything to a
    Hydra user.
    """
    text = re.sub(r"(?ms)^(```|~~~).*?^\1[^\n]*$", lambda m: blank(m.group(0)), text)
    text = re.sub(r"(?s)<!--.*?-->", lambda m: blank(m.group(0)), text)
    text = re.sub(r"`[^`\n]*`", lambda m: blank(m.group(0)), text)
    return text


def strip_html_code(text: str) -> str:
    """Blank HTML comments, then the comments inside script and style.

    The demo page is a single file holding its own stylesheet and its
    whole driver, so most of its em dashes are in JavaScript and CSS
    comments that no visitor ever sees.
    """
    text = re.sub(r"(?s)<!--.*?-->", lambda m: blank(m.group(0)), text)

    def inner(m):
        return m.group(1) + strip_comments(m.group(2), rust=False) + m.group(3)

    return re.sub(
        r"(?is)(<(?:script|style)\b[^>]*>)(.*?)(</(?:script|style)>)", inner, text
    )


def strip_embedded_comments(literal: str) -> str:
    """Blank `//` comments inside a template literal.

    A template literal here is as likely to hold GLSL as prose, and its
    shader comments are read by nobody. The `:` guard keeps a URL in a
    genuine sentence (`https://...`) from being mistaken for one.
    """
    return re.sub(r"(?<!:)//[^\n]*", lambda m: blank(m.group(0)), literal)


def strip_comments(text: str, *, rust: bool) -> str:
    """Blank every comment, leaving string literals and JSX text.

    Written as a scanner rather than a regex because the distinction
    that matters is whether a `//` sits inside a string, which no
    regex over lines can answer.
    """
    out = []
    i, n = 0, len(text)
    depth = 0  # block-comment nesting; Rust nests, and JSX comments are blocks
    while i < n:
        c = text[i]
        if depth:
            if text.startswith("/*", i):
                depth += 1
                out.append("  ")
                i += 2
                continue
            if text.startswith("*/", i):
                depth -= 1
                out.append("  ")
                i += 2
                continue
            out.append("\n" if c == "\n" else " ")
            i += 1
            continue
        if text.startswith("//", i) and not (i and text[i - 1] == ":"):
            # The `:` guard keeps a bare URL from opening a comment. Inside a
            # string the quote handling already covers it, but JSX text is
            # unquoted, so `<p>See https://x — now</p>` blanked from the
            # scheme to the end of the line and took the em dash with it.
            end = text.find("\n", i)
            end = n if end < 0 else end
            out.append(blank(text[i:end]))
            i = end
            continue
        if text.startswith("/*", i):
            depth = 1
            out.append("  ")
            i += 2
            continue
        if rust and (text.startswith('r"', i) or re.match(r'r#+"', text[i:])):
            m = re.match(r'r(#*)"', text[i:])
            close = '"' + m.group(1)
            end = text.find(close, i + m.end())
            end = n if end < 0 else end + len(close)
            out.append(text[i:end])
            i = end
            continue
        if c in "\"'`":
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == c:
                    j += 1
                    break
                # An unterminated single quote is a Rust lifetime, not a
                # char literal; stop at the line so it cannot swallow the file.
                if text[j] == "\n" and c != "`":
                    break
                j += 1
            out.append(strip_embedded_comments(text[i:j]) if c == "`" else text[i:j])
            i = j
            continue
        out.append(c)
        i += 1
    return "".join(out)


def strip_rust_tests(text: str) -> str:
    """Blank `#[cfg(test)]` modules and `#[test]` functions.

    A test's assertion messages are read by whoever broke the test, not
    by a user. Brace-matched rather than run to end of file, because
    several files here carry more than one test module.
    """
    for marker in ("#[cfg(test)]", "#[test]"):
        while True:
            start = text.find(marker)
            if start < 0:
                break
            open_brace = text.find("{", start)
            semi = text.find(";", start)
            if open_brace < 0 and semi < 0:
                break
            if semi >= 0 and (open_brace < 0 or semi < open_brace):
                # A brace-less item: `#[cfg(test)] const X: f64 = 2.0;`. It
                # ends at its semicolon. Matching braces here instead ran to
                # the next `{` anywhere in the file, which blanked 294 of
                # pdf.rs's 637 lines and hid them from every check below.
                i = semi + 1
            else:
                depth, i = 0, open_brace
                while i < len(text):
                    if text[i] == "{":
                        depth += 1
                    elif text[i] == "}":
                        depth -= 1
                        if depth == 0:
                            i += 1
                            break
                    i += 1
            text = text[:start] + blank(text[start:i]) + text[i:]
    return text


def drop_placeholders(line: str) -> str:
    """Remove the glyph placeholders this rule permits."""
    return "".join("" if is_placeholder(u) else u for u in UNIT_BOUNDARY.split(line))


def scan(path: pathlib.Path) -> list[tuple[int, str]]:
    """The offending (line number, line) pairs in one file."""
    raw = path.read_text(encoding="utf-8")
    if path.suffix == ".html":
        text = strip_html_code(raw)
    elif path.suffix == ".md":
        text = strip_prose_code(raw)
    else:
        rust = path.suffix == ".rs"
        text = strip_comments(raw, rust=rust)
        if rust:
            text = strip_rust_tests(text)
    found = []
    for n, line in enumerate(text.splitlines(), start=1):
        if EM in drop_placeholders(line):
            found.append((n, raw.splitlines()[n - 1].strip()))
    return found


def user_facing_files() -> list[pathlib.Path]:
    paths = []
    for pattern in PROSE_GLOBS + SOURCE_GLOBS:
        for p in sorted(REPO.glob(pattern)):
            rel = p.relative_to(REPO).as_posix()
            if any(rel.startswith(x) for x in EXCLUDED):
                continue
            if ".test." in p.name or rel.startswith("crates/demo/www/pkg"):
                continue
            paths.append(p)
    return paths


def main() -> int:
    bad = 0
    for path in user_facing_files():
        for n, line in scan(path):
            print(f"{path.relative_to(REPO)}:{n}: {line}")
            bad += 1
    if bad:
        print(f"\n{bad} em dash(es) in user-facing copy. AGENTS.md forbids them there.")
        print("Use a full stop, a comma, a colon, or parentheses instead.")
        return 1
    print(f"No em dashes in {len(user_facing_files())} user-facing files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
