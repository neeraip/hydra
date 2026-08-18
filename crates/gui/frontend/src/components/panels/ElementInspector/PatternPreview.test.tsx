// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PatternPreview } from "./PatternPreview";

/**
 * This chart never drew. It asked for every pattern in the model through
 * a command no backend defined, got an empty list back, and took the
 * branch that means "this reference points at nothing" — which is the
 * correct thing to draw for a dangling reference and the wrong thing to
 * draw for every pattern that exists.
 *
 * The command name is guarded across the language boundary in
 * `scripts/tests/test_gui_command_surface.py`. What is asserted here is
 * the half that lives in this file: that a pattern which resolves is
 * read out of the column the engines actually put the multipliers in,
 * and that an id resolving to nothing still draws nothing.
 */

const detail = vi.fn();

vi.mock("../../../AppContext", () => ({
  useAppState: () => ({ activeProjectId: "p1", activeScenarioId: "s1" }),
}));
vi.mock("../../../hooks", () => ({
  useCollectionDetail: (...args: unknown[]) => detail(...args),
}));

function served(rows: number[][]) {
  detail.mockReturnValue({
    detail: {
      columns: ["Interval", "Factor"],
      quantities: [null, null],
      rows,
      lines: [],
      editable: true,
    },
  });
}

beforeEach(() => {
  detail.mockReset();
});

describe("PatternPreview", () => {
  it("asks for the one pattern it names, in the open project and scenario", () => {
    served([[1, 1]]);
    render(<PatternPreview patternId="PAT1" stroke="#000" />);
    expect(detail).toHaveBeenCalledWith("p1", "s1", "pattern", "PAT1");
  });

  it("plots the multipliers, which are the second column and not the first", () => {
    // Interval counts 1..4 and would give a rising line with a max of 4;
    // the multipliers are the values the chart is about.
    served([
      [1, 0.6],
      [2, 1.4],
      [3, 1.0],
      [4, 0.8],
    ]);
    render(<PatternPreview patternId="PAT1" stroke="#000" />);
    expect(screen.getByText("×0.60")).toBeTruthy();
    expect(screen.getByText("×1.40")).toBeTruthy();
    expect(screen.getByText("4 steps")).toBeTruthy();
  });

  it("says one step in the singular", () => {
    served([[1, 1.25]]);
    render(<PatternPreview patternId="PAT1" stroke="#000" />);
    expect(screen.getByText("1 step")).toBeTruthy();
  });

  it("draws nothing when the id resolves to no pattern", () => {
    // A dangling reference, which validation reports. An empty chart here
    // would claim the pattern exists and is flat.
    served([]);
    const { container } = render(
      <PatternPreview patternId="GONE" stroke="#000" />,
    );
    expect(container.firstChild).toBeNull();
  });
});
