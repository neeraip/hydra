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

Press **⌘K** (macOS) or **Ctrl+K** (Windows/Linux) at any time to open the command palette, which lists every action — running simulations, switching views, imports and exports — with fuzzy search.

## Editing the Network

The **Network** tab provides editable tables for junctions, reservoirs, tanks, pipes, pumps, and valves, including each pipe's initial status. Links can be drawn as polylines: intermediate vertices are editable on the canvas. Committed edits can be undone and redone with **⌘Z** / **⇧⌘Z** (Ctrl+Z / Ctrl+Shift+Z).

The **Issues** panel collects network validation findings (structural problems detected before a run) and warnings produced by the last simulation run, with links to the affected elements.

If a model's coordinates use a projected coordinate system, the CRS picker on the canvas can scan the network's coordinates and suggest matching coordinate reference systems so the network lines up with the basemap.

## Scenarios and Comparison

Scenarios let you keep independent parameter sets side by side within one project. On the canvas, a comparison overlay can display the delta between the active scenario's results and a baseline (the base model or another scenario).

The **Analysis** tab includes result histograms, pipe criticality, audit panels, and tank level charts.

## Units

Choose between **SI (metric)** and **US customary** display units in Settings. This affects how values are shown and entered throughout the app; files and exports (INP, CSV, GeoJSON) always remain in the model's native units.

## Performance on Large Networks

Hydra GUI is tuned to stay responsive on larger models.

- Opening a project navigates immediately while network data finishes loading.
- Network Inspector node/link lists use virtualized rendering to avoid large DOM slowdowns.
- Basemap switching keeps network overlays attached so features remain visible while the style reloads.

## Exporting and Output Files

Hydra saves simulation results inside the project folder on disk. To open the folder for a scenario, go to the **Scenarios** panel and click the **Open in Finder** (macOS) / **Open in Explorer** (Windows) icon next to the scenario name. The folder contains `results.out` — EPANET-compatible binary output, readable by post-processing tools.

Other formats are available from the command palette (**⌘K** / **Ctrl+K**):

- **Export INP…** — save the current network as an EPANET `.inp` file
- **Export results as CSV…** — save node and link result time series as CSV files (shown once results exist)
- **Export results to GeoJSON** — save nodes/links with attributes, including result values when available

For a plain-text `.rpt` report, run the exported `.inp` through the [CLI](cli.md).

## Supported Networks

Any EPANET 2.3 `.inp` file works directly — no conversion needed. See [INP Format Support](../reference/inp-format.md) for the full coverage list.

## Troubleshooting

See the [Troubleshooting](troubleshooting.md) page for common issues including the macOS Gatekeeper error and Windows Defender prompts.
