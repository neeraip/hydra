/**
 * What the network list would have to be wide enough to show.
 *
 * This is the data half of fitting the panel to its contents. It does not
 * compute a width — it picks the *content* that sets the width, so a
 * caller can render one real row carrying it and let the browser do the
 * measuring. Reproducing the row's badge lane, gaps and padding here to
 * arrive at a number would be a second copy of the row's layout, and the
 * two would drift the first time someone changed a padding.
 *
 * Virtualisation is not an obstacle to this, which is worth stating
 * because it looks like one: the virtualiser windows the *DOM*, while
 * every row's data is in memory either way. Which row is widest is a
 * question about the data.
 *
 * ## Why this is cheap
 *
 * Every lane whose width varies is set in the monospace face, so a
 * character count *is* a width and no text has to be measured or even
 * built. One pass over the rows reads two string lengths and compares two
 * numbers per row — no allocation, nothing formatted.
 *
 * The value lane is the exception, and it is the reason this does not
 * format anything per row: with a fixed number of decimals and a constant
 * unit label, the widest rendering is one of the two extremes, so the
 * pass tracks the smallest and largest numbers and the caller formats
 * exactly those two. Formatting every row instead would allocate one
 * string per element, which at 50k elements is the whole cost of the
 * feature and all of it avoidable.
 */

import type { Row } from "./NetworkList";

/** The widest content the list holds, for a caller to render and measure. */
export interface FitContent {
  /** The longest element id — sets the width of the row's first line. */
  id: string;
  /**
   * The longest secondary line, or `null` when subtitles are not shown.
   *
   * Kept apart from `id` rather than reduced to one "longest string":
   * the two lines are set at different sizes, so their character counts
   * are not comparable and only a renderer can say which is wider.
   */
  context: string | null;
  /**
   * The two extreme values, for the caller to format. Whichever renders
   * longer sets the value lane; `null` when no row carries a value.
   */
  extremes: readonly [number, number] | null;
  /**
   * Whether any row carries the zoom affordance, which widens a row's
   * padding. A fit measured without it is narrow by that padding on
   * exactly the rows that have it.
   */
  zoomable: boolean;
}

/**
 * The content that sets the list's width.
 *
 * @param rows      every row currently listed, filters already applied —
 *                  fitting to what is on screen rather than to the whole
 *                  model, since that is what the reader is looking at.
 * @param searching whether the secondary line is being shown; it is not
 *                  rendered outside a search, and a width reserved for a
 *                  line nobody can see is just a wider panel.
 * @returns `null` for an empty list, which has no content to fit.
 */
export function fitContent(
  rows: readonly Row[],
  searching: boolean,
): FitContent | null {
  if (rows.length === 0) return null;

  let id = "";
  let idLength = -1;
  let context: string | null = null;
  let contextLength = -1;
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  let zoomable = false;

  for (const row of rows) {
    // Each field read once into a local. At this size the repeated
    // property access is the loop's whole cost, and there is nothing else
    // in here to dominate it.
    const rowId = row.id;
    // `.length` rather than any measurement: monospace, so this *is* the
    // width, and it neither allocates nor touches the DOM.
    if (rowId.length > idLength) {
      idLength = rowId.length;
      id = rowId;
    }
    if (searching) {
      const rowContext = row.context;
      if (rowContext.length > contextLength) {
        contextLength = rowContext.length;
        context = rowContext;
      }
    }
    const value = row.value;
    if (value != null) {
      if (value < min) min = value;
      if (value > max) max = value;
    }
    if (row.canZoom) zoomable = true;
  }

  return {
    id,
    context,
    extremes: min <= max ? [min, max] : null,
    zoomable,
  };
}
