#!/usr/bin/env python3
"""Open the desktop app on a chosen network, for marketing screenshots.

    scripts/screenshot-stage.py ~/models/city.inp
    scripts/screenshot-stage.py ~/models/city.inp --view analysis
    scripts/screenshot-stage.py ~/models/storm.inp --aux ~/models/rain.dat
    scripts/screenshot-stage.py                 # list what is staged
    scripts/screenshot-stage.py --reset ~/models/city.inp   # or: --reset all

The model is staged as an ordinary project bundle in your real app
profile, then the app launches with the dev-only boot override
(frontend/src/bootOverride.ts) landing it on the project's page. Staged
projects sit beside your own (delete them in-app or with --reset), and
everything your app already has applies to them: basemap tokens, canvas
preferences, theme.

`--isolate` stages into a scratch profile (~/.hydra-screenshots) by
pointing HOME at it instead. That keeps your project list clean, but on
macOS a foreign HOME has no login keychain, so basemap provider tokens
cannot be read there: tokened basemaps will not load. Isolate only when
a shot must not show your own projects anywhere.

The project id derives from the model's path, so the same file always
lands in the same project: simulation runs, CRS choices, and canvas
preferences made in the app survive across sessions, and a network only
ever needs to be run once. The staged copy of the model is kept too
(in-app edits win over the source file); use `--reset` to restage from
scratch after editing the source.

The engine is recognised from the file's section headers ([PIPES] and
friends read as water distribution, [SUBCATCHMENTS]/[CONDUITS] as
drainage); pass --engine to overrule. `--aux` files (rain records,
climate files) are copied beside the model.

Capture with the macOS screenshot tool: `screencapture -o -W out.png`
waits for a click on the window and omits the shadow. The window is
1440x900 from tauri.conf.json.
"""

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import uuid

REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_PROFILE = pathlib.Path.home() / ".hydra-screenshots"

# Mirrors ProjectView in crates/gui/frontend/src/projectConfig.ts.
VIEWS = ("overview", "canvas", "editor", "analysis", "report")

# Section headers that identify an engine's file format. Shared headers
# ([JUNCTIONS], [CURVES], [OPTIONS], ...) say nothing and are not listed.
# [PUMPS] appears in both formats and identifies neither.
WDS_SECTIONS = {"PIPES", "RESERVOIRS", "TANKS", "EMITTERS", "VALVES"}
UDS_SECTIONS = {"SUBCATCHMENTS", "CONDUITS", "OUTFALLS", "RAINGAGES", "XSECTIONS", "DIVIDERS"}


def sniff_engine(inp_text: str) -> str | None:
    """Which engine a model file belongs to, or None when its headers
    answer ambiguously (both engines', or neither's) and only the caller
    can say. [PUMPS] and [CURVES] exist in both formats' predecessors,
    but [PUMPS] beside [PIPES] is water distribution and beside
    [CONDUITS] is drainage; the sets above only hold unambiguous names.
    """
    sections = {m.group(1).upper() for m in re.finditer(r"^\s*\[(\w+)\]", inp_text, re.MULTILINE)}
    wds = bool(sections & WDS_SECTIONS)
    uds = bool(sections & UDS_SECTIONS)
    if wds == uds:
        return None
    return "wds" if wds else "uds"


# Element sections per engine, for meta.json's node/link counts. The counts
# gate the app's Run/Scenarios/Settings/Export offers (projectHasNetwork in
# frontend/src/hooks/projects.ts): left at zero, a staged project reads as
# "no network yet" and refuses to simulate. The app itself refreshes them
# only on save, which a freshly staged project has never done.
NODE_SECTIONS = {
    "wds": {"JUNCTIONS", "RESERVOIRS", "TANKS"},
    "uds": {"JUNCTIONS", "OUTFALLS", "DIVIDERS", "STORAGE"},
}
LINK_SECTIONS = {
    "wds": {"PIPES", "PUMPS", "VALVES"},
    "uds": {"CONDUITS", "PUMPS", "ORIFICES", "WEIRS", "OUTLETS"},
}


