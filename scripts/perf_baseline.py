#!/usr/bin/env python3
"""Record and check a performance baseline.

Every win in this engine's performance work came from finding work that
should not have been happening. The same work can be reintroduced by
anyone, and nothing in the test suite would notice: the results stay
byte-identical while the run takes twice as long. This writes down what
the numbers were and fails when they move the wrong way.

    baseline.py record  models.json  > baseline.json
    baseline.py check   baseline.json

Times are best-of-N wall clock, and the tolerance is deliberately loose:
a regression gate that fires on machine noise gets disabled within a week.
It exists to catch a doubling, not a drift.
"""

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
HYDRA = str(ROOT / "target" / "release" / "hydra")
# The predecessor, for the ratio column. Set HYDRA_SWMM to point at a
# build of it; without one the baseline still records and checks Hydra's
# own times, which is what the gate is actually for.
SWMM = os.environ.get(
    "HYDRA_SWMM",
    str(ROOT.parent / "Stormwater-Management-Model" / "build" / "bin" / "Release" / "runswmm"),
)
# A run may be this much slower than its baseline before it is a failure.
TOLERANCE = 1.25


def peak_rss_kb(cmd, cwd, timeout):
    """Best-of-one peak resident set, from /usr/bin/time -l."""
    try:
        p = subprocess.run(["/usr/bin/time", "-l", *cmd], cwd=cwd,
                           capture_output=True, timeout=timeout, text=True)
    except subprocess.TimeoutExpired:
        return None
    for line in p.stderr.splitlines():
        if "maximum resident set size" in line:
            return int(line.split()[0]) // 1024
    return None


def run(model, repeat, timeout):
    """Best-of-N seconds and peak RSS for both engines on one model."""
    src = pathlib.Path(model)
    with tempfile.TemporaryDirectory() as d:
        work = pathlib.Path(d)
        for sib in src.parent.iterdir():
            if sib.is_file():
                shutil.copy2(sib, work / sib.name)
        m = work / src.name
        out = {}
        engines = [
            ("hydra", [HYDRA, "run", str(m), "--results", "h.out", "--summary", "h.rpt", "-q"])
        ]
        if pathlib.Path(SWMM).exists():
            engines.append(("swmm", [SWMM, str(m), "s.rpt", "s.out"]))
        for tag, cmd in engines:
            times = []
            for _ in range(repeat):
                t0 = time.monotonic()
                try:
                    p = subprocess.run(cmd, cwd=work, capture_output=True, timeout=timeout)
                except subprocess.TimeoutExpired:
                    return None
                # The predecessor exits zero having refused the model,
                # saying so only in its report. A benchmark that silently
                # measures a refusal reads as a 400x win.
                if p.returncode != 0:
                    return None
                rpt = work / ("h.rpt" if tag == "hydra" else "s.rpt")
                if rpt.exists() and "ERROR" in rpt.read_text(errors="replace"):
                    return None
                times.append(time.monotonic() - t0)
            out[tag] = round(min(times), 3)
            out[tag + "_rss_mb"] = round((peak_rss_kb(cmd, work, timeout) or 0) / 1024.0, 1)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("action", choices=["record", "check"])
    ap.add_argument("path")
    ap.add_argument("--repeat", type=int, default=3)
    ap.add_argument("--timeout", type=float, default=1800.0)
    args = ap.parse_args()

    if args.action == "record":
        models = json.loads(pathlib.Path(args.path).read_text())
        out = {}
        for name, model in models.items():
            r = run(str(ROOT / model), args.repeat, args.timeout)
            if r is None:
                print(f"  {name}: did not run", file=sys.stderr)
                continue
            r["model"] = model
            out[name] = r
            ref = (f"  swmm {r['swmm']}s ({r['swmm_rss_mb']} MB)  "
                   f"{r['hydra'] / r['swmm']:.2f}x") if "swmm" in r else ""
            print(f"  {name}: hydra {r['hydra']}s ({r['hydra_rss_mb']} MB){ref}",
                  file=sys.stderr)
        print(json.dumps(out, indent=2))
        return 0

    base = json.loads(pathlib.Path(args.path).read_text())
    bad = []
    print(f"{'model':22}{'baseline':>10}{'now':>10}{'change':>10}{'ratio now':>11}")
    for name, b in base.items():
        r = run(str(ROOT / b["model"]), args.repeat, args.timeout)
        if r is None:
            print(f"{name:22}   did not run")
            bad.append((name, "did not run"))
            continue
        change = r["hydra"] / b["hydra"]
        flag = "  SLOWER" if change > TOLERANCE else ""
        ratio = f"{r['hydra'] / r['swmm']:10.2f}x" if "swmm" in r else f"{'-':>11}"
        print(f"{name:22}{b['hydra']:10.3f}{r['hydra']:10.3f}{change:9.2f}x{ratio}{flag}")
        if change > TOLERANCE:
            bad.append((name, f"{change:.2f}x slower than baseline"))
    if bad:
        print(f"\n{len(bad)} regressed:", file=sys.stderr)
        for n, why in bad:
            print(f"  {n}: {why}", file=sys.stderr)
        return 1
    print("\nno regression")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
