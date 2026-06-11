import { describe, expect, it } from "vitest";
import { buildPageNumbers } from ".";

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
