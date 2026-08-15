import { describe, expect, it } from "vitest";
import type { RecordColumn, RecordSet } from "../../../hooks";
import { blankRecord, canAddRecord } from "./recordRow";

function set(
  columns: RecordColumn[],
  rows: RecordSet["rows"],
  extra: Partial<RecordSet> = {},
): RecordSet {
  return {
    key: "s",
    label: "S",
    columns,
    rows,
    editable: true,
    ...extra,
  };
}

const NUMBER: RecordColumn = {
  key: "n",
  label: "N",
  kind: { type: "number", default: null, min: null, max: null },
};
const TEXT: RecordColumn = {
  key: "t",
  label: "T",
  kind: { type: "text", default: null },
};
const SURFACE: RecordColumn = {
  key: "surface",
  label: "Surface",
  kind: {
    type: "choice",
    default: null,
    items: [
      { value: "Plowable", label: "Plowable" },
      { value: "Impervious", label: "Impervious" },
      { value: "Pervious", label: "Pervious" },
    ],
  },
};

describe("whether another record is offered", () => {
  it("offers one on an open-ended set", () => {
    // No published bound is the ordinary case: a junction may have as
    // many demand categories as a modeller cares to separate.
    expect(canAddRecord(set([NUMBER, TEXT], [[1, "a"]]))).toBe(true);
  });

  it("stops at the capacity the engine published", () => {
    // A control measure has one surface layer or none. The button under
    // a layer it already had could only ever refuse.
    const layer = set([NUMBER], [[150]], { capacity: 1 });
    expect(canAddRecord(layer)).toBe(false);
    expect(canAddRecord({ ...layer, rows: [] })).toBe(true);
  });

  it("never offers one on a set served read-only", () => {
    expect(
      canAddRecord(set([NUMBER], [], { editable: false, capacity: 4 })),
    ).toBe(false);
  });
});

describe("what a new record holds", () => {
  it("gives a number nothing and a name nothing", () => {
    expect(blankRecord(set([NUMBER, TEXT], []))).toEqual([0, ""]);
  });

  it("prefers a column's own default to an invented one", () => {
    const withDefault: RecordColumn = {
      ...NUMBER,
      kind: { type: "number", default: 2.5, min: null, max: null },
    };
    expect(blankRecord(set([withDefault], []))).toEqual([2.5]);
  });

  it("picks a choice the column does not already carry", () => {
    // The defect this is here for: the row used to arrive with "" in
    // this cell, which is not one of the three, so every add of a snow
    // surface was refused and a pack missing its pervious one could not
    // be given it from the interface at all.
    const pack = set([SURFACE], [["Plowable"], ["Impervious"]], {
      capacity: 3,
    });
    expect(blankRecord(pack)).toEqual(["Pervious"]);
  });

  it("takes the first choice on a set that carries none of them", () => {
    expect(blankRecord(set([SURFACE], []))).toEqual(["Plowable"]);
  });

  it("falls back to the first when every choice is spoken for", () => {
    // The engine gets to say what is wrong with it. Guessing further
    // would be inventing a rule the set never published — and this set
    // is full anyway, so `canAddRecord` means nobody asks.
    const full = set([SURFACE], [["Plowable"], ["Impervious"], ["Pervious"]]);
    expect(blankRecord(full)).toEqual(["Plowable"]);
  });

  it("answers a yes/no with the engine's own word", () => {
    // The cell's select offers "Yes" and "No" back, so a `false` here
    // would be a value the engine never serves.
    const covered: RecordColumn = {
      key: "covered",
      label: "Covered",
      kind: { type: "boolean", default: null },
    };
    expect(blankRecord(set([covered], []))).toEqual(["No"]);
    expect(
      blankRecord(
        set([{ ...covered, kind: { type: "boolean", default: true } }], []),
      ),
    ).toEqual(["Yes"]);
  });

  it("gives one cell per column, whatever the shapes are", () => {
    expect(blankRecord(set([NUMBER, SURFACE, TEXT], []))).toHaveLength(3);
  });
});
