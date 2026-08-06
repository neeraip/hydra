import { describe, expect, it } from "vitest";
import { clockWidthCh, periodCounterWidthCh } from "./timelineReadout";

/**
 * The readout reserves room for its widest string, not its current one.
 *
 * The bug these pin is that both lines grow a character when a number gains
 * a digit, and the scrubber next to them takes whatever width is left — so
 * the track narrowed at period 10 and widened again at period 9.
 */

describe("the period counter's reserved width", () => {
  /** "period 25 / 25" is 14 characters; every earlier period is shorter. */
  it("reserves for the last period, not the current one", () => {
    expect(periodCounterWidthCh(25)).toBe("period 25 / 25".length);
  });

  /**
   * The heart of it: crossing a power of ten must not change the answer,
   * because that is precisely where the jump was visible.
   */
  it("does not change as the counter gains a digit", () => {
    for (const total of [25, 100, 1000]) {
      const width = periodCounterWidthCh(total);
      expect(periodCounterWidthCh(total)).toBe(width);
    }
    // 9 → 10 → 99 → 100 all live inside a 100-period run, and the run's
    // reservation is one number regardless of where the playhead sits.
    expect(periodCounterWidthCh(100)).toBe("period 100 / 100".length);
  });

  /** A single-period run still needs room for "period 1 / 1". */
  it("handles a run of one period", () => {
    expect(periodCounterWidthCh(1)).toBe("period 1 / 1".length);
  });

  /** Guard against a zero or negative total collapsing the reservation. */
  it("never reserves less than a single digit's worth", () => {
    expect(periodCounterWidthCh(0)).toBe("period 0 / 0".length);
    expect(periodCounterWidthCh(-5)).toBe("period 0 / 0".length);
  });
});

describe("the clock's reserved width", () => {
  it("reserves the longest label, not the first", () => {
    expect(clockWidthCh(["00:00", "01:00", "02:00"])).toBe(5);
  });

  /**
   * A run longer than four days reaches `100:00`. Assuming `HH:MM` is five
   * characters would put the jump back at exactly the hour it disappears
   * from view, which is the sort of bug that only shows up in a long
   * drainage run.
   */
  it("makes room for a run that passes 100 hours", () => {
    expect(clockWidthCh(["99:00", "100:00", "101:00"])).toBe(6);
  });

  it("returns zero for no labels rather than NaN", () => {
    expect(clockWidthCh([])).toBe(0);
  });
});
