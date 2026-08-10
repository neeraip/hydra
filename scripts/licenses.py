#!/usr/bin/env python3
"""Collect the licence notices of everything the desktop app ships.

Hydra's own licence is one file in this repository. The four hundred-odd
open-source packages the app is built from are not: MIT, BSD and Apache-2.0
all ask that their copyright notice travel with the binary, and a notice
that only exists in a `~/.cargo` cache on a build machine has not travelled
anywhere.

So this script walks what the GUI actually links — the normal (not dev, not
build) dependency graph of `hydra-gui`, plus the frontend's production npm
tree — reads the licence text out of each package, and writes one JSON
document the app embeds and shows under Settings → About.

Two decisions are worth stating, because both are visible in the output:

*Every platform's dependencies, not this one's.* `cargo metadata` is read
unfiltered, so the Windows and Linux halves of the tree are included on a
macOS machine. We ship all three, and a notices file that changes depending
on who regenerated it would be worse than useless as a `--check` gate.

*Texts are shared, components are not.* Hundreds of packages carry the same
Apache-2.0 text; MIT texts differ by a single copyright line and so mostly
do not. Identical texts are stored once and referenced by index, which is a
size decision only — every component still names its own licence, and a
package shipping two files keeps them as two, so the boilerplate half of a
dual licence dedupes even where the copyright half cannot.

Usage:
    scripts/licenses.py            regenerate the notices file
    scripts/licenses.py --check    fail if the committed file is stale
"""

import hashlib
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / "crates" / "gui" / "resources" / "third-party-licenses.json"
FRONTEND = ROOT / "crates" / "gui" / "frontend"

# The crate whose dependency graph is "what the app ships". The GUI links
# the engines, the report layer and the SDK, so starting here reaches
# everything without listing it.
ROOT_CRATE = "hydra-gui"

# Files a package might hold its licence in. Matched case-insensitively
# against the whole name: LICENSE, LICENSE.md, LICENSE-APACHE, licence.txt,
# COPYING, NOTICE, UNLICENSE.
#
# The extension list is closed, and the qualifier before it may not contain
# a dot. Allowing either to be `.anything` let `license_key.rs` through —
# a source file whose name begins with the word, whose contents are not a
# notice, and which would have been embedded in the app as one.
LICENSE_FILE_RE = re.compile(
    r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICEN[CS]E)"
    r"([-_][A-Za-z0-9-]+)*(\.(md|txt|rst))?$",
    re.IGNORECASE,
)

# A licence text is a page or two. Anything past this is a vendored corpus
# that happens to match the name pattern, and embedding it would bloat the
# binary for nothing.
MAX_TEXT_BYTES = 200_000


# ── Selection and normalisation (pure) ────────────────────────────────────────


def pick_license_files(names):
    """The licence-bearing file names among `names`, in a stable order.

    Sorted rather than left in directory order: the same package must
    produce the same text on every machine, and `os.listdir` promises
    nothing about order.
    """
    return sorted(n for n in names if LICENSE_FILE_RE.match(n))


def normalise_text(text: str) -> str:
    """The form two texts are compared in.

    Line endings and trailing whitespace differ between packages carrying
    what is otherwise the same licence — CRLF from a Windows contributor,
    a stray space before a newline. Neither changes what the licence says,
    and treating them as different texts would store the Apache-2.0 licence
    a hundred times over.
    """
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    return "\n".join(line.rstrip() for line in lines).strip() + "\n"


def dedupe(components):
    """Split components into a shared text pool and index references.

    Returns `(texts, components)` where each component's `files` is a list
    of `[file name, index into texts]` pairs — empty when the package
    shipped no licence file at all.

    Files stay separate rather than being concatenated per package. A
    dual-licensed crate ships LICENSE-APACHE beside LICENSE-MIT, and the
    Apache half is the same eleven kilobytes in every one of them while the
    MIT half carries a copyright line that is not. Joined, each pair is a
    unique text and the boilerplate is stored hundreds of times; kept
    apart, it is stored once.
    """
    texts = []
    index_of = {}
    out = []
    for c in components:
        files = []
        for name, body in c.get("files", []):
            key = hashlib.sha256(body.encode("utf-8")).hexdigest()
            if key not in index_of:
                index_of[key] = len(texts)
                texts.append(body)
            files.append([name, index_of[key]])
        out.append({**c, "files": files})
    return texts, out


def sort_key(component):
    """Alphabetical by name, then by version, ecosystem last.

    Version is compared as a string on purpose: this orders the file, it
    does not decide anything, and a semver parse here would be a second
    place for semver to be wrong.
    """
    return (component["name"].lower(), component["version"], component["ecosystem"])


# ── Collection (pure, given the tool output) ──────────────────────────────────


