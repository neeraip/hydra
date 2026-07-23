/**
 * Tests for the element time-series cache: key uniqueness across every key
 * part (the invalidation contract) and bounded LRU behavior.
 */
import { describe, expect, it } from "vitest";
import {
  type ElementSeriesKeyParts,
  elementSeriesCacheKey,
  LruCache,
} from "./seriesCache";

const base: ElementSeriesKeyParts = {
  projectId: "p1",
  scenarioId: null,
  resultGeneration: 0,
  kind: "node",
  elementId: "J1",
};

describe("elementSeriesCacheKey", () => {
  it("is stable for identical parts", () => {
    expect(elementSeriesCacheKey(base)).toBe(
      elementSeriesCacheKey({ ...base }),
    );
  });

  it("changes when any part changes", () => {
    const variants: ElementSeriesKeyParts[] = [
      { ...base, projectId: "p2" },
      { ...base, scenarioId: "s1" },
      { ...base, resultGeneration: 1 },
      { ...base, kind: "link" },
      { ...base, elementId: "J2" },
    ];
    const keys = new Set([
      elementSeriesCacheKey(base),
      ...variants.map(elementSeriesCacheKey),
    ]);
    expect(keys.size).toBe(1 + variants.length);
  });

  it("distinguishes the null scenario from a scenario literally named like the marker", () => {
    expect(elementSeriesCacheKey({ ...base, scenarioId: "base" })).not.toBe(
      elementSeriesCacheKey({ ...base, scenarioId: null }),
    );
  });

  it("re-running (generation bump) invalidates even for the same element", () => {
    // The core contract: re-selecting an element after a re-run must miss.
    const before = elementSeriesCacheKey(base);
    const after = elementSeriesCacheKey({ ...base, resultGeneration: 1 });
    expect(before).not.toBe(after);
  });

  it("does not collide when id content shifts across part boundaries", () => {
    const a = elementSeriesCacheKey({
      ...base,
      projectId: "ab",
      elementId: "c",
    });
    const b = elementSeriesCacheKey({
      ...base,
      projectId: "a",
      elementId: "bc",
    });
    expect(a).not.toBe(b);
  });
});

describe("LruCache", () => {
  it("misses return undefined; cached null is distinguishable from a miss", () => {
    const cache = new LruCache<number[] | null>(4);
    expect(cache.get("k")).toBeUndefined();
    cache.set("k", null);
    expect(cache.get("k")).toBeNull();
  });

  it("evicts the least recently used entry beyond capacity", () => {
    const cache = new LruCache<number>(2);
    cache.set("a", 1);
    cache.set("b", 2);
    cache.set("c", 3); // evicts "a"
    expect(cache.get("a")).toBeUndefined();
    expect(cache.get("b")).toBe(2);
    expect(cache.get("c")).toBe(3);
    expect(cache.size).toBe(2);
  });

  it("get refreshes recency", () => {
    const cache = new LruCache<number>(2);
    cache.set("a", 1);
    cache.set("b", 2);
    cache.get("a"); // "b" is now least recently used
    cache.set("c", 3); // evicts "b"
    expect(cache.get("a")).toBe(1);
    expect(cache.get("b")).toBeUndefined();
  });

  it("set on an existing key updates the value without growing", () => {
    const cache = new LruCache<number>(2);
    cache.set("a", 1);
    cache.set("a", 9);
    expect(cache.get("a")).toBe(9);
    expect(cache.size).toBe(1);
  });

  it("rejects a non-positive capacity", () => {
    expect(() => new LruCache(0)).toThrow(/positive integer/);
  });
});
