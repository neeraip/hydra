#!/usr/bin/env python3
"""Assemble the browser demo into one portable HTML file.

The served demo is four files and a directory of wasm. This produces one
file that runs from `file://` — mail it, drop it on a USB stick, open it
with no server and no install, and it still solves a network.

Two things stand between here and there, and both come from `file://`:

*   **ES modules are refused.** A `file://` document has an opaque origin,
    so importing another `file://` script fails CORS. The demo's own script
    is therefore loaded as a classic script (which is why `app.js` exposes
    `startHydraDemo` rather than importing anything), and the wasm bundle is
    built with wasm-pack's `no-modules` target, which defines a global
    instead of exporting.

*   **Streaming instantiation needs a Response.** `fetch` cannot read a
    `file://` URL either, so the wasm cannot be fetched at all. It is
    embedded in the page instead and instantiated from bytes.

Embedding is the size problem: base64 costs a third on top, and the bundle
is over a megabyte. So it is gzipped first and inflated in the browser by
`DecompressionStream`, which turns ~1.1 MB into ~600 kB of HTML — small
enough to mail.

Usage:
    python3 scripts/build-wasm-single.py [--out PATH]

Expects `wasm-pack build --target no-modules` to have run already; `just
demo-single` does both.
"""

from __future__ import annotations

import argparse
import base64
import gzip
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
WWW = REPO / "crates" / "demo" / "www"
PKG = WWW / "pkg-nomodules"
# The shared web theme is sourced from site/ (its single source of truth)
# rather than from a build-time copy in www/, so this script needs no
# prior copy step.
THEME = REPO / "site" / "hydra-theme.css"
DEFAULT_OUT = WWW / "hydra.html"


def guard_script_end(js: str) -> str:
    """Make `js` safe to place inside a `<script>` element.

    An HTML parser ends a script at the first `</script` in the text,
    whatever the JavaScript around it means — so a source file containing
    that sequence in a string or a comment truncates the page, and the
    failure looks like a syntax error a long way from the cause. Splitting
    the sequence with a backslash escape leaves the string identical to
    JavaScript and invisible to the parser.

    `<!--` gets the same treatment: it opens an HTML comment inside legacy
    script parsing and can swallow the rest of the file.
    """
    return js.replace("</script", "<\\/script").replace("<!--", "<\\!--")


def external_references(html: str) -> list[str]:
    """Every `src`/`href` in `html` that the page needs and cannot have.

    This is what "portable" means, and it is the claim most easily broken by
    accident: adding a font, an icon or a stylesheet to the served page
    leaves the single file loading it from a path that will not exist on the
    machine it was mailed to. The page does not fail — it renders wrong, or
    silently misses a script, which is worse.

    `data:` URIs are inside the file, and so is a bare `#` fragment. An
    absolute `http(s)` URL on an `<a>` is navigation, not an asset — the
    offline page renders whole without it, and clicking it is an ordinary
    trip to the web — so the site nav is allowed. A *relative* `<a>` is
    still reported: from `file://` it points at nothing.
    """
    out = []
    for attr in ("src", "href"):
        start = 0
        needle = f'{attr}="'
        while (i := html.find(needle, start)) != -1:
            value = html[i + len(needle) : html.index('"', i + len(needle))]
            start = i + len(needle)
            if value.startswith(("data:", "#")) or not value:
                continue
            tag = html[html.rfind("<", 0, i) + 1 :].split(None, 1)[0]
            if tag == "a" and value.startswith(("http://", "https://")):
                continue
            out.append(value)
    return out


def embed_payload(wasm: bytes) -> str:
    """The wasm bundle as one base64 string, gzipped first."""
    # mtime=0 so the same input always produces the same output — a build
    # that differs byte-for-byte between runs cannot be checked against a
    # published one.
    return base64.b64encode(gzip.compress(wasm, compresslevel=9, mtime=0)).decode("ascii")


BOOT = """
// Decode the embedded bundle and start the demo.
//
// The payload is gzipped base64: base64 alone would add a third to an
// already large page. `DecompressionStream` is what inflates it, and a
// browser without it gets told so plainly — a page that simply did nothing
// would look like a broken file rather than an old browser.
(async () => {
  const fail = (message) => {
    document.getElementById("term").textContent = message;
  };
  if (typeof DecompressionStream !== "function") {
    fail(
      "This page needs a browser with DecompressionStream (Chrome 80+, " +
        "Safari 16.4+, Firefox 113+).",
    );
    return;
  }
  try {
    const packed = Uint8Array.from(atob(HYDRA_WASM_GZ_BASE64), (c) => c.charCodeAt(0));
    const stream = new Blob([packed]).stream().pipeThrough(new DecompressionStream("gzip"));
    const wasm = await new Response(stream).arrayBuffer();
    await wasm_bindgen({ module_or_path: wasm });
    startHydraDemo(wasm_bindgen);
  } catch (e) {
    fail(`Hydra could not start: ${e && e.message ? e.message : e}`);
  }
})();
"""


def build(out: Path) -> Path:
    for name in ("hydra.js", "hydra_bg.wasm"):
        if not (PKG / name).exists():
            sys.exit(
                f"missing {PKG / name}\n"
                "run: wasm-pack build crates/demo --target no-modules "
                "--out-dir www/pkg-nomodules --out-name hydra"
            )

    html = (WWW / "index.html").read_text()
    theme = THEME.read_text()
    css = (WWW / "app.css").read_text()
    app = (WWW / "app.js").read_text()
    glue = (PKG / "hydra.js").read_text()
    payload = embed_payload((PKG / "hydra_bg.wasm").read_bytes())

    # The stylesheet and the two script tags the served page uses are
    # replaced wholesale; everything between them is the same markup.
    head_open = html.index("<head>")
    body = html[head_open:]
    body = body.replace(
        '    <link rel="stylesheet" href="hydra-theme.css" />\n',
        f"    <style>\n{theme}\n    </style>\n",
    )
    body = body.replace(
        '    <link rel="stylesheet" href="app.css" />\n',
        f"    <style>\n{css}\n    </style>\n",
    )

    scripts_start = body.index("    <!-- The demo first")
    scripts_end = body.index("</script>", body.index("<script type=\"module\">")) + len("</script>")
    inlined = (
        "    <script>\n"
        + guard_script_end(glue)
        + "\n    </script>\n"
        + "    <script>\n"
        + guard_script_end(app)
        + "\n    </script>\n"
        + "    <script>\n"
        + f"const HYDRA_WASM_GZ_BASE64 = {json.dumps(payload)};\n"
        + guard_script_end(BOOT)
        + "\n    </script>"
    )
    body = body[:scripts_start] + inlined + body[scripts_end:]
    page = html[:head_open] + body

    # Refuse to write a file that is not actually portable. Discovering this
    # by opening the result on another machine is the expensive way.
    leftover = external_references(page)
    if leftover:
        sys.exit(f"not self-contained — still references {leftover}")

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(page)
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = parser.parse_args()
    written = build(args.out)
    size = written.stat().st_size
    print(f"{written.relative_to(REPO)}  {size / 1_000_000:.2f} MB")


if __name__ == "__main__":
    main()
