// ── Ordering text the way a reader reads it ──────────────────────────────────
//
// Element ids in these models are usually numbers written as text — a
// drainage model numbers its junctions 1, 2, … 1423 — and the default
// `Array.prototype.sort` compares characters, so it lists 1, 10, 11, 2,
// 20. That is the order of the characters and of nothing anyone is
// looking for, and it appeared everywhere an id was offered: the sorted
// columns of the Editor's tables, and every datalist a reference field
// drops down.
//
// One comparator, because they are one question.

/**
 * Compare text with its digits read as numbers.
 *
 * Built once and shared: a collator is expensive to construct, and these
 * run per comparison — which is O(n log n) of them on tables that reach
 * tens of thousands of rows.
 *
 * `sensitivity: "base"` so a list of ids does not split on case alone.
 * The engines treat ids case-insensitively when they resolve them, so a
 * reader looking for `p1` should not have to know whether the file
 * capitalised it.
 */
const COLLATOR = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

/** Order two strings naturally — pass it straight to `sort`. */
export function compareNatural(a: string, b: string): number {
  return COLLATOR.compare(a, b);
}
