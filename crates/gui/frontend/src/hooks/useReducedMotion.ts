import { useRootFlag } from "./useRootAttribute";

/**
 * Live view of the app's "Reduce motion" accessibility setting.
 *
 * For the animations CSS does not own: the link pulse, the fit flight, and
 * anything else the canvas drives itself.
 */
export function useReducedMotion(): boolean {
  return useRootFlag("data-reduced-motion");
}
