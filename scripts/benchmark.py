#!/usr/bin/env python3
"""Benchmark the Hydra CLI end-to-end across the bundled fixture networks.

For each network in ``tests/benchmarks/wds/`` this runs the release ``hydra`` binary
(parse + full extended-period simulation + summary report) ``--runs`` times after
a warm-up run and records the best wall-clock time, alongside the network size.
It prints a Markdown table suitable for pasting into
``docs/src/reference/performance.md``.

Build the binary first, then run:

    cargo build --release -p hydra-cli
    python3 scripts/benchmark.py

Or via just:

    just bench-report

Options:
    --hydra PATH   Path to the hydra binary (default: target/release/hydra)
    --runs N       Timed runs per network, best is reported (default: 5)
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH_DIR = ROOT / "tests" / "benchmarks" / "wds"

# Slug -> display name. Networks absent from tests/benchmarks/wds/ are skipped.
NETWORKS = [
    ("balerma", "Balerma"),
    ("nytunnels", "NY Tunnels"),
    ("exnet", "Exnet"),
    ("richmond", "Richmond"),
    ("micropolis", "Micropolis"),
    ("dtown", "D-Town"),
    ("ltown", "L-Town"),
    ("ky8", "KY8"),
    ("ky9", "KY9"),
    ("ky10", "KY10"),
    ("bwsn2", "BWSN2"),
]


def metadata(hydra: str, inp: Path) -> tuple[int, int, int]:
    """Return (nodes, links, reporting_steps) from the JSON report."""
    with tempfile.TemporaryDirectory() as d:
        out = Path(d) / "report.json"
        subprocess.run(
            [hydra, str(inp), str(out)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        data = json.loads(out.read_text())
    i = data["input"]
    nodes = i["junctions"] + i["reservoirs"] + i["tanks"]
    links = i["pipes"] + i["pumps"] + i["valves"]
    report_step = i["report_timestep_s"]
    steps = int(round(i["duration_s"] / report_step)) + 1 if report_step else 1
    return nodes, links, steps


def best_time(hydra: str, inp: Path, runs: int) -> float:
    """Best of `runs` wall-clock seconds for a full run (after one warm-up)."""
    best = None
    for k in range(runs + 1):
        start = time.perf_counter()
        subprocess.run(
            [hydra, str(inp), "-q"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        elapsed = time.perf_counter() - start
        if k == 0:  # discard warm-up (warms the OS file cache)
            continue
        best = elapsed if best is None else min(best, elapsed)
    assert best is not None
    return best


def fmt_ms(seconds: float) -> str:
    ms = seconds * 1000.0
    return f"{ms:.0f} ms" if ms >= 10 else f"{ms:.1f} ms"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--hydra", default=str(ROOT / "target" / "release" / "hydra"))
    ap.add_argument("--runs", type=int, default=5)
    args = ap.parse_args()

    if not Path(args.hydra).exists():
        print(
            f"error: {args.hydra} not found — run `cargo build --release -p hydra-cli` first",
            file=sys.stderr,
        )
        return 1

    rows = []
    for slug, name in NETWORKS:
        inp = BENCH_DIR / f"{slug}.inp"
        if not inp.exists():
            print(f"skip: {inp} not found", file=sys.stderr)
            continue
        print(f"benchmarking {name} ...", file=sys.stderr, flush=True)
        nodes, links, steps = metadata(args.hydra, inp)
        elapsed = best_time(args.hydra, inp, args.runs)
        rows.append((name, nodes, links, steps, elapsed))

    rows.sort(key=lambda r: r[1])  # by node count, ascending

    print(f"| Network | Nodes | Links | Steps | Time (best of {args.runs}) |")
    print("|---|--:|--:|--:|--:|")
    for name, nodes, links, steps, elapsed in rows:
        print(f"| {name} | {nodes:,} | {links:,} | {steps:,} | {fmt_ms(elapsed)} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