def count_elements(inp_text: str, engine: str) -> tuple[int, int]:
    """(nodes, links) by counting data lines in the element sections.

    One data line is one element in both formats, so this matches what the
    app will compute on first save; if a format quirk ever skews it, the
    save corrects meta.json and nothing downstream is harmed, because the
    gates only ask whether the counts are zero.
    """
    nodes = links = 0
    section = None
    for line in inp_text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith(";"):
            continue
        m = re.match(r"\[(\w+)\]", stripped)
        if m:
            section = m.group(1).upper()
        elif section in NODE_SECTIONS[engine]:
            nodes += 1
        elif section in LINK_SECTIONS[engine]:
            links += 1
    return nodes, links


def project_id_for(model_path: pathlib.Path) -> str:
    """Stable project id for a model file: the same path always maps to
    the same project, so staged state survives restaging."""
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"hydra-screenshot:{model_path}"))


def meta_for(name: str, engine: str, nodes: int, links: int) -> dict:
    """The meta.json a staged bundle starts with."""
    return {
        "version": 1,
        "name": name,
        "engine": engine,
        "nodeCount": nodes,
        "linkCount": links,
    }


def heal_counts(meta_path: pathlib.Path, nodes: int, links: int) -> bool:
    """Fill zero/absent counts in an existing meta.json; True if written.

    Repairs bundles staged before counts were written at all, without
    touching anything else the app has since recorded there (sourceCrs,
    unit system). Non-zero counts are left alone: they are the app's own,
    refreshed on save, and fresher than a recount of the staged file.
    """
    m = json.loads(meta_path.read_text())
    if m.get("nodeCount") or m.get("linkCount"):
        return False
    m["nodeCount"], m["linkCount"] = nodes, links
    meta_path.write_text(json.dumps(m, indent=2) + "\n")
    return True


def app_projects_dir(profile: pathlib.Path) -> pathlib.Path:
    return profile / "Library" / "Application Support" / "app.hydra" / "projects"


# Dropped into every bundle this script creates. The default profile is the
# real one, where staged projects sit beside the user's own — so --reset and
# the staged-list refuse to touch any bundle that does not carry the marker.
MARKER = "staged-for-screenshots.json"


def stage(profile: pathlib.Path, model: pathlib.Path, name: str, engine: str, aux: list[pathlib.Path]) -> str:
    """Write the project bundle unless already present; returns its id."""
    pid = project_id_for(model)
    base = app_projects_dir(profile) / pid / "base"
    (base / "aux").mkdir(parents=True, exist_ok=True)
    (base.parent / "scenarios").mkdir(exist_ok=True)
    if not (base / "model.inp").exists():
        shutil.copyfile(model, base / "model.inp")
    # Count what is actually staged, which after the first staging may be
    # older than the source file (in-app edits win; see --reset).
    nodes, links = count_elements((base / "model.inp").read_text(errors="replace"), engine)
    meta = base.parent / "meta.json"
    if not meta.exists():
        meta.write_text(json.dumps(meta_for(name, engine, nodes, links), indent=2) + "\n")
    elif heal_counts(meta, nodes, links):
        print(f"{model.name}: filled in the missing element counts")
    (base.parent / MARKER).write_text(json.dumps({"source": str(model)}, indent=2) + "\n")
    for f in aux:
        dst = base / "aux" / f.name
        if not dst.exists():
            shutil.copyfile(f, dst)
    return pid


def staged_bundles(profile: pathlib.Path) -> list[pathlib.Path]:
    """Bundle dirs this script created, and no others."""
    projects = app_projects_dir(profile)
    if not projects.is_dir():
        return []
    return sorted(p.parent for p in projects.glob(f"*/{MARKER}"))


