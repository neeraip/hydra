// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearAllStacks,
  type EditSet,
  getUndoStacks,
  pushUndoEntry,
  stackKey,
  type UndoEntry,
} from "./undoStack";
import { applyEditSet, applyOp, useUndoRedo } from "./useUndoRedo";

/**
 * Undo is the code in this app most able to damage a model without
 * saying so. It writes to the network and then saves to disk, so a step
 * applied in the wrong order, an op routed to the wrong command, or an
 * entry left on a stack after a failed apply all end as a `.inp` on
 * disk that the user never asked for and cannot see is wrong.
 *
 * Three things are pinned here, each for that reason:
 *
 *   - the apply *order*, because recreate-after-delete and
 *     patch-before-create both succeed silently at the call site,
 *   - the *dispatch*, because an id is only half an address in the water
 *     engine and an op that loses its `kind` edits the wrong element,
 *   - what happens *after a failure*, because the entry is already
 *     popped by then and a stack that keeps it offers the user a second
 *     apply of something that did not work the first time.
 *
 * The hook's own behaviour is driven through a probe component rather
 * than a provider: `AppProvider` registers Tauri listeners that do not
 * exist here and throws before its children render.
 */

const calls: string[] = [];

// `vi.clearAllMocks` forgets calls but keeps implementations, so a
// `mockImplementation` set by one test survives into the next. That is
// how the redo test first failed: it inherited the busy test's promise
// that never resolves, and read the resulting early return as a
// dispatch bug. Every default is re-armed below instead.
const defaults: Array<() => void> = [];

function stub(impl: (...args: unknown[]) => unknown) {
  const mock = vi.fn(impl);
  defaults.push(() => mock.mockImplementation(impl));
  return mock;
}

function record(name: string) {
  return stub((...args: unknown[]) => {
    calls.push(`${name}(${args.map((a) => JSON.stringify(a)).join(",")})`);
    return Promise.resolve();
  });
}

const createNode = record("createNode");
const createLink = record("createLink");
const patchNodePosition = record("patchNodePosition");
const setElementAttribute = record("setElementAttribute");
const renameElement = record("renameElement");
const setElementEnds = record("setElementEnds");
const setCollectionContents = record("setCollectionContents");
const setElementRecords = record("setElementRecords");
const createElement = record("createElement");
const patchElements = stub((...a: unknown[]) => {
  calls.push(`patchElements(${JSON.stringify(a[0])})`);
  return Promise.resolve({ errors: [] as string[] });
});
const invoke = stub((...a: unknown[]) => {
  calls.push(`invoke:${a[0]}(${JSON.stringify(a[1])})`);
  return Promise.resolve(undefined);
});
const persistOrSay = record("persistOrSay");
const markEdited = vi.fn();
const showToast = vi.fn();

vi.mock("./network", () => ({
  createNode: (...a: unknown[]) => createNode(...a),
  createLink: (...a: unknown[]) => createLink(...a),
  createElement: (...a: unknown[]) => createElement(...a),
  patchElements: (p: unknown[]) => patchElements(p),
  patchNodePosition: (...a: unknown[]) => patchNodePosition(...a),
  renameElement: (...a: unknown[]) => renameElement(...a),
  setCollectionContents: (...a: unknown[]) => setCollectionContents(...a),
  setElementAttribute: (...a: unknown[]) => setElementAttribute(...a),
  setElementEnds: (...a: unknown[]) => setElementEnds(...a),
  setElementRecords: (...a: unknown[]) => setElementRecords(...a),
}));
vi.mock("./ipc", () => ({ invoke: (c: string, a?: unknown) => invoke(c, a) }));
vi.mock("./projects", () => ({
  persistOrSay: (...a: unknown[]) => persistOrSay(...a),
}));
vi.mock("./NetworkVersionContext", () => ({
  useNetworkVersion: () => ({ markEdited }),
}));
vi.mock("../AppContext", () => ({
  useAppState: () => ({
    activeProjectId: "p1",
    activeScenarioId: "s1",
    showToast,
  }),
}));

beforeEach(() => {
  calls.length = 0;
  vi.clearAllMocks();
  for (const rearm of defaults) rearm();
  clearAllStacks();
});

const KEY = stackKey("p1", "s1");

function entry(undo: EditSet, redo: EditSet = {}): UndoEntry {
  return { label: "Moved 9", undo, redo };
}

