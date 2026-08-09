/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_CRITERIA } from "../../hooks";

/**
 * The toolbar's criteria control: one chip, the standard read back on
 * hover, the engine's editor a click away. Criteria moved here because
 * they are project-scoped and three surfaces read them — the canvas, the
 * results, and the report — so what must not regress is that the chip
 * stays compact and still opens a working editor.
 *
 * `AppProvider` cannot mount under jsdom (it registers Tauri listeners),
 * so the app hooks are mocked rather than provided.
 */

const setCriteria = vi.fn();
vi.mock("../../AppContext", () => ({
  useActiveProject: () => ({ project: { id: "p1" } }),
}));
vi.mock("../../hooks", async () => {
  const actual =
    await vi.importActual<typeof import("../../hooks")>("../../hooks");
  return {
    ...actual,
    useProjectCriteria: () => ({
      criteria: actual.DEFAULT_CRITERIA,
      setCriteria,
      saved: true,
    }),
  };
});

import { WdsCriteriaControl } from "./CriteriaControl";

describe("WdsCriteriaControl", () => {
  it("is a bare chip that reads the standard back on hover", () => {
    render(<WdsCriteriaControl />);
    const chip = screen.getByRole("button", { name: "Criteria" });
    // The word alone — a summary would fight the scenario strip for width.
    expect(chip.textContent).toBe("Criteria");
    // …but the whole ruler is still one hover away.
    expect(chip.getAttribute("data-tooltip")).toContain("≥ 14 m");
    expect(chip.getAttribute("data-tooltip")).toContain("V 0.1–1.5 m/s");
  });

  it("opens the editor and edits reach the shared store", () => {
    render(<WdsCriteriaControl />);
    expect(screen.queryByRole("dialog", { name: "Criteria" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Criteria" }));
    expect(screen.getByRole("dialog", { name: "Criteria" })).toBeTruthy();

    const input = screen.getAllByRole("spinbutton")[0];
    fireEvent.change(input, { target: { value: "20" } });
    expect(setCriteria).toHaveBeenCalledWith(
      expect.objectContaining({ minPressureM: 20 }),
    );
  });

  it("Escape dismisses the editor", () => {
    render(<WdsCriteriaControl />);
    fireEvent.click(screen.getByRole("button", { name: "Criteria" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Criteria" })).toBeNull();
  });

  it("the defaults it shows are the ones the backend mirrors", () => {
    // Cheap guard on the pair: the chip reads whatever the store holds,
    // and the store's defaults mirror the Rust ones.
    expect(DEFAULT_CRITERIA.minPressureM).toBe(14);
  });
});
