import { describe, expect, it } from "vitest";
import type { GenericVariable } from "../../hooks";
import { railColumns } from "./CanvasView";

function v(id: string): GenericVariable {
  return {
    id,
    label: id.toUpperCase(),
    ramp: { type: "sequential" },
    min: 0,
    max: 1,
  };
}

const VARS = [v("depth"), v("head"), v("volume"), v("flooding")];

describe("railColumns", () => {
  it("leads with the variable the legend has selected", () => {
    expect(railColumns(VARS, "volume")[0].key).toBe("volume");
  });

  it("keeps each column pointing at its own values array", () => {
    // The whole hazard of reordering: column 0 is the third variable, so it
    // must still read arrays[2]. Reading arrays[0] would put depth's numbers
    // under a Volume heading.
    const cols = railColumns(VARS, "volume");
    expect(cols[0].at).toBe(2);
    for (const c of cols) {
      expect(VARS[c.at].id).toBe(c.key);
    }
  });

  it("falls back to the first variable when the selection is unknown", () => {
    expect(railColumns(VARS, "nosuchvariable")[0].key).toBe("depth");
  });

  it("returns nothing for an engine with no catalog", () => {
    // `Math.max(0, -1)` is 0, so an unguarded lookup reads vars[0] of an
    // empty array and throws — which it did.
    expect(railColumns([], "depth")).toEqual([]);
    expect(railColumns([], "")).toEqual([]);
  });

  it("caps the column count but never drops the selected one", () => {
    const cols = railColumns(VARS, "flooding");
    expect(cols).toHaveLength(3);
    expect(cols[0].key).toBe("flooding");
  });

  it("lists no variable twice", () => {
    const keys = railColumns(VARS, "head").map((c) => c.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("handles a single-variable catalog", () => {
    const one = [v("rainfall")];
    expect(railColumns(one, "rainfall")).toEqual([
      {
        key: "rainfall",
        label: "RAINFALL",
        symbol: undefined,
        quantity: undefined,
        at: 0,
      },
    ]);
  });
});
