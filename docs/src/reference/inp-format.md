# INP Format Support

Hydra parses the EPANET 2.3 `.inp` file format. This page documents which sections and keywords are supported, which are silently ignored, and where Hydra's behaviour differs from or extends the standard.

---

## Sections

### Fully Supported

All data in these sections is parsed and applied.

| Section | Contents |
|---|---|
| `[TITLE]` | Up to 3 title lines (preserved verbatim) |
| `[JUNCTIONS]` | ID, elevation, base demand, demand pattern |
| `[RESERVOIRS]` | ID, head, head pattern |
| `[TANKS]` | ID, elevation, initial/min/max level, diameter, minimum volume, volume curve, overflow flag |
| `[PIPES]` | ID, nodes, length, diameter, roughness, minor loss, status |
| `[PUMPS]` | ID, nodes, keyword parameters (HEAD, POWER, SPEED, PATTERN) |
| `[VALVES]` | ID, nodes, diameter, type (PRV, PSV, FCV, TCV, GPV, PBV, PCV), setting, minor loss |
| `[DEMANDS]` | Additional demand categories per junction |
| `[EMITTERS]` | Per-junction emitter coefficient |
| `[STATUS]` | Initial link open/closed status overrides and numeric setting overrides (pump speed, valve setting) |
| `[PATTERNS]` | Multiplier sequences (multi-line continuation supported) |
| `[CURVES]` | XY data points for pump head, pump efficiency, GPV headloss, PCV loss ratio, tank volume |
| `[CONTROLS]` | Simple time-based, level-based, and pressure-based controls |
| `[RULES]` | Rule-based controls with IF/AND/OR/THEN/ELSE/PRIORITY |
| `[QUALITY]` | Per-node initial quality concentrations |
| `[SOURCES]` | Quality source injection (CONCEN, MASS, FLOWPACED, SETPOINT) |
| `[MIXING]` | Per-tank mixing model (MIXED, 2COMP, FIFO, LIFO) |
| `[REACTIONS]` | Global and per-element bulk/wall reaction coefficients and orders |
| `[ENERGY]` | Global price/efficiency and per-pump energy settings (EFFIC, PRICE, PRICEPATTERN) |
| `[TIMES]` | Simulation duration, timesteps, report start, pattern start, clock offset |
| `[OPTIONS]` | See [OPTIONS keywords](#options-keywords) below |
| `[REPORT]` | Report field selection and formatting options |
| `[COORDINATES]` | Node XY positions (visual metadata, no unit conversion) |
| `[VERTICES]` | Link intermediate vertices (visual metadata) |
| `[TAGS]` | Node and link string tags (metadata) |
| `[LEAKAGE]` | Per-pipe FAVAD leakage coefficients, added in OWA-EPANET 2.3; not present in legacy EPANET 2.2 |

### Silently Ignored

These sections are recognised and accepted without error but produce no simulation effect. Files containing them parse cleanly.

| Section | Notes |
|---|---|
| `[ROUGHNESS]` | Legacy EPANET 1.x section, superseded by roughness column in `[PIPES]` |
| `[LABELS]` | Map label annotations (visual only) |
| `[BACKDROP]` | Background image metadata (visual only) |

Unknown sections (not listed in either table) are also silently ignored for forward compatibility.

---

## OPTIONS Keywords

All standard EPANET 2.3 `[OPTIONS]` keywords are supported. Unknown keywords are silently ignored.

| Keyword | Description |
|---|---|
| `UNITS` | Flow unit system (CFS, GPM, MGD, IMGD, AFD, LPS, LPM, MLD, CMH, CMD, CMS) |
| `HEADLOSS` | Head-loss formula (H-W, D-W, C-M) |
| `VISCOSITY` | Kinematic viscosity relative to water at 20 °C |
| `DIFFUSIVITY` | Molecular diffusivity relative to chlorine at 20 °C |
| `SPECIFIC GRAVITY` | Specific gravity relative to water at 4 °C |
| `TRIALS` | Maximum Newton-Raphson iterations |
| `ACCURACY` | Relative flow convergence tolerance |
| `UNBALANCED` | Behaviour on non-convergence (STOP or CONTINUE N) |
| `PATTERN` | Default demand pattern ID |
| `DEMAND MULTIPLIER` | Global demand scale factor |
| `DEMAND MODEL` | DDA or PDA |
| `MINIMUM PRESSURE` | PDA: pressure below which demand = 0 |
| `REQUIRED PRESSURE` | PDA: pressure at which full demand is delivered |
| `PRESSURE EXPONENT` | PDA: pressure-demand exponent |
| `EMITTER EXPONENT` | Global emitter discharge exponent |
| `QUALITY` | Quality mode and constituent name/units |
| `TOLERANCE` | Quality segment merge tolerance |
| `CHECKFREQ` | Status-check interval (iterations) |
| `MAXCHECK` | Iteration limit for status checks |
| `DAMPLIMIT` | Flow accuracy threshold for damping activation |
| `FLOWCHANGE` | Maximum per-iteration flow change limit |
| `HEADERROR` | Per-link head balance error limit |
| `HTOL` | Head tolerance for link status transitions |
| `QTOL` | Flow change tolerance for link status transitions |
| `RQTOL` | Minimum gradient clamp for emitter/pump linearisation |
| `BACKFLOW ALLOWED` | Whether emitters may admit reverse flow (YES/NO) |

---

## Pump Curves

A single-point pump curve `(Q₁, H₁)` is automatically expanded to a three-point power-function curve `(0, 1.33334·H₁), (Q₁, H₁), (2·Q₁, 0)`, matching EPANET's internal behaviour.

---

## LEAKAGE Section

`[LEAKAGE]` was added in OWA-EPANET 2.3 and is not present in legacy EPANET 2.2 files. Each row specifies per-pipe FAVAD (Fixed and Variable Area Discharge) leakage coefficients:

```
[LEAKAGE]
;PipeID   C1       C2
P1        0.0002   0.5
P2        0.00015  0.6
```

Where `C1` is the fixed-area discharge coefficient and `C2` is the variable-area discharge coefficient. Standard EPANET files (without a `[LEAKAGE]` section) parse cleanly; leakage is simply zero for all pipes.

---

## Differences from EPANET 2.3

| Area | EPANET 2.3 behaviour | Hydra behaviour |
|---|---|---|
| Quality timestep minimum | Can become 0 s (integer division truncation) when hydraulic step is very small | Enforced minimum of 1 s to prevent zero-length sub-steps |
| `UNBALANCED STOP` | Halts the EPS on the first step that does not converge within `TRIALS` iterations | Halts with a warning and returns a partial result; simulation terminates at that step |
| GGA numerical path | Specific convergence trajectory tied to EPANET's C implementation | Independent GGA path: per-step hydraulic solutions are close but not byte-identical; differences can cascade into larger deviations over long quality runs or in networks with many demand periods |
