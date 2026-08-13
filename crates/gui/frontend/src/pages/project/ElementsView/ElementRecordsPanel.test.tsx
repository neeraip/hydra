/**
 * @vitest-environment jsdom
 */
/**
 * The records panel below the Editor's table.
 *
 * A junction's demand categories showed in the canvas inspector and
 * nowhere in the Editor — one value giving two answers depending on
 * which surface you asked, which is the shape of defect this editor was
 * rebuilt to remove. These assert the panel appears where there is
 * something to show and stays out of the way where there is not.
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RecordSet } from "../../../hooks";
import { ElementRecordsPanel } from "./ElementRecordsPanel";

const TEXT = { type: "text", default: null } as const;

const DEMANDS: RecordSet = {
  key: "demands",
  label: "Demand categories",
  columns: [{ key: "name", label: "Category", kind: TEXT }],
  rows: [["Residential"]],
  editable: true,
};

const sets = vi.fn<() => RecordSet[]>(() => [DEMANDS]);

vi.mock(
  "../../../components/panels/ElementInspector/RecordSets",
  async (importOriginal) => ({
    ...(await importOriginal<Record<string, unknown>>()),
    RecordSets: ({ sets: s }: { sets: RecordSet[] }) => (
      <div>{s.map((set) => set.label).join(", ")}</div>
    ),
    useElementRecords: () => ({ sets: sets(), refetch: () => {} }),
  }),
);

describe("ElementRecordsPanel", () => {
  it("shows the sets the element carries", () => {
    render(<ElementRecordsPanel elementId="J1" />);
    expect(screen.getByText("Demand categories")).toBeDefined();
  });

  it("draws nothing at all for an element carrying none", () => {
    // Not an empty panel: a bordered strip with nothing in it reads as
    // something failing to load, and most elements carry no records.
    sets.mockReturnValueOnce([]);
    const { container } = render(<ElementRecordsPanel elementId="P1" />);
    expect(container.firstChild).toBeNull();
  });

  it("draws nothing for a set that is empty and cannot be added to", () => {
    // A drainage node's dry weather inflows are served for every node,
    // holding nothing for most of them and read-only for all of them. The
    // strip counted the *sets* rather than what they would draw, so it
    // opened under every node in every drainage model and showed a
    // heading with a row of column names and nothing beneath.
    sets.mockReturnValueOnce([
      { ...DEMANDS, label: "Dry weather inflow", rows: [], editable: false },
    ]);
    const { container } = render(<ElementRecordsPanel elementId="J2" />);
    expect(container.firstChild).toBeNull();
  });
});
