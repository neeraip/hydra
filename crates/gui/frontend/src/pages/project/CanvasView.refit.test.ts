import { describe, expect, it } from "vitest";
import { shouldRefitAfterOcclusionChange } from "./CanvasView";

// Renamed from `shouldRefitOnClear` when the view button gained its
// restore direction: the rule was always about occlusion changing, and
// opening a panel changes it exactly as closing one does.
describe("shouldRefitAfterOcclusionChange", () => {
  // The point of the feature: Fit frames against the *visible* map, so
  // closing a panel leaves the old framing too small and off-centre.
  it("re-fits an app-owned framing when the map grows", () => {
    expect(shouldRefitAfterOcclusionChange(true, true)).toBe(true);
  });

  // The asymmetry that shapes this: not re-fitting costs a convenience,
  // re-fitting over a camera someone positioned destroys deliberate work
  // with no undo. Ambiguity must therefore resolve to "leave it alone".
  it("never moves a camera the user positioned", () => {
    expect(shouldRefitAfterOcclusionChange(false, true)).toBe(false);
    expect(shouldRefitAfterOcclusionChange(false, false)).toBe(false);
  });

  // Closing something that never covered the map changes nothing about the
  // fit, and a camera animation with no visible cause reads as a glitch.
  it("stays still when nothing that occluded the map was closed", () => {
    expect(shouldRefitAfterOcclusionChange(true, false)).toBe(false);
  });
});
