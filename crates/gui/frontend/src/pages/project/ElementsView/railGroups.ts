/**
 * Where the drainage editor's rail draws its rule.
 *
 * The rail lists every kind the engine declares, and those kinds are two
 * different sorts of thing: the ones placed on the map — junctions,
 * conduits, subcatchments — and the ones that are not, like curves, time
 * series and pollutants. Selecting one of the first sorts highlights an
 * element on the canvas; selecting one of the second cannot, because
 * there is nothing out there to highlight.
 *
 * A single rule between the two says that much without inventing a second
 * level of navigation, which is the same choice the water-distribution
 * editor makes above its collections.
 *
 * A number rather than a flag per entry, because the answer is about the
 * list and not about any one member of it: the break belongs where the
 * first non-spatial kind is, and only the list knows where that falls.
 */

/**
 * The index the rule sits above, or `null` for no rule at all.
 *
 * Returns `null` when there is nothing to part — a catalog of only
 * spatial kinds, or only non-spatial ones. The second case matters
 * because a rule above the very first entry is not a divider, it is a
 * stray line under the heading.
 *
 * @param classes each rail entry's element class, in rail order.
 */
export function railGroupBreak(classes: readonly string[]): number | null {
  const first = classes.indexOf("collection");
  return first > 0 ? first : null;
}
