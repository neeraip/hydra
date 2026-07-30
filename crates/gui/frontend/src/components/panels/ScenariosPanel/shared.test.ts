import { describe, expect, it } from "vitest";
import type { ScenarioDto } from "../../../hooks";
import {
  activeLineage,
  buildScenarioTree,
  descendants,
  directChildren,
  flattenScenarios,
  flattenSubtrees,
  lineageLabel,
  scenarioChildren,
  variantTail,
} from "./shared";

function dto(
  id: string,
  parent: string | null,
  state = "not-run",
): ScenarioDto {
  return {
    id,
    projectId: "p1",
    parentScenarioId: parent,
    name: `Scenario ${id}`,
    state,
  };
}

// Multi-branch fixture:
//   a ─┬─ b ─── d
//      └─ c
//   e (second root)
const FOREST = [
  dto("a", null),
  dto("b", "a"),
  dto("c", "a"),
  dto("d", "b"),
  dto("e", null),
];

describe("buildScenarioTree", () => {
  it("builds a multi-branch forest preserving list order", () => {
    const roots = buildScenarioTree(FOREST);
    expect(roots.map((r) => r.id)).toEqual(["a", "e"]);
    const a = roots[0];
    expect(a.children.map((c) => c.id)).toEqual(["b", "c"]);
    expect(a.children[0].children.map((c) => c.id)).toEqual(["d"]);
    expect(roots[1].children).toEqual([]);
  });

  it("treats orphaned parent ids as roots", () => {
    const roots = buildScenarioTree([dto("a", null), dto("x", "gone")]);
    expect(roots.map((r) => r.id)).toEqual(["a", "x"]);
  });

  it("breaks parent cycles by promoting the first member to a root", () => {
    // a↔b cycle plus a normal root r.
    const roots = buildScenarioTree([
      dto("r", null),
      dto("a", "b"),
      dto("b", "a"),
    ]);
    expect(roots.map((x) => x.id)).toEqual(["r", "a"]);
    const a = roots[1];
    // b stays attached under a; the back-edge a→b's parent is severed.
    expect(a.children.map((c) => c.id)).toEqual(["b"]);
    expect(a.children[0].children).toEqual([]);
  });

  it("treats self-parented rows as roots", () => {
    const roots = buildScenarioTree([dto("s", "s")]);
    expect(roots.map((r) => r.id)).toEqual(["s"]);
    expect(roots[0].children).toEqual([]);
  });
});

describe("flattenScenarios", () => {
  it("flattens DFS with parent→child adjacency and depths", () => {
    const flat = flattenScenarios(FOREST);
    expect(flat.map((f) => [f.id, f.depth])).toEqual([
      ["a", 0],
      ["b", 1],
      ["d", 2],
      ["c", 1],
      ["e", 0],
    ]);
  });

  it("surfaces every row exactly once even with cycles", () => {
    const flat = flattenScenarios([dto("a", "b"), dto("b", "a")]);
    expect(flat.map((f) => f.id).sort()).toEqual(["a", "b"]);
  });
});

describe("activeLineage", () => {
  it("returns [] for Base (null) and for unknown/stale ids", () => {
    expect(activeLineage(FOREST, null)).toEqual([]);
    expect(activeLineage(FOREST, "nope")).toEqual([]);
  });

  it("returns just the root for a root scenario", () => {
    expect(activeLineage(FOREST, "a").map((s) => s.id)).toEqual(["a"]);
  });

  it("returns the root-first path for a deep leaf", () => {
    expect(activeLineage(FOREST, "d").map((s) => s.id)).toEqual([
      "a",
      "b",
      "d",
    ]);
  });

  it("stops at a missing parent (orphan treated as root)", () => {
    expect(
      activeLineage([dto("x", "gone"), dto("y", "x")], "y").map((s) => s.id),
    ).toEqual(["x", "y"]);
  });

  it("breaks parent cycles instead of looping", () => {
    const cyc = [dto("a", "b"), dto("b", "a"), dto("c", "b")];
    expect(activeLineage(cyc, "c").map((s) => s.id)).toEqual(["a", "b", "c"]);
  });
});

describe("scenarioChildren", () => {
  it("lists roots for null and direct children for an id", () => {
    expect(scenarioChildren(FOREST, null).map((s) => s.id)).toEqual(["a", "e"]);
    expect(scenarioChildren(FOREST, "a").map((s) => s.id)).toEqual(["b", "c"]);
    expect(scenarioChildren(FOREST, "d")).toEqual([]);
  });

  it("never lists a self-parented row as its own child", () => {
    expect(scenarioChildren([dto("s", "s")], "s")).toEqual([]);
  });
});

