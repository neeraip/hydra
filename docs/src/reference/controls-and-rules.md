# Controls & Rules

Hydra supports two mechanisms for changing the network during a simulation: **simple controls** (`[CONTROLS]`) and **rule-based controls** (`[RULES]`). Both change a link's status or setting in response to conditions; rules are more expressive and are evaluated more often.

---

## Simple controls (`[CONTROLS]`)

Each simple control is a single line that acts on one link. Every control begins with `LINK`. Keywords are case-insensitive.

| Trigger | Syntax |
|---|---|
| Node level | `LINK <id> <action> IF NODE <nodeId> ABOVE\|BELOW <value>` |
| Elapsed time | `LINK <id> <action> AT TIME <time>` |
| Time of day | `LINK <id> <action> AT CLOCKTIME <time> [AM\|PM]` |

**Action** is one of:

- `OPEN` or `CLOSED` (`CLOSE` is accepted as a synonym for `CLOSED`), or
- a **numeric setting** — a bare number sets a pump's relative speed or a valve's setting. For example, `LINK PU1 1.0000 IF NODE T1 BELOW 4.0` runs pump `PU1` at speed 1.0 while tank `T1` is below 4.0.

When a numeric setting is given without a status, the status is inferred from the link type: a pump or pipe opens for a value greater than zero and closes at zero; a valve becomes active.

**Time values** accept `H:MM` or `H:MM:SS`, or a plain number. A bare number is interpreted as **hours** unless followed by a unit (`SECONDS`, `MINUTES`, `HOURS`, or `DAYS`). `AT CLOCKTIME` additionally accepts a trailing `AM`/`PM`.

Examples:

```
[CONTROLS]
LINK 1A OPEN IF NODE A BELOW 2.5275
LINK 1A CLOSED IF NODE A ABOVE 3.2689
LINK LINK-7491 OPEN AT TIME 18:00:00
```

**Evaluation.** Simple controls are checked once per hydraulic timestep. If several controls act on the same link in one step, the last one (in file order) wins.

> A control line that does not begin with `LINK` or has too few tokens is silently ignored; an unknown node or link ID is a parse error.

---

## Rule-based controls (`[RULES]`)

A rule has the form:

```
RULE <label>
IF <premise>
AND|OR <premise>
...
THEN <action>
AND <action>
...
ELSE <action>
...
PRIORITY <value>
```

The `THEN` block runs when the premises are satisfied; the optional `ELSE` block runs otherwise. A rule must have at least one premise.

### Premises

Each premise tests one object's attribute against a value:

```
<object> <id> <attribute> <operator> <value>      # node or link
SYSTEM <attribute> <operator> <value>             # simulation clock
```

| Group | Keywords |
|---|---|
| **Node objects** | `NODE`, `JUNCTION`, `RESERVOIR`, `TANK` |
| **Link objects** | `LINK`, `PIPE`, `PUMP`, `VALVE` |
| **System** | `SYSTEM` (for `TIME` / `CLOCKTIME`) |

| Attribute | Applies to | Meaning |
|---|---|---|
| `PRESSURE` | node | Gauge pressure |
| `HEAD` (alias `GRADE`) | node | Hydraulic head |
| `DEMAND` | node | Demand |
| `LEVEL` | node (tank) | Water level above the tank bottom |
| `FILLTIME` / `DRAINTIME` | node (tank) | Hours to fill to max / drain to min at the current rate |
| `FLOW` | link | Flow magnitude |
| `STATUS` | link | `OPEN`, `CLOSED`, or `ACTIVE` |
| `SETTING` | link | Pump speed or valve setting |
| `POWER` | link (pump) | Pump power output |
| `TIME` | system | Elapsed simulation time |
| `CLOCKTIME` | system | Time of day |

**Operators:** `=` (also `==`, `IS`, `EQUALS`); `<>` (also `!=`, `NOT`); `<` (also `BELOW`); `>` (also `ABOVE`); `<=`; `>=`. Each operator is a single token — EPANET's two-word `IS NOT` is not supported.

### Combining premises

Premises are joined with `AND` and `OR`, and **`AND` binds more tightly than `OR`** — the condition is a disjunction of `AND`-clauses. For example:

```
IF  TANK T1 LEVEL BELOW 2
AND SYSTEM CLOCKTIME >= 22:00
OR  TANK T1 LEVEL BELOW 1
```

is read as `(T1 level < 2 AND clocktime ≥ 22:00) OR (T1 level < 1)`.

> **Write rule clock times in 24-hour form** (`H:MM`, e.g. `20:00`). A trailing `AM`/`PM` is honoured in simple-control `AT CLOCKTIME` lines but is **not** applied inside rule premises.

### Actions

`THEN`/`ELSE` actions set a **link's** status or setting (rule actions cannot target nodes):

```
LINK <id> STATUS OPEN|CLOSED
LINK <id> SETTING <value>          # SPEED is a synonym for SETTING
```

The object keyword before the link ID is cosmetic — the ID is always resolved as a link. An optional `IS` and `=` are allowed for readability (`THEN PUMP HSP#1 STATUS IS OPEN`).

### Priority and conflicts

`PRIORITY <value>` sets a rule's priority (default `0`). When rules that fire in the same step give conflicting instructions for one link, the higher priority wins; equal priorities are broken in favour of the earliest-defined rule. A `STATUS` action and a `SETTING` action on the same link do not conflict.

### Evaluation

Rules are evaluated at a **rule timestep** that subdivides each hydraulic step (defaulting to one-tenth of the hydraulic step). When a rule changes the network mid-step, the hydraulics are re-solved for the remainder of the step, so a rule can react to a tank crossing a level partway through a step.

Example:

```
[RULES]
RULE 1
IF SYSTEM CLOCKTIME >= 6:00
AND SYSTEM CLOCKTIME < 20:00
AND TANK Tank LEVEL BELOW 97
THEN PUMP HSP#1 STATUS IS OPEN
AND PUMP HSP#2 STATUS IS OPEN
PRIORITY 1
```

---

See [INP Format Support](inp-format.md) for how the `[CONTROLS]` and `[RULES]` sections fit into the wider file format, and [Diagnostics & Errors](diagnostics.md) for the validation errors these sections can produce.