def rust_components(metadata):
    """Every crate the GUI links, from `cargo metadata` output.

    Walks the resolve graph from `hydra-gui` following *normal* dependency
    edges only. Dev-dependencies are test-time and build-dependencies run
    on the build machine; neither is inside the binary we hand to anyone,
    and including them would put notices in the file for code nobody
    receives.
    """
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = [p["id"] for p in metadata["packages"] if p["name"] == ROOT_CRATE]
    if not roots:
        raise SystemExit(f"error: {ROOT_CRATE} not found in cargo metadata")

    seen = set()
    stack = list(roots)
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            # `kind: null` is a normal dependency; "dev" and "build" are not.
            if any(k.get("kind") is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])

    out = []
    for pid in seen:
        pkg = packages[pid]
        # A workspace member is Hydra's own code, licensed by the file the
        # About panel shows separately.
        if pkg.get("source") is None:
            continue
        out.append(
            {
                "name": pkg["name"],
                "version": pkg["version"],
                "ecosystem": "rust",
                "spdx": pkg.get("license") or "",
                "url": pkg.get("repository") or "",
                "dir": str(pathlib.Path(pkg["manifest_path"]).parent),
            }
        )
    return out


def npm_components(listing):
    """Every production npm package, from `pnpm licenses list --prod --json`.

    pnpm groups by licence and lists a package once with parallel
    `versions` and `paths` arrays. They are zipped rather than assumed
    equal in length: a mismatch there would silently attribute one
    version's notice to another.
    """
    out = []
    for entries in listing.values():
        for entry in entries:
            versions = entry.get("versions") or []
            paths = entry.get("paths") or []
            for version, path in zip(versions, paths):
                out.append(
                    {
                        "name": entry["name"],
                        "version": version,
                        "ecosystem": "npm",
                        "spdx": entry.get("license") or "",
                        "url": entry.get("homepage") or "",
                        "dir": path,
                    }
                )
    return out


# ── Reading the texts (filesystem) ────────────────────────────────────────────


def read_license_files(directory: str):
    """The licence files a package ships, as `(name, normalised text)`."""
    d = pathlib.Path(directory)
    if not d.is_dir():
        return []
    named = []
    for name in pick_license_files(p.name for p in d.iterdir() if p.is_file()):
        f = d / name
        if f.stat().st_size > MAX_TEXT_BYTES:
            continue
        try:
            named.append((name, normalise_text(f.read_text(encoding="utf-8", errors="replace"))))
        except OSError:
            continue
    return named


def run_json(cmd, cwd=None):
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if proc.returncode != 0:
        raise SystemExit(f"error: {' '.join(cmd)} failed:\n{proc.stderr.strip()}")
    return json.loads(proc.stdout)


def collect():
    metadata = run_json(["cargo", "metadata", "--format-version", "1"], cwd=ROOT)
    listing = run_json(["pnpm", "licenses", "list", "--prod", "--json"], cwd=FRONTEND)

    components = rust_components(metadata) + npm_components(listing)

    # A package whose directory is not there reads as one shipping no
    # licence file, which is a wrong notice rather than a missing one — and
    # under `--check` it would fail as unexplained drift. Say what happened
    # instead.
    absent = [c for c in components if not pathlib.Path(c["dir"]).is_dir()]
    if absent:
        names = ", ".join(f"{c['name']} {c['version']}" for c in absent[:5])
        raise SystemExit(
            f"error: {len(absent)} package directories are missing ({names}…). "
            f"Run `cargo fetch` and `pnpm install` before generating notices."
        )

    for c in components:
        c["files"] = read_license_files(c.pop("dir"))
    components.sort(key=sort_key)

    texts, components = dedupe(components)
    return {
        "counts": {
            "rust": sum(1 for c in components if c["ecosystem"] == "rust"),
            "npm": sum(1 for c in components if c["ecosystem"] == "npm"),
        },
        "texts": texts,
        "components": components,
    }


def serialise(document) -> str:
    # Compact separators: this file is generated, read by a program, and
    # embedded in the binary — the whitespace of a pretty print is a
    # hundred kilobytes of nothing.
    return json.dumps(document, ensure_ascii=False, separators=(",", ":"), sort_keys=True) + "\n"


def main():
    check = "--check" in sys.argv[1:]
    document = serialise(collect())

    if check:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != document:
            raise SystemExit(
                f"error: {OUT.relative_to(ROOT)} is out of date — run `just licenses` "
                f"and commit the result."
            )
        print(f"{OUT.relative_to(ROOT)} is up to date.")
        return

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(document, encoding="utf-8")
    counts = json.loads(document)["counts"]
    print(
        f"Wrote {OUT.relative_to(ROOT)} — {counts['rust']} crates, "
        f"{counts['npm']} npm packages, {len(json.loads(document)['texts'])} distinct texts."
    )


if __name__ == "__main__":
    main()
