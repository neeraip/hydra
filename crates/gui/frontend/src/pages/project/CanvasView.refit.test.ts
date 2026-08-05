import { describe, expect, it } from "vitest";
import { shouldRefitOnClear } from "./CanvasView";

describe("shouldRefitOnClear", () => {
  // The point of the feature: Fit frames against the *visible* map, so
  // closing a panel leaves the old framing too small and off-centre.
  it("re-fits an app-owned framing when the map grows", () => {
    expect(shouldRefitOnClear(true, true)).toBe(true);
  });

  // The asymmetry that shapes this: not re-fitting costs a convenience,
  // re-fitting over a camera someone positioned destroys deliberate work
  // with no undo. Ambiguity must therefore resolve to "leave it alone".
  it("never moves a camera the user positioned", () => {
    expect(shouldRefitOnClear(false, true)).toBe(false);
    expect(shouldRefitOnClear(false, false)).toBe(false);
  });

  // Closing something that never covered the map changes nothing about the
  // fit, and a camera animation with no visible cause reads as a glitch.
  it("stays still when nothing that occluded the map was closed", () => {
    expect(shouldRefitOnClear(true, false)).toBe(false);
  });
});
