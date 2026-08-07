import { describe, expect, it } from "vitest";
import {
  easeInOutCubic,
  FIT_DURATION_MS,
  type FitTrigger,
  interpolateOrtho,
  type OrthoView,
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

/**
 * The schematic camera is tweened by hand rather than by deck's own
 * transition, because deck is driven in controlled mode: a deck transition
 * emits interpolated states the app must store and hand back, and the value
 * handed back carries no transition props — which ends the transition.
 * Withholding it instead leaves deck rendering the destination at once.
 * Either way the camera snaps, which is what it did.
 */

const FROM: OrthoView = { target: [0, 0, 0], zoom: 0 };
const TO: OrthoView = { target: [100, -50, 0], zoom: 4 };

describe("easing", () => {
  it("starts and ends where it should", () => {
    expect(easeInOutCubic(0)).toBe(0);
    expect(easeInOutCubic(1)).toBe(1);
  });

  it("is halfway at halfway", () => {
    expect(easeInOutCubic(0.5)).toBeCloseTo(0.5, 6);
  });

  /** Eased, not linear — a camera that cuts in and out reads as mechanical. */
  it("moves slowly at both ends and quickly in the middle", () => {
    expect(easeInOutCubic(0.1)).toBeLessThan(0.1);
    expect(easeInOutCubic(0.9)).toBeGreaterThan(0.9);
  });

  it("never turns back", () => {
    let last = -1;
    for (let i = 0; i <= 20; i += 1) {
      const v = easeInOutCubic(i / 20);
      expect(v).toBeGreaterThanOrEqual(last);
      last = v;
    }
  });
});

describe("the camera partway through a fit", () => {
  it("begins at the start and lands on the destination", () => {
    expect(interpolateOrtho(FROM, TO, 0)).toEqual(FROM);
    expect(interpolateOrtho(FROM, TO, 1)).toEqual(TO);
  });

  /** A frame can arrive late; the camera must not overshoot past its mark. */
  it("clamps a t outside the flight", () => {
    expect(interpolateOrtho(FROM, TO, 1.4)).toEqual(TO);
    expect(interpolateOrtho(FROM, TO, -0.2)).toEqual(FROM);
  });

  it("moves both the target and the zoom", () => {
    const mid = interpolateOrtho(FROM, TO, 0.5);
    expect(mid.target[0]).toBeGreaterThan(0);
    expect(mid.target[1]).toBeLessThan(0);
    expect(mid.zoom).toBeGreaterThan(0);
    expect(mid.zoom).toBeLessThan(4);
  });

  /**
   * Zoom is interpolated in its own units. It is already logarithmic, so
   * moving linearly through it is what makes the approach feel steady —
   * interpolating the scale factor rushes the start and crawls at the end.
   */
  it("moves through zoom linearly, not through scale", () => {
    const mid = interpolateOrtho(FROM, TO, 0.5);
    expect(mid.zoom).toBeCloseTo(2, 6);
    // The scale midpoint would be log2((1 + 16) / 2) ≈ 3.09.
    expect(mid.zoom).not.toBeCloseTo(3.09, 1);
  });

  it("never leaves the path between the two views", () => {
    for (let i = 0; i <= 20; i += 1) {
      const v = interpolateOrtho(FROM, TO, i / 20);
      expect(v.zoom).toBeGreaterThanOrEqual(FROM.zoom);
      expect(v.zoom).toBeLessThanOrEqual(TO.zoom);
      expect(v.target[0]).toBeGreaterThanOrEqual(0);
      expect(v.target[0]).toBeLessThanOrEqual(100);
    }
  });
});
