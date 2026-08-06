import { describe, expect, it } from "vitest";
import { PLAYHEAD_EASE, playheadTransition } from "./timelineMotion";

/**
 * The fill and the handle are two drawings of one number, so what this
 * pins is that they are given the same motion — the defect was that one
 * eased and the other did not, leaving the fill permanently behind the
 * handle the user was holding.
 */

describe("the playhead's motion", () => {
  /** The bug, stated as the invariant it broke. */
  it("gives the fill and the handle the same timing", () => {
    for (const scrubbing of [true, false]) {
      const fill = playheadTransition("width", scrubbing);
      const handle = playheadTransition("left", scrubbing);
      // Same timing, different property — that is the whole contract.
      expect(fill?.replace("width", "")).toBe(handle?.replace("left", ""));
    }
  });

  /**
   * During a drag the playhead is being placed, not animated toward
   * anything, so any ease is latency between the cursor and the thing it
   * is holding.
   */
  it("animates nothing while the user is dragging", () => {
    expect(playheadTransition("width", true)).toBeUndefined();
    expect(playheadTransition("left", true)).toBeUndefined();
  });

  /**
   * But playback keeps the glide, which is what the ease was for: it turns
   * a sequence of discrete steps into continuous movement.
   */
  it("keeps the glide when the playhead is not being dragged", () => {
    expect(playheadTransition("width", false)).toBe(`width ${PLAYHEAD_EASE}`);
    expect(playheadTransition("left", false)).toBe(`left ${PLAYHEAD_EASE}`);
  });

  /** Each part names its own property, so neither animates the other's. */
  it("animates only the property it was asked about", () => {
    expect(playheadTransition("width", false)).not.toContain("left");
    expect(playheadTransition("left", false)).not.toContain("width");
  });
});