function harness(): { undo: () => void; redo: () => void } {
  let api: { undo: () => void; redo: () => void } | null = null;
  function Probe() {
    api = useUndoRedo();
    return null;
  }
  render(<Probe />);
  if (!api) throw new Error("hook never ran");
  return api;
}

describe("applyEditSet order", () => {
  const everything: EditSet = {
    ops: [{ op: "move", id: "J1", x: 1, y: 2 }],
    recreates: [
      {
        elementType: "node",
        kind: "junction",
        id: "J2",
        x: 3,
        y: 4,
        patches: [
          { kind: "junction", id: "J2", field: "baseDemand", value: 5 },
        ],
      },
    ],
    patches: [{ kind: "pipe", id: "P1", field: "diameter", value: 300 }],
    deletes: [{ kind: "pipe", id: "P2" }],
  } as EditSet;

  it("applies ops, then recreates, then patches, then deletes", async () => {
    await applyEditSet(everything, "p1");
    const order = calls.map((c) => c.split("(")[0]);
    expect(order).toEqual([
      "patchNodePosition",
      "createNode",
      "patchElements", // the recreate's own follow-up patches
      "patchElements", // the set's patches
      "invoke:delete_element",
    ]);
  });

  it("deletes last, so an element is never removed before it is patched", async () => {
    await applyEditSet(everything, "p1");
    const del = calls.findIndex((c) => c.startsWith("invoke:delete_element"));
    expect(del).toBe(calls.length - 1);
  });

  it("stops at the first failed step and runs nothing after it", async () => {
    createNode.mockImplementationOnce(() => Promise.reject(new Error("nope")));
    await expect(applyEditSet(everything, "p1")).rejects.toThrow("nope");
    expect(calls.some((c) => c.startsWith("invoke:delete_element"))).toBe(
      false,
    );
    expect(calls.some((c) => c.startsWith("patchElements"))).toBe(false);
  });

  it("throws on a per-item patch error, which patch_elements reports without rejecting", async () => {
    patchElements.mockImplementation(() =>
      Promise.resolve({ errors: ["P1: no such field"] }),
    );
    await expect(
      applyEditSet(
        { patches: [{ kind: "pipe", id: "P1", field: "d", value: 1 }] },
        "p1",
      ),
    ).rejects.toThrow("P1: no such field");
  });

  it("skips the patch call entirely when there is nothing to patch", async () => {
    await applyEditSet({ patches: [] }, "p1");
    expect(patchElements).not.toHaveBeenCalled();
  });

  it("deletes through the throwing invoke, not the wrapper that swallows failure", async () => {
    invoke.mockImplementation(() => Promise.reject(new Error("locked")));
    await expect(
      applyEditSet({ deletes: [{ kind: "pipe", id: "P2" }] }, "p1"),
    ).rejects.toThrow("locked");
  });
});

describe("applyOp dispatch", () => {
  it("routes a move to the position command", async () => {
    await applyOp({ op: "move", id: "J1", x: 1, y: 2 }, "p1");
    expect(patchNodePosition).toHaveBeenCalledWith("J1", 1, 2);
  });

  it("carries kind with a set, because an id alone is half an address", async () => {
    await applyOp(
      { op: "set", id: "9", key: "invert", value: 3, kind: "junction" },
      "p1",
    );
    expect(setElementAttribute).toHaveBeenCalledWith(
      "p1",
      "9",
      "invert",
      3,
      "junction",
    );
  });

  it("carries kind with records for the same reason", async () => {
    await applyOp(
      {
        op: "records",
        id: "9",
        set: "inflows",
        rows: [[1, 2]],
        kind: "junction",
      },
      "p1",
    );
    expect(setElementRecords).toHaveBeenCalledWith(
      "p1",
      "9",
      "inflows",
      [[1, 2]],
      "junction",
    );
  });

  it("routes a rename by kind and both names", async () => {
    await applyOp({ op: "rename", kind: "pipe", from: "P1", to: "P9" }, "p1");
    expect(renameElement).toHaveBeenCalledWith("pipe", "P1", "P9");
  });

  it("routes a reconnect to the ends command", async () => {
    await applyOp(
      { op: "reconnect", id: "P1", fromId: "J1", toId: "J2" },
      "p1",
    );
    expect(setElementEnds).toHaveBeenCalledWith("p1", "P1", "J1", "J2");
  });

  it("routes contents to the collection command", async () => {
    await applyOp(
      { op: "contents", kind: "curve", id: "C1", rows: [[0, 1]] },
      "p1",
    );
    expect(setCollectionContents).toHaveBeenCalledWith("p1", "curve", "C1", [
      [0, 1],
    ]);
  });

  it("removes through the throwing invoke so a failure aborts the apply", async () => {
    invoke.mockImplementation(() => Promise.reject(new Error("in use")));
    await expect(
      applyOp({ op: "remove", kind: "pipe", id: "P1" }, "p1"),
    ).rejects.toThrow("in use");
  });
});

