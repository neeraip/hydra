import { describe, expect, it } from "vitest";
import { shortcutSections } from "./ShortcutCard";

/**
 * The card is a hand-written list sitting beside a hand-written switch of
 * key handlers, and the drift between them is invisible: a shortcut card is
 * only ever read by someone who does not already know the answer, so a
 * wrong row is believed.
 *
 * These check what the list can be checked for on its own. What they cannot
 * check is whether a listed shortcut actually fires — that lives in a
 * keydown handler in `App.tsx` and would need the whole app mounted.
 */

const SECTIONS = shortcutSections("⌘", "⇧");

const allRows = SECTIONS.flatMap((s) => s.rows);
const combo = (keys: string[]) => keys.join("+");

describe("the shortcut card", () => {
  it("lists something in every section", () => {
    for (const s of SECTIONS) {
      expect(s.title).toBeTruthy();
      expect(s.rows.length).toBeGreaterThan(0);
    }
  });

  it("names an action and at least one key on every row", () => {
    for (const r of allRows) {
      expect(r.action.trim()).toBeTruthy();
      expect(r.keys.length).toBeGreaterThan(0);
      for (const k of r.keys) expect(k.trim()).toBeTruthy();
    }
  });

  /**
   * The load-bearing one. Two rows carrying the same combination read as a
   * clash whether or not the handlers actually collide — which is exactly
   * what happened when the element finder was added beside the projects
   * search, both on the same key, guarded on pages that can never both be
   * active. One shortcut doing the obvious thing in each place is one row.
   */
  it("never shows one combination on two rows", () => {
    const seen = new Map<string, string>();
    for (const r of allRows) {
      const key = combo(r.keys);
      const first = seen.get(key);
      expect(
        first,
        `${key} is listed for both "${first}" and "${r.action}"`,
      ).toBeUndefined();
      seen.set(key, r.action);
    }
  });

  /** Two rows describing the same action are a copy someone forgot to
   *  delete, whichever keys they carry. */
  it("never describes one action twice", () => {
    const actions = allRows.map((r) => r.action);
    expect(new Set(actions).size).toBe(actions.length);
  });

  /**
   * The modifier is a parameter so this runs the same on either platform.
   * A row that hard-coded a symbol would pass here and lie on Windows.
   */
  it("takes its modifier from the platform, not from a literal", () => {
    const windows = shortcutSections("Ctrl", "Shift").flatMap((s) => s.rows);
    const mac = allRows;
    expect(windows.length).toBe(mac.length);
    const macKeys = mac.flatMap((r) => r.keys);
    const winKeys = windows.flatMap((r) => r.keys);
    expect(macKeys.filter((k) => k === "⌘").length).toBeGreaterThan(0);
    expect(winKeys.filter((k) => k === "⌘").length).toBe(0);
  });

  /** The conventions people try first, and the reason each was added. */
  it("lists the shortcuts a reader arrives expecting", () => {
    const combos = new Set(allRows.map((r) => combo(r.keys)));
    expect(combos).toContain("⌘+K");
    expect(combos).toContain("⌘+,");
    expect(combos).toContain("⌘+F");
    expect(combos).toContain("?");
  });
});
