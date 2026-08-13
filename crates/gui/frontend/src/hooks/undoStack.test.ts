/**
 * Pure-logic tests for the undo/redo stack store (push/cap/redo-clear/key
 * isolation) and the inverse-set construction used at capture time
 * (`inverseFieldPatch`, `recreateSpecsForDelete`, `inverseOp`).
 */
import { afterEach, describe, expect, it } from "vitest";
import type { Link, Node } from "../types";
import {
  clearAllStacks,
  clearRedo,
  getUndoStacks,
  inverseFieldPatch,
  inverseOp,
  MAX_UNDO_ENTRIES,
  pushRedoEntry,
  pushUndoEntry,
  recreateSpecForLink,
  recreateSpecForNode,
  recreateSpecsForDelete,
  restoreUndoEntry,
  stackKey,
  takeRedo,
  takeUndo,
  type UndoEntry,
} from "./undoStack";

function entry(label: string): UndoEntry {
  return { label, undo: {}, redo: {} };
}

const KEY = stackKey("p1", null);

afterEach(() => {
  clearAllStacks();
});

// ── Fixtures ───────────────────────────────────────────────────────────────

const junction: Node = {
  id: "J1",
  type: "junction",
  x: 10,
  y: 20,
  elevation: 55,
  baseDemand: 2.5,
  pressure: null,
  demand: null,
};

const tank: Node = {
  id: "T1",
  type: "tank",
  x: 1,
  y: 2,
  elevation: 100,
  baseDemand: 0,
  pressure: null,
  demand: null,
  tankMinLevel: 0.5,
  tankMaxLevel: 6,
  tankInitialLevel: 3,
  tankDiameter: 20,
  tankVolumeCurve: "VC1",
};

const reservoir: Node = {
  id: "R1",
  type: "reservoir",
  x: -3,
  y: 4,
  elevation: 120,
  baseDemand: 0,
  pressure: null,
  demand: null,
  headPattern: "PAT7",
};

const pipe: Link = {
  id: "P1",
  type: "pipe",
  fromId: "J1",
  toId: "T1",
  velocity: 0,
  diameter: 300,
  length: 1200,
  roughness: 110,
  initialStatus: "closed",
};

const pump: Link = {
  id: "PU1",
  type: "pump",
  fromId: "R1",
  toId: "J1",
  velocity: 0,
  diameter: 0,
  pumpCurve: "C1",
  pumpPowerKw: null,
  pumpSpeed: 1,
};

const valve: Link = {
  id: "V1",
  type: "valve",
  fromId: "T1",
  toId: "J1",
  velocity: 0,
  diameter: 200,
  valveType: "PRV",
  valveSetting: 35.5,
  valveCurve: null,
};

const nodes = [junction, tank, reservoir];
const links = [pipe, pump, valve];
const nodesById = new Map(nodes.map((n) => [n.id, n]));
const linksById = new Map(links.map((l) => [l.id, l]));

// ── Stack store ────────────────────────────────────────────────────────────

