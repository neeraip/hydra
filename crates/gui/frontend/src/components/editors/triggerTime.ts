/**
 * The two times a simple control can fire at, which are not the same
 * quantity.
 *
 * `timer` is elapsed time since the run began, and has no ceiling. A
 * three-day extended-period simulation opening a link `AT TIME 30` is
 * ordinary, and an imported EPANET model is where such a control comes
 * from — nothing in this editor can type one.
 *
 * `clocktime` is a reading on a wall clock. It wraps at midnight, because
 * that is what a wall clock does. The engine treats them differently too:
 * the spec's `TIMER` condition compares elapsed time directly, while
 * `TIMEOFDAY` takes a circular distance modulo a day.
 *
 * Both arrive in one `triggerSeconds` field, which is how a single
 * formatter came to serve them — and it was the wrapping one. A timer at
 * thirty hours rendered as `06:00`, indistinguishable from a genuine
 * six-hour control, and committing the field wrote six hours back to the
 * model. Opening the editor and nudging the card was enough to lose it.
 *
 * So the wrap lives here, applied to one kind and not the other, with the
 * distinction in the argument rather than in whichever helper the caller
 * happened to reach for.
 */

/** Which of the two a trigger time is. */
export type TriggerTimeKind = "timer" | "clocktime";

const SECONDS_PER_DAY = 86400;

/**
 * `H:MM`, with optional seconds, and no ceiling on the hours.
 *
 * Up to four hour digits because elapsed time is unbounded in principle and
 * a run long enough to need five is not a run anyone is doing. Minutes and
 * seconds are held to 0–59 so a typo lands as unparseable rather than as
 * some other valid time.
 */
const HHMM = /^(\d{1,4}):([0-5]?\d)(?::([0-5]?\d))?$/;

/**
 * The text a trigger-time field shows.
 *
 * Zero-padded to two hour digits, because a `time` input renders nothing at
 * all for a value it cannot parse and `6:00` is one of those. Seconds
 * appear only when there are any, so the overwhelmingly common whole-minute
 * control reads exactly as it always did.
 *
 * Clock times are given to the minute. The control that edits them is a
 * native time picker at minute resolution, so emitting seconds it cannot
 * display would show a blank field — a sub-minute clock trigger is
 * therefore rounded here, and that is the one thing this cannot round-trip.
 */
export function triggerTimeText(
  kind: TriggerTimeKind,
  seconds: number | null,
): string {
  const given =
    seconds != null && Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
  const total =
    kind === "clocktime"
      ? Math.round(given / 60) * 60 // to the minute, as the picker shows it
      : Math.round(given);
  const wrapped = kind === "clocktime" ? total % SECONDS_PER_DAY : total;
  const hh = String(Math.floor(wrapped / 3600)).padStart(2, "0");
  const mm = String(Math.floor((wrapped % 3600) / 60)).padStart(2, "0");
  const ss = wrapped % 60;
  return ss === 0
    ? `${hh}:${mm}`
    : `${hh}:${mm}:${String(ss).padStart(2, "0")}`;
}

/**
 * The seconds a trigger-time field holds, or `null` if it holds nothing
 * usable.
 *
 * `null` rather than zero. The old parser answered zero for an empty field,
 * so clearing the input moved the control to the start of the run instead
 * of leaving it alone — and a `time` input reports empty every time it is
 * mid-edit. Callers skip the write.
 */
export function parseTriggerTime(
  kind: TriggerTimeKind,
  text: string,
): number | null {
  const m = HHMM.exec(text.trim());
  if (!m) return null;
  const total = Number(m[1]) * 3600 + Number(m[2]) * 60 + Number(m[3] ?? "0");
  return kind === "clocktime" ? total % SECONDS_PER_DAY : total;
}

/**
 * Whether this kind can hold more than a day.
 *
 * Which decides the control it gets: a native `time` input cannot represent
 * `30:00` and silently blanks instead, so elapsed times take a text field
 * and clock times keep the picker they belong in.
 */
export function triggerTimeIsElapsed(kind: TriggerTimeKind): boolean {
  return kind === "timer";
}
