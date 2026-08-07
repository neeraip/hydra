/**
 * The command palette's element-finder mode.
 *
 * The palette searches commands until the query opens with a marker, and
 * then it searches the model's elements instead. That marker is one
 * decision with several readers — the palette deciding which list to show,
 * the helper command that drops you into the mode, and the shortcut that
 * does the same without the palette — and a literal `"#"` written out at
 * each of them is the shape that has already cost this codebase a day: the
 * copies agree until one is changed.
 */

/** What turns the palette from a command list into an element search. */
export const ELEMENT_FINDER_PREFIX = "#";

/**
 * The query that opens the finder with nothing typed yet.
 *
 * The prefix alone. Both routes into the mode use this, so the shortcut
 * cannot land the user somewhere the menu command would not.
 */
export function elementFinderSeed(): string {
  return ELEMENT_FINDER_PREFIX;
}

/** Whether a query is asking for elements rather than commands. */
export function isElementFinderQuery(query: string): boolean {
  return query.startsWith(ELEMENT_FINDER_PREFIX);
}

/**
 * What to search for, with the marker and surrounding space removed.
 *
 * Lowercased, because element ids are matched case-insensitively and doing
 * it here means no caller has to remember to. Empty for a query that is
 * only the marker — the mode is open, nothing is being looked for yet.
 */
export function elementFinderTerm(query: string): string {
  return isElementFinderQuery(query)
    ? query.slice(ELEMENT_FINDER_PREFIX.length).trim().toLowerCase()
    : "";
}
