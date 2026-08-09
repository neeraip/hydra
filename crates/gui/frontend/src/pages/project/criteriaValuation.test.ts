import { describe, expect, it } from "vitest";
import { DEFAULT_CRITERIA } from "../../hooks";
import { wdsValuation } from "./criteriaValuation";

/**
 * The bridge from the saved wds shape to the contract valuation. The
 * backend holds the mirror mapping (`wds_valuation_of`), and its test
 * pins these same keys to the engine's criteria catalog — so this side
 * asserts the exact keys and orderings, and drift on either side fails
 * one of the pair.
 */

describe("wdsValuation", () => {
  it("maps the saved shape onto the cataloged keys, bands in cut order", () => {
    expect(wdsValuation(DEFAULT_CRITERIA)).toEqual({
      minPressure: 14,
      minResidual: 0.2,
      maxAge: 24,
      pressure: [24, 35, 45],
      velocity: [0.1, 0.5, 1.5],
      flow: [0.1, 1, 10],
    });
  });
});
