import { describe, expect, it } from "vitest";
import { type Issue, mergeIssues } from "./issues";

const issue = (id: string, firstSeen: string): Issue => ({
  id,
  severity: "warn",
  source: "runtime",
  title: id,
  detail: "",
  firstSeen,
});

describe("mergeIssues", () => {
  // The list is re-derived from scratch whenever anything it watches
  // changes, and each derivation stamps `firstSeen` as now. Without the
  // merge, an issue present for an hour reports itself as new on every
  // refresh — which hides a persistent fault among transient ones.
  it("keeps a surviving issue's original first-seen time", () => {
    const merged = mergeIssues([issue("a", "09:00")], [issue("a", "11:30")]);
    expect(merged[0].firstSeen).toBe("09:00");
  });

  it("stamps a genuinely new issue with its own time", () => {
    const merged = mergeIssues([issue("a", "09:00")], [issue("b", "11:30")]);
    expect(merged).toHaveLength(1);
    expect(merged[0].firstSeen).toBe("11:30");
  });

  // An issue that no longer derives has been resolved and must leave,
  // however long it was present.
  it("drops issues that no longer derive", () => {
    const merged = mergeIssues(
      [issue("a", "09:00"), issue("b", "09:00")],
      [issue("b", "11:30")],
    );
    expect(merged.map((i) => i.id)).toEqual(["b"]);
  });

  it("takes order and content from the fresh list", () => {
    const merged = mergeIssues(
      [issue("a", "09:00")],
      [
        { ...issue("b", "11:30"), title: "second" },
        { ...issue("a", "11:30"), title: "renamed" },
      ],
    );
    expect(merged.map((i) => i.id)).toEqual(["b", "a"]);
    // Only the timestamp is inherited; everything else is the new value.
    expect(merged[1].title).toBe("renamed");
  });

  it("handles an empty screen and an empty derivation", () => {
    expect(mergeIssues([], [issue("a", "11:30")])[0].firstSeen).toBe("11:30");
    expect(mergeIssues([issue("a", "09:00")], [])).toEqual([]);
  });
});