describe("flattenSubtrees", () => {
  it("flattens each requested subtree at depth 0, in id order", () => {
    // Siblings of b at branch point a: [c]; children picker on a: [b, c].
    expect(
      flattenSubtrees(FOREST, ["b", "c"]).map((f) => [f.id, f.depth]),
    ).toEqual([
      ["b", 0],
      ["d", 1],
      ["c", 0],
    ]);
    expect(flattenSubtrees(FOREST, ["c", "b"]).map((f) => f.id)).toEqual([
      "c",
      "b",
      "d",
    ]);
  });

  it("skips unknown ids and returns [] for none", () => {
    expect(flattenSubtrees(FOREST, ["nope"])).toEqual([]);
    expect(flattenSubtrees(FOREST, [])).toEqual([]);
  });
});

describe("lineageLabel", () => {
  it("joins the root-first lineage names with \u200a\u25b8 separators", () => {
    expect(lineageLabel(FOREST, "d")).toBe(
      "Scenario a \u25b8 Scenario b \u25b8 Scenario d",
    );
    expect(lineageLabel(FOREST, "a")).toBe("Scenario a");
  });

  it("returns an empty string for unknown ids", () => {
    expect(lineageLabel(FOREST, "nope")).toBe("");
  });
});

// ── directChildren ───────────────────────────────────────────────────────────

describe("directChildren", () => {
  const dto = (id: string, parentScenarioId: string | null): ScenarioDto => ({
    id,
    projectId: "p1",
    parentScenarioId,
    name: id,
    state: "not-run",
  });

  // base → a → b → c, plus a second child of a.
  const forest = [dto("a", null), dto("b", "a"), dto("c", "b"), dto("d", "a")];

  it("returns only the immediate children", () => {
    // `c` is a grandchild: deleting `a` leaves it parented to `b`, which
    // still exists, so its lineage is untouched.
    expect(directChildren(forest, "a").map((s) => s.id)).toEqual(["b", "d"]);
  });

  it("returns nothing for a leaf", () => {
    expect(directChildren(forest, "c")).toEqual([]);
  });

  it("returns nothing for an unknown id", () => {
    expect(directChildren(forest, "nope")).toEqual([]);
  });

  it("never counts a root's null parent as a match", () => {
    // A root carries parentScenarioId === null; nothing should treat that as
    // being a child of some scenario.
    expect(directChildren(forest, "")).toEqual([]);
  });
});

// ── descendants ──────────────────────────────────────────────────────────────

describe("descendants", () => {
  const dto = (id: string, parentScenarioId: string | null): ScenarioDto => ({
    id,
    projectId: "p1",
    parentScenarioId,
    name: id,
    state: "not-run",
  });

  it("reaches every depth, not just the immediate children", () => {
    // a → b → c, and a → d. Deleting `a` leaves all three standing; only
    // b and d move.
    const forest = [
      dto("a", null),
      dto("b", "a"),
      dto("c", "b"),
      dto("d", "a"),
    ];
    expect(
      descendants(forest, "a")
        .map((s) => s.id)
        .sort(),
    ).toEqual(["b", "c", "d"]);
    expect(directChildren(forest, "a").map((s) => s.id)).toEqual(["b", "d"]);
  });

  it("returns nothing for a leaf or an unknown id", () => {
    const forest = [dto("a", null), dto("b", "a")];
    expect(descendants(forest, "b")).toEqual([]);
    expect(descendants(forest, "nope")).toEqual([]);
  });

  it("terminates on a parent cycle", () => {
    // The raw rows can carry cycles; buildScenarioTree defends against them
    // and so must this, or the confirmation would hang the panel.
    const cyclic = [dto("a", "b"), dto("b", "a"), dto("c", "b")];
    const found = descendants(cyclic, "a")
      .map((s) => s.id)
      .sort();
    expect(found).toEqual(["b", "c"]);
  });

  it("never revisits a scenario reachable by two paths", () => {
    const forest = [dto("a", null), dto("b", "a"), dto("c", "b")];
    expect(descendants(forest, "a")).toHaveLength(2);
  });
});

describe("variantTail", () => {
  it("shows nothing below a variant with no children", () => {
    const forest = [dto("a", null)];
    expect(variantTail(forest, "a")).toEqual({ kind: "none" });
  });

  it("inlines a lone leaf child", () => {
    const forest = [dto("a", null), dto("a1", "a")];
    expect(variantTail(forest, "a")).toEqual({
      kind: "child",
      child: dto("a1", "a"),
    });
  });

  it("hides a lone child that has children of its own", () => {
    // Inlining it would imply the branch ended at that chip.
    const forest = [dto("a", null), dto("a1", "a"), dto("a1a", "a1")];
    expect(variantTail(forest, "a")).toEqual({ kind: "dropdown", count: 1 });
  });

  it("counts only direct children in the picker, not the whole subtree", () => {
    const forest = [
      dto("a", null),
      dto("a1", "a"),
      dto("a2", "a"),
      dto("a1a", "a1"),
      dto("a1b", "a1"),
    ];
    expect(variantTail(forest, "a")).toEqual({ kind: "dropdown", count: 2 });
  });

  it("ignores scenarios in a sibling variant's subtree", () => {
    const forest = [dto("a", null), dto("b", null), dto("b1", "b")];
    expect(variantTail(forest, "a")).toEqual({ kind: "none" });
  });
});
