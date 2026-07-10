# GUI

Hydra's desktop application lets you load, run, and explore water network simulations without using the command line.

## Download and Install

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

| Platform | File |
|---|---|
| macOS | `.dmg` — drag Hydra to Applications |
| Windows | `.msi` — run the installer |
| Linux | `.AppImage` — make executable and run; or `.deb` for Debian/Ubuntu |

> **macOS — "Hydra is damaged and can't be opened"**
>
> Hydra GUI macOS releases are notarised. If Gatekeeper still shows this warning after dragging Hydra to Applications, run this once in Terminal:
> ```sh
> xattr -cr /Applications/Hydra.app
> ```
> Then open the app normally.

## Basic Workflow

Hydra organises work into **projects**. Each project holds a network model and one or more **scenarios** — independent parameter sets you can run and compare.

1. **Create a project** — on the Projects screen, click **New Project**. Choose *Import INP file* to bring in an existing EPANET `.inp` file, or start from a blank network.
2. **Configure and run** — press **⌘R** (macOS) or **Ctrl+R** (Windows/Linux), or click the **Simulate** button in the scenario strip at the bottom of the screen. Select which scenarios to run and confirm.
3. **Explore results** — after the simulation completes, the network map updates with colour-coded results. Click any node or link to inspect its time-series values (pressure, head, flow, velocity, water age, etc.). Use the timeline scrubber to step through reporting periods.

## Accessing Output Files

Hydra saves output files inside the project folder on disk. To open the folder for a scenario, go to the **Scenarios** panel and click the **Open in Finder** (macOS) / **Open in Explorer** (Windows) icon next to the scenario name. The folder contains:

- `results.out` — EPANET-compatible binary output, readable by post-processing tools
- `results.rpt` — plain-text report in EPANET report style

## Supported Networks

Any EPANET 2.3 `.inp` file works directly — no conversion needed. See [INP Format Support](../reference/inp-format.md) for the full coverage list.

## Troubleshooting

See the [Troubleshooting](troubleshooting.md) page for common issues including the macOS Gatekeeper error and Windows Defender prompts.
