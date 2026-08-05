import { describe, expect, it } from "vitest";
import { rankRow } from "./NetworkInspectorHome";

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
