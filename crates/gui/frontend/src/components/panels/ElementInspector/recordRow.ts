// ── Offering a new record ────────────────────────────────────────────────────
//
// Two decisions the add button used to make inline, both of them wrong
// for a set that is not an open-ended list.
//
// A record set is written whole (hydra-common §4.5.2.3), so adding a row
// is sending the set with one more. What that row *holds* was
// `kind === "number" ? 0 : ""` — which is fine for a demand category,
// whose cells are a number and two free names, and never once worked for
// a snow pack, whose first cell chooses which of three surfaces the row
// is about. An empty name is not one of the three, so every add was
// refused, and a pack missing its pervious surface could not be given
// one from the interface at all though the engine took it happily.
//
// And whether to offer the row at all: a control measure holds one
// surface layer or none, so the button under a layer it already had
// could only ever refuse. The engine publishes the bound now, so the
// question is answerable rather than guessable.

import type { RecordColumn, RecordSet } from "../../../hooks";

/**
 * Whether a set has room for another record.
 *
 * A set with no published capacity is open-ended — a junction may have
 * as many demand categories as a modeller cares to separate — so the
 * answer there is whether it can be written at all.
 */
export function canAddRecord(set: RecordSet): boolean {
  if (!set.editable) return false;
  return set.capacity == null || set.rows.length < set.capacity;
}

/**
 * The row an add sends: one cell per column, each the emptiest value
 * that column can actually hold.
 *
 * A choice gets a value the column does not already carry. That rule
 * costs nothing where the choice is an ordinary one — any item is as
 * good as another, and an unused one is still an item — and is the whole
 * answer where the choice is what the row is keyed by, as a snow
 * surface's is. It cannot tell the two apart and does not need to.
 */
export function blankRecord(set: RecordSet): Array<number | string> {
  return set.columns.map((column, i) =>
    blankCell(
      column,
      set.rows.map((row) => row[i]),
    ),
  );
}

function blankCell(
  column: RecordColumn,
  taken: Array<number | string | null>,
): number | string {
  switch (column.kind?.type) {
    case "number":
    case "integer":
      return column.kind.default ?? 0;
    case "boolean":
      // The engine's own word for it, because that is what it serves and
      // what the cell's select offers back.
      return column.kind.default ? "Yes" : "No";
    case "choice": {
      const { default: fallback, items } = column.kind;
      const candidates = [
        ...(fallback == null ? [] : [fallback]),
        ...items.map((i) => i.value),
      ];
      const free = candidates.find((c) => !taken.includes(c));
      // Every value spoken for: send the first anyway and let the engine
      // say what is wrong with it. Guessing further would be inventing a
      // rule the set never published.
      return free ?? candidates[0] ?? "";
    }
    default:
      // Text, and the list shapes a cell cannot edit either way.
      return "";
  }
}
