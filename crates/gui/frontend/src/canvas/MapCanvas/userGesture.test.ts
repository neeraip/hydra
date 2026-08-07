import { describe, expect, it } from "vitest";
import { deckMoveWasUserDriven, mapMoveWasUserDriven } from "./userGesture";

/**
 * The canvas moves its own camera constantly — framing a network, flying to
 * an element, re-fitting when a panel opens — and almost every question
 * downstream turns on telling those apart from a drag or a scroll: whether
 * a framing may be replaced without asking, whether a fit should animate,
 * whether a position is worth keeping.
 *
 * The two renderers report it differently and the rule was written out at
 * each. Same judgement, two spellings, no name — which is how one comes to
 * be extended and the other forgotten.
 */

describe("a map move", () => {
  /** `fitBounds`, `flyTo` and `jumpTo` all arrive without one. */
  it("is the reader's when a real event drove it", () => {
    expect(mapMoveWasUserDriven({ originalEvent: { type: "wheel" } })).toBe(
      true,
    );
  });

  it("is the app's when nothing drove it", () => {
    expect(mapMoveWasUserDriven({})).toBe(false);
    expect(mapMoveWasUserDriven({ originalEvent: undefined })).toBe(false);
    expect(mapMoveWasUserDriven({ originalEvent: null })).toBe(false);
  });
});

describe("a schematic move", () => {
  /**
   * All four gestures count. A scroll-zoom and a rotate are choices as much
   * as a pan is, and treating them as the app's own movement would let them
   * be overwritten.
   */
  it("is the reader's under any gesture", () => {
    for (const flag of [
      "isDragging",
      "isPanning",
      "isZooming",
      "isRotating",
    ] as const) {
      expect(deckMoveWasUserDriven({ [flag]: true })).toBe(true);
    }
  });

  it("is the app's when no gesture is under way", () => {
    expect(deckMoveWasUserDriven({})).toBe(false);
    expect(deckMoveWasUserDriven(undefined)).toBe(false);
    expect(deckMoveWasUserDriven({ isDragging: false, isZooming: false })).toBe(
      false,
    );
  });
});
