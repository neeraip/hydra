# Migrating from EPANET

This page is for engineers and developers switching from EPANET to Hydra. It covers what works out of the box, what to expect numerically, and where behaviour intentionally differs.

---

## Your `.inp` Files Work

Hydra parses the EPANET `.inp` format directly: any 2.x release. No conversion is needed. Pass your existing `.inp` file to the CLI or the library and Hydra will run it.

**The command line is Hydra's own, not EPANET's.** Hydra deliberately does not
mimic `epanet input.inp report.rpt output.out`: that argument order encodes one
engine and one pair of artifacts, which stops being true as Hydra adds engines.
File-format compatibility and command-line compatibility are separate promises,
and Hydra keeps the first.

| EPANET | Hydra |
|---|---|
| `epanet net.inp net.rpt` | `hydra run net.inp --summary net.rpt` |
| `epanet net.inp net.rpt net.out` | `hydra run net.inp --summary net.rpt --results net.out` |

See [INP Format Support](inp-format.md) for the full section-by-section reference.

---

## Output Formats

| Format | Compatibility |
|---|---|
| `.out` binary | EPANET-compatible. Post-processing tools that read EPANET binary output files will work with Hydra's output. |
| `.rpt` text report | EPANET-style **summary** report (header, input summary, warnings, analysis timestamps). It does not include per-node/link result tables; use the `.out` file for those. |
| `.json` report | Hydra extension (not an EPANET format). |

---

## Expect Small Numerical Differences

Hydra and EPANET solve the same physics using the same Global Gradient Algorithm, but they follow independent numerical paths. On most networks you will see differences of less than 0.1% in head and flow values. These are not bugs; they are the expected consequence of floating-point arithmetic being non-associative.

The practical impact depends on network topology. Simple, stable networks with
few controls and no quality agree to well within a rounding error. Differences
grow with the number of demand nodes and control switches, because a step that
lands either side of a control threshold changes what happens next.

Quality results are the most sensitive, and for a structural reason rather than
a numerical one: quality **integrates** the hydraulic solution. A flow
difference too small to notice in heads is carried into transport, where it
compounds across periods. If your workflow depends on sub-percent quality agreement with EPANET output, treat both results as independent estimates of the same physical system; neither is more "correct" than the other in an absolute sense.

**Hydra's result is authoritative.** If you observe a difference and suspect a Hydra bug, open a [GitHub issue](https://github.com/neeraip/hydra/issues) with a minimal reproducer.

---

## Behavioural Differences

### Unbalanced-stop mode

EPANET halts the simulation when a hydraulic step does not converge within the configured iteration limit (`UNBALANCED STOP`). Hydra honours this setting: when a hydraulic step is genuinely unbalanced (fails to converge), Hydra also halts and records an `UnbalancedHydraulics` warning. The `UNBALANCED CONTINUE N` option is also supported.

Because the two engines follow independent numerical paths, the step at which
non-convergence first occurs can differ, so the same model may halt at
different periods, or converge throughout in one engine and stop partway in the
other, even though both apply the same rule.

There is a second, harder stop in both engines: if the linear system becomes
singular and no control valve can be demoted to recover it, the run aborts with
no result saved for that step. That differs from the unbalanced stop, which
saves the failing step before ending.

### Quality timestep handling

EPANET's quality timestep can reach 0 seconds via integer truncation when hydraulic timesteps are very short. Hydra keeps the quality timestep as a real number and, when it is 0 or unset, defaults it to one-tenth of the hydraulic timestep, so it never truncates to zero. An explicitly set sub-second step is used as given.

This only matters for networks with very short hydraulic timesteps (well under 60 seconds), which is unusual in practice.

### FIFO tank quality while filling

For a FIFO (plug-flow) tank that is filling, EPANET reports the tank node's quality as approximately the **inflow** concentration, even when the tank is still full of water at a different concentration. Hydra reports the concentration at the tank's **outlet end** (its oldest water): the water the tank would actually deliver to the network. During a long fill of a tank whose initial water differs from the inflow, the two reports diverge until the old water flushes through. Water delivered downstream is identical in both engines; only the reported tank-node value differs.

---

## Newer EPANET Features Worth Knowing

Both are fully supported by Hydra, and both are optional: a file that uses neither still loads and runs.

### FAVAD Leakage (OWA-EPANET 2.3)

Per-pipe background leakage is modelled using the FAVAD (Fixed and Variable Area Discharge) model, configured via a `[LEAKAGE]` section in the `.inp` file. This section is the one genuine 2.3 addition. Older files (without `[LEAKAGE]`) parse cleanly; leakage is simply zero for all pipes.

### Pressure-Dependent Analysis (EPA EPANET 2.2)

PDA is configured exactly as in EPANET (`DEMAND MODEL PDA` in `[OPTIONS]`, with `MINIMUM PRESSURE`, `REQUIRED PRESSURE`, and `PRESSURE EXPONENT`). No changes needed.

---

## EPANET API Mapping

If you are migrating code that uses the EPANET Toolkit C API, the equivalent Hydra library workflow is:

| EPANET Toolkit | Hydra library |
|---|---|
| `EN_createproject` + `EN_open` | `io::parse(&bytes)` + `Simulation::from_network(network)` |
| `EN_runH` (full hydraulics) | `sim.run_hydraulics()` |
| `EN_runQ` (full quality) | `sim.run_quality()` |
| `EN_runH` + `EN_runQ` combined | `sim.run()` |
| `EN_nextH` | `sim.step_hydraulics()` |
| `EN_nextQ` | `sim.step_quality()` |
| `EN_getnodevalue(EN_HEAD)` | `sim.get_node_result(id, NodeQuantity::Head, t)` |
| `EN_getnodevalue(EN_PRESSURE)` | `sim.get_node_result(id, NodeQuantity::GaugePressure, t)` |
| `EN_getlinkvalue(EN_FLOW)` | `sim.get_link_result(id, LinkQuantity::Flow, t)` |
| `EN_deleteproject` | Drop the `Simulation`, handled by Rust's ownership system |

See the [SDK overview](../sdk/overview.md) for complete library usage examples.
