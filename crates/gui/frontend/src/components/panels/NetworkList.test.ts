import { describe, expect, it } from "vitest";
import { clickSelects, doubleClickSelects, rankRow } from "./NetworkList";

type Row = Parameters<typeof rankRow>[0];

function row(id: string, kind = "junction", context = ""): Row {
  return {
    id,
    kind,
    cls: "point",
    context,
    value: null,
    format: null,
    canZoom: true,
  };
}

describe("rankRow", () => {
  it("ranks an exact id above every prefix of it", () => {
    expect(rankRow(row("J-4"), "j-4")).toBeLessThan(
      rankRow(row("J-40"), "j-4"),
    );
    expect(rankRow(row("J-40"), "j-4")).toBeLessThan(
      rankRow(row("X-J-4"), "j-4"),
    );
  });

  it("matches case-insensitively", () => {
    expect(rankRow(row("J-401"), "j-401")).toBe(0);
  });

  it("matches the kind, below any id match", () => {
    const byKind = rankRow(row("N1", "outfall"), "outfall");
    expect(byKind).toBeGreaterThan(0);
    expect(byKind).toBeGreaterThan(rankRow(row("outfall-x"), "outfall"));
  });

  it("matches what an element connects to, ranked last", () => {
    const byContext = rankRow(row("C-12", "conduit", "J-401 → J-402"), "j-401");
    expect(byContext).toBeGreaterThan(0);
    expect(byContext).toBeGreaterThan(rankRow(row("J-401x"), "j-401"));
  });

  it("reports no match as negative", () => {
    expect(rankRow(row("J-1", "junction", "A → B"), "zzz")).toBeLessThan(0);
  });

  it("does not let an empty context match everything", () => {
    // "" .includes("") is true, so an unguarded context test would rank
    // every node as a match for any query that reached it.
    expect(rankRow(row("J-1"), "zzz")).toBeLessThan(0);
  });
});

// ── Click and double-click on a row ─────────────────────────────────────────

describe("row click gestures", () => {
  it("acts on the first click of a burst and no other", () => {
    expect(clickSelects(1)).toBe(true);
    expect(clickSelects(2)).toBe(false);
    expect(clickSelects(3)).toBe(false);
    // Some environments report 0 for a synthesised click; treat it as first.
    expect(clickSelects(0)).toBe(true);
  });

  /**
   * The guarantee, composed from the two decisions the handlers actually
   * use rather than restated.
   *
   * A double-click must leave the row selected *and* zoomed to, from
   * either starting state. It used to leave it deselected: selection
   * toggles, both clicks were acted on, and the second undid the first —
   * so the list zoomed to a row it had just cleared.
   *
   * The burst is one toggling click (the second is ignored), then the
   * double-click selecting whatever that left unselected.
   */
  it("ends selected after a double-click, wherever it started", () => {
    for (const startedSelected of [true, false]) {
      const afterClick = clickSelects(1) ? !startedSelected : startedSelected;
      const final = doubleClickSelects(afterClick) ? true : afterClick;
      expect(final, `started ${startedSelected ? "" : "un"}selected`).toBe(
        true,
      );
    }
  });

  /** A single click still toggles — the double-click handling must not
   *  have turned selection into a one-way door. */
  it("leaves a single click toggling", () => {
    for (const startedSelected of [true, false]) {
      const after = clickSelects(1) ? !startedSelected : startedSelected;
      expect(after).toBe(!startedSelected);
    }
  });
});
