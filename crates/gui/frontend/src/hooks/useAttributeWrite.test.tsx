// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { clearAllStacks, getUndoStacks, stackKey } from "./undoStack";
import {
  recordEntryLabel,
  useElementAttributeWrite,
  useElementMoveWrite,
  useElementRemoveWrite,
} from "./useAttributeWrite";

/**
 * A write is three things, and the defect this file exists to prevent is
 * shipping only the first: the inspector's editing landed setting the
 * value on the in-memory model and nothing else, so an edit was lost on
 * close and the results on screen went on describing a model that no
 * longer existed. Neither failure is visible while the app is running,
 * which is why it survived a manual check.
 *
 * Asserted on the hook rather than through a component, because both
 * editing surfaces reach it and neither can mount its own provider here.
 */

const setElementAttribute = vi.fn(
  (
    _project: string,
    _id: string,
    _key: string,
    _value: number | string,
    _kind?: string,
  ) => Promise.resolve(),
);
const persistOrSay = vi.fn(
  (
    _id: string,
    _scenarioId?: string | null,
    _toast?: (m: string, t?: string) => void,
  ) => Promise.resolve(),
);
const markEdited = vi.fn();
const showToast = vi.fn();

const patchNodePosition = vi.fn((_id: string, _x: number, _y: number) =>
  Promise.resolve(),
);
const deleteElement = vi.fn((_kind: string, _id: string) =>
  Promise.resolve({ id: "J1", links: ["P1"], attachments: [] }),
);

