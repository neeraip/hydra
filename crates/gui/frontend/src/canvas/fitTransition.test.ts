import { describe, expect, it } from "vitest";
import {
  FIT_DURATION_MS,
  type FitTrigger,
  shouldAnimateFit,
} from "./fitTransition";

/**
 * Fitting the network happens for two reasons, and they want different
 * answers. Asking for it moves the camera from somewhere to somewhere, and
 * the movement is what says the two views are the same network. Loading a
 * model has no "from" — the camera is wherever the map initialised — so a
 * flight there is a swoop that means nothing and delays the first frame
 * worth looking at.
 *
 * And reduced motion outranks both. That setting is not about polish.
 */

const TRIGGERS: FitTrigger[] = ["request", "load"];

describe("fitting on request", () => {
  it("travels", () => {
    expect(shouldAnimateFit("request", false)).toBe(true);
  });
});

describe("fitting because the network just loaded", () => {
  /** There is nowhere to travel from. */
  it("does not", () => {
    expect(shouldAnimateFit("load", false)).toBe(false);
  });
});

describe("reduced motion", () => {
  /**
   * The load-bearing one. It has to outrank the reason for the fit, not sit
   * beside it — a setting that is honoured for some triggers and not others
   * is not honoured.
   */
  it("stops every fit animating, whatever asked for it", () => {
    for (const trigger of TRIGGERS) {
      expect(shouldAnimateFit(trigger, true)).toBe(false);
    }
  });

  it("is the only thing that can override a request", () => {
    expect(shouldAnimateFit("request", false)).toBe(true);
    expect(shouldAnimateFit("request", true)).toBe(false);
  });
});

describe("the flight time", () => {
  /**
   * Long enough to read as one movement rather than a cut, short enough
   * that someone fitting repeatedly is never waiting. MapLibre's own
   * default scales with distance, which across a national network runs to
   * seconds.
   */
  it("is a fixed, brief duration", () => {
    expect(FIT_DURATION_MS).toBeGreaterThan(150);
    expect(FIT_DURATION_MS).toBeLessThan(900);
  });
});
