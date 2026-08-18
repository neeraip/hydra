# Output Files

Hydra produces three output files for a water distribution run. Only one of them holds full time-series results; the other two are summaries of the same run.

> For a drainage run see [Output Files (Drainage)](drainage/output-files.md), which are written in SWMM's layouts.

| File | Format | Contents | Use it for |
|---|---|---|---|
| `.out` | Binary | Full per-node and per-link results at every reporting period, plus energy and reaction summaries | Post-processing, plotting, analytics, any tool that reads EPANET output |
| `.rpt` | Plain text | Run summary: header, input/options recap, warnings, timestamps | A quick human-readable overview of a run |
| `.json` | JSON | The same summary data as `.rpt`, plus energy and flow/mass balance, in a structured form | Scripts and data pipelines |

**Only the `.out` file contains time series.** The `.rpt` and `.json` reports are two serialisations of the same summary information; neither includes per-node or per-link result tables.

How each is produced:

- **CLI**: `--summary` takes the report path, and its extension (`.rpt` or `.json`) selects the text or JSON report; `--results` writes the `.out` file. See [CLI](../getting-started/cli.md).
- **GUI**: every run writes `results.out` into the scenario folder; CSV and GeoJSON exports are available from the command palette. See [GUI](../getting-started/gui.md).
- **SDK**: `io::out_writer::write_binary_output`, `io::rpt_writer::build_text_report`, and `io::rpt_writer::build_json_report`. See [SDK Examples](../sdk/examples.md).

---

## `.out`: Binary results

The `.out` file **is** the EPANET 2.3 binary output layout (format version `20012`), so tools that read EPANET output files read Hydra's. Hydra records its own metadata (such as the topology digest that detects a since-edited model) in a `run.json` beside the results rather than inside them, so the results file stays a format EPANET defines. Values are stored as 32-bit floats (`REAL4`) and 32-bit integers (`INT4`), little-endian; IDs and strings are fixed-width, zero-padded.

The file is written in five sections:

1. **Prolog**: a 15-integer header (magic number, format version, element counts, quality mode, trace node, flow-unit code, pressure-unit code, report start/step, duration) followed by the network's static data: title lines, input/report filenames, chemical name and units, node and link IDs, link end-node indices and type codes, tank node indices and cross-section areas, and node elevations, link lengths, and link diameters.
2. **Energy**: a per-pump summary (percent online, average efficiency, average energy per unit flow, average and peak power, and average cost), plus the network demand charge.
3. **Dynamic results**: **one record per reporting period** (not per hydraulic step), stored column-major. Each record holds, for that period:
   - **Node quantities (4):** demand, head, pressure, quality
   - **Link quantities (8):** flow, velocity, head loss, quality, status, setting, reaction rate, friction factor
4. **Network reactions**: average bulk, wall, tank, and source reaction rates over the run.
5. **Epilog**: the number of reporting periods, a warning flag, a network content digest, and a closing magic number.

### Units

All numeric values are written in the unit system chosen at write time (`output_units`). The CLI and GUI default to the model's declared flow units; the SDK writer takes an explicit `FlowUnits` argument (pass `sim.net().options.flow_units` for the model's units). Pressures are written in metres (SI) or PSI (US customary).

Two categories are **not** unit-converted: tank cross-section areas (always internal m²) and all quality/reaction-rate values (written in their native quality units).

---

## `.rpt`: Text report { #rpt--text-report }

A human-readable summary in EPANET report style. It contains, in order:

- A date stamp and the **Hydra** banner with the version number
- The network **title** lines
- An **input summary**: counts of junctions, reservoirs, tanks, pipes, pumps, and valves; head-loss formula; demand model (DDA/PDA); hydraulic timestep; hydraulic accuracy; maximum trials; the quality-analysis mode (with constituent name, quality timestep, and tolerance where applicable); specific gravity; demand multiplier; total duration; and report timestep
- An **"Analysis begun"** timestamp
- **Warnings** raised during the run, grouped by simulation time (unbalanced hydraulics, negative pressures, pump-exceeds-maximum-head)
- An **"Analysis ended"** timestamp

It does **not** contain per-node or per-link result tables. For full results, read the `.out` file. For the catalogue of warning types, see [Diagnostics & Errors](diagnostics.md).

---

## `.json`: JSON report { #json--json-report }

The same summary data as the `.rpt` report, in a structured form suited to scripts and pipelines. Top-level keys:

```json
{
  "input": {
    "junctions": 92, "reservoirs": 1, "tanks": 2,
    "pipes": 117, "pumps": 2, "valves": 0,
    "headloss_formula": "Hazen-Williams", "demand_model": "DDA",
    "hydraulic_timestep_s": 3600.0, "quality_timestep_s": 360.0,
    "duration_s": 86400.0, "report_timestep_s": 3600.0
  },
  "warnings": [
    { "time": 3600.0, "code": "warning/negative_pressure", "message": "...", "object_id": "J1" }
  ],
  "energy": {
    "pumps": [
      { "pump_id": "PU1", "kwh": 120.0, "total_cost": 14.4,
        "avg_efficiency": 0.75, "max_kw": 8.1, "time_online_s": 86400.0 }
    ],
    "peak_demand_kw": 12.3
  },
  "flow_balance": {
    "total_inflow": 0.0, "total_outflow": 0.0,
    "tank_change": 0.0, "unaccounted": 0.0, "ratio": 1.0
  },
  "mass_balance": {
    "initial": 0.0, "added": 0.0, "demand": 0.0,
    "reacted": 0.0, "reacted_bulk": 0.0, "reacted_wall": 0.0,
    "reacted_tank": 0.0, "source": 0.0, "final_mass": 0.0, "ratio": 1.0
  },
  "analysis": { "begun_epoch": "1615687166", "ended_epoch": "1615687167" }
}
```

Notes:

- `warnings[].time` is the simulation time in seconds; `object_id` is the affected node/link ID or `null`.
- `avg_efficiency` is a fraction in `[0, 1]`.
- `flow_balance` is `null` if unavailable, and `mass_balance` is `null` when no quality analysis was run. Balance volumes are in m³; mass values are in mg.
- `begun_epoch` / `ended_epoch` are **strings holding raw seconds since the Unix epoch** (or `null`), not formatted datetimes.
