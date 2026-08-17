/**
 * The period the canvas fetches, and the case it must not fetch at all.
 *
 * The defect this guards: a run spanning no time writes a results file
 * with zero periods, and the inline clamp `max(0, min(hour, n - 1))`
 * turned "no periods" into "period 0" — a request the backend could only
 * refuse, surfaced as a raw error toast.
 */
import { describe, expect, it } from "vitest";
import { periodToFetch } from "./periodToFetch";

describe("periodToFetch", () => {
  it("fetches nothing from a file with no periods", () => {
    expect(periodToFetch(0, 0)).toBeNull();
    expect(periodToFetch(5, 0)).toBeNull();
  });

  it("clamps the playhead into the timeline", () => {
    // Switching to a shorter result set can run this before the playhead
    // is corrected.
    expect(periodToFetch(30, 25)).toBe(24);
    expect(periodToFetch(-1, 25)).toBe(0);
  });

  it("passes an in-range playhead through", () => {
    expect(periodToFetch(7, 25)).toBe(7);
    expect(periodToFetch(0, 1)).toBe(0);
  });
});
