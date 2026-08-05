import { describe, expect, it } from "vitest";
import { periodRange } from "./CanvasView";

const f = (xs: number[]) => new Float32Array(xs);

describe("periodRange", () => {
  it("scales to the period's own values", () => {
    expect(periodRange(f([2, 5, 9]), 0, 100)).toEqual({ min: 2, max: 9 });
  });

  it("falls back to the run range when the period is essentially flat", () => {
    // Everything dry before the storm arrives: the span is floating-point
    // dust, and autoscaling to it paints that dust across the whole ramp —
    // noise rendered exactly like signal.
    expect(periodRange(f([0, 1e-9, 0, 2e-9]), 0, 100)).toEqual({
      min: 0,
      max: 100,
    });
  });

  it("keeps a period whose span is a real fraction of the run", () => {
    // 5% of the run: small, but genuinely varying — rescale it.
    expect(periodRange(f([10, 15]), 0, 100)).toEqual({ min: 10, max: 15 });
  });

  it("ignores non-finite readings rather than poisoning the range", () => {
    expect(periodRange(f([Number.NaN, 4, 8]), 0, 100)).toEqual({
      min: 4,
      max: 8,
    });
  });

  it("falls back when there is nothing to measure", () => {
    expect(periodRange(null, 3, 7)).toEqual({ min: 3, max: 7 });
    expect(periodRange(f([]), 3, 7)).toEqual({ min: 3, max: 7 });
    expect(periodRange(f([Number.NaN]), 3, 7)).toEqual({ min: 3, max: 7 });
  });

  it("survives a degenerate run range", () => {
    // Nothing to compare the period's span against; take it as given.
    expect(periodRange(f([2, 4]), 5, 5)).toEqual({ min: 2, max: 4 });
  });
});

// The scale control was offered on the water-distribution canvas while
// nothing consumed it: that engine colours from fixed range props rather
// than the catalog channel that rescales itself, so "This step" and "Whole
// run" produced identical ramps at every step. These pin the two shapes
// the wiring has to produce.
describe("per-step rescaling of a run range", () => {
  const run = { min: 0, max: 100 };

  it("narrows to the period when the period genuinely varies", () => {
    const stepped = periodRange(f([40, 60]), run.min, run.max);
    expect(stepped).toEqual({ min: 40, max: 60 });
    // ...and that is a different scale from the run's, which is the whole
    // point of the control.
    expect(stepped).not.toEqual(run);
  });

  it("keeps the run range when the period is flat", () => {
    expect(periodRange(f([50, 50]), run.min, run.max)).toEqual(run);
  });

  it("keeps the run range when there is no period loaded", () => {
    expect(periodRange(null, run.min, run.max)).toEqual(run);
  });
});
