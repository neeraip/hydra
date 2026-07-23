/**
 * Tests for the Network Editor's pure search/sort helpers (tableSearch.ts):
 * the per-row haystack builder + cache, the section filter/sort used by
 * ElementsEditor, and the shared-datalist size decision used by the
 * Pipe/Pump/Valve tables' reference inputs.
 */
import { describe, expect, it } from "vitest";
import {
  buildRowHaystack,
  compareIds,
  filterSortRows,
  filterSortRowsWithPinned,
  getRowHaystack,
  idCollator,
  REF_DATALIST_MAX_OPTIONS,
  SEARCH_DEBOUNCE_MS,
  shouldUseRefDatalist,
} from "./tableSearch";

describe("buildRowHaystack", () => {
  it("lowercases and stringifies every field value", () => {
    const h = buildRowHaystack({ id: "J-101", elevation: 42.5, flag: true });
    expect(h).toContain("j-101");
    expect(h).toContain("42.5");
    expect(h).toContain("true");
  });

  it("stringifies null like the historical Object.values filter did", () => {
    expect(buildRowHaystack({ pressure: null })).toContain("null");
  });

  it("does not allow matches spanning two adjacent fields", () => {
    // Historical behaviour matched within a single field only; the NUL
    // separator preserves that for the joined haystack.
    const h = buildRowHaystack({ a: "J1", b: "2" });
    expect(h.includes("12")).toBe(false);
  });
});

describe("getRowHaystack", () => {
  it("returns the same cached string for the same row object", () => {
    const row = { id: "P7", from: "J1", to: "J2" };
    const first = getRowHaystack(row);
    expect(getRowHaystack(row)).toBe(first);
    expect(first).toBe(buildRowHaystack(row));
  });

  it("computes independent haystacks for distinct row objects", () => {
    expect(getRowHaystack({ id: "A" })).not.toEqual(
      getRowHaystack({ id: "B" }),
    );
  });
});

describe("filterSortRows", () => {
  const rows = [
    { id: "J10", elevation: 5, note: "Main" },
    { id: "J2", elevation: 12, note: "spur" },
    { id: "T1", elevation: 3, note: "tank MAIN" },
  ];

  it("returns the input array untouched for empty query and no sort", () => {
    expect(filterSortRows(rows, "", null, true)).toBe(rows);
  });

  it("filters case-insensitively across any field", () => {
    expect(filterSortRows(rows, "main", null, true).map((r) => r.id)).toEqual([
      "J10",
      "T1",
    ]);
    expect(filterSortRows(rows, "J1", null, true).map((r) => r.id)).toEqual([
      "J10",
    ]);
    // Numbers are matched via their string form.
    expect(filterSortRows(rows, "12", null, true).map((r) => r.id)).toEqual([
      "J2",
    ]);
  });

  it("returns an empty array when nothing matches", () => {
    expect(filterSortRows(rows, "no-such-value", null, true)).toEqual([]);
  });

  it("sorts numerically on number fields", () => {
    expect(
      filterSortRows(rows, "", "elevation", true).map((r) => r.elevation),
    ).toEqual([3, 5, 12]);
    expect(
      filterSortRows(rows, "", "elevation", false).map((r) => r.elevation),
    ).toEqual([12, 5, 3]);
  });

  it("sorts strings with collator ordering (same as localeCompare)", () => {
    const asc = filterSortRows(rows, "", "id", true).map((r) => r.id);
    const expected = rows.map((r) => r.id).sort((a, b) => a.localeCompare(b));
    expect(asc).toEqual(expected);
    const desc = filterSortRows(rows, "", "id", false).map((r) => r.id);
    expect(desc).toEqual([...expected].reverse());
  });

  it("does not mutate the input when sorting", () => {
    const before = [...rows];
    filterSortRows(rows, "", "id", true);
    expect(rows).toEqual(before);
  });

  it("filters then sorts when both are requested", () => {
    const out = filterSortRows(rows, "main", "elevation", true);
    expect(out.map((r) => r.id)).toEqual(["T1", "J10"]);
  });
});