describe("useUndoRedo", () => {
  it("moves a successful undo onto the redo stack and saves the model", async () => {
    pushUndoEntry(KEY, entry({ ops: [{ op: "move", id: "J1", x: 1, y: 2 }] }));
    const { undo } = harness();
    await act(async () => undo());

    expect(patchNodePosition).toHaveBeenCalledWith("J1", 1, 2);
    const { undo: u, redo: r } = getUndoStacks(KEY);
    expect(u).toHaveLength(0);
    expect(r).toHaveLength(1);
    expect(persistOrSay).toHaveBeenCalledWith("p1", "s1", showToast);
    expect(markEdited).toHaveBeenCalledWith("p1", "s1");
    expect(showToast).toHaveBeenCalledWith("Undid: Moved 9", "success");
  });

  it("drops a failed entry from both stacks rather than offering it again", async () => {
    patchNodePosition.mockImplementation(() =>
      Promise.reject(new Error("refused")),
    );
    pushUndoEntry(KEY, entry({ ops: [{ op: "move", id: "J1", x: 1, y: 2 }] }));
    const { undo } = harness();
    await act(async () => undo());

    const { undo: u, redo: r } = getUndoStacks(KEY);
    expect(u).toHaveLength(0);
    expect(r).toHaveLength(0);
    expect(showToast).toHaveBeenCalledWith("Undo failed: refused", "error");
  });

  it("still saves after a partial failure, because earlier steps already mutated", async () => {
    // The move lands, the delete does not: in-memory state has changed
    // and a skipped save would leave the file describing the old model.
    invoke.mockImplementation(() => Promise.reject(new Error("locked")));
    pushUndoEntry(
      KEY,
      entry({
        ops: [{ op: "move", id: "J1", x: 1, y: 2 }],
        deletes: [{ kind: "pipe", id: "P2" }],
      }),
    );
    const { undo } = harness();
    await act(async () => undo());

    expect(patchNodePosition).toHaveBeenCalled();
    expect(persistOrSay).toHaveBeenCalledWith("p1", "s1", showToast);
    expect(markEdited).toHaveBeenCalledWith("p1", "s1");
  });

  it("does nothing at all on an empty stack, including saving", async () => {
    const { undo } = harness();
    await act(async () => undo());

    expect(persistOrSay).not.toHaveBeenCalled();
    expect(markEdited).not.toHaveBeenCalled();
    expect(showToast).not.toHaveBeenCalled();
  });

  it("applies one entry at a time when the shortcut is held down", async () => {
    // Two applies in flight would interleave their steps and save twice.
    let release: (() => void) | null = null;
    patchNodePosition.mockImplementation(
      () =>
        new Promise<void>((res) => {
          release = () => res();
        }),
    );
    pushUndoEntry(KEY, entry({ ops: [{ op: "move", id: "J1", x: 1, y: 2 }] }));
    pushUndoEntry(KEY, entry({ ops: [{ op: "move", id: "J2", x: 3, y: 4 }] }));
    const { undo } = harness();

    await act(async () => {
      undo();
      undo();
      undo();
    });
    expect(patchNodePosition).toHaveBeenCalledTimes(1);

    await act(async () => {
      release?.();
    });
    expect(getUndoStacks(KEY).undo).toHaveLength(1);
  });

  it("redoes from the redo stack and puts the entry back on the undo stack", async () => {
    pushUndoEntry(
      KEY,
      entry(
        { ops: [{ op: "move", id: "J1", x: 1, y: 2 }] },
        { ops: [{ op: "move", id: "J1", x: 9, y: 9 }] },
      ),
    );
    const { undo, redo } = harness();
    await act(async () => undo());
    await act(async () => redo());

    expect(patchNodePosition).toHaveBeenLastCalledWith("J1", 9, 9);
    expect(getUndoStacks(KEY).undo).toHaveLength(1);
    expect(getUndoStacks(KEY).redo).toHaveLength(0);
    expect(showToast).toHaveBeenLastCalledWith("Redid: Moved 9", "success");
  });
});
