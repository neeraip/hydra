import { describe, expect, it } from "vitest";
import type { OptionKind } from "../../../hooks/reports";
import {
  formatNumberList,
  numberIssue,
  parseNumberList,
  withOption,
} from "./BlockOptions";

const ASCENDING = { minLen: 1, ascending: true };

describe("parseNumberList", () => {
  it("reads a comma-separated list", () => {
    const result = parseNumberList("0, 10, 20", ASCENDING);
    expect(result).toEqual({ ok: true, values: [0, 10, 20] });
  });

  it("treats blank as unset rather than invalid", () => {
    // Blank means "use the engine default", so it must not surface an error.
    expect(parseNumberList("   ", ASCENDING)).toEqual({ ok: true, values: [] });
  });

  it("rejects a list that does not ascend", () => {
    const result = parseNumberList("0, 20, 10", ASCENDING);
    expect(result.ok).toBe(false);
  });

  it("rejects equal neighbours, which would make an empty band", () => {
    const result = parseNumberList("0, 10, 10", ASCENDING);
    expect(result.ok).toBe(false);
  });

  it("allows a descending list when the engine does not require ascent", () => {
    const result = parseNumberList("3, 2, 1", {
      minLen: null,
      ascending: false,
    });
    expect(result).toEqual({ ok: true, values: [3, 2, 1] });
  });

  it("names a non-numeric entry", () => {
    const result = parseNumberList("0, abc", ASCENDING);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error).toContain("abc");
  });

  it("rejects a trailing separator instead of silently dropping it", () => {
    expect(parseNumberList("0, 10,", ASCENDING).ok).toBe(false);
  });

  it("enforces the engine's minimum length", () => {
    expect(parseNumberList("5", { minLen: 2, ascending: true }).ok).toBe(false);
  });

  it("round-trips through formatNumberList", () => {
    const parsed = parseNumberList(
      formatNumberList([0.1, 0.3, 0.6]),
      ASCENDING,
    );
    expect(parsed).toEqual({ ok: true, values: [0.1, 0.3, 0.6] });
  });
});

describe("numberIssue", () => {
  const number: OptionKind = { type: "number", default: 14, min: 0, max: null };
  const integer: OptionKind = {
    type: "integer",
    default: 10,
    min: 1,
    max: null,
  };

  it("accepts a value inside the bounds", () => {
    expect(numberIssue(20, number)).toBeNull();
  });

  it("rejects a value below the minimum", () => {
    expect(numberIssue(-1, number)).not.toBeNull();
  });

  it("rejects a fractional value for an integer option", () => {
    expect(numberIssue(2.5, integer)).not.toBeNull();
  });

  it("rejects a value that is not a number at all", () => {
    expect(numberIssue(Number.NaN, number)).not.toBeNull();
  });
});

describe("withOption", () => {
  it("sets a key on an absent options object", () => {
    expect(withOption(undefined, "minPressure", 20)).toEqual({
      minPressure: 20,
    });
  });

  it("clears a key when the value is undefined", () => {
    expect(
      withOption({ minPressure: 20, worstCount: 5 }, "minPressure", undefined),
    ).toEqual({
      worstCount: 5,
    });
  });

  it("collapses to undefined once nothing is left", () => {
    // A block back at its defaults must write no `options` member at all,
    // so the template stays identical to a hand-authored default one.
    expect(
      withOption({ minPressure: 20 }, "minPressure", undefined),
    ).toBeUndefined();
  });

  it("keeps keys it does not know about", () => {
    // Hand-authored or newer-engine keys must survive an edit to a sibling.
    expect(withOption({ handAuthored: 1 }, "worstCount", 3)).toEqual({
      handAuthored: 1,
      worstCount: 3,
    });
  });
});
