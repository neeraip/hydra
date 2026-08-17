/**
 * Which reporting period the canvas should fetch, or null when there is
 * none to fetch.
 *
 * The playhead is clamped into the timeline because on switching to a
 * shorter result set the fetch can run before the playhead-clamp effect
 * corrects it. The clamp used to be inline — `max(0, min(hour, n - 1))`
 * — and for a results file with *zero* periods that arithmetic yields 0,
 * a period the file does not have. The backend refused it honestly and
 * the refusal surfaced as an error toast on a run that had merely
 * produced no output: a simulation whose end time equals its start
 * completes at once and writes an empty results file.
 */
export function periodToFetch(
  currentHour: number,
  periods: number,
): number | null {
  if (periods <= 0) return null;
  return Math.max(0, Math.min(currentHour, periods - 1));
}
