import { describe, expect, it } from "vitest";
import {
  shouldZoomOnFollow,
  type ViewportCause,
  viewportIsUserOwned,
} from "./viewportCause";

/**
 * One value, two questions — and the point of these tests is that the
 * answers are allowed to differ.
 *
 * This replaced a boolean that meant "the camera is the user's", which was
 * the right answer to the refit question and the wrong one to the follow
 * question: a fly-to and a deliberate pan are the same boolean and the
 * opposite intent. Every test below that separates the two is guarding
 * against the two collapsing back together.
 */

const ALL: ViewportCause[] = ["fit", "feature", "user"];

describe("following a relationship", () => {
  /** The mode is entered by flying to something, and only by that. */
  it("frames the next element only after a fly-to", () => {
    expect(shouldZoomOnFollow("feature")).toBe(true);
    expect(shouldZoomOnFollow("user")).toBe(false);
    expect(shouldZoomOnFollow("fit")).toBe(false);
  });

  /**
   * A pan is the exit. This is what makes the mode safe to leave
   * undiscoverable: a user who does not want it never has to learn it
   * exists, because the thing they would naturally do already ends it.
   */
  it("stops framing once the user moves the camera themselves", () => {
    expect(shouldZoomOnFollow("user")).toBe(false);
  });

  /**
   * A fit is the user asking to see the whole network. Yanking them to a
   * single element immediately afterwards is exactly the theft this
   * feature is meant to avoid, so a fit must not enter the mode even
   * though it is not a manual pan either.
   */
  it("does not frame after a fit, which is neither a pan nor a fly-to", () => {
    expect(shouldZoomOnFollow("fit")).toBe(false);
  });
});

describe("who owns the camera", () => {
  /** A fit hands framing back to the app; nothing else does. */
  it("is the app only after a fit", () => {
    expect(viewportIsUserOwned("fit")).toBe(false);
    expect(viewportIsUserOwned("feature")).toBe(true);
    expect(viewportIsUserOwned("user")).toBe(true);
  });
});

describe("the two questions", () => {
  /**
   * The defect this whole change exists to prevent, stated directly: a
   * fly-to leaves the camera user-owned *and* in follow mode, while a pan
   * leaves it user-owned and out of it. One boolean cannot say that, and
   * a regression that made these agree everywhere would mean the cause had
   * quietly become a boolean again.
   */
  it("disagree on a fly-to, which is why the cause is not a boolean", () => {
    expect(viewportIsUserOwned("feature")).toBe(true);
    expect(shouldZoomOnFollow("feature")).toBe(true);

    expect(viewportIsUserOwned("user")).toBe(true);
    expect(shouldZoomOnFollow("user")).toBe(false);

    // Same ownership answer, opposite follow answer — the split, in one
    // assertion.
    expect(viewportIsUserOwned("feature")).toBe(viewportIsUserOwned("user"));
    expect(shouldZoomOnFollow("feature")).not.toBe(shouldZoomOnFollow("user"));
  });

  /** Neither reader may throw or go undefined on a cause it is handed. */
  it("answer for every cause", () => {
    for (const cause of ALL) {
      expect(typeof shouldZoomOnFollow(cause)).toBe("boolean");
      expect(typeof viewportIsUserOwned(cause)).toBe("boolean");
    }
  });
});
