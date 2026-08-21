#!/usr/bin/env python3
"""Generate the drainage benchmark models.

The water-distribution benchmark runs published networks, vendored under
``tests/benchmarks/wds/``. Drainage has no equivalent suite this repository
can redistribute, so its models are generated instead: a committed
generator rather than a committed megabyte, deterministic, and sized by
argument rather than by whatever a third party happened to publish.

What it builds is a trunk sewer taking laterals, each node draining a
parcel, discharging through a storage basin with a weir and an orifice and
a pump. Diameters step up along the trunk but not enough for the design
storm, which is the point: the trunk surcharges, and a surcharged network
is where a dynamic-wave solver spends its time. A model that never
surcharges would benchmark the wrong thing.

    python3 scripts/make_uds_benchmark.py            # write every size
    python3 scripts/make_uds_benchmark.py --size s   # just one

Written to ``tests/benchmarks/uds/``, which is ignored by git.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "tests" / "benchmarks" / "uds"

# slug -> (trunk nodes, laterals per trunk node, hours simulated)
SIZES = {
    "s": (20, 4, 6),
    "m": (60, 8, 6),
    "l": (140, 12, 6),
}


def model(trunk: int, laterals: int, hours: int) -> str:
    """One model as INP text. Deterministic: no randomness anywhere."""
    lines: list[str] = []
    add = lines.append

    add("[TITLE]")
    add(f";; Hydra drainage benchmark: {trunk} trunk nodes x {laterals} laterals")
    add("")
    add("[OPTIONS]")
    add("FLOW_UNITS           CMS")
    add("INFILTRATION         HORTON")
    add("FLOW_ROUTING         DYNWAVE")
    add("LINK_OFFSETS         DEPTH")
    add("START_DATE           01/15/2024")
    add("START_TIME           00:00:00")
    add("END_DATE             01/15/2024")
    add(f"END_TIME             {hours:02d}:00:00")
    add("REPORT_STEP          0:05:00")
    add("WET_STEP             0:01:00")
    add("DRY_STEP             0:05:00")
    add("ROUTING_STEP         0:00:04")
    add("VARIABLE_STEP        0.65")
    add("INERTIAL_DAMPING     PARTIAL")
    add("NORMAL_FLOW_LIMITED  BOTH")
    add("SURCHARGE_METHOD     SLOT")
    add("ALLOW_PONDING        YES")
    add("MAX_TRIALS           20")
    add("HEAD_TOLERANCE       0.0015")
    add("")
    add("[RAINGAGES]")
    add("G1  INTENSITY  0:05  1.0  TIMESERIES  STORM")
    add("")

    # Ground falls along the trunk. A lateral chain climbs away from the
    # trunk node it joins, so `L{i}_0` sits just above `T{i}` and each
    # further one above the last: the chain drains down to the trunk.
    trunk_invert = lambda i: 100.0 - 0.30 * i
    lat_invert = lambda i, j: trunk_invert(i) + 0.35 * (j + 1)

    add("[JUNCTIONS]")
    for i in range(trunk):
        add(f"T{i}  {trunk_invert(i):.3f}  3.0  0  0  0")
        for j in range(laterals):
            add(f"L{i}_{j}  {lat_invert(i, j):.3f}  2.0  0  0  0")
    add("")
    add("[STORAGE]")
    add(f"BASIN  {trunk_invert(trunk):.3f}  4.0  0  FUNCTIONAL  120  0  0  0  0.5")
    add("")
    add("[OUTFALLS]")
    add(f"OUT  {trunk_invert(trunk) - 1.5:.3f}  FREE  NO")
    add("")

    add("[CONDUITS]")
    for i in range(trunk):
        down = f"T{i + 1}" if i + 1 < trunk else "BASIN"
        add(f"C_T{i}  T{i}  {down}  60  0.013  0  0")
        for j in range(laterals):
            up = f"L{i}_{j + 1}" if j + 1 < laterals else None
            if up is None:
                continue
            add(f"C_L{i}_{j}  {up}  L{i}_{j}  35  0.013  0  0")
        add(f"C_LJ{i}  L{i}_0  T{i}  25  0.013  0  0")
    add("")

    add("[WEIRS]")
    add("W_OVER  BASIN  OUT  TRANSVERSE  3.2  1.7  NO  0  0  NO")
    add("")
    add("[ORIFICES]")
    add("O_LOW  BASIN  OUT  BOTTOM  0  0.65  NO  0")
    add("")
    add("[PUMPS]")
    add("P_LIFT  BASIN  OUT  PC1  OFF  0.6  0.2")
    add("")
    add("[CURVES]")
    add("PC1  PUMP4  0.0  0.00")
    add("PC1        1.5  0.09")
    add("PC1        4.0  0.16")
    add("")

    add("[XSECTIONS]")
    for i in range(trunk):
        # Steps up downstream, and still short of the storm's peak.
        d = 0.25 + 0.35 * (i / max(trunk - 1, 1))
        add(f"C_T{i}  CIRCULAR  {d:.3f}  0  0  0")
        for j in range(laterals - 1):
            add(f"C_L{i}_{j}  CIRCULAR  0.150  0  0  0")
        add(f"C_LJ{i}  CIRCULAR  0.200  0  0  0")
    add("W_OVER  RECT_OPEN  1.2  3.0  0  0")
    add("O_LOW   CIRCULAR   0.25  0  0  0")
    add("")

    add("[SUBCATCHMENTS]")
    add("[SUBAREAS]")
    add("[INFILTRATION]")
    subs, areas, infil = [], [], []
    for i in range(trunk):
        for j in range(laterals):
            node = f"L{i}_{j}"
            subs.append(f"S{i}_{j}  G1  {node}  0.35  72  55  1.2  0")
            areas.append(f"S{i}_{j}  0.014  0.10  0.05  0.05  25  OUTLET")
            infil.append(f"S{i}_{j}  75  8  4  7  0")
    # Rebuild the three headers with their rows in order.
    lines = lines[:-3]
    add = lines.append
    add("[SUBCATCHMENTS]")
    lines.extend(subs)
    add("")
    add("[SUBAREAS]")
    lines.extend(areas)
    add("")
    add("[INFILTRATION]")
    lines.extend(infil)
    add("")

    add("[DWF]")
    for i in range(trunk):
        add(f"T{i}  FLOW  0.0004  HRLY")
    add("")
    add("[PATTERNS]")
    add("HRLY  HOURLY  0.4 0.3 0.3 0.3 0.4 0.7 1.2 1.6 1.7 1.5 1.3 1.2")
    add("HRLY          1.2 1.1 1.1 1.2 1.3 1.5 1.6 1.5 1.3 1.0 0.7 0.5")
    add("")

    add("[CONTROLS]")
    add("RULE R_PUMP")
    add("IF NODE BASIN DEPTH > 2.0")
    add("THEN PUMP P_LIFT STATUS = ON")
    add("ELSE PUMP P_LIFT STATUS = OFF")
    add("")

    # A design storm: dry, then a sharp hour, then a long recession. The
    # trunk fills during the peak and drains through the recession, so the
    # run covers both a surcharged network and a quiet one.
    add("[TIMESERIES]")
    profile = [0, 0, 4, 14, 38, 86, 130, 104, 62, 36, 21, 13, 8, 5, 3, 2]
    for k in range(hours * 12):
        mm = profile[k] if k < len(profile) else 0
        add(f"STORM  {k // 12:d}:{5 * (k % 12):02d}  {mm}")
    add("")
    add("[REPORT]")
    add("INPUT      NO")
    add("CONTINUITY YES")
    add("FLOWSTATS  YES")
    add("NODES      ALL")
    add("LINKS      ALL")
    add("")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--size", choices=sorted(SIZES), help="only this size")
    ap.add_argument("--out", default=str(OUT_DIR))
    args = ap.parse_args()

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    wanted = [args.size] if args.size else sorted(SIZES)
    for slug in wanted:
        trunk, laterals, hours = SIZES[slug]
        path = out_dir / f"bench_{slug}.inp"
        path.write_text(model(trunk, laterals, hours))
        nodes = trunk * (1 + laterals) + 2
        print(f"{path}  ({nodes:,} nodes, {trunk * laterals:,} parcels)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
