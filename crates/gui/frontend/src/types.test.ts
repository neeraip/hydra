import { describe, expect, it } from "vitest";
import {
  deltaColor,
  PRESSURE_MAX,
  PRESSURE_MIN,
  PRESSURE_THRESHOLD,
  pressureColor,
} from "./types";

// ── Constants ─────────────────────────────────────────────────────────────────

describe("pressure constants", () => {
  it("PRESSURE_THRESHOLD is 24", () => expect(PRESSURE_THRESHOLD).toBe(24));
  it("PRESSURE_MIN is 21", () => expect(PRESSURE_MIN).toBe(21));
  it("PRESSURE_MAX is 53", () => expect(PRESSURE_MAX).toBe(53));
});

// ── pressureColor ─────────────────────────────────────────────────────────────

describe("pressureColor", () => {
  it("returns red for pressure below threshold", () => {
    expect(pressureColor(0)).toBe("#c94040");
    expect(pressureColor(23.9)).toBe("#c94040");
  });

  it("returns amber for pressure 24–34", () => {
    // PRESSURE_THRESHOLD = 24; threshold for amber is [24, 35)
    expect(pressureColor(24)).toBe("#d4a017");
    expect(pressureColor(34.9)).toBe("#d4a017");
  });

  it("returns green for pressure 35–44", () => {
    expect(pressureColor(35)).toBe("#3daf75");
    expect(pressureColor(44.9)).toBe("#3daf75");
  });

  it("returns blue for pressure ≥ 45", () => {
    expect(pressureColor(45)).toBe("#4a90d9");
    expect(pressureColor(100)).toBe("#4a90d9");
  });
});

// ── deltaColor ────────────────────────────────────────────────────────────────

describe("deltaColor", () => {
  it("returns strong blue for large positive delta (> 6)", () => {
    expect(deltaColor(6.1)).toBe("#4a90d9");
    expect(deltaColor(100)).toBe("#4a90d9");
  });

  it("returns medium blue for delta in (2, 6]", () => {
    expect(deltaColor(2.1)).toBe("#7ab8e8");
    expect(deltaColor(6)).toBe("#7ab8e8");
  });

  it("returns light blue for delta in (0, 2]", () => {
    expect(deltaColor(0.1)).toBe("#a8d4f5");
    expect(deltaColor(2)).toBe("#a8d4f5");
  });

  it("returns amber for delta in (-2, 0]", () => {
    expect(deltaColor(0)).toBe("#d4a017");
    expect(deltaColor(-1.9)).toBe("#d4a017");
  });

  it("returns red for large negative delta (≤ -2)", () => {
    expect(deltaColor(-2)).toBe("#c94040");
    expect(deltaColor(-100)).toBe("#c94040");
  });
});