describe("undo stack store", () => {
  it("pushes entries and pops them LIFO", () => {
    pushUndoEntry(KEY, entry("a"));
    pushUndoEntry(KEY, entry("b"));
    expect(getUndoStacks(KEY).undo.map((e) => e.label)).toEqual(["a", "b"]);
    expect(takeUndo(KEY)?.label).toBe("b");
    expect(takeUndo(KEY)?.label).toBe("a");
    expect(takeUndo(KEY)).toBeNull();
  });

  it("caps the undo stack at MAX_UNDO_ENTRIES, dropping the oldest", () => {
    for (let i = 0; i < MAX_UNDO_ENTRIES + 5; i += 1) {
      pushUndoEntry(KEY, entry(`e${i}`));
    }
    const { undo } = getUndoStacks(KEY);
    expect(undo).toHaveLength(MAX_UNDO_ENTRIES);
    expect(undo[0].label).toBe("e5"); // e0..e4 dropped
    expect(undo[undo.length - 1].label).toBe(`e${MAX_UNDO_ENTRIES + 4}`);
  });

  it("clears the redo stack on any new capture", () => {
    pushUndoEntry(KEY, entry("a"));
    const popped = takeUndo(KEY);
    if (!popped) throw new Error("expected entry");
    pushRedoEntry(KEY, popped);
    expect(getUndoStacks(KEY).redo).toHaveLength(1);
    pushUndoEntry(KEY, entry("b"));
    expect(getUndoStacks(KEY).redo).toHaveLength(0);
  });

  it("restoreUndoEntry (redo apply) keeps the remaining redo branch", () => {
    pushUndoEntry(KEY, entry("a"));
    pushUndoEntry(KEY, entry("b"));
    // Undo both.
    for (let i = 0; i < 2; i += 1) {
      const popped = takeUndo(KEY);
      if (!popped) throw new Error("expected entry");
      pushRedoEntry(KEY, popped);
    }
    // Redo one — the other redo entry must survive.
    const redone = takeRedo(KEY);
    expect(redone?.label).toBe("a");
    if (!redone) throw new Error("expected entry");
    restoreUndoEntry(KEY, redone);
    expect(getUndoStacks(KEY).undo.map((e) => e.label)).toEqual(["a"]);
    expect(getUndoStacks(KEY).redo.map((e) => e.label)).toEqual(["b"]);
  });

  it("isolates stacks per (projectId, scenarioId) key", () => {
    const keyB = stackKey("p1", "scenario-2");
    const keyC = stackKey("p2", null);
    pushUndoEntry(KEY, entry("base"));
    pushUndoEntry(keyB, entry("scen"));
    expect(getUndoStacks(KEY).undo.map((e) => e.label)).toEqual(["base"]);
    expect(getUndoStacks(keyB).undo.map((e) => e.label)).toEqual(["scen"]);
    expect(getUndoStacks(keyC).undo).toHaveLength(0);
    expect(takeUndo(keyC)).toBeNull();
  });

  it("clearRedo drops only the redo branch", () => {
    pushUndoEntry(KEY, entry("a"));
    pushUndoEntry(KEY, entry("b"));
    const popped = takeUndo(KEY);
    if (!popped) throw new Error("expected entry");
    pushRedoEntry(KEY, popped);
    clearRedo(KEY);
    expect(getUndoStacks(KEY).redo).toHaveLength(0);
    expect(getUndoStacks(KEY).undo.map((e) => e.label)).toEqual(["a"]);
  });
});

// ── inverseFieldPatch ──────────────────────────────────────────────────────

describe("inverseFieldPatch", () => {
  const inv = (kind: string, id: string, field: string) =>
    inverseFieldPatch(kind, id, field, nodesById, linksById);

  it("restores node scalar fields from the snapshot", () => {
    expect(inv("junction", "J1", "elevation")).toEqual({
      kind: "junction",
      id: "J1",
      field: "elevation",
      value: 55,
    });
    expect(inv("junction", "J1", "baseDemand")?.value).toBe(2.5);
    expect(inv("junction", "J1", "x")?.value).toBe(10);
    expect(inv("tank", "T1", "minLevel")?.value).toBe(0.5);
    expect(inv("reservoir", "R1", "head")?.value).toBe(120);
    expect(inv("reservoir", "R1", "headPattern")?.value).toBe("PAT7");
  });

  it("restores pipe fields including an open/closed initial status", () => {
    expect(inv("pipe", "P1", "length")?.value).toBe(1200);
    expect(inv("pipe", "P1", "diameter")?.value).toBe(300);
    expect(inv("pipe", "P1", "status")?.value).toBe("closed");
  });

  it("inverts a cv pipe status (the patch API accepts open/closed/cv)", () => {
    const cvPipe: Link = { ...pipe, id: "P2", initialStatus: "cv" };
    const withCv = new Map(linksById);
    withCv.set("P2", cvPipe);
    expect(
      inverseFieldPatch("pipe", "P2", "status", nodesById, withCv),
    ).toEqual({ kind: "pipe", id: "P2", field: "status", value: "cv" });
  });

  it("cannot invert an unknown/pre-v3 pipe status (field absent)", () => {
    const bare: Link = { ...pipe, id: "P3" };
    delete bare.initialStatus;
    const withBare = new Map(linksById);
    withBare.set("P3", bare);
    expect(
      inverseFieldPatch("pipe", "P3", "status", nodesById, withBare),
    ).toBeNull();
  });

  it("inverts pump curve/power edits to whichever the pump carries", () => {
    // PU1 carries a curve → editing powerKw inverts back to the curve.
    expect(inv("pump", "PU1", "powerKw")).toEqual({
      kind: "pump",
      id: "PU1",
      field: "curve",
      value: "C1",
    });
    const powerPump: Link = {
      ...pump,
      id: "PU2",
      pumpCurve: null,
      pumpPowerKw: 15,
    };
    const withPower = new Map(linksById);
    withPower.set("PU2", powerPump);
    expect(
      inverseFieldPatch("pump", "PU2", "curve", nodesById, withPower),
    ).toEqual({ kind: "pump", id: "PU2", field: "powerKw", value: 15 });
  });

  it("returns null for unknown elements and fields", () => {
    expect(inv("junction", "NOPE", "elevation")).toBeNull();
    expect(inv("junction", "J1", "frobnicate")).toBeNull();
    expect(inv("valve", "V1", "valveCurve")?.value).toBe("");
  });
});

