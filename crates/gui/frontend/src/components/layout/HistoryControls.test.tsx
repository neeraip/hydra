/**
 * @vitest-environment jsdom
 */
/**
 * The undo/redo controls in the top bar.
 *
 * The stacks were reachable by ⌘Z, by the command palette and by the
 * shortcut card, and by nothing on screen — so a history existed and
 * there was no way to learn that without being told. These assert what
 * the reader can actually see: whether there is anything to undo, what
 * pressing the button would undo, and what is behind it.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UndoEntry } from "../../hooks/undoStack";
import { HistoryControls } from "./HistoryControls";

const undo = vi.fn();
const redo = vi.fn();
const state = {
  page: "project" as string,
  activeScenarioId: null as string | null,
};
const stacks = {
  undo: [] as UndoEntry[],
  redo: [] as UndoEntry[],
};

vi.mock("../../AppContext", () => ({
  useAppState: () => state,
  useActiveProject: () => ({ project: { id: "p1", engine: "wds" } }),
}));
vi.mock("../../hooks/undoStack", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useUndoStacks: () => stacks,
}));
vi.mock("../../hooks/useUndoRedo", () => ({
  useUndoRedo: () => ({ undo, redo }),
}));
vi.mock("../../hooks/engines", () => ({
  useElementKinds: () => [{ id: "junction", label: "Junction" }],
}));

function entry(
  label: string,
  subject?: { kind: string; id: string },
): UndoEntry {
  return { label, subject, undo: {}, redo: {} };
}

beforeEach(() => {
  undo.mockClear();
  redo.mockClear();
  state.page = "project";
  stacks.undo = [entry("Changed diameter on P1"), entry("Deleted P3")];
  stacks.redo = [];
});

describe("HistoryControls", () => {
  it("names the edit it would undo, not the control", () => {
    // "Undo" over a live button says nothing the icon has not already
    // said. The label is what makes the history legible without opening
    // anything.
    render(<HistoryControls />);
    expect(screen.getByLabelText("Undo").getAttribute("data-tooltip")).toBe(
      "Undo: Deleted P3",
    );
  });

  it("offers nothing when there is nothing, and says so", () => {
    stacks.undo = [];
    render(<HistoryControls />);
    const button = screen.getByLabelText("Undo") as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.getAttribute("data-tooltip")).toBe("Nothing to undo");
    // A dead button's menu is dead too: there is no history to open.
    expect(
      (screen.getByLabelText("Undo history") as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("shows the history newest first when its menu is opened", () => {
    render(<HistoryControls />);
    expect(screen.queryByText("Deleted P3")).toBeNull();
    fireEvent.click(screen.getByLabelText("Undo history"));
    const shown = screen.getAllByText(/Deleted P3|Changed diameter on P1/);
    expect(shown.map((n) => n.textContent)).toEqual([
      "Deleted P3",
      "Changed diameter on P1",
    ]);
  });

  it("counts the entries it did not list", () => {
    stacks.undo = Array.from({ length: 14 }, (_, i) => entry(`Edit ${i}`));
    render(<HistoryControls />);
    fireEvent.click(screen.getByLabelText("Undo history"));
    // Not a silent truncation: ten shown and four said, so a clipped list
    // cannot read as the whole history.
    expect(screen.getByText("4 older")).toBeDefined();
  });

  it("applies the edit when the button itself is pressed", () => {
    render(<HistoryControls />);
    fireEvent.click(screen.getByLabelText("Undo"));
    expect(undo).toHaveBeenCalledTimes(1);
    expect(redo).not.toHaveBeenCalled();
  });

  it("draws nothing away from a project", () => {
    // A history belongs to a project and a scenario, so off the project
    // page there is not an empty one — there is no question being asked.
    // The keyboard shortcut is gated the same way.
    state.page = "settings";
    const { container } = render(<HistoryControls />);
    expect(container.firstChild).toBeNull();
  });

  it("names the kind in the tooltip, where a glyph cannot go", () => {
    // The one place this interface says a kind in words: a tooltip is a
    // plain-text attribute and can hold no badge, and "Changed invert on
    // 9" cannot say whether it was the junction or the conduit.
    stacks.undo = [entry("Changed invert on 9", { kind: "junction", id: "9" })];
    render(<HistoryControls />);
    expect(screen.getByLabelText("Undo").getAttribute("data-tooltip")).toBe(
      "Undo: Changed invert on 9 (Junction)",
    );
  });

  it("shows the kind's glyph beside the edit, never its name alone", () => {
    // "Changed invert on 9" names half an element: an id is unique only
    // within its class, so a junction 9 and a conduit 9 are two things
    // sharing a name — and deciding whether to undo means knowing which
    // one it happened to. The badge is the same mark the canvas, the
    // network list and the tables use, so it is one vocabulary rather
    // than one per surface.
    stacks.undo = [entry("Changed invert on 9", { kind: "junction", id: "9" })];
    render(<HistoryControls />);
    fireEvent.click(screen.getByLabelText("Undo history"));
    const row = screen.getByText("Changed invert on 9").parentElement;
    expect(row?.querySelector("[data-tooltip='Junction']")).not.toBeNull();
  });

  it("draws the row without a badge where the capture knew no kind", () => {
    // Absent rather than guessed: a badge for the wrong kind says
    // something false with the authority of a glyph.
    stacks.undo = [entry("Edited the model")];
    render(<HistoryControls />);
    fireEvent.click(screen.getByLabelText("Undo history"));
    const row = screen.getByText("Edited the model").parentElement;
    expect(row?.querySelector("[data-tooltip]")).toBeNull();
  });

  it("keeps redo separate from undo", () => {
    stacks.redo = [entry("Moved J1")];
    render(<HistoryControls />);
    expect(screen.getByLabelText("Redo").getAttribute("data-tooltip")).toBe(
      "Redo: Moved J1",
    );
    fireEvent.click(screen.getByLabelText("Redo"));
    expect(redo).toHaveBeenCalledTimes(1);
    expect(undo).not.toHaveBeenCalled();
  });
});