vi.mock("./network", () => ({
  patchNodePosition: (id: string, x: number, y: number) =>
    patchNodePosition(id, x, y),
  deleteElement: (kind: string, id: string) => deleteElement(kind, id),
  setElementAttribute: (
    project: string,
    id: string,
    key: string,
    value: number | string,
    kind?: string,
  ) => setElementAttribute(project, id, key, value, kind),
}));
vi.mock("./projects", () => ({
  persistOrSay: (
    id: string,
    scenarioId?: string | null,
    toast?: (m: string, t?: string) => void,
  ) => persistOrSay(id, scenarioId, toast),
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

function harness() {
  const calls: Array<
    (
      id: string,
      key: string,
      v: number | string,
      previous?: number | string,
      kind?: string,
    ) => Promise<void>
  > = [];
  function Probe() {
    calls.push(useElementAttributeWrite());
    return null;
  }
  render(<Probe />);
  const write = calls[0];
  if (!write) throw new Error("hook never ran");
  return write;
}

beforeEach(() => {
  setElementAttribute.mockClear();
  persistOrSay.mockClear();
  markEdited.mockClear();
  showToast.mockClear();
  patchNodePosition.mockClear();
  deleteElement.mockClear();
  deleteElement.mockImplementation(() =>
    Promise.resolve({ id: "J1", links: ["P1"], attachments: [] }),
  );
  setElementAttribute.mockImplementation(() => Promise.resolve());
  clearAllStacks();
});

describe("useElementAttributeWrite", () => {
  it("sets the value, persists it, and marks the results stale", async () => {
    const write = harness();
    await act(async () => {
      await write("J1", "invert", 12.5);
    });
    // The project travels with the write: the command needs it to know
    // which engine's model it is addressing.
    expect(setElementAttribute).toHaveBeenCalledWith(
      "p1",
      "J1",
      "invert",
      12.5,
      undefined,
    );
    expect(persistOrSay).toHaveBeenCalledWith("p1", "s1", showToast);
    expect(markEdited).toHaveBeenCalledWith("p1", "s1");
  });

  it("persists after the write, not before it", async () => {
    // A save that runs first writes the old model to disk and reports
    // success — the worst of the three outcomes, because nothing looks
    // wrong.
    const order: string[] = [];
    setElementAttribute.mockImplementation(() => {
      order.push("set");
      return Promise.resolve();
    });
    persistOrSay.mockImplementation(() => {
      order.push("save");
      return Promise.resolve();
    });
    const write = harness();
    await act(async () => {
      await write("J1", "invert", 12.5);
    });
    expect(order).toEqual(["set", "save"]);
  });

  it("carries the kind, which is half the address in one engine", async () => {
    // A water-distribution id names an element only within its family:
    // a junction `10` and a pipe `10` are two elements, and EPANET keeps
    // the namespaces apart deliberately. A write with only the id used to
    // resolve to whichever the lookup reached first, so a tag typed on
    // the pipe landed on the junction and reported success.
    const write = harness();
    await act(async () => {
      await write("10", "tag", "Main", undefined, "pipe");
    });
    expect(setElementAttribute).toHaveBeenCalledWith(
      "p1",
      "10",
      "tag",
      "Main",
      "pipe",
    );
  });

  it("reports a refused write and lets the caller restore the field", async () => {
    setElementAttribute.mockImplementation(() =>
      Promise.reject("'invert' cannot be edited here"),
    );
    const write = harness();
    await act(async () => {
      await expect(write("J1", "invert", 12.5)).rejects.toBeTruthy();
    });
    expect(showToast).toHaveBeenCalledWith(
      "'invert' cannot be edited here",
      "error",
    );
    // Nothing was written, so nothing is saved and no result is stale.
    expect(persistOrSay).not.toHaveBeenCalled();
    expect(markEdited).not.toHaveBeenCalled();
  });
});

/**
 * An element may carry several record sets, and the history has to say
 * which one moved. A control measure carries six layers, so six edits to
 * one measure produced six entries reading "Edited GR1" — a list whose
 * rows cannot be told apart is a list nobody can undo to a point in.
 */
describe("recordEntryLabel", () => {
  it("names the set alongside the element", () => {
    expect(recordEntryLabel("GR1", "Soil")).toBe("Edited GR1 soil");
  });

  it("lowercases a heading dropped into a sentence", () => {
    // The engine capitalises these as headings — "Snow surfaces" — and a
    // heading left capitalised mid-sentence reads as a proper noun.
    expect(recordEntryLabel("SP1", "Snow surfaces")).toBe(
      "Edited SP1 snow surfaces",
    );
  });

  it("says only the id where the element carries one set", () => {
    // Every water-distribution element and most drainage ones. Naming
    // the only set there is would be noise.
    expect(recordEntryLabel("J1")).toBe("Edited J1");
    expect(recordEntryLabel("J1", "")).toBe("Edited J1");
  });
});

/**
 * The two operations that were not gathered here, and drifted the way
 * everything ungathered in this file has drifted: the canvas did all
 * four things in its own callback, the Editor did the command and
 * stopped. So a coordinate typed into the Editor's X column, or an
 * element deleted from its table, reached the loaded model and never the
 * disk — and the results beside them went on reading as current.
 *
 * Neither is visible while the app is running. That is the whole reason
 * these are asserted rather than clicked.
 */
describe("useElementMoveWrite", () => {
  function harnessMove() {
    const calls: Array<
      (
        id: string,
        before: readonly [number, number] | null | undefined,
        x: number,
        y: number,
        kind?: string,
      ) => Promise<void>
    > = [];
    function Probe() {
      calls.push(useElementMoveWrite());
      return null;
    }
    render(<Probe />);
    const move = calls[0];
    if (!move) throw new Error("hook never ran");
    return move;
  }

  it("patches, saves, and marks the results stale", async () => {
    const move = harnessMove();
    await act(async () => {
      await move("J1", [1, 2], 5, 6, "junction");
    });
    expect(patchNodePosition).toHaveBeenCalledWith("J1", 5, 6);
    expect(persistOrSay).toHaveBeenCalledWith("p1", "s1", showToast);
    expect(markEdited).toHaveBeenCalledWith("p1", "s1");
  });

  it("captures the move when the caller knows where it was", async () => {
    const move = harnessMove();
    await act(async () => {
      await move("J1", [1, 2], 5, 6, "junction");
    });
    const { undo } = getUndoStacks(stackKey("p1", "s1"));
    expect(undo[0]?.undo.ops).toEqual([{ op: "move", id: "J1", x: 1, y: 2 }]);
  });

  it("still moves when it cannot be captured", async () => {
    // An inverse nobody can supply is better absent than guessed — but
    // the move itself is not in doubt, and neither is saving it.
    const move = harnessMove();
    await act(async () => {
      await move("J1", null, 5, 6);
    });
    expect(persistOrSay).toHaveBeenCalled();
    expect(getUndoStacks(stackKey("p1", "s1")).undo).toHaveLength(0);
  });
});

describe("useElementRemoveWrite", () => {
  function harnessRemove() {
    const calls: Array<(kind: string, id: string) => Promise<unknown>> = [];
    function Probe() {
      calls.push(useElementRemoveWrite());
      return null;
    }
    render(<Probe />);
    const remove = calls[0];
    if (!remove) throw new Error("hook never ran");
    return remove;
  }

  it("saves the model and marks the results stale", async () => {
    const remove = harnessRemove();
    await act(async () => {
      await remove("junction", "J1");
    });
    expect(deleteElement).toHaveBeenCalledWith("junction", "J1");
    expect(persistOrSay).toHaveBeenCalledWith("p1", "s1", showToast);
    expect(markEdited).toHaveBeenCalledWith("p1", "s1");
  });

  it("hands back what else went, and captures no history of its own", async () => {
    // The history is the one part the two surfaces differ on: only the
    // canvas reads a snapshot first and can offer a way back, so it
    // stays with the caller.
    const remove = harnessRemove();
    let removed: unknown;
    await act(async () => {
      removed = await remove("junction", "J1");
    });
    expect(removed).toEqual({ id: "J1", links: ["P1"], attachments: [] });
    expect(getUndoStacks(stackKey("p1", "s1")).undo).toHaveLength(0);
  });

  it("does not save when the removal was refused", async () => {
    deleteElement.mockImplementationOnce(() => Promise.reject("in use"));
    const remove = harnessRemove();
    await act(async () => {
      await expect(remove("junction", "J1")).rejects.toBeTruthy();
    });
    expect(persistOrSay).not.toHaveBeenCalled();
    expect(markEdited).not.toHaveBeenCalled();
  });
});
