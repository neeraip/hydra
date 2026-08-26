# GUI

Hydra's desktop application lets you load, run, and explore simulations without using the command line. It is the front end to every available Hydra [engine](../engines.md): water distribution models can be created and edited here; drainage models start from an imported SWMM model and are then edited, run and explored.

## Download and Install

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

| Platform | File |
|---|---|
| macOS | `.dmg`: drag Hydra to Applications |
| Windows | `.msi`: run the installer |
| Linux | `.AppImage`: make executable and run; or `.deb` for Debian/Ubuntu |

> **macOS: "Hydra is damaged and can't be opened"**
>
> Hydra GUI macOS releases are notarised. If Gatekeeper still shows this warning after dragging Hydra to Applications, run this once in Terminal:
> ```sh
> xattr -cr /Applications/Hydra.app
> ```
> Then open the app normally.

## Basic Workflow

Hydra organises work into **projects**. Each project holds a network model and one or more **scenarios**: independent parameter sets you can run and compare.

1. **Create a project.** On the Projects screen, click **New Project**. The wizard asks for the **engine** first, then project details, then a review. Choosing the engine up front is deliberate: `.inp` belongs to both EPANET and SWMM, so the file extension cannot decide the modelling domain on your behalf. With Water Distribution chosen, either import an existing EPANET `.inp` file or start from a blank network. Urban Drainage has no blank-network option: choose it and import a SWMM `.inp`. That is a limit on starting from nothing, not on editing: a drainage model is edited like any other once it is open. Hydra cannot represent a network with no elements at all, so a blank project opens onto a smallest-valid starter model, and only water distribution has one.
2. **Configure and run.** Press **⌘R** (macOS) or **Ctrl+R** (Windows/Linux), or click the **Simulate** button in the scenario strip at the bottom of the screen. Select which scenarios to run and confirm.
3. **Explore results.** After the simulation completes, the network map updates with colour-coded results. Click any node or link to inspect its time-series values (pressure, head, flow, velocity, water age, etc.). Use the timeline scrubber to step through reporting periods. A drainage model with a 2D mesh also shows its surface on the map. The mesh itself is drawn from the moment the model opens, coloured by ground elevation, so you can read the terrain and see where water will collect before running anything; the Overview page counts its cells. Zoom in far enough for the cells to be told apart and their edges are drawn too, which is how you check where a mesh is refined. A large mesh viewed whole shows the footprint alone, because every edge drawn at that size is a dark wash rather than a picture of anything. After a run the cells are coloured by water depth (or water surface, speed, or back to the ground, from the legend's picker), dry cells transparent, stepping with the same timeline. Hovering a cell reads out its value, and a toolbar button toggles the layer. The legend has a blend control beside the animation button: it draws the surface as a continuous field instead of one flat colour per cell, and the pointer then reads the value at its own position rather than the cell's. Blending softens the cell boundaries without moving what a cell says about itself: each cell keeps its own colour at its centre, neighbouring cells meet smoothly along their shared edges, and the mixing is done on the graphics card, so it stays smooth at any zoom and costs nothing to step through time. For the ground that is the truer picture, since the mesh stores its elevations at the cell corners.

Press **⌘K** (macOS) or **Ctrl+K** (Windows/Linux) at any time to open the command palette, which lists every action (running simulations, switching views, imports and exports), filtered as you type by substring match. Type `#` in the palette to find any node or link by ID and zoom to it.

## Keyboard Shortcuts

Beyond ⌘R (simulate), ⌘K (command palette), and ⌘Z / ⇧⌘Z (undo/redo), the app has shortcuts for navigation and the canvas. Press **?** at any time to open the in-app cheatsheet, or see the full [Keyboard Shortcuts](keyboard-shortcuts.md) reference. Common ones (⌘ on macOS, Ctrl elsewhere):

| Shortcut | Action |
|---|---|
| ⌘1 – ⌘5 | Switch between the Overview, Canvas, Editor, Results, and Report views |
| ⌘M | Toggle the geographic ↔ orthogonal (schematic) canvas layout |
| ⌘F | Search projects |
| ⌘⇧M | Toggle the Issues panel |
| ⌘= / ⌘- / ⌘0 | Zoom in / zoom out / fit to view |

On the canvas, single-key tools select elements and add or measure geometry (select, edit, add node, add link, measure), and annotations can be placed on the map.

## Editing the Network

The **Editor** tab holds one table per kind of element, listed down a rail on the left. Both engines are edited here and the screen is the same for each: the rail, the columns, the units and which values may be changed all come from the engine itself, so a water distribution model and a drainage model are edited the same way rather than through two editors that drift apart.

- **Change a value** by typing in its cell. There is no save step: an edit is part of the model when it lands, and the file on disk is written straight away.
- **Add an element** with the **Add** button above the table. The dialog offers only the kinds that engine can create and asks for exactly what that kind needs; a kind that cannot be created yet is absent, with the reason given.
- **Rename or delete** an element from the actions at the end of its row. A deletion says what else went with it, and one that something still points at is refused by name rather than left half-done.
- **Undo and redo** with **⌘Z** / **⇧⌘Z** (Ctrl+Z / Ctrl+Shift+Z), or from the history control in the top bar, which lists what each step will undo.

Some elements hold more than a row of values, and those appear beneath the table when the element is selected:

- **Contents**: a curve's points, a pattern's multipliers, a transect's survey points.
- **Records**: a junction's demand categories, a control measure's layers, a snow pack's surfaces, a unit hydrograph's monthly responses.

On the canvas, links are drawn as polylines and their intermediate vertices are preserved; dragging a node moves the link's endpoint while the vertices stay fixed.

The **Issues** panel collects network validation findings (structural problems detected before a run) and warnings produced by the last simulation run, with links to the affected elements.

If a model's coordinates use a projected coordinate system, the CRS picker on the canvas can scan the network's coordinates and suggest matching coordinate reference systems so the network lines up with the basemap. You can also define and save custom CRS entries.

## Scenarios and Comparison

Scenarios let you keep independent parameter sets side by side within one project. Each is run on its own and its results are read on its own; there is no side-by-side overlay of one scenario against another.

The **Results** tab includes a system summary (key metric chips), result histograms, pipe criticality, pump energy, audit panels, and tank level charts.

## Reports

The **Report** tab builds a document from the run. A report is an ordered list of **sections**, each one a block the engine publishes (a balance, a summary table, a chart), so the sections on offer are the ones that engine can actually produce for the model you have open.

Add sections from the palette. A block that cannot be produced for this scenario is shown with the reason instead of being hidden, so a section missing from your report is missing for a stated cause rather than silently. Sections can be reordered, and one that accepts options exposes them beside it.

The arrangement is saved with the project as a template, so the same report can be regenerated after the next run without rebuilding it.

Export from the tab in any of four formats:

| Format | Use |
|---|---|
| `txt` | Plain text, in the layout the CLI's own report uses |
| `csv` | The tables, for a spreadsheet |
| `html` | A self-contained page, charts included |
| `pdf` | The same document, paginated |

## Units

Choose between **SI (metric)** and **US customary** display units in Settings. This affects how values are shown and entered throughout the app; files and exports (INP, CSV, GeoJSON) always remain in the model's native units. Settings also offers a light / dark / system theme.

## Performance on Large Networks

Hydra GUI is tuned to stay responsive on larger models.

- Opening a project navigates immediately while network data finishes loading.
- Network Inspector node/link lists use virtualized rendering to avoid large DOM slowdowns.
- Basemap switching keeps network overlays attached so features remain visible while the style reloads.

## Exporting and Output Files

Hydra saves simulation results inside the project folder on disk. To open the folder for a scenario, go to the **Scenarios** panel and click the **Open in Finder** icon next to the scenario name; it reveals the folder in your platform's file manager (Finder, Explorer, or the Linux equivalent). The folder contains `results.out`: binary output in the format of the model's own engine, EPANET-compatible for a water distribution model and SWMM-compatible for a drainage one, readable by post-processing tools built for either. A drainage model with a 2D mesh also keeps `results.2d.out` there, the surface results the canvas renders.

Other formats are available from the command palette (**⌘K** / **Ctrl+K**):

- **Export INP…** saves the current network as a model input file, in the format of the project's own engine
- **Export results as CSV…** saves node and link result time series as CSV files (shown once results exist)
- **Export results to GeoJSON** saves nodes/links with attributes, including result values when available

For a plain-text `.rpt` report, run the exported `.inp` through the [CLI](cli.md).

## Supported Networks

Any EPANET `.inp` file from any 2.x release works directly, with no conversion needed. See [INP Format Support](../reference/inp-format.md) for the full coverage list. SWMM `.inp` files open the same way, into a drainage project.

## Troubleshooting

See the [Troubleshooting](troubleshooting.md) page for common issues including the macOS Gatekeeper error and Windows Defender prompts.
