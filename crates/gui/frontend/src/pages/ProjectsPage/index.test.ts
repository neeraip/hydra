import { describe, expect, it } from "vitest";
import { buildPageNumbers, msSortValue } from ".";

describe("msSortValue", () => {
  it("passes finite epoch-ms numbers through", () => {
    expect(msSortValue(0)).toBe(0);
    expect(msSortValue(1721600000000)).toBe(1721600000000);
  });

  it("maps null/undefined to undefined so sortUndefined:'last' applies", () => {
    expect(msSortValue(null)).toBeUndefined();
    expect(msSortValue(undefined)).toBeUndefined();
  });

  it("maps non-finite numbers to undefined", () => {
    expect(msSortValue(Number.NaN)).toBeUndefined();
    expect(msSortValue(Number.POSITIVE_INFINITY)).toBeUndefined();
  });

  it("orders newest-first under a descending numeric sort with nulls last", () => {
    // Simulates the table's ordering: numeric desc, undefined always last.
    const rows: Array<{ name: string; ms: number | null }> = [
      { name: "never-run", ms: null },
      { name: "old", ms: 1000 },
      { name: "new", ms: 2000 },
    ];
    const sorted = [...rows].sort((a, b) => {
      const av = msSortValue(a.ms);
      const bv = msSortValue(b.ms);
      if (av === undefined && bv === undefined) return 0;
      if (av === undefined) return 1;
      if (bv === undefined) return -1;
      return bv - av;
    });
    expect(sorted.map((r) => r.name)).toEqual(["new", "old", "never-run"]);
  });
});

describe("buildPageNumbers", () => {
  it("returns all pages when total ≤ 7", () => {
    expect(buildPageNumbers(0, 5)).toEqual([0, 1, 2, 3, 4]);
    expect(buildPageNumbers(3, 7)).toEqual([0, 1, 2, 3, 4, 5, 6]);
  });

  it("returns first, window around current, and last for large page counts", () => {
    const pages = buildPageNumbers(5, 20);
    // Must include first page (0) and last page (19)
    expect(pages[0]).toBe(0);
    expect(pages[pages.length - 1]).toBe(19);
    // Must include the current page
    expect(pages).toContain(5);
    // Must include neighbours
    expect(pages).toContain(4);
    expect(pages).toContain(6);
  });

  it("uses ellipsis when current is far from start", () => {
    const pages = buildPageNumbers(10, 20);
    expect(pages).toContain("…");
  });

  it("uses ellipsis when current is far from end", () => {
    const pages = buildPageNumbers(0, 20);
    expect(pages).toContain("…");
  });

  it("does not use ellipsis when current is near start", () => {
    const pages = buildPageNumbers(1, 10);
    // page 0, 1, 2 are all within 2 of start — leading ellipsis should be absent
    expect(pages[1]).not.toBe("…");
  });

  it("returns a single page [0] for total = 1", () => {
    expect(buildPageNumbers(0, 1)).toEqual([0]);
  });

  it("never contains duplicate entries", () => {
    for (let current = 0; current < 15; current++) {
      const pages = buildPageNumbers(current, 15);
      const numeric = pages.filter((p) => p !== "…") as number[];
      const unique = [...new Set(numeric)];
      expect(numeric.length).toBe(unique.length);
    }
  });

  it("is always in ascending order (ignoring ellipsis)", () => {
    for (let current = 0; current < 15; current++) {
      const pages = buildPageNumbers(current, 15);
      const numeric = pages.filter((p) => p !== "…") as number[];
      for (let i = 1; i < numeric.length; i++) {
        expect(numeric[i]).toBeGreaterThan(numeric[i - 1]);
      }
    }
  });
});