// ── recreate specs ─────────────────────────────────────────────────────────

describe("recreateSpecsForDelete", () => {
  it("captures a node plus every link that cascade-deletes with it", () => {
    const specs = recreateSpecsForDelete("junction", "J1", nodes, links);
    expect(specs).not.toBeNull();
    if (!specs) throw new Error("unreachable");
    // Node first, then its attached links (P1 and PU1 and V1 touch J1).
    expect(specs.map((s) => s.id)).toEqual(["J1", "P1", "PU1", "V1"]);
    expect(specs[0].elementType).toBe("node");
  });

  it("captures a lone link for link deletes", () => {
    const specs = recreateSpecsForDelete("pipe", "P1", nodes, links);
    expect(specs?.map((s) => s.id)).toEqual(["P1"]);
  });

  it("returns null when the element is not in the snapshot", () => {
    expect(recreateSpecsForDelete("pipe", "NOPE", nodes, links)).toBeNull();
  });

  it("carries create args + follow-up patches for a tank", () => {
    const spec = recreateSpecForNode(tank);
    if (spec.elementType !== "node") throw new Error("unreachable");
    expect(spec).toMatchObject({
      kind: "tank",
      id: "T1",
      x: 1,
      y: 2,
      elevation: 100,
      minLevel: 0.5,
      maxLevel: 6,
      initialLevel: 3,
    });
    expect(spec.patches).toEqual([
      { kind: "tank", id: "T1", field: "diameter", value: 20 },
      { kind: "tank", id: "T1", field: "volumeCurve", value: "VC1" },
    ]);
  });

  it("orders valveType before valveSetting (setting units depend on type)", () => {
    const spec = recreateSpecForLink(valve);
    const fields = spec.patches.map((p) => p.field);
    expect(fields.indexOf("valveType")).toBeLessThan(
      fields.indexOf("valveSetting"),
    );
  });

  it("restores a closed or cv pipe via a status patch; open needs none", () => {
    const spec = recreateSpecForLink(pipe);
    expect(spec.patches).toContainEqual({
      kind: "pipe",
      id: "P1",
      field: "status",
      value: "closed",
    });
    const cvSpec = recreateSpecForLink({ ...pipe, initialStatus: "cv" });
    expect(cvSpec.patches).toContainEqual({
      kind: "pipe",
      id: "P1",
      field: "status",
      value: "cv",
    });
    // create_link already defaults to open — no redundant patch.
    const openSpec = recreateSpecForLink({ ...pipe, initialStatus: "open" });
    expect(openSpec.patches.some((p) => p.field === "status")).toBe(false);
  });
});

