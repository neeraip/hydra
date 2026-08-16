/**
 * Which theme is actually on screen.
 *
 * The app's setting is `dark`, `light` or `system`; what a reader is looking
 * at is only ever one of the first two. `AppContext` already resolves that —
 * including following the OS while the app is open — and writes the answer
 * to `data-theme` on the document element, where the whole stylesheet reads
 * it.
 *
 * So this reads the same attribute rather than resolving the setting a
 * second time. A second resolution is a second answer, and the two would
 * differ the moment one of them learned about an OS change and the other
 * did not.
 *
 * Only for text that has to *name* the theme — a menu row saying what
 * "Match theme" comes out as. Anything that merely needs to be the right
 * colour should use a CSS variable and let the cascade answer.
 */

import { useRootAttribute } from "./hooks/useRootAttribute";

export type ResolvedTheme = "dark" | "light";

/** The app's own default, and what an unset attribute means. */
const FALLBACK: ResolvedTheme = "dark";

/** Interpret the attribute's value. Anything unexpected reads as the app's
 *  own default rather than throwing at render time. */
function resolvedThemeFrom(attribute: string | null): ResolvedTheme {
  return attribute === "light" ? "light" : FALLBACK;
}

/**
 * The theme on screen, kept current.
 *
 * Observes the attribute rather than polling or subscribing to the setting:
 * every route that can change the theme — the Settings menu, the command
 * palette, the OS at sunset — ends by writing here, so watching this one
 * place needs no list of them.
 */
export function useResolvedTheme(): ResolvedTheme {
  return resolvedThemeFrom(useRootAttribute("data-theme"));
}
