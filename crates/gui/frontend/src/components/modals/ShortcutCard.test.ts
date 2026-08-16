import { describe, expect, it } from "vitest";
import { PROJECT_VIEWS, VIEW_SHORTCUTS } from "../../projectConfig";
import { shortcutSections, viewRows } from "./ShortcutCard";

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

/**
 * The rows the card no longer writes by hand.
 *
 * Both defects these prevent were invisible for the same reason: nobody
 * who knows a shortcut reads the card, so a wrong row survives until it
 * misleads someone who is not in a position to notice.
 */
describe("the view rows", () => {
  it("names each view as the app names it", () => {
    // ⌘4 was captioned "Go to Analysis" after that view had been
    // relabelled "Results" in PROJECT_VIEWS. The card and the activity
    // bar disagreed about what one screen was called.
    const rows = viewRows("⌘");
    for (const r of rows) {
      const label = r.action.replace("Go to ", "");
      expect(PROJECT_VIEWS.map((v) => v.label)).toContain(label);
    }
  });

  it("lists exactly the views a number key reaches", () => {
    const digits = viewRows("⌘").map((r) => r.keys[1]);
    expect(digits).toEqual(Object.keys(VIEW_SHORTCUTS).sort());
  });

  it("claims no shortcut for a view that has none", () => {
    // Nothing is in that state today — every view has a key. The rule
    // still holds the line the other way: a row the handler would ignore
    // is the same drift as a missing one, and the Report view spent its
    // whole life on the wrong side of it because this list was typed by
    // hand and stopped at four.
    const named = viewRows("⌘").map((r) => r.action);
    const unreachable = PROJECT_VIEWS.filter(
      (v) => !Object.values(VIEW_SHORTCUTS).includes(v.id),
    );
    for (const v of unreachable) {
      expect(named).not.toContain(`Go to ${v.label}`);
    }
  });

  it("reaches every view the activity bar draws", () => {
    const named = viewRows("⌘").map((r) => r.action);
    for (const v of PROJECT_VIEWS) {
      expect(named).toContain(`Go to ${v.label}`);
    }
  });

  it("shows no save shortcut", () => {
    // An edit is part of the model when the operation returns, so there
    // is nothing to save. ⌘S is swallowed to keep the browser's dialog
    // away, and the card advertised it as an action for months after
    // the staged editors it belonged to were deleted.
    const combos = allRows.map((r) => combo(r.keys));
    expect(combos).not.toContain("⌘+S");
    expect(allRows.map((r) => r.action).join(" ")).not.toMatch(/\bSave\b/);
  });
});
