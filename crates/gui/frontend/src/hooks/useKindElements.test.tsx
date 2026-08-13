// @vitest-environment jsdom
/**
 * The Editor's table refetches when the model changes, not only when it
 * is the one that changed it.
 *
 * Every editing surface calls its own `refetch` after its own write, and
 * that hid this for as long as the only way to change a model was to
 * type into it. Undo is a change nothing on screen made: the value went
 * back in the model and the table went on showing the one that had been
 * undone, so the only way to find out it had worked was to reload the
 * project.
 */
import { render } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useKindElements } from "./network";

const getKindElements = vi.fn(() =>
  Promise.resolve({
    ids: [],
    columns: [],
    positions: [],
    ends: [],
  }),
);
const version = { current: 0 };

vi.mock("./ipc", () => ({
  invoke: () => Promise.resolve(),
  isTauri: () => false,
  tryInvoke: () => Promise.resolve(null),
  tryInvokeOr: (_cmd: string, _args: unknown, fallback: unknown) =>
    getKindElements().then(() => fallback),
}));
vi.mock("./NetworkVersionContext", () => ({
  useNetworkVersion: () => ({ version: version.current }),
}));

function Probe() {
  useKindElements("p1", null, "junction");
  return null;
}

beforeEach(() => {
  getKindElements.mockClear();
  version.current = 0;
});

describe("useKindElements", () => {
  it("fetches once for a given model version", async () => {
    await act(async () => {
      render(<Probe />);
    });
    expect(getKindElements).toHaveBeenCalledTimes(1);
  });

  it("fetches again when the model changes underneath it", async () => {
    // The reported bug: undo put the old value back and the table kept
    // drawing the new one. The version is what says the model moved, and
    // it is bumped by the payload-less `network-changed` every structural
    // mutation emits — an undo included, because an undo is applied by
    // the same commands an edit is.
    const { rerender } = render(<Probe />);
    await act(async () => {});
    expect(getKindElements).toHaveBeenCalledTimes(1);

    version.current = 1;
    await act(async () => {
      rerender(<Probe />);
    });
    expect(getKindElements).toHaveBeenCalledTimes(2);
  });

  it("does not refetch when nothing about the model moved", async () => {
    // A re-render is not a change. Refetching on every one would put the
    // table's contents on the render loop and make a long list flicker
    // for no reason.
    const { rerender } = render(<Probe />);
    await act(async () => {});
    await act(async () => {
      rerender(<Probe />);
    });
    expect(getKindElements).toHaveBeenCalledTimes(1);
  });
});
