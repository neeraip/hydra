# A pump that will not settle: status chatter blocking convergence

**Status:** fixed, 2026-08-28. §3.9 gained a repeated-reversal rule and the
engine implements it. Kept as the record of how the cycle was diagnosed and
what the first version of the rule got wrong.

**Found:** 2026-08-28, while measuring the engine against EPANET on a large
network.

---

## What happens

On a 46,171-junction network, Hydra converges at the `Accuracy 0.1` the
model ships with, and fails to converge at 0.08 or anything tighter. It runs
the full 200 trials at t=0 and, because the model also sets
`Unbalanced Stop`, halts the whole run after one period. EPANET converges at
every setting from 0.1 to 0.001, taking 16 trials at t=0 where Hydra takes
more than 200.

It is not a numerical criterion that fails. At the final iterations all four
numeric criteria pass comfortably:

    epsQ 1.3e-3  (tolerance 5.0e-2)
    head_ok true
    flow_ok true
    epsR 7.8e-7  (tolerance 5.0e-2)

What blocks convergence is criterion 4, "no link status change during the
most recent iteration". Two pumps alternate forever:

    WPMP-D9100  Open -> XHead   at q = +1.7e-3
    WPMP-D9100  XHead -> Open   at q = -5.9e-7
    WPMP-D9103  Open -> XHead   at q = +4.9e-3
    WPMP-D9103  XHead -> Open   at q = -8.2e-7

Open, the pump draws flow and the head across it rises past its shutoff, so
§3.9 closes it. Closed, its flow falls to about 1e-6, the head condition
reverses, and §3.9 reopens it. A limit cycle around a pump that is carrying
numerically nothing.

## Why the existing escape hatch does not reach it

§3.8 already provides for frozen status: "if convergence is not reached
within `max_iter` iterations and `extra_iter > 0`, an additional `extra_iter`
iterations are run with all status changes frozen". Its DEVIATION note names
this exact failure mode, that "continued switching can keep cycling and
converge to nothing when those valves are the cycling elements".

But the freeze is reachable only under `Unbalanced Continue N`. This model
sets `Unbalanced Stop`, which is `extra_iter = -1`, so there is no freeze and
the run halts. Setting `Unbalanced Continue 10` does complete the run.

So the mechanism exists and the diagnosis was anticipated; what is missing is
that nothing damps chatter *during* the ordinary iterations, and the only
thing that does is unreachable for a model that asked to stop on
non-convergence.

## What was decided

Option 1 below was taken: pin a link that keeps reversing. The others are
kept because they were live options and the reasons for not taking them
still apply.

The rule, now in §3.9: a **reversal** is a status change, *on an iteration
where criteria 1, 2, 3 and 5 of §3.8 already pass*, that sets a link to a
status it has already held during the current solve. At the fourth reversal
the link is pinned for the rest of that solve and reported.

**The qualification is the whole rule, and the first version did not have
it.** Counting reversals from the first iteration pinned links on healthy
solves: micropolis alone pinned sixty, mostly pumps starting and stopping
under controls across a 240 hour run, and five of the eleven byte-gate
networks changed results. While the numerics are still moving a status
change is how the solve finds its configuration. Only once every numeric
criterion is met is a status change the sole thing denying convergence.

With the qualification, exactly one corpus model moves: bwsn2, which is the
one that already failed to converge. Its worst step went from exhausting 200
iterations to converging in 31, at the cost of 17% more runtime, because the
run no longer coasts on an unconverged state and the adaptive tank stepping
resolves properly.

The original list of decisions, for the record:

1. Whether a link that has toggled the same way more than N times in one
   solve should be pinned for the rest of that solve, and what N is.
2. Whether the pin is per link or global, and whether it is reported.
3. Whether a pump at essentially zero flow should be exempt from the XHead
   test at all. The physics of the toggle is a pump carrying 1e-6 of flow,
   which is a numerical artefact rather than an operating condition.
4. Whether `Unbalanced Stop` should still halt when the only thing
   preventing convergence is a status oscillation, given every numeric
   criterion is satisfied.

Point 3 looks like the most honest fix and the smallest: the cycle exists
because a closed pump's residual flow is treated as a real operating point.
It is also the one most likely to change results on other networks, so it
needs the byte gate across the corpus and a decision about which behaviour
is correct rather than merely faster.

## How widely it was checked

Every benchmark network at four convergence tolerances, 0.1 down to 0.0001,
counting pins and non-converged steps:

- Pins fire on two networks only, twice each: bwsn2 (which previously
  reported an unbalanced step at any tolerance) and richmond at 0.1. Every
  other network, at every tolerance, pins nothing.
- Non-convergence is gone from the set with one exception, richmond at
  0.0001, where **no link is pinned at all**. The rule correctly stays out of
  it: the numeric criteria never all pass there, so a status change is not
  what is denying convergence and pinning could not help.
- EPANET also fails to converge richmond at 0.0001. That tolerance is hard
  for the network, not for this engine.

So the rule engages rarely, engages where the cycle is, and does not paper
over a numerical failure that is not one.

## Reproducing

Set `Accuracy` to 0.05 on a network with pumps near their shutoff head. The
diagnosis came from a temporary probe printing the four criteria and the
per-link status transitions at the last iterations; it was removed, and
would be worth re-adding as a proper `HYDRA_CONV_PROBE` alongside
`HYDRA_SOLVE_TIMING` if this is picked up.

## Not a performance problem

This was found while chasing what looked like a 3x performance deficit
against EPANET. It is not one: at the model's loose tolerance Hydra does 346
iterations to EPANET's 153, and when EPANET is asked for comparable
convergence (`Accuracy 0.001`) it takes 458 trials and 0.97 s against
Hydra's 346 and 0.88 s. Per-iteration cost is at parity. The extra
iterations are criterion 5 and this chatter, not slow arithmetic.
