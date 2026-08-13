// ── Ordering a kind's rows ───────────────────────────────────────────────────
//
// The comparison a sorted column makes, out of the component so it can be
// read and asserted. Two things it was getting wrong were invisible in
// the code and obvious on screen:
//
//  - Ids were compared as text, so a model whose elements are numbered
//    listed 1, 10, 11, 2, 20 — which is the order of the characters and
//    not of anything a reader is looking for.
//  - A cell with no value was coerced to the empty string, which compares
//    against a number as zero. A pump with no rated power sorted among
//    the pumps rated at nothing, and §4.5.1 is explicit that those are
//    different states: one has no value to show, the other has a value
//    and it is zero.

import { compareNatural } from "../../naturalOrder";

/** What one cell can hold: the shapes a column's values arrive in. */
export type Cell = number | string | null;

/**
 * Order two cells, with absent values last.
 *
 * **Last in both directions.** A missing value is not a small one, and
 * reversing the sort should not march every element that has nothing to
 * say to the top — the reader reversed the column they were reading, not
 * their interest in the rows that have no answer for it.
 *
 * Returns a comparison for the ascending direction; the caller negates it
 * for descending, which is why the null rule is applied before that and
 * not inside it.
 */
export function compareCells(a: Cell, b: Cell): number {
  const aEmpty = a == null || a === "";
  const bEmpty = b == null || b === "";
  if (aEmpty || bEmpty) return aEmpty && bEmpty ? 0 : aEmpty ? 1 : -1;
  if (typeof a === "number" && typeof b === "number") {
    return a === b ? 0 : a < b ? -1 : 1;
  }
  return compareNatural(String(a), String(b));
}

/**
 * The row indices of a sorted column, in the order they are drawn.
 *
 * Indices rather than rows: the values live in columnar arrays, so an
 * order is a permutation and sorting never copies a value.
 */
export function sortRows(
  rows: readonly number[],
  cell: (row: number) => Cell,
  dir: "asc" | "desc",
): number[] {
  // A copy, because the caller's array is what the unsorted table draws
  // from and `sort` works in place.
  return [...rows].sort((a, b) => {
    const cmp = compareCells(cell(a), cell(b));
    // The empty-last rule survives the reversal: it is about whether a
    // value exists, which reversing the column does not change.
    if (cmp === 0) return 0;
    const aEmpty = isEmpty(cell(a));
    const bEmpty = isEmpty(cell(b));
    if (aEmpty !== bEmpty) return cmp;
    return dir === "asc" ? cmp : -cmp;
  });
}

function isEmpty(v: Cell): boolean {
  return v == null || v === "";
}
