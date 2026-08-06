/**
 * How much room the timeline's readout has to reserve.
 *
 * The readout sits between the transport buttons and the scrubber, and the
 * scrubber takes whatever is left. So the readout's width is not a detail
 * of the readout: every character it gains comes out of the track, moving
 * the playhead and every tick under the user's cursor. Stepping from period
 * 9 to period 10 did exactly that, and stepping back undid it, so a run
 * playing through the tens jittered once per decade.
 *
 * Both lines are set in the monospace face, where `1ch` is exactly one
 * character, so the fix is to reserve the width of the widest string each
 * line can ever hold rather than the one it happens to hold now. These
 * functions are that width, in `ch`.
 */

/** The fixed part of the counter: `"period "` plus `" / "`. */
const COUNTER_CHROME = "period ".length + " / ".length;

/**
 * Width for the `period n / total` line.
 *
 * Sized for the counter at its last period, which is where both numbers are
 * at their widest — reserving for the *current* value is the bug.
 *
 * @param totalPeriods the number of reported periods, counted from one.
 */
export function periodCounterWidthCh(totalPeriods: number): number {
  const digits = String(Math.max(1, Math.floor(totalPeriods))).length;
  return COUNTER_CHROME + digits * 2;
}

/**
 * Width for the clock line.
 *
 * Taken from the labels themselves rather than assumed to be `HH:MM`: a run
 * longer than four days reaches `100:00`, and assuming five characters
 * would reintroduce the same jump at the same cost.
 *
 * @param labels every time label the run will show.
 */
export function clockWidthCh(labels: readonly string[]): number {
  let widest = 0;
  for (const label of labels) widest = Math.max(widest, label.length);
  return widest;
}
