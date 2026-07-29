/**
 * User-controlled text size.
 *
 * Hydra can't inherit the OS text-size setting: macOS Dynamic Type doesn't
 * reach WKWebView content, and there is no equivalent signal on the other
 * platforms. So the app carries its own control, persisted locally and applied
 * before first paint.
 *
 * The mechanism is one CSS custom property. `--text-scale` multiplies the root
 * font-size (see the `html` rule in `app.css`), and every `--text-*` token is
 * rem-relative, so setting it here resizes all typography at once. Nothing
 * else needs to know this setting exists.
 */

/** Persisted alongside the other accessibility preferences in `main.tsx`. */
export const TEXT_SCALE_KEY = "hydra2-text-scale";

export interface TextScaleOption {
  value: number;
  label: string;
}

/**
 * The offered sizes. Deliberately a short, modest range rather than a free
 * slider: each step has to keep every panel, table and toolbar usable, and
 * that is a claim we verify per step rather than for arbitrary input.
 */
export const TEXT_SCALES: readonly TextScaleOption[] = [
  { value: 0.9, label: "Small" },
  { value: 1, label: "Default" },
  { value: 1.1, label: "Large" },
  { value: 1.25, label: "Larger" },
] as const;

export const DEFAULT_TEXT_SCALE = 1;

/**
 * Coerce a persisted value to a supported scale.
 *
 * Anything unrecognised — absent, malformed, or a scale from a future version
 * that offered a step this build doesn't — falls back to the default rather
 * than being applied. An unbounded stored number would otherwise be able to
 * render the app unusable with no way back to Settings to fix it.
 *
 * Pure, so it can be tested without a DOM.
 */
export function parseTextScale(raw: string | null): number {
  if (raw === null) return DEFAULT_TEXT_SCALE;
  const parsed = Number.parseFloat(raw);
  if (!Number.isFinite(parsed)) return DEFAULT_TEXT_SCALE;
  const match = TEXT_SCALES.find((o) => Math.abs(o.value - parsed) < 1e-9);
  return match ? match.value : DEFAULT_TEXT_SCALE;
}

/** The persisted scale, or the default outside a browser. */
export function readTextScale(): number {
  if (typeof localStorage === "undefined") return DEFAULT_TEXT_SCALE;
  return parseTextScale(localStorage.getItem(TEXT_SCALE_KEY));
}

/** Apply `scale` to the document root. Does not persist. */
export function applyTextScale(scale: number): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.setProperty("--text-scale", String(scale));
}

/** Apply and persist `scale`. */
export function setTextScale(scale: number): void {
  applyTextScale(scale);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(TEXT_SCALE_KEY, String(scale));
  }
}
