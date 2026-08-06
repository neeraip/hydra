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

/**
 * A categorical variable's states reach the rail.
 *
 * Link status is a code — 3 means "Open" — and the network list printed
 * the code while the inspector beside it printed the name. The states are
 * published by the engine in the variable's own ramp hint, so the fix is
 * for the column to carry them across; the contract's note on
 * `Categorical` warns that a variable whose states are dropped in transit
 * cannot be drawn as anything but a gradient over status codes.
 *
 * The first attempt at this patched the fixed-variable path instead, which
 * only runs before a simulation exists — so it changed nothing a user
 * could see. These pin the path that actually serves a simulated run.
 */
describe("railColumns and categorical variables", () => {
  const STATUS: GenericVariable = {
    id: "status",
    label: "Status",
    ramp: {
      type: "categorical",
      items: [
        { value: 2, label: "Closed", severity: "alarm" },
        { value: 3, label: "Open", severity: "nominal" },
      ],
    },
    min: 0,
    max: 3,
  } as GenericVariable;

  it("carries a categorical variable's state labels", () => {
    const [col] = railColumns([STATUS], "status");
    expect(col.codes?.[2].label).toBe("Closed");
    expect(col.codes?.[3].label).toBe("Open");
  });

  /** The engine's judgement of each state travels with it, so a reader
   *  can colour the state without knowing what the state means. */
  it("carries the severity the engine gave each state", () => {
    const [col] = railColumns([STATUS], "status");
    expect(col.codes?.[2].severity).toBe("alarm");
    expect(col.codes?.[3].severity).toBe("nominal");
  });

  it("leaves a measured variable without a code table", () => {
    const [col] = railColumns(VARS, "volume");
    expect(col.codes).toBeUndefined();
  });

  /**
   * The states ride along on a non-selected column too, for the GeoJSON
   * export — but only while the variable is inside the handful of columns
   * the rail carries. A catalog longer than that drops the tail, selected
   * variable aside, which is the existing cap rather than anything to do
   * with categories.
   */
  it("carries them on a column that is not the selected one", () => {
    const cols = railColumns([STATUS, VARS[0]], VARS[0].id);
    expect(cols[0].key).toBe(VARS[0].id);
    expect(cols.find((c) => c.key === "status")?.codes?.[3].label).toBe("Open");
  });
});
