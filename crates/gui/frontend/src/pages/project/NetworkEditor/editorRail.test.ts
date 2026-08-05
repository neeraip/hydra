import { describe, expect, it } from "vitest";
import {
  COLLECTIONS,
  FOLDED_KINDS,
  SECTION_FOR_KIND,
  SECTION_LABEL,
} from "./editorRail";

/**
 * The water-distribution §4.2 element catalog, mirrored from
 * `crates/engine-wds/src/descriptors.rs`.
 *
 * A hand-mirror, like every other cross-boundary claim in this codebase:
 * this test cannot invoke `list_element_kinds`, so the Rust side asserts
 * that the engine still publishes exactly this, and this side asserts the
 * rail still covers exactly this. Neither test alone catches drift — the
 * Rust one notices the engine changing, this one notices the rail
 * changing, and updating one without the other fails the pair.
 *
 * The Rust half is `the_gui_editor_rail_mirrors_this_catalog` in
 * `crates/gui/src/commands/projects.rs`.
 */
const CATALOG: ReadonlyArray<{ id: string; labelPlural: string }> = [
  { id: "junction", labelPlural: "Junctions" },
  { id: "reservoir", labelPlural: "Reservoirs" },
  { id: "tank", labelPlural: "Tanks" },
  { id: "pipe", labelPlural: "Pipes" },
  { id: "pump", labelPlural: "Pumps" },
  { id: "valve", labelPlural: "Valves" },
  { id: "pattern", labelPlural: "Patterns" },
  { id: "curve", labelPlural: "Curves" },
  { id: "control", labelPlural: "Controls" },
  { id: "rule", labelPlural: "Rules" },
];

describe("the wds Editor rail against the engine catalog", () => {
  /**
   * Every kind the engine declares is reachable. A kind with no section is
   * a part of the model the Editor silently cannot open — and because the
   * rail is hand-declared, adding a kind to the engine does nothing here
   * until someone remembers to.
   */
  it("gives every declared kind a section", () => {
    const missing = CATALOG.filter((k) => !(k.id in SECTION_FOR_KIND));
    expect(missing.map((k) => k.id)).toEqual([]);
  });

  /** The reverse: a section for a kind the engine no longer publishes is a
   * rail entry that can only ever be empty. */
  it("declares no section for a kind the engine does not publish", () => {
    const known = new Set(CATALOG.map((k) => k.id));
    const stray = Object.keys(SECTION_FOR_KIND).filter((k) => !known.has(k));
    expect(stray).toEqual([]);
  });

  /**
   * Sections are named by the catalog, not by hand.
   *
   * This is the assertion that has already earned its place: the Curves
   * section read "Pump curves" while the engine called it "Curves" and its
   * curve payload distinguished tank-volume and valve-headloss curves from
   * pump ones — so a model's tank volume curve was filed under a heading
   * claiming to be about pumps.
   */
  it("labels each section with the catalog's own plural", () => {
    for (const kind of CATALOG) {
      const section = SECTION_FOR_KIND[kind.id];
      if (kind.id in FOLDED_KINDS) continue; // labelled by its host kind
      expect(SECTION_LABEL[section]).toBe(kind.labelPlural);
    }
  });

  /**
   * Two kinds may share a section only where that fold is written down.
   * Rules join Controls because they are one decision to the user; any
   * *other* collision is a section quietly showing two kinds under one
   * kind's name.
   */
  it("shares a section only where a fold is declared", () => {
    const bySection = new Map<string, string[]>();
    for (const [kind, section] of Object.entries(SECTION_FOR_KIND)) {
      bySection.set(section, [...(bySection.get(section) ?? []), kind]);
    }
    for (const [section, kinds] of bySection) {
      if (kinds.length === 1) continue;
      const folded = kinds.filter((k) => k in FOLDED_KINDS);
      const hosts = kinds.filter((k) => !(k in FOLDED_KINDS));
      expect(hosts, `section ${section} has two unfolded kinds`).toHaveLength(
        1,
      );
      for (const k of folded) {
        expect(SECTION_FOR_KIND[FOLDED_KINDS[k]]).toBe(section);
      }
    }
  });

  /** Each declared fold names a kind that exists and is itself shown. */
  it("folds only onto kinds the catalog declares", () => {
    const known = new Set(CATALOG.map((k) => k.id));
    for (const [folded, host] of Object.entries(FOLDED_KINDS)) {
      expect(known.has(folded)).toBe(true);
      expect(known.has(host)).toBe(true);
    }
  });

  /** The collection entries carry the same labels the section map does —
   * they are two views of one manifest, not two manifests. */
  it("labels the collections from the section map", () => {
    for (const c of COLLECTIONS) {
      expect(c.label).toBe(SECTION_LABEL[c.id]);
      expect(SECTION_FOR_KIND[c.kindId]).toBe(c.id);
    }
  });
});
