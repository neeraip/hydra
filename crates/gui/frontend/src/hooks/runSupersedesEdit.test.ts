/**
 * Whether a finished run answers an edit.
 *
 * A run reads the model when it starts, so an edit made while it was in
 * flight is not in the results it produced. Clearing the stale marker on
 * completion regardless left those results looking current — and the
 * topology digest cannot cover the case, because changing a diameter
 * changes no topology. The marker was the only signal there was.
 */
import { describe, expect, it } from "vitest";
import { runSupersedesEdit } from "./NetworkVersionContext";

describe("runSupersedesEdit", () => {
  it("answers an edit that came before it", () => {
    expect(runSupersedesEdit(100, 200)).toBe(true);
  });

  it("does not answer an edit made while it was running", () => {
    // The solver read the model at 100. What was typed at 150 is not in
    // what it produced, whatever the clock says when it finishes.
    expect(runSupersedesEdit(150, 100)).toBe(false);
  });

  it("gives a tie to warning rather than to silence", () => {
    // Both clocks are epoch seconds, and a second is coarse enough to
    // straddle the start. Treating the tie as "answered" is the reading
    // that can be silently wrong.
    expect(runSupersedesEdit(100, 100)).toBe(false);
  });

  it("answers nothing when the run never started", () => {
    // Cancelled before it began, or an item from a queue that did not
    // record the time.
    expect(runSupersedesEdit(100, null)).toBe(false);
    expect(runSupersedesEdit(100, undefined)).toBe(false);
  });
});
