import { describe, expect, it } from "vitest";
import { networkListRowHeight } from "./NetworkList";

describe("networkListRowHeight", () => {
  /**
   * The bug this exists for: a search adds a second line to rows that
   * matched on what they connect to, and the row height did not follow.
   * The virtualiser positions rows by this number, so the second line was
   * not clipped — the next row was laid on top of it.
   */
  it("is taller while searching, when rows carry a second line", () => {
    expect(networkListRowHeight(1, true)).toBeGreaterThan(
      networkListRowHeight(1, false),
    );
  });

  /** The default state must be untouched — this fix should not resize a
   * list nobody is searching. */
  it("leaves the resting row height exactly as it was", () => {
    expect(networkListRowHeight(1, false)).toBe(27);
  });

  /**
   * Only the text scales; the padding around it does not. Interpolating
   * the whole row would overshoot and leave a growing gap under each row —
   * an error that repeats tens of thousands of times down the list.
   */
  it("grows by the text portion alone, not the whole row", () => {
    const rest = networkListRowHeight(1, false);
    const scaled = networkListRowHeight(2, false);
    expect(scaled).toBeLessThan(rest * 2);
    expect(scaled).toBeGreaterThan(rest);
  });

  it("keeps both lines fitting as the text scale rises", () => {
    for (const scale of [0.9, 1, 1.15, 1.3, 1.5]) {
      const searching = networkListRowHeight(scale, true);
      const resting = networkListRowHeight(scale, false);
      // The extra room is at least a full second line at this scale.
      expect(searching - resting).toBeGreaterThanOrEqual(Math.round(9 * scale));
    }
  });

  it("returns whole pixels, so row offsets never accumulate a fraction", () => {
    for (const scale of [0.9, 1.15, 1.3]) {
      expect(Number.isInteger(networkListRowHeight(scale, true))).toBe(true);
      expect(Number.isInteger(networkListRowHeight(scale, false))).toBe(true);
    }
  });
});