def launch_env(project_id: str, view: str, scratch_home: pathlib.Path | None) -> dict:
    """Environment for `cargo tauri dev`. With `scratch_home` set
    (--isolate), the app's HOME moves there while the toolchain's stays
    put; without it the environment is untouched, so the keychain, and
    with it every stored basemap token, keeps working."""
    env = os.environ.copy()
    if scratch_home is not None:
        home = pathlib.Path.home()
        env.setdefault("CARGO_HOME", str(home / ".cargo"))
        env.setdefault("RUSTUP_HOME", str(home / ".rustup"))
        env["HOME"] = str(scratch_home)
    env["VITE_HYDRA_BOOT_PROJECT"] = project_id
    env["VITE_HYDRA_BOOT_VIEW"] = view
    return env


def list_staged(profile: pathlib.Path) -> int:
    bundles = staged_bundles(profile)
    if not bundles:
        print("Nothing staged.")
        return 0
    for bundle in bundles:
        try:
            m = json.loads((bundle / "meta.json").read_text())
        except (OSError, json.JSONDecodeError):
            continue
        source = json.loads((bundle / MARKER).read_text()).get("source", "?")
        print(f"{m.get('engine', '?')}  {m.get('name', '?')}  ← {source}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0], usage=argparse.SUPPRESS)
    ap.add_argument("model", nargs="?", help="path to the .inp to open; omit to list staged networks")
    ap.add_argument("--view", choices=VIEWS, default="canvas")
    ap.add_argument("--engine", choices=("wds", "uds"), help="overrule format recognition")
    ap.add_argument("--name", help="display name (default: the file's stem)")
    ap.add_argument("--aux", action="append", default=[], help="auxiliary file (rain, climate); repeatable")
    ap.add_argument(
        "--isolate",
        action="store_true",
        help="stage into a scratch profile instead of your real one (tokened basemaps unavailable there)",
    )
    ap.add_argument("--stage-only", action="store_true", help="stage without launching")
    ap.add_argument("--reset", action="store_true", help="wipe the staged bundle for <model> (or 'all')")
    args = ap.parse_args()
    profile = DEFAULT_PROFILE if args.isolate else pathlib.Path.home()

    if args.reset:
        if args.model == "all":
            for bundle in staged_bundles(profile):
                shutil.rmtree(bundle, ignore_errors=True)
                print(f"reset {bundle.name}")
            return 0
        if not args.model:
            ap.error("--reset needs a model path or 'all'")
        model = pathlib.Path(args.model).expanduser().resolve()
        bundle = app_projects_dir(profile) / project_id_for(model)
        if not (bundle / MARKER).is_file():
            # Never delete a bundle this script cannot prove it created —
            # in the real profile the neighbours are the user's projects.
            print(f"{model.name}: not staged by this script; nothing deleted", file=sys.stderr)
            return 1
        shutil.rmtree(bundle, ignore_errors=True)
        print(f"{model.name}: reset")
        return 0

    if not args.model:
        return list_staged(profile)

    model = pathlib.Path(args.model).expanduser().resolve()
    if not model.is_file():
        print(f"not a file: {model}", file=sys.stderr)
        return 1
    engine = args.engine or sniff_engine(model.read_text(errors="replace"))
    if engine is None:
        print(
            f"{model.name}: could not tell which engine this model belongs to; pass --engine wds|uds",
            file=sys.stderr,
        )
        return 1
    aux = [pathlib.Path(a).expanduser().resolve() for a in args.aux]
    for f in aux:
        if not f.is_file():
            print(f"not a file: {f}", file=sys.stderr)
            return 1

    pid = stage(profile, model, args.name or model.stem, engine, aux)
    print(f"{model.name}: staged as {pid} ({engine})")
    if args.stage_only:
        return 0

    print(f"Opening on the {args.view} view…")
    print("Capture with: screencapture -o -W shot.png  (then click the window)")
    return subprocess.run(
        ["cargo", "tauri", "dev"],
        cwd=REPO / "crates" / "gui",
        env=launch_env(pid, args.view, profile if args.isolate else None),
    ).returncode


if __name__ == "__main__":
    sys.exit(main())
