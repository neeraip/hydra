// ── What a history menu lists ────────────────────────────────────────────────
//
// The undo and redo stacks are stored oldest-first, because that is what a
// stack is: `takeUndo` reads the *last* entry. A menu reads the other way
// round — the thing that happens next is the thing at the top — so the
// order has to be turned over exactly once, somewhere. Here, with a test,
// rather than in a `.reverse()` inside JSX where turning it over twice
// looks the same as turning it over once.

import type { UndoEntry } from "../../hooks/undoStack";

/**
 * How many entries a menu shows before it stops listing and starts
 * counting.
 *
 * The stack holds up to `MAX_UNDO_ENTRIES` (50), and a fifty-item menu is
 * a wall rather than a list. Ten is about what fits without scrolling and
 * covers the span anyone is actually looking back over.
 */
export const HISTORY_LIMIT = 10;

/** One row of a history menu. */
export interface HistoryItem {
  label: string;
  /**
   * The element the entry is about, where the capture knew it.
   *
   * Kept apart from `label` so the row can show the kind's glyph beside
   * the name. "Changed invert on 9" names half an element: an id is
   * unique only within its class, so a junction `9` and a conduit `9`
   * are two different things sharing a name — and deciding whether to
   * undo something means knowing which one it happened to.
   */
  subject?: { kind: string; id: string };
}

/** The entries a menu lists, and how many it did not. */
export interface HistoryMenu {
  /** Rows, newest first — the next to be applied is at index 0. */
  items: HistoryItem[];
  /** How many entries exist beyond `items`. Zero when all of them fit. */
  more: number;
}

/**
 * The menu for one stack.
 *
 * `more` is counted rather than dropped in silence. A list that stopped at
 * ten and said nothing would read as the whole history, which is the same
 * defect as a truncated set of ids looking authoritative: the reader
 * cannot tell a complete answer from a clipped one.
 */
export function historyMenu(
  entries: readonly UndoEntry[],
  limit: number = HISTORY_LIMIT,
): HistoryMenu {
  const newestFirst = [...entries].reverse();
  return {
    items: newestFirst
      .slice(0, limit)
      .map((e) => ({ label: e.label, subject: e.subject })),
    more: Math.max(0, newestFirst.length - limit),
  };
}

/**
 * The entry pressing the button would apply.
 *
 * `null` for an empty stack, which is the case where the button is
 * disabled and has nothing to promise. A tooltip that said "Undo" over a
 * dead button would be describing the control rather than what it does.
 */
export function nextEntry(entries: readonly UndoEntry[]): UndoEntry | null {
  return entries[entries.length - 1] ?? null;
}

/**
 * What the button's tooltip says.
 *
 * The kind is named **in words here**, which is the one place this
 * interface does that. The rule is that a kind travels as its glyph,
 * because a glyph carries the kind's colour and is the same mark every
 * other surface uses — and a tooltip is a plain-text attribute that can
 * hold no glyph at all. The rule exists so a reader can tell two
 * elements apart, and in a medium with no glyphs the word is the only
 * thing that does that job. So the exception is the medium's, not a
 * preference: "Changed invert on 9" cannot say whether it was the
 * junction or the conduit, and "(Junction)" can.
 *
 * The engine's own word for the kind, not one derived from the id: a
 * catalog id capitalised gives "Lidcontrol" where the engine says "LID
 * control".
 */
export function historyTooltip(
  action: string,
  entry: UndoEntry | null,
  kindLabel: (kind: string) => string | undefined,
): string {
  if (!entry) return `Nothing to ${action.toLowerCase()}`;
  const kind = entry.subject && kindLabel(entry.subject.kind);
  return kind
    ? `${action}: ${entry.label} (${kind})`
    : `${action}: ${entry.label}`;
}
