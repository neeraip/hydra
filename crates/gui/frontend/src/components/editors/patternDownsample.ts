/**
 * Pure min/max bucket downsampling for time-pattern previews.
 *
 * EPANET patterns can be very long (hourly-for-a-year = 8,760 multipliers).
 * Rendering one DOM element per multiplier is the same failure class that
 * froze the Issues panel, so long patterns are drawn as a downsampled
 * envelope instead: each bucket keeps the min and max of its slice, which
 * preserves every spike/trough visually no matter how far the series is
 * compressed.
 *
 * Kept free of React/DOM so it runs under the node-env vitest setup.
 */

export interface MinMaxBucket {
  min: number;
  max: number;
}

/**
 * Compress `values` into at most `maxBuckets` {min,max} buckets.
 *
 * - `values.length <= maxBuckets` → one bucket per value (lossless).
 * - Otherwise buckets partition the index range contiguously (every index
 *   lands in exactly one bucket) and each keeps its slice's min/max.
 */
export function downsampleMinMax(
  values: readonly number[],
  maxBuckets: number,
): MinMaxBucket[] {
  if (maxBuckets <= 0 || values.length === 0) return [];
  const n = values.length;
  if (n <= maxBuckets) return values.map((v) => ({ min: v, max: v }));
  const buckets: MinMaxBucket[] = [];
  for (let b = 0; b < maxBuckets; b++) {
    const start = Math.floor((b * n) / maxBuckets);
    const end = Math.max(start + 1, Math.floor(((b + 1) * n) / maxBuckets));
    let min = values[start];
    let max = values[start];
    for (let i = start + 1; i < end; i++) {
      const v = values[i];
      if (v < min) min = v;
      if (v > max) max = v;
    }
    buckets.push({ min, max });
  }
  return buckets;
}

/**
 * Closed SVG path for the min/max envelope of `buckets`, in a
 * `width × height` coordinate space with y=0 at the top.
 *
 * The top edge traces bucket maxima left→right, the bottom edge traces the
 * minima right→left. Values are clamped to [0, yMax] so a pathological
 * multiplier cannot push the path outside the viewBox.
 */
export function envelopePath(
  buckets: readonly MinMaxBucket[],
  width: number,
  height: number,
  yMax: number,
): string {
  if (buckets.length === 0 || width <= 0 || height <= 0 || yMax <= 0) {
    return "";
  }
  const n = buckets.length;
  const xAt = (i: number) => (n === 1 ? width / 2 : (i / (n - 1)) * width);
  const yAt = (v: number) => {
    const clamped = Math.max(0, Math.min(yMax, v));
    return height - (clamped / yMax) * height;
  };
  const top = buckets
    .map((b, i) => `${xAt(i).toFixed(2)},${yAt(b.max).toFixed(2)}`)
    .join(" L ");
  const bottom = [...buckets]
    .reverse()
    .map((b, i) => `${xAt(n - 1 - i).toFixed(2)},${yAt(b.min).toFixed(2)}`)
    .join(" L ");
  return `M ${top} L ${bottom} Z`;
}
