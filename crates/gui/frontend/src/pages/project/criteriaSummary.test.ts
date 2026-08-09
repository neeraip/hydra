import { describe, expect, it } from "vitest";
import { DEFAULT_CRITERIA } from "../../hooks";
import { criteriaSummary } from "./criteriaSummary";

/**
 * The chip is read at a glance, so what matters is that it shows the whole
 * ruler (minimum plus all three bands), converts with the display system,
 * and never prints float dust.
 */

describe("criteriaSummary", () => {
  it("shows the minimum and every band's endpoints in SI", () => {
    expect(criteriaSummary(DEFAULT_CRITERIA, "si")).toBe(
      "≥ 14 m  ·  P 24–45 m  ·  V 0.1–1.5 m/s  ·  Q 0.1–10 L/s",
    );
  });

  it("converts to the US display system", () => {
    const s = criteriaSummary(DEFAULT_CRITERIA, "us");
    // 14 m ≈ 19.9 psi; 1.5 m/s ≈ 4.92 ft/s.
    expect(s).toContain("≥ 19.9 psi");
    expect(s).toContain("4.92 ft/s");
    expect(s).toContain("gpm");
  });

  it("trims precision by magnitude rather than printing float dust", () => {
    const c = {
      ...DEFAULT_CRITERIA,
      pressure: { low: 24.3333, required: 35, high: 123.456 },
    };
    const s = criteriaSummary(c, "si");
    expect(s).toContain("P 24.3–123 m");
  });
});
