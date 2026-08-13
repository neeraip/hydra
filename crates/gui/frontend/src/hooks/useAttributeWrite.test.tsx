// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useElementAttributeWrite } from "./useAttributeWrite";

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

vi.mock("./network", () => ({
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
  setElementAttribute.mockImplementation(() => Promise.resolve());
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