describe("inverseOp", () => {
  /**
   * The vocabulary an undo entry is made of now. Every one of these is
   * an ordinary edit applied by the same command that made the change,
   * which is what §4.5.5 means by an undo built above the contract
   * rather than out of one engine's commands — the previous stack
   * replayed water-distribution commands, so an entry captured from a
   * drainage model looked undoable and refused when applied.
   */
  it("puts a move back where it came from", () => {
    expect(
      inverseOp({ op: "move", id: "J1", x: 5, y: 6 }, { x: 1, y: 2 }),
    ).toEqual({ op: "move", id: "J1", x: 1, y: 2 });
  });

  it("puts an attribute back to the value it had", () => {
    expect(
      inverseOp(
        { op: "set", id: "J1", key: "invert", value: 90 },
        { value: 85 },
      ),
    ).toEqual({ op: "set", id: "J1", key: "invert", value: 85 });
  });

  it("declines when nobody can say what was there", () => {
    // An inverse nobody can supply is better absent than guessed: an
    // entry captured without one would sit in the history looking
    // undoable and fail on apply, which is the exact defect this
    // replaced.
    expect(inverseOp({ op: "move", id: "J1", x: 5, y: 6 })).toBeNull();
    expect(
      inverseOp({ op: "set", id: "J1", key: "invert", value: 90 }),
    ).toBeNull();
  });

  it("puts both ends back where they were", () => {
    // Both, not just the one that changed: the pair is the whole of what
    // a reconnection replaced, so restoring one end would leave the line
    // half undone and pointing somewhere nobody chose.
    expect(
      inverseOp(
        { op: "reconnect", id: "C1", fromId: "J2", toId: "O1" },
        { fromId: "J1", toId: "O1" },
      ),
    ).toEqual({ op: "reconnect", id: "C1", fromId: "J1", toId: "O1" });
    expect(
      inverseOp({ op: "reconnect", id: "C1", fromId: "J2", toId: "O1" }),
    ).toBeNull();
  });

  it("puts a whole table back, which is why the write takes one", () => {
    // A sequence of per-row operations could not be reversed without
    // replaying each one, and half of them are illegal on their own —
    // a curve with a point inserted before it is sorted into place.
    expect(
      inverseOp(
        { op: "contents", kind: "curve", id: "C1", rows: [[0, 1]] },
        {
          rows: [
            [0, 50],
            [5, 0],
          ],
        },
      ),
    ).toEqual({
      op: "contents",
      kind: "curve",
      id: "C1",
      rows: [
        [0, 50],
        [5, 0],
      ],
    });
    expect(
      inverseOp({ op: "contents", kind: "curve", id: "C1", rows: [[0, 1]] }),
    ).toBeNull();
  });

  it("puts a whole record set back", () => {
    // Same shape as contents, and for the same reason: the set is
    // replaced whole, so the rows that were there are the whole of what
    // an undo restores. A junction's demand categories cannot be undone
    // one at a time — the total they add to is what the model holds.
    expect(
      inverseOp(
        { op: "records", id: "J1", set: "demands", rows: [[4, "", ""]] },
        { records: [[10, "P1", "Residential"]] },
      ),
    ).toEqual({
      op: "records",
      id: "J1",
      set: "demands",
      rows: [[10, "P1", "Residential"]],
    });
    expect(
      inverseOp({ op: "records", id: "J1", set: "demands", rows: [] }),
    ).toBeNull();
  });

  it("inverts a rename by renaming back", () => {
    expect(
      inverseOp({ op: "rename", kind: "junction", from: "J1", to: "J2" }),
    ).toEqual({ op: "rename", kind: "junction", from: "J2", to: "J1" });
  });

  it("inverts a create by removing what it made", () => {
    expect(
      inverseOp({
        op: "create",
        element: { kind: "junction", id: "J9" },
      }),
    ).toEqual({ op: "remove", kind: "junction", id: "J9" });
  });

  it("has no inverse for a removal", () => {
    // Recreating the element is expressible; the records a drainage
    // removal also takes — an inflow, a treatment — are not. An undo
    // that silently gave back less than it removed would be worse than
    // none, so a removal clears the history instead.
    expect(inverseOp({ op: "remove", kind: "junction", id: "J1" })).toBeNull();
  });
});