describe("filterSortRowsWithPinned", () => {
  const existing = [
    { id: "J10", elevation: 5, note: "Main" },
    { id: "J2", elevation: 12, note: "spur" },
    { id: "T1", elevation: 3, note: "tank MAIN" },
  ];
  const pinned = [
    { id: "__new__:junction_1", elevation: 0, note: "" },
    { id: "__new__:junction_2", elevation: 0, note: "" },
  ];

  it("pins pending rows at the top ahead of existing rows", () => {
    const out = filterSortRowsWithPinned(existing, pinned, "", null, true);
    expect(out.map((r) => r.id)).toEqual([
      "__new__:junction_1",
      "__new__:junction_2",
      "J10",
      "J2",
      "T1",
    ]);
  });

  it("keeps pinned rows on top regardless of the active sort", () => {
    // Ascending elevation would place the (elevation 0) pending rows first
    // anyway; descending would sort them to the very end without pinning.
    const out = filterSortRowsWithPinned(
      existing,
      pinned,
      "",
      "elevation",
      false,
    );
    expect(out.map((r) => r.id)).toEqual([
      "__new__:junction_1",
      "__new__:junction_2",
      "J2",
      "J10",
      "T1",
    ]);
  });

  it("exempts pinned rows from the query filter", () => {
    // Mostly-empty pending rows match almost no query — they must stay
    // visible while the user has a search active.
    const out = filterSortRowsWithPinned(existing, pinned, "main", null, true);
    expect(out.map((r) => r.id)).toEqual([
      "__new__:junction_1",
      "__new__:junction_2",
      "J10",
      "T1",
    ]);
  });

  it("preserves the pinned rows' add order", () => {
    const reversed = [...pinned].reverse();
    const out = filterSortRowsWithPinned(existing, reversed, "", "id", true);
    expect(out.slice(0, 2).map((r) => r.id)).toEqual([
      "__new__:junction_2",
      "__new__:junction_1",
    ]);
  });

  it("matches filterSortRows exactly when there are no pinned rows", () => {
    expect(
      filterSortRowsWithPinned(existing, [], "main", "elevation", true),
    ).toEqual(filterSortRows(existing, "main", "elevation", true));
    // Referential stability: empty query + no sort + no pinned rows returns
    // the input array itself, like filterSortRows.
    expect(filterSortRowsWithPinned(existing, [], "", null, true)).toBe(
      existing,
    );
  });

  it("does not mutate its inputs", () => {
    const existingBefore = [...existing];
    const pinnedBefore = [...pinned];
    filterSortRowsWithPinned(existing, pinned, "main", "elevation", false);
    expect(existing).toEqual(existingBefore);
    expect(pinned).toEqual(pinnedBefore);
  });
});

describe("compareIds / idCollator", () => {
  it("matches localeCompare ordering", () => {
    const pairs: Array<[string, string]> = [
      ["J1", "J2"],
      ["J2", "J1"],
      ["a", "a"],
      ["T-1", "t-1"],
    ];
    for (const [a, b] of pairs) {
      expect(Math.sign(compareIds(a, b))).toBe(Math.sign(a.localeCompare(b)));
    }
  });

  it("is usable directly as an Array.prototype.sort comparator", () => {
    expect(["J10", "J2", "A"].sort(compareIds)).toEqual(
      ["J10", "J2", "A"].sort((a, b) => idCollator.compare(a, b)),
    );
  });
});

describe("shouldUseRefDatalist", () => {
  it("keeps the datalist at and below the threshold", () => {
    expect(shouldUseRefDatalist(0)).toBe(true);
    expect(shouldUseRefDatalist(1)).toBe(true);
    expect(shouldUseRefDatalist(REF_DATALIST_MAX_OPTIONS)).toBe(true);
  });

  it("drops the datalist above the threshold (e.g. 46k node ids)", () => {
    expect(shouldUseRefDatalist(REF_DATALIST_MAX_OPTIONS + 1)).toBe(false);
    expect(shouldUseRefDatalist(46000)).toBe(false);
  });
});

describe("SEARCH_DEBOUNCE_MS", () => {
  it("is a small positive delay", () => {
    expect(SEARCH_DEBOUNCE_MS).toBeGreaterThan(0);
    expect(SEARCH_DEBOUNCE_MS).toBeLessThanOrEqual(500);
  });
});
