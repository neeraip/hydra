/**
 * What the canvas is drawn on when there is no basemap under it.
 *
 * The ground already followed the app's theme — `.canvas-bg` paints
 * `--bg-app`, and the deck canvas over it is transparent — so a schematic
 * or a local-grid plan has always been light in the light theme. What was
 * missing is the override: a reader exporting a figure wants a light ground
 * without switching the whole application to a light theme, and someone
 * reading a dark schematic all day wants it to stay dark when the OS
 * decides otherwise at sunset.
 *
 * So this follows the shape the unit system already uses: a default that
 * tracks a setting made elsewhere, and an explicit choice that pins against
 * it. `theme` is not the same as choosing whichever value the theme
 * currently has — the first keeps tracking, the second stops.
 */

import type { ResolvedTheme } from "../theme";

/** The ground the canvas paints, or the theme's answer. */
export type CanvasBackground = "theme" | ResolvedTheme;

/** Track the app's theme, which is what this did before it could be set. */
export const DEFAULT_CANVAS_BACKGROUND: CanvasBackground = "theme";

const ALL: Record<CanvasBackground, true> = {
  theme: true,
  dark: true,
  light: true,
};

export const CANVAS_BACKGROUNDS = Object.keys(
  ALL,
) as readonly CanvasBackground[];

/** The two grounds, named. `theme` is not here: it has no name of its own,
 *  it wears whichever of these the theme resolves to. */
export const GROUND_LABEL: Record<ResolvedTheme, string> = {
  dark: "Dark",
  light: "Light",
};

/** The explicit choices, in menu order. */
export const CANVAS_BACKGROUND_OVERRIDES: readonly ResolvedTheme[] = [
  "dark",
  "light",
];

/**
 * Which ground is actually on screen.
 *
 * For labelling only — what the closed control shows, and what the Default
 * row has to name so it does not read as a third explicit choice. Painting
 * still goes through {@link canvasBackgroundStyle}, which deliberately says
 * nothing in the tracking case.
 */
export function effectiveCanvasBackground(
  background: CanvasBackground,
  theme: ResolvedTheme,
): ResolvedTheme {
  return background === "theme" ? theme : background;
}

/** Coerce a stored or corrupt value into one this understands. */
export function readCanvasBackground(value: unknown): CanvasBackground {
  return typeof value === "string" &&
    (CANVAS_BACKGROUNDS as readonly string[]).includes(value)
    ? (value as CanvasBackground)
    : DEFAULT_CANVAS_BACKGROUND;
}

/**
 * The CSS colour to paint the ground, or `undefined` to leave it to the
 * stylesheet.
 *
 * `undefined` rather than a resolved colour for `theme`, on purpose. The
 * stylesheet already answers that question and re-answers it when the theme
 * changes — including on an OS change while the app is open, which nothing
 * here would be told about. Reading the theme in JavaScript to paint the
 * same colour CSS would have painted is a second answer that can only ever
 * be wrong later.
 *
 * The two overrides use tokens rather than literals so the ground a reader
 * pins to is the same colour the theme would have given them.
 */
export function canvasBackgroundStyle(
  background: CanvasBackground,
): string | undefined {
  if (background === "theme") return undefined;
  return `var(--bg-app-${background})`;
}

/**
 * Whether the background picker takes the basemap picker's place.
 *
 * The two are alternatives for one slot, never both. A basemap *is* the
 * ground when there is one, so offering a background colour beside it would
 * be offering a choice with no effect; and where there is no basemap the
 * picker sits disabled in prime toolbar space saying nothing.
 *
 * `mapOnly` is the toolbar's existing name for "no basemap is possible
 * here" — a schematic, or a model with no georeference. Keying off it means
 * the swap cannot disagree with the control it replaces.
 */
export function backgroundPickerShown(mapOnly: boolean): boolean {
  return mapOnly;
}
