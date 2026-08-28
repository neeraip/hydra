#!/usr/bin/env python3
"""Generate the water-distribution benchmark models.

The vendored networks under ``tests/benchmarks/wds/`` are published ones,
and every single one of them finishes in under half a second. That is too
short to measure a solver: process start and parsing dominate the ratio, and
a 3x deficit on a large network sat undetected behind them for a year. These
are generated instead, for the same reasons drainage generates its own: a
committed generator rather than a committed megabyte, deterministic, and
sized by argument rather than by whatever a third party happened to publish.

What it builds is a branched network of trunk mains with laterals, a few
cross-connections closing loops, a tank, pattern-driven demands and a
scattering of throttle valves.

The link-to-junction ratio is the thing to get right, and it is close to
one. A published distribution network is very nearly a tree: the model this
was calibrated against has 46,564 links to 46,171 junctions, a few hundred
loops in a forest. A full grid instead has two links per junction, and its
fill-in makes the factorisation 93% of every iteration where a real network
spends 40% there. Benchmarking the grid measures Cholesky; benchmarking
this measures the solver.

    python3 scripts/make_wds_benchmark.py            # write every size
    python3 scripts/make_wds_benchmark.py --size l   # just one

Written to ``tests/benchmarks/wds/``, alongside the vendored networks; the
generated files are ignored by git.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "tests" / "benchmarks" / "wds"

# slug -> (rows, columns, hours). Junctions = rows·cols; links ≈ 1.1·junctions.
SIZES = {
    "gen_s": (40, 40, 24),
    "gen_m": (90, 90, 24),
    "gen_l": (215, 215, 24),
}

# A cross-connection every this many columns closes loops without turning the
# network into a grid.
LOOP_EVERY = 12

# One valve per this many mains, so status checking is exercised without the
# network becoming a valve benchmark.
VALVE_EVERY = 40


def model(rows: int, cols: int, hours: int) -> str:
    side = max(rows, cols)
    out: list[str] = []
    w = out.append

    w("[TITLE]")
    w(f"Generated branched network, {rows}x{cols} junctions, {hours} h EPS")
    w("")

    # Sources along one edge rather than a single corner tie-in. One source
    # feeding a large grid has to push the whole demand through one pipe,
    # which drives pressures negative and benchmarks the warning path
    # instead of the solver. Real distribution systems have several sources,
    # and the count scales with the grid so every size is served.
    sources = list(range(0, rows, 16))
    w("[RESERVOIRS]")
    w(";ID              Head")
    for c in sources:
        w(f" SRC{c:<13} 380")
    w("")

    # A tank on the far corner gives the run tank dynamics: level integration,
    # the fill/drain status checks, and a second boundary the solve must
    # balance against.
    w("[TANKS]")
    w(";ID              Elev   InitLvl  MinLvl  MaxLvl  Diam   MinVol  VolCurve")
    w(" TNK             150    20       2       32      120    0")
    w("")

    w("[JUNCTIONS]")
    w(";ID              Elev    Demand   Pattern")
    for i in range(rows):
        for j in range(cols):
            # A gentle elevation ramp across the grid, so head loss is not
            # uniform and the solve has real gradients to resolve. The total
            # rise is fixed rather than the per-step rise: a constant slope
            # would put the far corner of the largest grid above the tank
            # that serves it, which is a badly posed model rather than a
            # bigger one.
            elev = 100 + (i + j) * (40.0 / (rows + cols - 2))
            w(f" J{i}_{j:<12} {elev:<7.2f} {3.0:<8.2f} DIURNAL")
    w("")

    w("[PIPES]")
    w(";ID              Node1           Node2           Length  Diam   Rough  MinorLoss Status")
    pid = 0
    valves: list[tuple[int, str, str]] = []
    for i in range(rows):
        for j in range(cols):
            # Along the row: the trunk main and its laterals. This alone is a
            # forest of rows, one per source tie-in.
            if j + 1 < cols:
                pid += 1
                trunk = i % 8 == 0
                diam = 16.0 if trunk else 8.0
                if pid % VALVE_EVERY == 0:
                    valves.append((pid, f"J{i}_{j}", f"J{i}_{j + 1}"))
                else:
                    w(f" P{pid:<15} J{i}_{j:<13} J{i}_{j + 1:<13} "
                      f"{300.0:<7.1f} {diam:<6.1f} {130:<6} {0.0:<9.1f} Open")
            # Across rows, only every LOOP_EVERY columns: enough to close
            # loops and give the solve something to circulate around,
            # without the fill-in of a full grid.
            if i + 1 < rows and j % LOOP_EVERY == 0:
                pid += 1
                w(f" P{pid:<15} J{i}_{j:<13} J{i + 1}_{j:<13} "
                  f"{400.0:<7.1f} {12.0:<6.1f} {130:<6} {0.0:<9.1f} Open")
    # Source and tank tie-ins.
    for c in sources:
        pid += 1
        w(f" P{pid:<15} {f'SRC{c}':<14} {f'J{c}_0':<14} "
          f"{200.0:<7.1f} {24.0:<6.1f} {130:<6} {0.0:<9.1f} Open")
    pid += 1
    w(f" P{pid:<15} {f'J{rows-1}_{cols-1}':<14} {'TNK':<14} "
      f"{200.0:<7.1f} {20.0:<6.1f} {130:<6} {0.0:<9.1f} Open")
    w("")

    w("[VALVES]")
    w(";ID              Node1           Node2           Diam   Type  Setting MinorLoss")
    for vid, a, b in valves:
        w(f" V{vid:<15} {a:<15} {b:<15} {8.0:<6.1f} TCV   {2.5:<7.1f} 0")
    w("")

    w("[PATTERNS]")
    w(";ID              Multipliers")
    # A 24-point diurnal curve: the demands move, so every reporting step is a
    # genuinely different solve rather than the same one repeated.
    mult = [0.62, 0.55, 0.52, 0.53, 0.60, 0.78, 1.05, 1.32, 1.42, 1.35, 1.24, 1.18,
            1.15, 1.12, 1.14, 1.20, 1.32, 1.45, 1.42, 1.28, 1.10, 0.95, 0.80, 0.70]
    for k in range(0, 24, 6):
        w(" DIURNAL         " + "  ".join(f"{m:.2f}" for m in mult[k:k + 6]))
    w("")

    w("[OPTIONS]")
    w(" Units               GPM")
    w(" Headloss            H-W")
    w(" Specific Gravity    1.0")
    w(" Viscosity           1.0")
    w(" Trials              200")
    w(" Accuracy            0.001")
    w(" Unbalanced          Continue 10")
    w(" Pattern             DIURNAL")
    w(" Demand Multiplier   1.0")
    w(" Emitter Exponent    0.5")
    w(" Quality             None")
    w("")

    w("[TIMES]")
    w(f" Duration            {hours}:00")
    w(" Hydraulic Timestep  1:00")
    w(" Quality Timestep    0:05")
    w(" Pattern Timestep    1:00")
    w(" Report Timestep     1:00")
    w(" Report Start        0:00")
    w("")

    w("[REPORT]")
    w(" Status              No")
    w(" Summary             No")
    w(" Page                0")
    w("")

    w("[END]")
    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--size", choices=sorted(SIZES), help="only this size")
    args = ap.parse_args()

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for slug in ([args.size] if args.size else sorted(SIZES)):
        rows, cols, hours = SIZES[slug]
        path = OUT_DIR / f"{slug}.inp"
        text = model(rows, cols, hours)
        path.write_text(text)
        links = text.count("\n P")
        print(f"{path}  ({rows * cols:,} junctions, {links:,} links, "
              f"{links / (rows * cols):.2f} links/junction)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
