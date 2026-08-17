# Output Files (Drainage)

A drainage run writes the two files SWMM's own readers expect, in SWMM's own layouts.

## `.out`: Binary results

Written to SWMM 5.2.4's layout: magic number 516114522, version 52004, the flow-units code, object counts, identifier tables, pollutant unit codes, static property tables, result-variable code lists, the reporting clock, fixed-size per-period records, and the six-integer epilog that readers locate by seeking back from the end of the file.

Two details exist because SWMM's readers expect them:

- Per-object records appear only for objects the `[REPORT]` selection flagged.
- The stored start date is backdated one period when reporting starts after the simulation begins.

Node and link values are period-interpolated, and the period-averaged variant is served as SWMM defines it, with settings exempted.

**Reading one back** is the same format's other half. Results files can dwarf the model that produced them, so Hydra's reader takes a path and seeks (metadata, one period, one element's series, or a sequential scan visiting every period once) rather than loading the file whole.

Opening validates before serving: the leading and trailing magic numbers, the version, the epilog's section positions against the actual file length, and the stored error code. A file whose writer recorded an error, or whose geometry does not reconcile, is refused rather than served as data.

## `.rpt`: Text report

The run summary follows SWMM's layout: the same sections in the same order, with the same tables, column headings and field widths. A Hydra report can be diffed side by side against a SWMM run, and tools that already parse SWMM reports read Hydra's without changes.

It carries the continuity balances, the node and link summaries, the subcatchment runoff summary, storage volumes, the routing time-step summary, flow instability indexes, and the control actions taken, dated rather than given as elapsed time.

## `[REPORT]` selection

`[REPORT]`'s dual grammar is reproduced: six yes/no directives and three list-valued ones (`SUBCATCHMENTS`, `NODES`, `LINKS`) taking `ALL`, `NONE`, or an identifier list.

These select what a **SWMM-format export** carries. They do not restrict Hydra's own access to results: the SDK and the desktop app can reach any element's series whatever the file selected.

## Interface files

Routing interface files are read and written in SWMM's formats, so a run can be split at a system boundary and the pieces exchanged with SWMM itself.

## Model export

The model can be written back out as a SWMM `.inp`. The display sections a file arrived with are preserved verbatim through the cycle, so a load-and-save keeps the coordinates, polygons, labels and tags a drawing tool put there.
