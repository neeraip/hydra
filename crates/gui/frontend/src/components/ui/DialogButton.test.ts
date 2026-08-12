import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * The buttons a dialog ends with come from `DialogButton`.
 *
 * This is a source scan rather than a render assertion because the
 * defect is not something a rendered dialog gets wrong — it is a dialog
 * drawing its own button instead of using the shared one, which looks
 * fine in isolation and only reads as wrong beside its neighbours. Six
 * modals had six versions: 6px padding against 7px, radius 5 against 6,
 * a confirm that was solid accent here and an outlined `tool-btn`
 * there. Nobody chose any of it.
 *
 * The same shape of rule as the element badges: one renderer, never a
 * hand-rolled copy. The list below is the backlog of dialogs written
 * before the shared one existed — it may shrink and must never grow.
 */

const MODALS = join(__dirname, "..", "modals");

/**
 * Dialogs that still draw their own action buttons.
 *
 * Empty, and meant to stay that way. It is kept rather than deleted
 * because the alternative to a named exemption is an untested one: a
 * dialog that genuinely must differ should appear here with a comment
 * saying why, not quietly stop being checked.
 */
const NOT_YET_CONVERTED = new Set<string>([]);

/**
 * The words a dialog's action button says.
 *
 * Matched on the label rather than on the element, because a modal is
 * full of buttons that are not this: an icon to close it, a tab, a
 * toggle. Those draw themselves and should. What must not is the row at
 * the bottom that commits or abandons what the dialog was opened for.
 */
const ACTION_WORDS =
  /\b(Cancel|Save|Delete|Remove|Rename|Add|Create|Confirm|Apply|Discard)\b/;

/** A labelled action button with its own inline background — the tell
 * for one drawn by hand instead of taken from `DialogButton`. */
function drawsItsOwnButton(source: string): boolean {
  const buttons = source.match(/<button\b[\s\S]*?<\/button>/g) ?? [];
  return buttons.some(
    (b) => b.includes("background:") && ACTION_WORDS.test(stripMarkup(b)),
  );
}

/** The button's visible words, with attributes and nested elements
 * removed — so a `background:` in a style does not read as a label. */
function stripMarkup(button: string): string {
  return button.replace(/<[^>]*>/g, " ");
}

describe("dialog action buttons", () => {
  const files = readdirSync(MODALS).filter(
    (f) => f.endsWith(".tsx") && !f.includes(".test."),
  );

  it("finds the modals to check", () => {
    // A scan that silently matched nothing would pass forever.
    expect(files.length).toBeGreaterThan(10);
  });

  it("are the shared ones, in every dialog that has been converted", () => {
    const offenders = files.filter(
      (f) =>
        !NOT_YET_CONVERTED.has(f) &&
        drawsItsOwnButton(readFileSync(join(MODALS, f), "utf8")),
    );
    expect(offenders).toEqual([]);
  });

  it("has no stale entries in the backlog", () => {
    // A converted dialog left on the list would let the next one drift
    // back unnoticed.
    const stale = [...NOT_YET_CONVERTED].filter(
      (f) =>
        files.includes(f) &&
        !drawsItsOwnButton(readFileSync(join(MODALS, f), "utf8")),
    );
    expect(stale).toEqual([]);
  });
});
