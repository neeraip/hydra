/**
 * The water-distribution Editor's rail manifest: which engine element kind
 * each rail section shows, and what each section is called.
 *
 * Unlike the drainage Editor — whose rail is built straight from the
 * engine's §4.2 catalog because its table renders any kind from that
 * kind's schema — this rail is declared by hand, because each wds section
 * is a bespoke editable table with its own columns, validation and draft
 * plumbing. A new kind here needs a table, a create modal and dirty
 * tracking; a rail entry is the cheapest part, so deriving the entry buys
 * little and the hand-written kind→component map would remain either way.
 *
 * What hand-declaring does cost is drift, and it has already been paid:
 * this rail labelled the Curves section "Pump curves" long after the
 * engine's catalog called it "Curves" and its curve payload distinguished
 * tank-volume and valve-headloss curves from pump ones. So the manifest
 * lives here, apart from the component, and `editorRail.test.ts` pins it
 * to the catalog — every declared kind reaches exactly one section, under
 * the catalog's own label.
 */

import type { Section } from "./ElementsEditor";

/** Rail sections that are not element kinds — the model's named registries. */
export type CollectionId = "curves" | "patterns" | "controls";

export type EditorSectionId = Section | CollectionId;

/**
 * Engine kind id → the rail section that shows it.
 *
 * Covers **every** kind the wds catalog declares, which is what makes the
 * coverage test meaningful. Two kinds may share a section (see
 * `FOLDED_KINDS`); none may be missing.
 */
export const SECTION_FOR_KIND: Record<string, EditorSectionId> = {
  junction: "junctions",
  pipe: "pipes",
  pump: "pumps",
  tank: "tanks",
  reservoir: "reservoirs",
  valve: "valves",
  curve: "curves",
  pattern: "patterns",
  control: "controls",
  rule: "controls",
};

/**
 * Kinds deliberately shown under another kind's section, and the kind
 * whose section they join.
 *
 * Simple controls and rules are one editor because they are one decision
 * to the user — "what makes this network act on its own" — and splitting
 * them put two halves of one answer behind two rail entries. A ratified
 * deviation from one-section-per-kind, written down so the coverage test
 * can allow exactly this and nothing else.
 */
export const FOLDED_KINDS: Record<string, string> = { rule: "control" };

/**
 * Section id → its rail label.
 *
 * These are the catalog's `labelPlural` for the kind each section shows —
 * asserted, not merely intended.
 */
export const SECTION_LABEL: Record<EditorSectionId, string> = {
  junctions: "Junctions",
  pipes: "Pipes",
  pumps: "Pumps",
  tanks: "Tanks",
  reservoirs: "Reservoirs",
  valves: "Valves",
  curves: "Curves",
  patterns: "Patterns",
  controls: "Controls",
};

/** The collection sections, in rail order, with the kind each shows. */
export const COLLECTIONS: {
  id: CollectionId;
  label: string;
  kindId: string;
}[] = [
  { id: "curves", label: SECTION_LABEL.curves, kindId: "curve" },
  { id: "patterns", label: SECTION_LABEL.patterns, kindId: "pattern" },
  { id: "controls", label: SECTION_LABEL.controls, kindId: "control" },
];
