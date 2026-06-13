# Migrating from EPANET

This page is for engineers and developers switching from EPANET to Hydra. It covers what works out of the box, what to expect numerically, and where behaviour intentionally differs.

---

## Your `.inp` Files Work

Hydra parses the EPANET 2.3 `.inp` format directly. No conversion is needed. Drop your existing `.inp` file into the CLI or pass it to the library and Hydra will run it.

See [INP Format Support](inp-format.md) for the full section-by-section reference.

---

## Output Formats

| Format | Compatibility |
|---|---|
| `.out` binary | EPANET-compatible. Post-processing tools that read EPANET binary output files will work with Hydra's output. |
| `.rpt` text report | EPANET-style report. Field names and layout follow EPANET conventions. |
| `.json` report | Hydra extension (not an EPANET format). |

---

## Expect Small Numerical Differences

Hydra and EPANET solve the same physics using the same Global Gradient Algorithm, but they follow independent numerical paths. On most networks you will see differences of less than 0.1% in head and flow values. These are not bugs; they are the expected consequence of floating-point arithmetic being non-associative.

The practical impact depends on network topology:

| Network type | Typical difference |
|---|---|
| Simple, stable (few controls, no quality) | < 0.01% (effectively zero) |
| Medium complexity (KY8/KY9/KY10 scale) | 3–4 individual node/link values at t=0 |
| Large with quality simulation (D-Town scale) | Flow differences at t=0 can cascade into quality concentration drift of 1–2 orders of magnitude over 100+ periods |

Quality results are more sensitive than hydraulic results because transport errors accumulate over time. If your workflow depends on sub-percent quality agreement with EPANET output, treat both results as independent estimates of the same physical system; neither is more "correct" than the other in an absolute sense.

**Hydra's result is authoritative.** If you observe a difference and suspect a Hydra bug, open a [GitHub issue](https://github.com/neeraip/hydra/issues) with a minimal reproducer.

---

## Behavioural Differences

### Unbalanced-stop mode

EPANET halts the simulation when a hydraulic step does not converge within the configured iteration limit (`UNBALANCED STOP`). Hydra honours this setting: when a hydraulic step is genuinely unbalanced (fails to converge), Hydra also halts and records an `UnbalancedHydraulics` warning. The `UNBALANCED CONTINUE N` option is also supported.

In practice this rarely matters, because Hydra's solver is more robust than EPANET's and typically converges for steps where EPANET cannot. On the Richmond network, EPANET halts after 28 of 49 reporting periods; Hydra converges all 49 because it finds valid equilibria for those steps.

### Quality timestep minimum

EPANET's quality timestep can reach 0 seconds via integer truncation when hydraulic timesteps are very short. Hydra enforces a 1-second minimum to prevent zero-length sub-steps.

This only matters for networks with very short hydraulic timesteps (well under 60 seconds), which is unusual in practice.

---

## OWA-EPANET 2.3 Features Worth Knowing

These features were added in OWA-EPANET 2.3 and are fully supported by Hydra. They are not present in the original EPA EPANET 2.2 distribution.

### FAVAD Leakage

Per-pipe background leakage is modelled using the FAVAD (Fixed and Variable Area Discharge) model, configured via a `[LEAKAGE]` section in the `.inp` file. Standard EPANET 2.2 files (without `[LEAKAGE]`) parse cleanly; leakage is simply zero for all pipes.

### Pressure-Dependent Analysis

PDA is configured the same way as in EPANET 2.3 (`DEMAND MODEL PDA` in `[OPTIONS]`). No changes needed.

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

See the [README](https://github.com/neeraip/hydra#library-usage) for complete library usage examples.
