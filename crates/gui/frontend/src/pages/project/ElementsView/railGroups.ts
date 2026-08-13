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

/** What the rail draws above one entry. */
export interface RailHeading {
  /** The engine's name for the group this entry opens, or `null` when it
   *  continues the one above it (§4.2.1). */
  label: string | null;
  /** Whether the heavier rule parting map kinds from the rest sits here. */
  division: boolean;
}

/**
 * The heading and the rule above each rail entry.
 *
 * Two different marks, and they answer different questions. The rule says
 * "below here, selecting something highlights nothing on the canvas" —
 * derivable, and true whatever an engine calls its groups. The heading is
 * the engine's own word for what a run of kinds is, and the application
 * never learns what any of them mean.
 *
 * A heading is drawn wherever the group *changes*, which is why §4.2.1
 * requires a group to be one run of the catalog: gathering scattered
 * kinds under one heading would mean reordering a list the engine
 * ordered.
 *
 * A run of one still gets its heading. A lone kind under no heading, in a
 * rail where everything else has one, reads as an oversight rather than
 * as a group with one member.
 */
export function railHeadings(
  entries: readonly { class: string; group?: string }[],
): RailHeading[] {
  const division = railGroupBreak(entries.map((e) => e.class));
  return entries.map((e, i) => ({
    label:
      e.group != null && e.group !== entries[i - 1]?.group ? e.group : null,
    division: i === division,
  }));
}
