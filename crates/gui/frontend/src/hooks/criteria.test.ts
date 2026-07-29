import { describe, expect, it } from "vitest";
import { DEFAULT_CRITERIA, DEFAULT_MIN_PRESSURE_M } from "./criteria";

describe("criteria defaults", () => {
  it("mirror the backend defaults", () => {
    // These stand in before the per-project file resolves and outside a Tauri
    // shell, so a drift from the Rust side would show one value then silently
    // swap to another once the fetch lands.
    expect(DEFAULT_CRITERIA.version).toBe(1);
    expect(DEFAULT_CRITERIA.minPressureM).toBe(DEFAULT_MIN_PRESSURE_M);
    expect(DEFAULT_MIN_PRESSURE_M).toBe(14);
    expect(DEFAULT_CRITERIA.pressure).toEqual({
      low: 24,
      required: 35,
      high: 45,
    });
    expect(DEFAULT_CRITERIA.velocity).toEqual({
      low: 0.1,
      target: 0.5,
      high: 1.5,
    });
    expect(DEFAULT_CRITERIA.flow).toEqual({
      low: 0.1,
      target: 1.0,
      high: 10.0,
    });
  });

  it("orders every band low < middle < high", () => {
    const { pressure, velocity, flow } = DEFAULT_CRITERIA;
    expect(pressure.low).toBeLessThan(pressure.required);
    expect(pressure.required).toBeLessThan(pressure.high);
    expect(velocity.low).toBeLessThan(velocity.target);
    expect(velocity.target).toBeLessThan(velocity.high);
    expect(flow.low).toBeLessThan(flow.target);
    expect(flow.target).toBeLessThan(flow.high);
  });
});
