/** The run-timestamps line on the Results page: when the simulation behind
 * the current results started and finished, from the app's own run
 * metadata (`run.json`, served on `ResultMeta`) — wall-clock facts the
 * engines cannot know, so they are app-authored and never part of any
 * engine's block catalog.
 */

/** The slice of `ResultMeta` the stamps read. */
export interface RunInstants {
  startedAtMs?: number | null;
  finishedAtMs?: number | null;
}

/** The two wall-clock instants of the run behind the current results, or
 * null when the results predate the stamps (a `run.json` without the
 * fields, or none at all). Both instants are required: they are written
 * together, so a lone one describes nothing trustworthy. */
export function runInstants(
  meta: RunInstants | null | undefined,
): { started: Date; finished: Date } | null {
  const s = meta?.startedAtMs;
  const f = meta?.finishedAtMs;
  if (typeof s !== "number" || typeof f !== "number") return null;
  return { started: new Date(s), finished: new Date(f) };
}

/** Whether the stamps belong on the active tab. The catalog's first
 * category is each engine's overview tab (both current engines title it
 * "Summary"), and app-level run metadata joins the engine's own summary
 * there rather than shadowing every tab. Decided by position, not by
 * matching an engine's category string, so shared code stays
 * engine-neutral. */
export function onOverviewCategory(
  active: string | null,
  categories: string[],
): boolean {
  return categories.length <= 1 || active === categories[0];
}

/** Human-scale duration: sub-ten-second runs keep a decimal, minutes and
 * hours carry their remainder only when it is non-zero. Clamped at zero so
 * a clock that stepped backwards mid-run reads as instant, not negative. */
export function durationLabel(ms: number): string {
  const s = Math.max(0, ms) / 1000;
  if (s < 10) return `${(Math.round(s * 10) / 10).toLocaleString()} s`;
  if (s < 60) return `${Math.round(s)} s`;
  const whole = Math.round(s);
  if (whole < 3600) {
    const rem = whole % 60;
    const min = (whole - rem) / 60;
    return rem === 0 ? `${min} min` : `${min} min ${rem} s`;
  }
  const min = Math.round(whole / 60);
  const remMin = min % 60;
  const h = (min - remMin) / 60;
  return remMin === 0 ? `${h} h` : `${h} h ${remMin} min`;
}

/** One line for both instants. The finish repeats the date only when the
 * run crossed midnight; the common same-day case reads as a time span. */
export function runStampsLabel(
  started: Date,
  finished: Date,
  locale?: string,
): string {
  const date: Intl.DateTimeFormatOptions = {
    year: "numeric",
    month: "short",
    day: "numeric",
  };
  const time: Intl.DateTimeFormatOptions = {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  };
  const at = (d: Date) =>
    `${d.toLocaleDateString(locale, date)}, ${d.toLocaleTimeString(locale, time)}`;
  const sameDay = started.toDateString() === finished.toDateString();
  const finish = sameDay
    ? finished.toLocaleTimeString(locale, time)
    : at(finished);
  const took = durationLabel(finished.getTime() - started.getTime());
  return `Ran ${at(started)} · finished ${finish} · took ${took}`;
}
