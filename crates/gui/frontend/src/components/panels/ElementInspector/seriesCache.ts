/**
 * Small LRU cache for element time series keyed on result identity.
 *
 * The key includes `resultGeneration` (AppContext's freshness token for
 * result metadata) so a re-run that produces value-equal metadata still
 * invalidates cached series, while re-selecting the same element between
 * runs is a hit and never refetches.
 */

export interface ElementSeriesKeyParts {
  projectId: string;
  /** `null` = base model (distinct from any scenario id). */
  scenarioId: string | null;
  /** Freshness token from `useSimulation().resultGeneration`. */
  resultGeneration: number;
  kind: "node" | "link";
  elementId: string;
}

/** Separator that cannot appear in ids (ASCII unit separator). */
const SEP = "\u001f";

/** Marker for the null (base-model) scenario — contains the separator so it
 *  can never collide with a real scenario id. */
const BASE_SCENARIO = `${SEP}base`;

/**
 * Build the cache key. Joins the parts with a control character never
 * present in ids so distinct part tuples can never collide, and encodes
 * the null scenario distinctly from any real scenario id.
 */
export function elementSeriesCacheKey(parts: ElementSeriesKeyParts): string {
  return [
    parts.projectId,
    parts.scenarioId === null ? BASE_SCENARIO : parts.scenarioId,
    String(parts.resultGeneration),
    parts.kind,
    parts.elementId,
  ].join(SEP);
}

/**
 * Bounded map with least-recently-used eviction. `get` returns `undefined`
 * on a miss — `null` is a valid cached value (a "no series" backend answer),
 * so callers must distinguish miss (`undefined`) from cached-null.
 */
export class LruCache<V> {
  private readonly map = new Map<string, V>();

  constructor(private readonly maxEntries: number) {
    if (!Number.isInteger(maxEntries) || maxEntries < 1) {
      throw new Error("LruCache: maxEntries must be a positive integer");
    }
  }

  get(key: string): V | undefined {
    if (!this.map.has(key)) return undefined;
    // Refresh recency: Map iteration order is insertion order.
    const value = this.map.get(key) as V;
    this.map.delete(key);
    this.map.set(key, value);
    return value;
  }

  set(key: string, value: V): void {
    if (this.map.has(key)) this.map.delete(key);
    this.map.set(key, value);
    if (this.map.size > this.maxEntries) {
      // Evict the least recently used entry (first in iteration order).
      const oldest = this.map.keys().next().value;
      if (oldest !== undefined) this.map.delete(oldest);
    }
  }

  get size(): number {
    return this.map.size;
  }
}
