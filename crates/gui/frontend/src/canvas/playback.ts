/**
 * How fast the playhead advances.
 *
 * The speed a reader picks is a multiplier, and this turns it into the
 * gap between steps. Kept apart from the effect that runs the timer so
 * the ladder can be checked without starting one.
 */

/** The speeds offered, slowest first. */
export const PLAYBACK_SPEEDS = [1, 2, 4, 8] as const;

/**
 * Milliseconds between steps at a given speed.
 *
 * The base is what 1× means, and it moved: the ladder used to start at
 * 0.5× and 1× was 800ms, which was too quick to read a step at. Every
 * option now behaves as the one below it used to, and the slowest is
 * gone rather than doubled again — so 1× is the old 0.5×, 2× the old 1×,
 * and so on.
 */
export function stepIntervalMs(speed: number): number {
  const s = Number.isFinite(speed) && speed > 0 ? speed : 1;
  return 1600 / s;
}
