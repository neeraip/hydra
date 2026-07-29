import { describe, expect, it } from "vitest";
import {
  defaultDecimals,
  formatBytes,
  formatDistance,
  formatQty,
  formatQtyRaw,
  formatQtyValue,
  fromDisplay,
  getUnitSystem,
  parseNumericInput,
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

describe("formatQtyValue", () => {
  it("rounds to the quantity's default decimals in SI, with no label", () => {
    expect(formatQtyValue(216.408, "elevation", "si")).toBe("216.4");
    expect(formatQtyValue(300, "diameter", "si")).toBe("300");
    expect(formatQtyValue(0.5, "flow", "si")).toBe("0.50");
  });

  /**
   * The point of the fixed decimal count: a right-aligned numeric column only
   * reads as a column when every cell has the same number of decimals. SI used
   * to pass the raw value through, so a table showed "42.21" above "42.4"
   * above "42.07" with the decimal points wandering.
   */
  it("gives every value in a column the same decimal count", () => {
    const column = [42.21, 42.4, 42.07, 100].map((v) =>
      formatQtyValue(v, "elevation", "si"),
    );
    expect(column).toEqual(["42.2", "42.4", "42.1", "100.0"]);
    const decimals = new Set(column.map((s) => s.split(".")[1]?.length ?? 0));
    expect(decimals.size).toBe(1);
  });

  it("honours explicit decimals in SI", () => {
    expect(formatQtyValue(1.234, "demand", "si", 2)).toBe("1.23");
  });

  it("converts and rounds in US with formatQty's default decimals", () => {
    expect(formatQtyValue(100, "length", "us")).toBe("328.1");
    expect(formatQtyValue(300, "diameter", "us")).toBe("11.81");
    expect(formatQtyValue(20, "pressure", "us")).toBe("28.4");
    expect(formatQtyValue(1, "flow", "us")).toBe("15.9");
  });

  it("honours explicit decimals in US", () => {
    expect(formatQtyValue(1, "velocity", "us", 3)).toBe("3.281");
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

describe("parseNumericInput", () => {
  it("parses plain and signed numbers", () => {
    expect(parseNumericInput("8.62")).toEqual({ kind: "number", value: 8.62 });
    expect(parseNumericInput("-.5")).toEqual({ kind: "number", value: -0.5 });
    expect(parseNumericInput("+3")).toEqual({ kind: "number", value: 3 });
    expect(parseNumericInput("1e3")).toEqual({ kind: "number", value: 1000 });
  });

  it("tolerates a trailing display unit, with or without a space", () => {
    expect(parseNumericInput("8.62 m")).toEqual({
      kind: "number",
      value: 8.62,
    });
    expect(parseNumericInput("300mm")).toEqual({ kind: "number", value: 300 });
    expect(parseNumericInput("4.2 L/s")).toEqual({
      kind: "number",
      value: 4.2,
    });
    expect(parseNumericInput("12.5 ft")).toEqual({
      kind: "number",
      value: 12.5,
    });
  });

  it("rejects interleaved garbage instead of prefix-parsing", () => {
    expect(parseNumericInput("8F.6G2Y")).toEqual({ kind: "invalid" });
    expect(parseNumericInput("1.2.3")).toEqual({ kind: "invalid" });
    expect(parseNumericInput("m 8")).toEqual({ kind: "invalid" });
    expect(parseNumericInput("8 m 9")).toEqual({ kind: "invalid" });
    expect(parseNumericInput("--5")).toEqual({ kind: "invalid" });
  });

  it("treats whitespace-only input as empty, not zero", () => {
    expect(parseNumericInput("")).toEqual({ kind: "empty" });
    expect(parseNumericInput("   ")).toEqual({ kind: "empty" });
  });
});

// ── formatBytes ──────────────────────────────────────────────────────────────

describe("formatBytes", () => {
  it("reports small sizes in whole bytes", () => {
    expect(formatBytes(0)).toBe("0 bytes");
    expect(formatBytes(1)).toBe("1 bytes");
    expect(formatBytes(999)).toBe("999 bytes");
  });

  it("uses decimal units so figures match the OS file browser", () => {
    // 1 kB = 1000 B, not 1024 — a binary scale would read ~7% smaller than
    // Finder/Explorer for the same file.
    expect(formatBytes(1000)).toBe("1.0 kB");
    expect(formatBytes(1_500_000)).toBe("1.5 MB");
    expect(formatBytes(12_400_000)).toBe("12 MB");
    expect(formatBytes(2_000_000_000)).toBe("2.0 GB");
  });

  it("drops the decimal once the number is large enough not to need it", () => {
    expect(formatBytes(9_900_000)).toBe("9.9 MB");
    expect(formatBytes(10_000_000)).toBe("10 MB");
  });

  it("treats absent or nonsensical sizes as nothing", () => {
    expect(formatBytes(-1)).toBe("0 bytes");
    expect(formatBytes(Number.NaN)).toBe("0 bytes");
  });

  it("caps at the largest unit rather than inventing one", () => {
    expect(formatBytes(5e12)).toBe("5.0 TB");
    // Beyond TB it keeps counting in TB rather than reaching for PB, which
    // no results file will ever need.
    expect(formatBytes(5e15)).toBe("5000 TB");
  });
});
