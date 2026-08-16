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

    fireEvent.click(screen.getAllByLabelText("Remove record")[0]);
    expect(write.mock.calls[1][2]).toEqual([[2.5, "", ""]]);
  });

  it("caps its stretch so every set shares one column rhythm", () => {
    // In the Editor's full-width panel each set divided the width by its
    // own column count: a five-column layer and a seven-column layer
    // agreed on no column edge, and a row's delete icon sat at the far
    // edge of the panel. The cap binds only where there is room — a
    // narrow inspector rail still divides its width exactly as before.
    const { container } = renderSet();
    const table = container.querySelector("table") as HTMLTableElement;
    expect(table.style.maxWidth).toBe(`${recordTableMaxWidth(3, true)}px`);
    expect(table.style.tableLayout).toBe("fixed");
    // The action column is one icon wide; the data columns split the
    // remainder equally, which is what makes the rhythm.
    const actionTh = table.querySelectorAll("th")[3] as HTMLElement;
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
    expect(screen.getAllByLabelText("Remove record")).toHaveLength(1);
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
