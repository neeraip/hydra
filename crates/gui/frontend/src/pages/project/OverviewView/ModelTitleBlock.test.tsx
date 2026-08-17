/**
 * @vitest-environment jsdom
 */
/**
 * The model title block, and the write discipline its save owes.
 *
 * The defect this guards: saving a title wrote the in-memory model and
 * stopped — no persist, no staleness mark — so a title edited and left
 * alone was gone at the next open, for either engine. It survived
 * whenever some later element edit happened to flush the dirty state
 * alongside it, which is exactly the kind of accident that keeps a
 * missing persist invisible.
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModelTitleBlock } from "./ModelTitleBlock";

const updateNetworkTitle = vi.fn<(lines: string[]) => Promise<void>>(() =>
  Promise.resolve(),
);
vi.mock("../../../hooks/network", () => ({
  getNetworkTitle: () => Promise.resolve(["Old description"]),
  updateNetworkTitle: (lines: string[]) => updateNetworkTitle(lines),
}));
const persistOrSay = vi.fn(() => Promise.resolve());
vi.mock("../../../hooks/projects", () => ({
  persistOrSay: (...args: unknown[]) => persistOrSay(...(args as [])),
}));
const showToast = vi.fn();
const markEdited = vi.fn();
vi.mock("../../../AppContext", () => ({
  useAppState: () => ({
    showToast,
    activeProjectId: "p1",
    activeScenarioId: null,
  }),
  // A drainage project: the engine whose title capability shipped last.
  useActiveProject: () => ({ engine: { key: "uds" } }),
}));
vi.mock("../../../hooks/NetworkVersionContext", () => ({
  useNetworkVersion: () => ({ version: 0, markEdited }),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe("ModelTitleBlock", () => {
  it("saves the title, persists it, and marks the model edited", async () => {
    const { container } = render(<ModelTitleBlock />);
    fireEvent.click(await screen.findByLabelText("Edit model title"));

    const textarea = container.querySelector("textarea");
    expect(textarea).not.toBeNull();
    fireEvent.change(textarea as HTMLTextAreaElement, {
      target: { value: "New description" },
    });
    fireEvent.click(
      container.querySelector('button[data-tooltip^="Save"]') as HTMLElement,
    );

    await waitFor(() =>
      expect(updateNetworkTitle).toHaveBeenCalledWith(["New description"]),
    );
    // The other three quarters of the write: on disk, and marked.
    await waitFor(() => expect(persistOrSay).toHaveBeenCalled());
    expect(markEdited).toHaveBeenCalledWith("p1", null);
  });
});
