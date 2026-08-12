import { describe, expect, it } from "vitest";
import type { KindColumn } from "../../hooks";
import { cellEditor } from "./cellEditor";

const column = (kind: KindColumn["kind"], editable = true): KindColumn =>
  ({ key: "k", label: "L", editable, kind, values: [] }) as KindColumn;

const NUMBER = { type: "number", default: null, min: null, max: null } as const;
const TEXT = { type: "text", default: null } as const;
const CHOICE: KindColumn["kind"] = {
  type: "choice",
  default: null,
  items: [
    { value: "PRV", label: "PRV" },
    { value: "FCV", label: "FCV" },
  ],
};

describe("cellEditor", () => {
  it("offers a number for a numeric column", () => {
    expect(cellEditor(column(NUMBER), 12.5, true)).toEqual({
      kind: "number",
      value: 12.5,
    });
  });

  it("offers zero, which is a value and not an absence", () => {
    // An invert at datum, a minor loss of none. A falsy check here made
    // every such cell read-only.
    expect(cellEditor(column(NUMBER), 0, true)).toEqual({
      kind: "number",
      value: 0,
    });
  });

  it("offers a field for text, which used to be refused outright", () => {
    // The rule this replaced said text is never editable — true while
    // the only writable values were numbers, and wrong the day a tag
    // and an outlet became text that is. It lived beside the inspector,
    // so the same attribute took an input in a table and read as fixed
    // in the panel.
    expect(
      cellEditor(column({ type: "text", default: null }), "Zone A", true),
    ).toEqual({ kind: "text", value: "Zone A" });
  });

  it("offers the declared list for a choice, not a box to type in", () => {
    // The reason the column carries its shape at all: a valve type
    // typed by hand is a valve type that can be misspelled.
    expect(cellEditor(column(CHOICE), "PRV", true)).toEqual({
      kind: "choice",
      value: "PRV",
      items: CHOICE.type === "choice" ? CHOICE.items : [],
    });
  });

  it("offers Yes and No for a boolean", () => {
    // Rendered as a choice of two rather than a checkbox: the value
    // arrives as the engine's own word, and a checkbox would have to
    // invent which word means true.
    const editor = cellEditor(
      column({ type: "boolean", default: null }),
      "No",
      true,
    );
    expect(editor).toEqual({
      kind: "choice",
      value: "No",
      items: [
        { value: "Yes", label: "Yes" },
        { value: "No", label: "No" },
      ],
    });
  });

  it("offers nothing for a column the engine will not write", () => {
    expect(cellEditor(column(NUMBER, false), 12.5, true).kind).toBe("none");
  });

  it("offers nothing where the element has no value", () => {
    // The table serves a column for every attribute the kind declares,
    // including ones a given element has none of.
    expect(cellEditor(column(NUMBER), null, true).kind).toBe("none");
    expect(cellEditor(column(TEXT), undefined, true).kind).toBe("none");
  });

  it("offers nothing when nobody is listening for a write", () => {
    expect(cellEditor(column(NUMBER), 12.5, false).kind).toBe("none");
  });

  it("offers nothing for a shape that is not one value", () => {
    // A set of threshold edges is not a cell. Shown, never offered.
    const list = {
      type: "numberList",
      default: null,
      minLen: null,
      ascending: false,
    } as const;
    expect(cellEditor(column(list), 1, true).kind).toBe("none");
  });

  it("offers nothing when the value contradicts the declared shape", () => {
    // A column that says number and carries text is an engine and a
    // schema disagreeing; the cell reads it rather than offering an
    // input that cannot work.
    expect(cellEditor(column(NUMBER), "wide", true).kind).toBe("none");
    expect(cellEditor(column(TEXT), 3, true).kind).toBe("none");
  });
});
