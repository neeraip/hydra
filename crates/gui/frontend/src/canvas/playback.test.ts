import { describe, expect, it } from "vitest";
import { PLAYBACK_SPEEDS, stepIntervalMs } from "./playback";

/**
 * The playback ladder.
 *
 * It used to start at 0.5× with 1× meaning 800ms a step, which was too
 * quick to read a step at. The slowest option is gone and every remaining
 * one behaves as the option below it used to, so what changed is the
 * meaning of each label rather than the range on offer.
 */

describe("the speeds offered", () => {
  it("starts at 1× and doubles", () => {
    expect([...PLAYBACK_SPEEDS]).toEqual([1, 2, 4, 8]);
  });

  it("offers nothing slower than 1×", () => {
    expect(Math.min(...PLAYBACK_SPEEDS)).toBe(1);
  });
});

describe("the gap between steps", () => {
  /**
   * The shift, stated as what each label now inherits. These are the
   * intervals the old ladder produced one rung down.
   */
  it("gives each speed the interval of the option below it", () => {
    expect(stepIntervalMs(1)).toBe(1600); // was 0.5×
    expect(stepIntervalMs(2)).toBe(800); //  was 1×
    expect(stepIntervalMs(4)).toBe(400); //  was 2×
    expect(stepIntervalMs(8)).toBe(200); //  was 4×
  });

  it("halves as the speed doubles", () => {
    for (const s of PLAYBACK_SPEEDS) {
      expect(stepIntervalMs(s * 2)).toBe(stepIntervalMs(s) / 2);
    }
  });

  /** A nonsense speed must not stop playback or spin the timer flat out. */
  it("falls back to 1× rather than dividing by nothing", () => {
    for (const bad of [0, -2, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(stepIntervalMs(bad)).toBe(1600);
    }
  });
});
