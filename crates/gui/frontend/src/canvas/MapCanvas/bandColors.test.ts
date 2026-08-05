import { describe, expect, it } from "vitest";
import { pressureRgba, velocityRgba, wdsBandColors } from "./colorUtils";

/** The map's colour for a value, as the legend would express it. */
const asCss = (c: readonly number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;

describe("wdsBandColors", () => {
  const pressure = { low: 24, required: 35, high: 45 };
  const velocity = { low: 0.1, target: 0.5, high: 1.5 };

  // The regression: the legend drew the shared banded ramp for every
  // banded variable, while the canvas painted pressure in EPANET's
  // service colours. The legend advertised an orange scale over a map of
  // reds, greens and blues.
  it("matches what the canvas paints for pressure, band for band", () => {
    const bands = wdsBandColors("pressure");
    expect(bands).not.toBeNull();
    const sampled = [10, 30, 40, 60].map((p) =>
      asCss(pressureRgba(p, pressure)),
    );
    expect(bands).toEqual(sampled);
  });

  it("matches what the canvas paints for velocity, band for band", () => {
    const bands = wdsBandColors("velocity");
    expect(bands).not.toBeNull();
    // Three bands: below low, in band, above high. The target is a design
    // aim rather than a compliance edge, so it does not split the verdict.
    const sampled = [0.05, 1.0, 2.0].map((v) =>
      asCss(velocityRgba(v, velocity)),
    );
    expect(bands).toEqual(sampled);
  });

  // Both now speak one severity language, but they band it differently:
  // pressure is non-monotonic (both ends concerning), velocity is not.
  it("bands the two variables differently", () => {
    expect(wdsBandColors("pressure")).not.toEqual(wdsBandColors("velocity"));
  });

  it("declines for variables with no threshold bands", () => {
    for (const id of ["head", "demand", "flow", "quality", "status"]) {
      expect(wdsBandColors(id), id).toBeNull();
    }
  });

  it("returns one colour per band the canvas distinguishes", () => {
    expect(wdsBandColors("pressure")).toHaveLength(4);
    expect(wdsBandColors("velocity")).toHaveLength(3);
  });
});
