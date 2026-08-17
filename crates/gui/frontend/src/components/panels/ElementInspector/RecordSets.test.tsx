/**
 * @vitest-environment jsdom
 */
/**
 * The records attached to an element (hydra-common §4.5.2.3).
 *
 * A junction with two demand categories used to read as one: the
 * attribute schema could publish only their sum and the first one's
 * pattern, so the second was invisible and the write refused rather than
 * distribute a total across categories nobody had described. These
 * assert the table that replaced that — which rows appear, what each
 * cell offers, and that a change sends the whole set.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RecordSet } from "../../../hooks";
import type { RecordWriteContext } from "../../../hooks/useAttributeWrite";
import {
  RECORD_ACTION_WIDTH,
  RecordSets,
  recordTableMaxWidth,
  shownRecordSets,
} from "./RecordSets";

const NUMBER = { type: "number", default: null, min: null, max: null } as const;
const TEXT = { type: "text", default: null } as const;
const DEMAND = {
  key: "demand",
  siLabel: "L/s",
  usLabel: "gpm",
  siToUsScale: 15.8503,
  siToUsOffset: 0,
  siDecimals: 2,
  usDecimals: 2,
};

const DEMANDS: RecordSet = {
  key: "demands",
  label: "Demand categories",
  columns: [
    { key: "baseDemand", label: "Base demand", kind: NUMBER, quantity: DEMAND },
    { key: "pattern", label: "Pattern", kind: TEXT, references: ["pattern"] },
    { key: "name", label: "Category", kind: TEXT },
  ],
  rows: [
    [10, "P1", "Residential"],
    [2.5, "", ""],
  ],
  editable: true,
};

const write = vi.fn<
  (
    elementId: string,
    set: string,
    rows: RecordSet["rows"],
    context?: RecordWriteContext,
  ) => Promise<void>
>(() => Promise.resolve());

vi.mock("../../../AppContext", () => ({
  useActiveProject: () => ({ project: { id: "p1", engine: "wds" } }),
  useAppState: () => ({ activeScenarioId: null }),
}));
vi.mock("../../../hooks", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getElementRecords: () => Promise.resolve([]),
  useReferenceIds: () => ({ pattern: ["P1", "P2"] }),
}));
vi.mock("../../../hooks/useAttributeWrite", () => ({
  useElementRecordsWrite: () => write,
}));
vi.mock("../../../units", () => ({ useUnitSystem: () => "si" }));

function renderSet(set: RecordSet = DEMANDS) {
  return render(<RecordSets elementId="J1" kind="junction" sets={[set]} />);
}

describe("RecordSets", () => {
  it("shows every record, not their total", () => {
    renderSet();
    expect(
      (
        screen.getByLabelText(
          "J1 Demand categories 1 Base demand",
        ) as HTMLInputElement
      ).value,
    ).toBe("10");
    expect(
      (
        screen.getByLabelText(
          "J1 Demand categories 2 Base demand",
        ) as HTMLInputElement
      ).value,
    ).toBe("2.5");
  });

  it("labels the column with the unit the engine declared", () => {
    renderSet();
    expect(screen.getByText("Base demand (L/s)")).toBeDefined();
  });

  it("offers the ids a reference column may name", () => {
    const { container } = renderSet();
    const lists = [...container.querySelectorAll("datalist")];
    expect(lists).toHaveLength(1);
    expect(
      [...lists[0].querySelectorAll("option")].map((o) => o.value),
    ).toEqual(["P1", "P2"]);
    // Only the column that declared a reference: a category name is text
    // and gets no list.
    expect(
      screen
        .getByLabelText("J1 Demand categories 1 Category")
        .getAttribute("list"),
    ).toBeNull();
  });

  it("sends the whole set when one cell changes", () => {
    write.mockClear();
    renderSet();
    const cell = screen.getByLabelText(
      "J1 Demand categories 2 Base demand",
    ) as HTMLInputElement;
    fireEvent.change(cell, { target: { value: "4" } });
    fireEvent.blur(cell);
    expect(write).toHaveBeenCalledWith(
      "J1",
      "demands",
      [
        [10, "P1", "Residential"],
        [4, "", ""],
      ],
      {
        previous: DEMANDS.rows,
        // The kind travels with the write: a water-distribution id names
        // an element only within its family, and every record set here
        // hangs off a node, so a pipe sharing a junction's id used to be
        // served — and to write — the junction's categories.
        kind: "junction",
        // And the set's name, so the history says which of an element's
        // sets was edited. A control measure carries six.
        label: "Demand categories",
      },
    );
  });

  it("marks each row and its remove cell for the hover affordances", () => {
    // The stylesheet's row hover and the danger tint target these class
    // names; this pins them so a rename cannot silently detach the CSS.
    // The tint exists because the remove icon deletes the whole row, and
    // the row should say so before the click.
    const { container } = renderSet();
    const rows = container.querySelectorAll("tbody tr.record-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector("td.record-remove")).not.toBeNull();
  });

  it("adds and removes a record by writing the set that results", () => {
    write.mockClear();
    renderSet();
    // A new record is the set with a row more — the same write, which is
    // why there is no separate add operation to refuse.
    fireEvent.click(screen.getByLabelText("Add record"));
    expect(write.mock.calls[0][2]).toEqual([
      [10, "P1", "Residential"],
      [2.5, "", ""],
      [0, "", ""],
    ]);

    fireEvent.click(screen.getAllByLabelText("Remove row")[0]);
    expect(write.mock.calls[1][2]).toEqual([[2.5, "", ""]]);
  });

  it("lays every set out on the widest set's grid", () => {
    // Two failures, one cause each. Sized to the panel, each set divided
    // the full width by its own column count and no column edge lined
    // up. Sized to itself, each table ended at its own last column and
    // the delete icons staggered — a five-column layer's icon sat 380px
    // left of a seven-column one's. The widest set's grid fixes both:
    // same width, same shares, ghost cells padding the narrower sets,
    // one right edge for the action rail.
    const wide: RecordSet = {
      ...DEMANDS,
      key: "wider",
      label: "Wider",
      columns: [
        ...DEMANDS.columns,
        { key: "a", label: "A", kind: NUMBER },
        { key: "b", label: "B", kind: NUMBER },
      ],
      rows: [[1, "", "", 2, 3]],
    };
    const { container } = render(
      <RecordSets elementId="J1" kind="junction" sets={[DEMANDS, wide]} />,
    );
    const tables = [...container.querySelectorAll("table")];
    expect(tables).toHaveLength(2);
    for (const t of tables) {
      // Both capped by the five-column set, whichever holds fewer.
      expect((t as HTMLTableElement).style.maxWidth).toBe(
        `${recordTableMaxWidth(5, true)}px`,
      );
      expect((t as HTMLTableElement).style.tableLayout).toBe("fixed");
      // Same column count too: 5 data shares (ghosts included) + action.
      expect(t.querySelectorAll("thead th")).toHaveLength(6);
    }
    // The narrower set pads with empty ghost headers; the wider one
    // needs none.
    const empties = (t: Element) =>
      [...t.querySelectorAll("thead th")].filter((h) => !h.textContent).length;
    // Demands: 2 ghosts + the action column's own empty th.
    expect(empties(tables[0])).toBe(3);
    expect(empties(tables[1])).toBe(1);
    // The action column is one icon wide in both.
    const actionTh = tables[0].querySelectorAll("thead th")[5] as HTMLElement;
    expect(actionTh.style.width).toBe(`${RECORD_ACTION_WIDTH}px`);
  });

  it("gives a read-only set no action column and no room for one", () => {
    const { container } = renderSet({ ...DEMANDS, editable: false });
    const table = container.querySelector("table") as HTMLTableElement;
    expect(table.style.maxWidth).toBe(`${recordTableMaxWidth(3, false)}px`);
    expect(table.querySelectorAll("th")).toHaveLength(3);
  });

  it("stops offering a record once the set is full", () => {
    // A control measure holds one surface layer or none, so the button
    // under a layer it already had could only ever refuse. Removing the
    // row is still offered — that is how the layer comes off.
    renderSet({ ...DEMANDS, rows: [[10, "P1", "Residential"]], capacity: 1 });
    expect(screen.queryByLabelText("Add record")).toBeNull();
    expect(screen.getAllByLabelText("Remove row")).toHaveLength(1);
  });

  it("reads only when the engine did not mark the set editable", () => {
    // A drainage vertex's dry-weather inflows today: shown so they can be
    // read, which is worth doing whether or not they can be rewritten.
    renderSet({ ...DEMANDS, editable: false });
    expect(
      screen.queryByLabelText("J1 Demand categories 1 Base demand"),
    ).toBeNull();
    expect(screen.queryByLabelText("Add record")).toBeNull();
    expect(screen.getByText("Residential")).toBeDefined();
  });

  it("draws nothing for an element with no records", () => {
    const { container } = render(<RecordSets elementId="J1" sets={[]} />);
    expect(container.querySelector("table")).toBeNull();
  });

  // An empty set is not always nothing, and which it is depends on
  // whether the empty table is an offer.
  describe("shownRecordSets", () => {
    it("keeps an empty set that can be added to", () => {
      // A junction with no demand categories: the headings and the add
      // button are how the first one is entered.
      const empty = { ...DEMANDS, rows: [] };
      expect(shownRecordSets([empty])).toEqual([empty]);
    });

    it("drops an empty set that cannot", () => {
      // A drainage node's dry weather inflows, which are served
      // read-only. This drew a heading and a row of column names under
      // every node in every drainage model, with nothing beneath them —
      // which reads as a panel that failed to load.
      expect(
        shownRecordSets([{ ...DEMANDS, rows: [], editable: false }]),
      ).toEqual([]);
    });

    it("keeps a read-only set that holds something", () => {
      const readable = { ...DEMANDS, editable: false };
      expect(shownRecordSets([readable])).toEqual([readable]);
    });
  });
});
