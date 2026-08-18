# Performance

Hydra ships no published performance figures. This page is about measuring it
yourself: how to build for speed, and how to check whether a change made the
solver slower.

Numbers are absent on purpose. The bundled networks were picked to exercise
the solver, not to represent anyone's system, and timings on a set chosen that
way say nothing a reader can carry over to their own model. Published figures
deserve a benchmark with real standing behind them, so there are none here
yet.

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
