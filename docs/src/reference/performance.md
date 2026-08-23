# Performance

Both engines are benchmarked against their predecessors: the drainage
engine against SWMM 5.2.4 and the water distribution engine against
EPANET, on published networks. This page carries the published figures,
the method behind them, and the tools to reproduce or check them on your
own machine.

## Measured against SWMM 5.2.4

Release builds of both engines, the same machine (Apple M-series), best
of three runs each, timed end to end: parse, solve, and write. SWMM runs
in its own default surcharge closure. Where a model sets
`INERTIAL_DAMPING`, SWMM is given the setting Hydra substitutes, so the
comparison is between implementations and not between documented
modelling deviations.

| Workload | Hydra / SWMM runtime |
|---|---|
| Bellinge (published, 1,020 nodes), 7 h storm | 1.00 |
| Bellinge, 24 h event | 1.24 |
| SWMM test corpus, models running 0.2 to 1 s | 1.08 |
| A 4,394-node combined system, 48 h dynamic wave | **0.66** |

The corpus is the predecessor's own regression suite. Its many
sub-0.2-second models are dominated by process start and parse and are
excluded from the ratio, because a single aggregate over them measures
process creation rather than either solver.

Accuracy rides the same runs. On the 48-hour system, 4,373 of 4,394 node
depths agree within 5 cm. On Bellinge, node depths match SWMM's own
Preissmann-slot closure on 992 of 1,020 nodes. Every remaining
difference across the corpus is either fixed or documented in the
specification with its cause and a source citation; the specifications'
correspondence notes are the index of them.

Memory on the 4,394-node model (a 320 MB input): 613 MB peak during
import, 194 MB for the rest of the run, against SWMM's roughly 160 MB.
SWMM reads the file from disk in passes; Hydra holds the model bytes in
memory, which is also what lets the same engine run in a browser.

## Water distribution, measured against EPANET

The same method: release builds of both engines, the same machine, best
of three runs, timed end to end, over the published research networks
bundled in `tests/benchmarks/wds/`.

| Network | Hydra / EPANET runtime |
|---|---|
| Balerma, Exeter, Kentucky 8/9/10, NY Tunnels | 0.9 to 1.2 (sub-10 ms; process start dominates) |
| BWSN-2 (12,527 nodes) | 1.21 |
| L-Town | 1.03 |
| D-Town | 1.12 |
| Micropolis | 1.29 |
| Richmond | 2.00, or **0.85 with `--tank-tolerance 0`** |

Richmond's ratio is a documented purchase, not a loss. Hydra integrates
tank levels with a second-order predictor-corrector carrying a per-step
error estimate, where EPANET takes one uncontrolled first-order step;
on Richmond's eighteen level-switched pumps that costs a corrector solve
per step plus the error control's retries. Setting the tolerance to
zero (`--tank-tolerance 0`, or `level_err_tol = 0` in the session API)
restores EPANET's own scheme exactly: 55 hydraulic solves to EPANET's
54, and a faster run. The default keeps the error bound.

Accuracy rides the same runs: 95.4% of 2.6 million node pressures agree
within 0.1 (in each model's own pressure unit) across the set, and every
residual is classified in the specifications' correspondence notes.
The survivors are three kinds, none a solver disagreement: isolated
pockets behind closed links, whose head is physically undefined and
where EPANET's own value wanders between hours; runs both engines halt
identically under `Unbalanced Stop`, where only the abandoned final
iterate differs; and threshold events, where centimetre-scale tank
drift between two different integrators flips a level-switched rule by
one time step and the trajectories re-converge.

## Reproducing the comparison

The tracked baseline harness times both engines when SWMM is available:

```sh
HYDRA_SWMM=/path/to/runswmm just perf-check
```

Without `HYDRA_SWMM` it times Hydra alone against the recorded baseline.
The water networks are timed by `just bench-report`; comparing against
EPANET means building EPANET's `runepanet` from its repository and
timing the same models with both, best-of-N, as above.
The baseline (`tests/benchmarks/uds/baseline.json`) is tracked in the
repository and gates every change on both runtime (25% band) and peak
memory (15% band), so the published figures cannot quietly rot: a change
that slows a benchmark model or grows its memory fails the check.

Timing methodology worth copying: always best-of-N, never single-shot
(single runs on the 24-hour event scattered across 57 to 60 seconds
where best-of-three gives 50.4), and read the report for errors rather
than trusting the exit code, because SWMM exits zero after refusing a
model.

## Checking for regressions

The bundled networks are worth timing against each other. Comparing one Hydra
build with another over a fixed set answers the question that matters during
development: did this change make the solver slower? The arbitrariness that
makes these networks useless as a public claim costs nothing here, because
both runs meet the same models.

```sh
just bench-report
```

This builds the release CLI and runs `scripts/benchmark.py`, which times each
of the networks in `tests/benchmarks/wds/` and prints a Markdown table. Pass
`--runs N` to change the sample count, or `--hydra PATH` to time a specific
binary, which is how two builds are compared.

Note that it reads only the bundled directory, so it measures Hydra against
Hydra. It is not a way to time your own model.

## Building for maximum speed

The release profile already enables fat LTO and a single codegen unit. For the best local performance, build with native CPU features:

```sh
just release-native
```

This tunes the binary for the machine it is built on (`-C target-cpu=native`); such binaries are not portable to older CPUs.

## Solver micro-benchmarks

For work on the solver itself, the criterion suite times the hydraulic solve step (warm and cold) in isolation:

```sh
just bench
```
