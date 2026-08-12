/**
 * How much unsaved work is staged, by section and in total.
 *
 * This is the answer to "is there unsaved work", which is load-bearing
 * well beyond the save bar: the navigation guard reads it before letting
 * you leave the project page, and the Editor's rail marks sections from
 * it. An undercount does not show a wrong number — it lets staged work be
 * discarded without anyone being asked.
 *
 * Extracted from the provider for two reasons. It was fifteen inline
 * additions spread over four expressions, which is a lot of places to
 * forget one when a new kind of draft is added. And the total was summed
 * *separately* from the per-section counts, so the two could disagree: a
 * new section counted in one and not the other would mark a rail entry
 * dirty while the guard let you walk away from it. The total is now
 * derived from the sections, so that particular divergence is not
 * expressible.
 */

/** Sizes of every container the draft holds, in staging order. */
export interface DraftContainerSizes {
  curveAdds: number;
  curveEdits: number;
  curveDeletes: number;
  patternAdds: number;
  patternEdits: number;
  patternDeletes: number;
  controlAdds: number;
  controlEdits: number;
  controlDeletes: number;
  ruleAdds: number;
  ruleEdits: number;
  ruleDeletes: number;
}

export interface DirtyBySection {
  curves: number;
  patterns: number;
  controls: number;
}

/**
 * Which containers make up each Editor section.
 *
 * A table rather than arithmetic so adding a container is one entry in
 * one place — and so a test can assert that every declared container
 * reaches the total.
 */
const SECTION_CONTAINERS: Record<
  keyof DirtyBySection,
  readonly (keyof DraftContainerSizes)[]
> = {
  curves: ["curveAdds", "curveEdits", "curveDeletes"],
  patterns: ["patternAdds", "patternEdits", "patternDeletes"],
  // Controls and rules are edited in one section, so they count as one.
  controls: [
    "controlAdds",
    "controlEdits",
    "controlDeletes",
    "ruleAdds",
    "ruleEdits",
    "ruleDeletes",
  ],
};

/** Every container the draft knows about — the completeness reference. */
export const DRAFT_CONTAINERS = Object.values(SECTION_CONTAINERS).flat();

export function draftDirty(sizes: DraftContainerSizes): {
  bySection: DirtyBySection;
  total: number;
} {
  const bySection = {} as DirtyBySection;
  for (const section of Object.keys(
    SECTION_CONTAINERS,
  ) as (keyof DirtyBySection)[]) {
    bySection[section] = SECTION_CONTAINERS[section].reduce(
      (n, key) => n + (sizes[key] ?? 0),
      0,
    );
  }
  // Derived from the sections, never re-added: the guard and the rail
  // then cannot disagree about whether there is work to lose.
  const total = Object.values(bySection).reduce((a, b) => a + b, 0);
  return { bySection, total };
}
