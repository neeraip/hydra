# Interleaving quality with hydraulics, and deleting the run history

**Status:** specified, not implemented. `simulation/spec.md` §8.2 and §8.3
already carry the rule below; the engine does not yet obey it.

**Written:** 2026-08-27.

**Why now:** a memory pass found the water-distribution session retaining every
reporting instant of a run. The question "why do we need snapshots at all?"
turned out to have a better answer than "fewer of them".

---

## What was measured

`NodeState` is 64 bytes, `LinkState` 40, so one retained instant costs
`nodes x 64 + links x 40`.

| Model | Per instant | Instants | Retained |
|---|---|---|---|
| L-Town, 168 h at 5 min, quality NONE | 85 KB | 2,017 | 170 MB |
| 46k-node network, 24 h at 5 min | 4.6 MB | 288 | 1.28 GB |
| 46k-node network, 168 h at 5 min | 4.6 MB | 2,017 | 9.0 GB |

L-Town's peak process footprint measures 228 MB from a 0.6 MB input file. The
46k-node figures are the projection onto the network the GUI treats as its
performance bar.

## Why the history exists

Five consumers. Four do not need it.

| Consumer | Needs a history? |
|---|---|
| `get_node_result` / `get_link_result` by time | The public API promise. No caller in the workspace outside tests and one SDK doc example. |
| `result_ranges()` | A fold. No caller at all, and `interop-epanet`'s `result_ranges_update_from_period` already computes the same answer incrementally over the stream. |
| `first_snapshot_states()` | Needs the first instant, not all of them. |
| `before_first_hydraulic_step()` | Needs a boolean. Reads `hyd_snapshots.is_empty()` instead. |
| Quality | Yes, and only because the session runs it as a second pass. |

The fifth is inherited. EPANET solves hydraulics to completion, spills the flow
field to a temporary file, then replays that file to advance quality. The
arrangement exists to allow re-running quality under new settings without
re-solving hydraulics. Hydra offers no such feature, nothing calls the quality
phase twice, and `quality/spec.md` §1 already describes quality as advancing in
steps that *sub-divide each hydraulic period* — it never mentions a replay. The
only normative statement of the split was one line of the §8.3 lifecycle.

Quality is causal: concentration at an instant depends on flows at or before
it, and flows are constant across a hydraulic interval. Nothing prevents
quality from advancing through an interval as soon as the solve that opens it
completes.

## What the spec now says

- The lifecycle is `run` / `step`. A step is one hydraulic step together with
  the quality sub-steps dividing it. `run_hydraulics`, `step_hydraulics`,
  `run_quality` and `step_quality` are gone.
- A session holds one instant and accumulates no history (§8.2 Retention).
- The result API drops its `time` argument and answers for the current instant.
- A period is final within a bounded number of steps of the solve that opened
  it, never at the end of the run, so streaming is the ordinary path rather
  than an optimisation for quality-disabled runs.
- Quality initialises at the first step, which narrows the window for
  initial-quality mutation. Recorded in the mutation semantics.

## Implementation, in order

1. **Fold the aggregates.** Accumulate energy, mass balance and flow balance
   incrementally. Delete `result_ranges` from the session; the reader owns it.
2. **Replace the two flags.** Keep the initial instant explicitly for quality
   initialisation. Replace `hyd_snapshots.is_empty()` with a `has_stepped`
   boolean; it is the "one identifier meaning two things" shape.
3. **Interleave.** Advance quality through each hydraulic interval inside the
   step that produces it. Retire the phase enum.
4. **Delete the history.** Remove `hyd_snapshots` and the binary search over
   it. The result API narrows to the current instant.
5. **Follow the callers.** `hydra-engines`' `WdsRun` loses its phase machine;
   the GUI's two progress phases become one.

## The gate

Byte-identical `.out` against the current build, on the benchmark corpus **with
quality enabled**, because interleaving reorders when quality is computed and
must not change what is computed. A nanometre of input perturbs a drainage
total by 16 m³ (see `reference/performance.md`), so a diff is the only proof
that a reordering was value-neutral. If bytes move, the change is wrong until
explained.

Secondary: peak RSS on L-Town falls from 228 MB toward the size of the network,
and the 46k-node projection stops scaling with run length.

## Known costs

- The library's public API breaks. Major bump on the library track.
- The GUI reports one progress phase where it reported two.
- Re-running quality without re-solving hydraulics becomes impossible. Nothing
  offers it today, and no user has asked.
- A third party stepping hydraulics and then reading arbitrary past instants
  loses that. The migration is to attach a result stream.
