import { useRootFlag } from "./useRootAttribute";

/**
 * Live view of the app's "High contrast" accessibility setting.
 *
 * Most of what this setting does happens in the stylesheet, which reads the
 * same attribute. This is for the canvas, which paints into a GL context
 * the cascade cannot reach.
 */
export function useHighContrast(): boolean {
  return useRootFlag("data-high-contrast");
}
