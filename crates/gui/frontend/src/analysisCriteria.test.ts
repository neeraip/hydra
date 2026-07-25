import { afterEach, describe, expect, it } from "vitest";
import {
  DEFAULT_MIN_PRESSURE_M,
  getMinPressure,
  setMinPressure,
} from "./analysisCriteria";

// The store is a module singleton; reset it after every test.
afterEach(() => {
  setMinPressure(DEFAULT_MIN_PRESSURE_M);
});

describe("analysisCriteria min-pressure store", () => {
  it("starts at the default", () => {
    setMinPressure(DEFAULT_MIN_PRESSURE_M);
    expect(getMinPressure()).toBe(DEFAULT_MIN_PRESSURE_M);
  });

  it("stores a valid non-negative value", () => {
    setMinPressure(21);
    expect(getMinPressure()).toBe(21);
  });

  it("accepts zero", () => {
    setMinPressure(0);
    expect(getMinPressure()).toBe(0);
  });

  it("clamps negatives back to the default", () => {
    setMinPressure(-5);
    expect(getMinPressure()).toBe(DEFAULT_MIN_PRESSURE_M);
  });

  it("rejects non-finite values, keeping the default", () => {
    setMinPressure(21);
    setMinPressure(Number.NaN);
    expect(getMinPressure()).toBe(DEFAULT_MIN_PRESSURE_M);
    setMinPressure(Number.POSITIVE_INFINITY);
    expect(getMinPressure()).toBe(DEFAULT_MIN_PRESSURE_M);
  });
});
