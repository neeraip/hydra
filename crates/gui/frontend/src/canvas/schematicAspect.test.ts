import { describe, expect, it } from "vitest";
import {
  ASPECT_SLIDER_DEFAULT,
  ASPECT_SLIDER_MAX,
  ASPECT_SLIDER_MIN,
  aspectFactor,
  aspectScales,
  clampSliderValue,
  sliderValueFromPointer,
} from "./schematicAspect";

describe("aspectScales", () => {
  it("is the identity at the midpoint", () => {
    // Anyone who never touches the slider must see the layout the schematic
    // view has always drawn, so this has to be exactly 1 — not merely close.
    expect(aspectScales(ASPECT_SLIDER_DEFAULT)).toEqual({ x: 1, y: 1 });
  });

  it("preserves area, which is what stops it behaving like a zoom", () => {
    // A uniform scale is arithmetically the same as zooming, and the camera fit
    // divides it out. Holding the product at 1 leaves only the reshape.
    for (const v of [
      ASPECT_SLIDER_MIN,
      10,
      30,
      50,
      70,
      90,
      ASPECT_SLIDER_MAX,
    ]) {
      const { x, y } = aspectScales(v);
      expect(x * y).toBeCloseTo(1, 12);
    }
  });

  it("trades the axes against each other in the direction dragged", () => {
    const up = aspectScales(ASPECT_SLIDER_MAX);
    expect(up.x).toBeGreaterThan(1); // layers spread
    expect(up.y).toBeLessThan(1); // siblings tighten

    const down = aspectScales(ASPECT_SLIDER_MIN);
    expect(down.x).toBeLessThan(1);
    expect(down.y).toBeGreaterThan(1);
  });

  it("spans a 16x range of aspect ratios end to end", () => {
    // The two motivating networks were a tall spike and a wide fan; a single
    // axis moving alone would only have reached 4x.
    const widest = aspectScales(ASPECT_SLIDER_MAX);
    const tallest = aspectScales(ASPECT_SLIDER_MIN);
    expect(widest.x / widest.y / (tallest.x / tallest.y)).toBeCloseTo(16, 6);
  });

  it("changes the ratio monotonically, so the slider never reverses", () => {
    // The two-slider version reversed direction partway up the track. A single
    // monotonic ratio cannot.
    let previous = Number.NEGATIVE_INFINITY;
    for (let v = ASPECT_SLIDER_MIN; v <= ASPECT_SLIDER_MAX; v += 5) {
      const { x, y } = aspectScales(v);
      const ratio = x / y;
      expect(ratio).toBeGreaterThan(previous);
      previous = ratio;
    }
  });

  it("clamps rather than extrapolating, and rejects unusable input", () => {
    expect(aspectFactor(-500)).toBe(aspectFactor(ASPECT_SLIDER_MIN));
    expect(aspectFactor(500)).toBe(aspectFactor(ASPECT_SLIDER_MAX));
    for (const bad of [Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(clampSliderValue(bad)).toBe(ASPECT_SLIDER_DEFAULT);
      expect(aspectScales(bad)).toEqual({ x: 1, y: 1 });
    }
  });
});

describe("sliderValueFromPointer", () => {
  const TOP = 100;
  const HEIGHT = 200;

  it("treats dragging up as increasing", () => {
    expect(sliderValueFromPointer(TOP, TOP, HEIGHT)).toBeGreaterThan(
      sliderValueFromPointer(TOP + HEIGHT, TOP, HEIGHT),
    );
  });

  it("puts the neutral ratio at the midpoint of the track", () => {
    expect(sliderValueFromPointer(TOP + HEIGHT / 2, TOP, HEIGHT)).toBeCloseTo(
      ASPECT_SLIDER_DEFAULT,
      10,
    );
  });

  it("clamps a pointer dragged past either end", () => {
    expect(sliderValueFromPointer(TOP - 999, TOP, HEIGHT)).toBe(
      ASPECT_SLIDER_MAX,
    );
    expect(sliderValueFromPointer(TOP + HEIGHT + 999, TOP, HEIGHT)).toBe(
      ASPECT_SLIDER_MIN,
    );
  });

  it("survives a zero-height track", () => {
    // Reachable if the pointer lands before layout has measured the element.
    expect(sliderValueFromPointer(TOP, TOP, 0)).toBe(ASPECT_SLIDER_DEFAULT);
  });
});
