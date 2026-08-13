/**
 * What a history menu lists.
 *
 * The stacks are stored oldest-first and read newest-first, so the order
 * is turned over exactly once. Asserted because turning it over twice
 * looks identical at a glance and puts the oldest edit under the button
 * that would apply the newest.
 */
import { describe, expect, it } from "vitest";
import type { UndoEntry } from "../../hooks/undoStack";
import {
  HISTORY_LIMIT,
  historyMenu,
  historyTooltip,
  nextEntry,
} from "./history";

function entry(
  label: string,
  subject?: { kind: string; id: string },
): UndoEntry {
  return { label, subject, undo: {}, redo: {} };
}

const THREE = [
  entry("Changed diameter on P1"),
  entry("Moved J1"),
  entry("Deleted P3"),
];

describe("historyMenu", () => {
  it("lists the newest first", () => {
    // `takeUndo` reads the last entry, so the last entry is what the
    // button applies — and what the menu has to show at the top.
    expect(historyMenu(THREE).items.map((i) => i.label)).toEqual([
      "Deleted P3",
      "Moved J1",
      "Changed diameter on P1",
    ]);
  });

  it("counts what it did not list rather than dropping it quietly", () => {
    // A list that stopped at the limit and said nothing would read as the
    // whole history. The reader cannot tell a complete answer from a
    // clipped one, which is the same defect as a truncated list of ids
    // that still looks authoritative.
    const many = Array.from({ length: HISTORY_LIMIT + 4 }, (_, i) =>
      entry(`Edit ${i}`),
    );
    const menu = historyMenu(many);
    expect(menu.items).toHaveLength(HISTORY_LIMIT);
    expect(menu.more).toBe(4);
    expect(menu.items[0].label).toBe(`Edit ${HISTORY_LIMIT + 3}`);
  });

  it("says nothing is missing when everything fits", () => {
    expect(historyMenu(THREE).more).toBe(0);
    expect(historyMenu([])).toEqual({ items: [], more: 0 });
  });

  it("carries the element each entry is about", () => {
    // Kept apart from the label so the row can show the kind's glyph.
    // "Changed invert on 9" names half an element: an id is unique only
    // within its class, so a junction 9 and a conduit 9 are two things
    // that share a name.
    const menu = historyMenu([
      entry("Changed invert on 9", { kind: "junction", id: "9" }),
    ]);
    expect(menu.items[0].subject).toEqual({ kind: "junction", id: "9" });
  });

  it("leaves the subject out where the capture did not know it", () => {
    expect(historyMenu([entry("Moved J1")]).items[0].subject).toBeUndefined();
  });

  it("does not disturb the stack it was given", () => {
    // The reversal is on a copy: the stack is the store's, and reversing
    // it in place would leave the next `takeUndo` reading the oldest
    // entry.
    const stack = [...THREE];
    historyMenu(stack);
    expect(stack.map((e) => e.label)).toEqual(THREE.map((e) => e.label));
  });
});

describe("nextEntry", () => {
  it("is the entry the button would apply", () => {
    expect(nextEntry(THREE)?.label).toBe("Deleted P3");
  });

  it("has nothing to promise for an empty stack", () => {
    expect(nextEntry([])).toBeNull();
  });
});

describe("historyTooltip", () => {
  const label = (id: string) =>
    ({ junction: "Junction", lidcontrol: "LID control" })[id];

  it("names the kind in words, which nothing else here does", () => {
    // A tooltip is a plain-text attribute and can hold no glyph, so the
    // rule that a kind travels as its badge has nothing to offer here.
    // The rule exists so a reader can tell two elements apart, and
    // "Changed invert on 9" cannot say whether it was the junction or
    // the conduit.
    expect(
      historyTooltip(
        "Undo",
        entry("Changed invert on 9", { kind: "junction", id: "9" }),
        label,
      ),
    ).toBe("Undo: Changed invert on 9 (Junction)");
  });

  it("uses the engine's word rather than one derived from the id", () => {
    // Capitalising the catalog id gives "Lidcontrol"; the engine says
    // "LID control", and the engine is the one that names its kinds.
    expect(
      historyTooltip(
        "Undo",
        entry("Edited GR1", { kind: "lidcontrol", id: "GR1" }),
        label,
      ),
    ).toBe("Undo: Edited GR1 (LID control)");
  });

  it("says only the edit where no kind was captured", () => {
    expect(historyTooltip("Redo", entry("Moved J1"), label)).toBe(
      "Redo: Moved J1",
    );
  });

  it("says nothing is there for an empty stack", () => {
    // Not "Undo" over a dead button: that describes the control rather
    // than what pressing it would do, which is nothing.
    expect(historyTooltip("Undo", null, label)).toBe("Nothing to undo");
    expect(historyTooltip("Redo", null, label)).toBe("Nothing to redo");
  });

  it("falls back to the edit alone for a kind the catalog does not know", () => {
    // A stale entry naming a kind this engine has no word for: the edit
    // is still worth reporting, and an empty pair of brackets is not.
    expect(
      historyTooltip(
        "Undo",
        entry("Edited X", { kind: "sluice", id: "X" }),
        label,
      ),
    ).toBe("Undo: Edited X");
  });
});
