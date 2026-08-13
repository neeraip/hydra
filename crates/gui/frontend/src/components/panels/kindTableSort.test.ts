/**
 * Ordering a kind's rows.
 *
 * Both of the defects pinned here were invisible in the code and obvious
 * on screen: a table of numbered elements listed 1, 10, 11, 2, and a
 * pump with no rated power sorted among the pumps rated at nothing.
 */
import { describe, expect, it } from "vitest";
import { type Cell, compareCells, sortRows } from "./kindTableSort";

/** Sort a list of cells directly, which is what the table does by index. */
function order(cells: Cell[], dir: "asc" | "desc" = "asc"): Cell[] {
  return sortRows(
    cells.map((_, i) => i),
    (i) => cells[i],
    dir,
  ).map((i) => cells[i]);
}

describe("compareCells", () => {
  it("reads digits in text as numbers", () => {
    // The ids in a drainage model are usually numbers, and comparing them
    // as characters put 10 before 2 — the order of the characters, and of
    // nothing a reader is looking for.
    expect(order(["9", "10", "2", "18"])).toEqual(["2", "9", "10", "18"]);
    expect(order(["C1", "C10", "C2"])).toEqual(["C1", "C2", "C10"]);
  });

  it("compares numbers as numbers", () => {
    expect(order([10, 2, 9])).toEqual([2, 9, 10]);
  });

  it("keeps a value of zero apart from having no value", () => {
    // §4.5.1 draws this distinction and the old comparison erased it: an
    // absent cell became "", which compares against a number as zero.
    expect(compareCells(0, null)).toBe(-1);
    expect(compareCells(null, 0)).toBe(1);
    expect(compareCells(null, null)).toBe(0);
  });

  it("puts the empty cells last in both directions", () => {
    // A missing value is not a small one. Reversing the column should not
    // march every element with nothing to say to the top — the reader
    // reversed what they were reading, not their interest in the rows
    // that have no answer for it.
    expect(order([3, null, 1])).toEqual([1, 3, null]);
    expect(order([3, null, 1], "desc")).toEqual([3, 1, null]);
  });

  it("treats an empty string as no value, which is how a tag arrives", () => {
    // A tag is always served, empty for an element that has none — the
    // one attribute that is present and blank rather than absent.
    expect(order(["b", "", "a"])).toEqual(["a", "b", ""]);
  });
});

describe("sortRows", () => {
  it("returns a permutation and does not disturb the rows it was given", () => {
    // The unsorted table draws from the caller's array, and `sort` works
    // in place.
    const rows = [0, 1, 2];
    const cells = [20, 3, 100];
    expect(sortRows(rows, (i) => cells[i], "asc")).toEqual([1, 0, 2]);
    expect(rows).toEqual([0, 1, 2]);
  });

  it("reverses for a descending column", () => {
    const cells = [20, 3, 100];
    expect(sortRows([0, 1, 2], (i) => cells[i], "desc")).toEqual([2, 0, 1]);
  });
});
