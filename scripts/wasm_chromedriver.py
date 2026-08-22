#!/usr/bin/env python3
"""Resolve a chromedriver matching the Chrome installed on this machine.

`wasm-pack test --chrome` downloads a chromedriver when it cannot find one
on the `PATH`, and it downloads the newest one rather than one matching the
browser it is about to drive. When the two majors differ, the new session
request 404s, the driver is killed, and the entire failure reads:

    driver status: signal: 9 (SIGKILL)
    Error: http status: 404

Nothing in that names a version, or a browser, or a driver. `just test-wasm`
is the only check that executes engine code on wasm, so when it fails this
way it stops being run, and the wasm commitment goes unverified while the
engines keep changing. That is not hypothetical: it is what happened here.

This prints the path to a driver whose major version matches the installed
Chrome, preferring one already on the machine and downloading from Chrome
for Testing only when nothing on the machine will do. When it cannot, it
says which two versions disagreed instead of leaving a 404 behind.

Usage:
    python3 scripts/wasm_chromedriver.py        # prints a path on stdout
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import platform
import re
import shutil
import subprocess
import sys
import urllib.request
import zipfile

VERSION = re.compile(r"(\d+\.\d+\.\d+\.\d+)")

# Latest patch of every Chrome build, with the driver download for each.
# Keyed by build (major.minor.build), which is the key the installed
# browser's own version gives us.
CATALOG = (
    "https://googlechromelabs.github.io/chrome-for-testing/"
    "latest-patch-versions-per-build-with-downloads.json"
)

# Where Chrome lives when it is not on the PATH, which on macOS it never is.
CHROME_CANDIDATES = (
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
)
CHROME_ON_PATH = (
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "chrome",
)


def version_tuple(v: str) -> tuple[int, ...]:
    """A dotted version as integers, for ordering."""
    return tuple(int(p) for p in v.split("."))


def major_of(v: str) -> str:
    """The major version, which is the part a driver has to agree on."""
    return v.split(".")[0]


def build_of(v: str) -> str:
    """The build (major.minor.build), which is the catalog's key."""
    return ".".join(v.split(".")[:3])


def platform_key(system: str, machine: str) -> str | None:
    """The Chrome for Testing platform name for this host, if it has one."""
    machine = machine.lower()
    if system == "Darwin":
        return "mac-arm64" if machine in ("arm64", "aarch64") else "mac-x64"
    if system == "Linux":
        return "linux64" if machine in ("x86_64", "amd64") else None
    if system == "Windows":
        return "win64" if machine in ("amd64", "x86_64") else "win32"
    return None


def pick_driver(candidates, want_major: str):
    """The newest candidate agreeing with `want_major`, or None.

    Candidates are (path, version) pairs. Newest wins so that a machine
    holding several cached drivers for one major uses the latest patch,
    which is the pairing the browser vendor tests.
    """
    matching = [c for c in candidates if major_of(c[1]) == want_major]
    if not matching:
        return None
    return max(matching, key=lambda c: version_tuple(c[1]))[0]


def driver_url(payload, build: str, plat: str) -> str | None:
    """The driver download for a build, falling back within the major.

    A browser can be newer than the catalog's entry for its own build, so
    an exact miss falls back to the newest build sharing its major rather
    than giving up: majors are what the driver protocol requires to agree.
    """
    builds = payload.get("builds", {})
    entry = builds.get(build)
    if entry is None:
        same = [b for b in builds if major_of(b) == major_of(build)]
        if not same:
            return None
        entry = builds[max(same, key=version_tuple)]
    for d in entry.get("downloads", {}).get("chromedriver", []):
        if d.get("platform") == plat:
            return d.get("url")
    return None


def binary_version(path: str) -> str | None:
    """The four-part version a browser or driver reports for itself."""
    try:
        out = subprocess.run([path, "--version"], capture_output=True,
                             text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    hit = VERSION.search(out.stdout or out.stderr or "")
    return hit.group(1) if hit else None


def find_chrome() -> tuple[str, str] | None:
    """The Chrome this test will drive, and its version."""
    named = os.environ.get("CHROME") or os.environ.get("CHROME_PATH")
    seen = [named] if named else []
    seen += [p for p in CHROME_CANDIDATES if pathlib.Path(p).exists()]
    seen += [p for p in (shutil.which(n) for n in CHROME_ON_PATH) if p]
    for p in seen:
        v = binary_version(p)
        if v:
            return p, v
    return None


def cached_drivers(root: pathlib.Path):
    """Every chromedriver already on this machine, with its version.

    wasm-pack keeps its downloads in a per-user cache and accumulates one
    directory per version, so a machine that has run this test before very
    often already holds the driver it needs.
    """
    found = []
    named = os.environ.get("CHROMEDRIVER")
    paths = [pathlib.Path(named)] if named else []
    on_path = shutil.which("chromedriver")
    if on_path:
        paths.append(pathlib.Path(on_path))
    for cache in (pathlib.Path.home() / "Library/Caches/.wasm-pack",
                  pathlib.Path.home() / ".cache/.wasm-pack"):
        paths += sorted(cache.glob("chromedriver-*/chromedriver"))
    paths += sorted(root.glob("*/chromedriver"))
    for p in paths:
        if not p.exists():
            continue
        v = binary_version(str(p))
        if v:
            found.append((str(p), v))
    return found


def download(url: str, into: pathlib.Path) -> pathlib.Path:
    """Fetch and unpack one driver, returning the executable's path."""
    into.mkdir(parents=True, exist_ok=True)
    archive = into / "chromedriver.zip"
    with urllib.request.urlopen(url, timeout=120) as r, archive.open("wb") as f:
        shutil.copyfileobj(r, f)
    with zipfile.ZipFile(archive) as z:
        member = next(n for n in z.namelist()
                      if n.rsplit("/", 1)[-1] in ("chromedriver", "chromedriver.exe"))
        with z.open(member) as src, (into / "chromedriver").open("wb") as dst:
            shutil.copyfileobj(src, dst)
    archive.unlink()
    exe = into / "chromedriver"
    exe.chmod(0o755)
    return exe


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--cache", default="target/wasm-chromedriver",
                    help="where a downloaded driver is kept")
    args = ap.parse_args()
    root = pathlib.Path(args.cache)

    chrome = find_chrome()
    if chrome is None:
        print("no Chrome found: `just test-wasm` drives the system browser, so "
              "install Chrome or point $CHROME at it", file=sys.stderr)
        return 1
    chrome_path, chrome_version = chrome
    want = major_of(chrome_version)

    have = cached_drivers(root)
    picked = pick_driver(have, want)
    if picked:
        print(picked)
        return 0

    plat = platform_key(platform.system(), platform.machine())
    if plat is None:
        print(f"no Chrome for Testing driver is published for "
              f"{platform.system()}/{platform.machine()}", file=sys.stderr)
        return 1

    print(f"Chrome {chrome_version} at {chrome_path} needs a {want}.x driver; "
          f"none of the {len(have)} on this machine matches. Fetching one.",
          file=sys.stderr)
    try:
        with urllib.request.urlopen(CATALOG, timeout=60) as r:
            payload = json.load(r)
        url = driver_url(payload, build_of(chrome_version), plat)
        if url is None:
            print(f"Chrome for Testing publishes no {want}.x driver for {plat}",
                  file=sys.stderr)
            return 1
        exe = download(url, root / build_of(chrome_version))
    except OSError as e:
        print(f"could not fetch a {want}.x chromedriver for {plat}: {e}",
              file=sys.stderr)
        return 1
    print(exe)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
