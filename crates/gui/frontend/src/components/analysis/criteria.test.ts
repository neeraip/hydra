import { describe, expect, it } from "vitest";
import {
  type Criterion,
  criterionValue,
  defaultValuation,
  fromDisplayValue,
  toDisplayValue,
  valuationSummary,
} from "./criteria";

/**
 * The pure decisions of editing an engine-published standard: defaults
 * from the catalog, the absent-key fallback, display conversion by the
 * engine's affine map, and the chip's read-back line.
 */

const DEPTH = {
  key: "depth",
  siLabel: "m",
  usLabel: "ft",
  siToUsScale: 3.28084,
  siToUsOffset: 0,
  siDecimals: 2,
  usDecimals: 2,
};

const CATALOG: Criterion[] = [
  {
    key: "freeboard",
    label: "Freeboard",
    help: "Clearance below the rim.",
    quantity: DEPTH,
    kind: { type: "value", default: 0.3 },
  },
  {
    key: "capacity",
    label: "Capacity threshold",
    help: "Fraction treated as full.",
    quantity: {
      ...DEPTH,
      key: "percent",
      siLabel: "%",
      usLabel: "%",
      siToUsScale: 1,
    },
    kind: { type: "value", default: 80 },
  },
  {
    key: "velocity",
    label: "Velocity",
    help: "Self-cleansing to erosive.",
    quantity: {
      ...DEPTH,
      key: "velocity",
      siLabel: "m/s",
      usLabel: "ft/s",
    },
    kind: {
      type: "band",
      cuts: [
        { key: "selfCleansing", label: "Self-cleansing", default: 0.6 },
        { key: "erosive", label: "Erosive", default: 3 },
      ],
    },
  },
];

describe("defaultValuation and criterionValue", () => {
  it("builds the default standard from the catalog", () => {
    expect(defaultValuation(CATALOG)).toEqual({
      freeboard: 0.3,
      capacity: 80,
      velocity: [0.6, 3],
    });
  });

  it("falls back per criterion when a key is absent or misshapen", () => {
    // §7.3: absent keys mean the defaults — including a band whose held
    // value has the wrong length, which a catalog that grew a cut
    // produces from an old saved valuation.
    expect(criterionValue(CATALOG[0], {})).toBe(0.3);
    expect(criterionValue(CATALOG[2], { velocity: [1] })).toEqual([0.6, 3]);
    expect(criterionValue(CATALOG[2], { velocity: [0.9, 2] })).toEqual([
      0.9, 2,
    ]);
  });
});

describe("display conversion", () => {
  it("round-trips through the engine's affine map", () => {
    const us = toDisplayValue(0.3, DEPTH, "us");
    expect(us).toBeCloseTo(0.984, 3);
    expect(fromDisplayValue(us, DEPTH, "us")).toBeCloseTo(0.3, 12);
    // Dimensionless and SI are identity.
    expect(toDisplayValue(0.8, undefined, "us")).toBe(0.8);
    expect(toDisplayValue(0.3, DEPTH, "si")).toBe(0.3);
  });
});

describe("valuationSummary", () => {
  it("reads the whole standard back in the active system", () => {
    const s = valuationSummary(CATALOG, defaultValuation(CATALOG), "si");
    expect(s).toBe(
      "Freeboard 0.3 m  ·  Capacity threshold 80 %  ·  Velocity 0.6–3 m/s",
    );
  });

  it("converts for the US system", () => {
    const s = valuationSummary(CATALOG, defaultValuation(CATALOG), "us");
    expect(s).toContain("Freeboard 0.98 ft");
    expect(s).toContain("Velocity 1.97–9.84 ft/s");
  });
});
