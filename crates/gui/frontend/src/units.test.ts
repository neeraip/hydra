import { describe, expect, it } from "vitest";
import {
  defaultDecimals,
  formatDistance,
  formatQty,
  formatQtyRaw,
  fromDisplay,
  getUnitSystem,
  type Quantity,
  setUnitSystem,
  toDisplay,
  unitLabel,
} from "./units";

const QUANTITIES: Quantity[] = [
  "length",
  "elevation",
  "head",
  "diameter",
  "flow",
  "velocity",
  "pressure",
  "headloss",
  "volume",
  "demand",
];

// ── Conversion round-trips ────────────────────────────────────────────────────

describe("toDisplay / fromDisplay", () => {
  it("round-trips to identity for every quantity in US units", () => {
    for (const q of QUANTITIES) {
      for (const v of [0, 0.001, 1, 24, 12345.678]) {
        expect(fromDisplay(toDisplay(v, q, "us"), q, "us")).toBeCloseTo(v, 9);
      }
    }
  });

  it("is a passthrough in SI", () => {
    for (const q of QUANTITIES) {
      expect(toDisplay(42.5, q, "si")).toBe(42.5);
      expect(fromDisplay(42.5, q, "si")).toBe(42.5);
    }
  });

  it("applies the documented SI → US factors", () => {
    expect(toDisplay(1, "length", "us")).toBeCloseTo(3.28084, 6);
    expect(toDisplay(1, "elevation", "us")).toBeCloseTo(3.28084, 6);
    expect(toDisplay(1, "head", "us")).toBeCloseTo(3.28084, 6);
    expect(toDisplay(1, "diameter", "us")).toBeCloseTo(0.0393701, 8);
    expect(toDisplay(1, "flow", "us")).toBeCloseTo(15.850323, 6);
    expect(toDisplay(1, "demand", "us")).toBeCloseTo(15.850323, 6);
    expect(toDisplay(1, "velocity", "us")).toBeCloseTo(3.28084, 6);
    expect(toDisplay(1, "pressure", "us")).toBeCloseTo(1.4219702, 7);
    expect(toDisplay(1, "volume", "us")).toBeCloseTo(264.172, 4);
  });

  it("headloss is numerically unchanged (m/km ≡ ft/kft), only the label differs", () => {
    expect(toDisplay(7.3, "headloss", "us")).toBe(7.3);
    expect(fromDisplay(7.3, "headloss", "us")).toBe(7.3);
    expect(unitLabel("headloss", "si")).toBe("m/km");
    expect(unitLabel("headloss", "us")).toBe("ft/kft");
  });

  it("demand converts identically to flow", () => {
    expect(toDisplay(3.2, "demand", "us")).toBe(toDisplay(3.2, "flow", "us"));
  });
});

// ── Labels ────────────────────────────────────────────────────────────────────

describe("unitLabel", () => {
  it("returns the SI labels", () => {
    expect(unitLabel("length", "si")).toBe("m");
    expect(unitLabel("elevation", "si")).toBe("m");
    expect(unitLabel("head", "si")).toBe("m");
    expect(unitLabel("diameter", "si")).toBe("mm");
    expect(unitLabel("flow", "si")).toBe("L/s");
    expect(unitLabel("demand", "si")).toBe("L/s");
    expect(unitLabel("velocity", "si")).toBe("m/s");
    expect(unitLabel("pressure", "si")).toBe("m");
    expect(unitLabel("volume", "si")).toBe("m³");
  });

  it("returns the US labels", () => {
    expect(unitLabel("length", "us")).toBe("ft");
    expect(unitLabel("elevation", "us")).toBe("ft");
    expect(unitLabel("head", "us")).toBe("ft");
    expect(unitLabel("diameter", "us")).toBe("in");
    expect(unitLabel("flow", "us")).toBe("gpm");
    expect(unitLabel("demand", "us")).toBe("gpm");
    expect(unitLabel("velocity", "us")).toBe("ft/s");
    expect(unitLabel("pressure", "us")).toBe("psi");
    expect(unitLabel("volume", "us")).toBe("gal");
  });
});

// ── Formatting ────────────────────────────────────────────────────────────────

describe("formatQty", () => {
  it("formats SI values with default decimals", () => {
    expect(formatQty(24, "pressure", "si")).toBe("24.0 m");
    expect(formatQty(1.234, "flow", "si")).toBe("1.23 L/s");
    expect(formatQty(300, "diameter", "si")).toBe("300 mm");
  });

  it("formats converted US values with default decimals", () => {
    expect(formatQty(20, "pressure", "us")).toBe("28.4 psi");
    expect(formatQty(1, "flow", "us")).toBe("15.9 gpm");
    expect(formatQty(300, "diameter", "us")).toBe("11.81 in");
    expect(formatQty(100, "length", "us")).toBe("328.1 ft");
  });

  it("honours explicit decimals", () => {
    expect(formatQty(1, "velocity", "us", 3)).toBe("3.281 ft/s");
  });
});

describe("formatQtyRaw", () => {
  it("passes the raw value through in SI", () => {
    expect(formatQtyRaw(216.408, "elevation", "si")).toBe("216.408 m");
  });

  it("converts and rounds in US", () => {
    expect(formatQtyRaw(100, "length", "us")).toBe("328.1 ft");
  });
});

describe("formatDistance", () => {
  it("uses m below 1 km and km above in SI", () => {
    expect(formatDistance(999, "si")).toBe("999 m");
    expect(formatDistance(1500, "si")).toBe("1.50 km");
  });

  it("uses ft below a mile and mi above in US", () => {
    expect(formatDistance(100, "us")).toBe("328 ft");
    expect(formatDistance(1609.344, "us")).toBe("1.00 mi");
  });
});

describe("defaultDecimals", () => {
  it("matches the per-quantity precision policy", () => {
    expect(defaultDecimals("flow", "us")).toBe(1); // gpm 1dp
    expect(defaultDecimals("diameter", "us")).toBe(2); // in 2dp
    expect(defaultDecimals("pressure", "us")).toBe(1); // psi 1dp
    expect(defaultDecimals("length", "us")).toBe(1); // ft 1dp
  });
});

// ── Store ────────────────────────────────────────────────────────────────────

describe("unit-system store", () => {
  it("defaults to SI and persists changes", () => {
    expect(getUnitSystem()).toBe("si");
    setUnitSystem("us");
    expect(getUnitSystem()).toBe("us");
    setUnitSystem("si");
    expect(getUnitSystem()).toBe("si");
  });
});
