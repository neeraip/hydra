/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { KindElements } from "../../hooks";
import { KindTable } from "./KindTable";

/** A sortable header is a real control; the click goes to the button. */
function sortButton(th: Element): HTMLButtonElement {
  const button = th.querySelector("button");
  if (!button) throw new Error(`"${th.textContent}" has no sort button`);
  return button;
}

/** The shape a numeric column declares; spelled once here because most
 * of the fixtures below are numbers. */
const NUMBER = { type: "number", default: null, min: null, max: null } as const;

const junctions: KindElements = {
  ids: ["J1", "J2"],
  columns: [
    {
      key: "invert",
      label: "Invert",
      editable: false,
      kind: NUMBER,
      values: [12, 4],
    },
  ],
  positions: [],
} as KindElements;

const conduits: KindElements = {
  ids: ["C1", "C2"],
  columns: [
    {
      key: "length",
      label: "Length",
      editable: false,
      kind: NUMBER,
      values: [30, 10],
    },
  ],
  positions: [],
} as KindElements;

describe("KindTable", () => {
  // The table is already scoped to one kind by the rail, so a badge per
  // row repeats the same glyph down the page and spends a column on it.
  it("shows no kind column", () => {
    const { container } = render(<KindTable elements={junctions} />);
    const headers = [...container.querySelectorAll("th")];
    // Every header is a real, labelled column — no blank badge slot.
    expect(headers.length).toBe(2);
    expect(headers.every((h) => (h.textContent ?? "").trim().length > 0)).toBe(
      true,
    );
  });

  it("lists every element with its property columns", () => {
    render(<KindTable elements={junctions} />);
    expect(screen.getByText("J1")).toBeDefined();
    expect(screen.getByText("Invert")).toBeDefined();
  });

  it("sorts on a column header", () => {
    const { container } = render(<KindTable elements={junctions} />);
    fireEvent.click(screen.getByText(/Invert/));
    const firstId = container.querySelectorAll("tbody tr td")[0];
    expect(firstId?.textContent).toBe("J2"); // invert 4 before 12
  });

  // Sort belongs to the kind being shown. Carried across, a table would
  // claim to be sorted by a column its kind does not have — which is why
  // callers mount one table per kind.
  it("starts unsorted when mounted for a different kind", () => {
    const { container, rerender } = render(
      <KindTable key="junction" elements={junctions} />,
    );
    fireEvent.click(screen.getByText(/Invert/));
    rerender(<KindTable key="conduit" elements={conduits} />);
    const firstId = container.querySelectorAll("tbody tr td")[0];
    expect(firstId?.textContent).toBe("C1"); // model order, not sorted
  });

  it("filters rows by id", () => {
    const { container } = render(<KindTable elements={junctions} />);
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "J2" },
    });
    const ids = [...container.querySelectorAll("tbody tr")].map(
      (r) => r.querySelector("td")?.textContent,
    );
    expect(ids).toEqual(["J2"]);
  });

  it("matches ids case-insensitively", () => {
    const { container } = render(<KindTable elements={junctions} />);
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "j1" },
    });
    expect(container.querySelectorAll("tbody tr")).toHaveLength(1);
  });

  // "No elements of this kind" would be a lie when the kind has plenty
  // and the query simply matched none of them.
  it("distinguishes no matches from an empty kind", () => {
    render(<KindTable elements={junctions} />);
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "zzz" },
    });
    expect(screen.getByText(/No ids match/)).toBeDefined();
    expect(screen.queryByText("No elements of this kind.")).toBeNull();
  });

  it("says so when a kind has no elements", () => {
    render(
      <KindTable
        elements={{ ids: [], columns: [], positions: [] } as KindElements}
      />,
    );
    expect(screen.getByText("No elements of this kind.")).toBeDefined();
  });

  /**
   * Revealing an element — the canvas inspector's "Open in editor" — has to
   * end with that element on screen. The search box is the one thing that
   * can hide it: a user filtered to "J2" who then asks to see J1 would
   * otherwise get a table that looks empty, which is a correct answer to
   * the filter and the wrong answer to what they asked for.
   */
  it("clears a search that would hide the revealed element", () => {
    const { rerender } = render(
      <KindTable elements={junctions} activeId="J1" revealToken={1} />,
    );
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "J2" },
    });
    expect(screen.queryByText("J1")).toBeNull();

    rerender(<KindTable elements={junctions} activeId="J1" revealToken={2} />);
    expect(screen.getByText("J1")).toBeDefined();
  });

  // A token, not a boolean: asking for the same element twice has to act
  // both times, and a flag that is already set cannot say "again".
  it("acts again when the same element is revealed a second time", () => {
    const { rerender } = render(
      <KindTable elements={junctions} activeId="J1" revealToken={1} />,
    );
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "J2" },
    });
    rerender(<KindTable elements={junctions} activeId="J1" revealToken={2} />);
    expect(screen.getByText("J1")).toBeDefined();

    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "J2" },
    });
    expect(screen.queryByText("J1")).toBeNull();
    rerender(<KindTable elements={junctions} activeId="J1" revealToken={3} />);
    expect(screen.getByText("J1")).toBeDefined();
  });

  /**
   * The sort marks were `↕`, `↑` and `↓` — characters, so their shape came
   * from whatever font the platform resolved and their size from the text
   * rather than from any icon rule.
   *
   * Asserted over every heading rather than one, so a heading added later
   * has to carry a real mark too.
   */
  it("draws its sort marks as icons, not characters", () => {
    const { container } = render(<KindTable elements={junctions} />);
    const headers = [...container.querySelectorAll("th")];
    expect(headers.length).toBeGreaterThan(0);
    for (const th of headers) {
      expect(
        th.querySelector("svg"),
        `"${th.textContent}" has no sort mark`,
      ).not.toBeNull();
      // Named explicitly: an icon added *beside* a surviving character
      // would pass the check above while still drawing the glyph.
      expect(th.textContent).not.toMatch(/[↕↑↓]/);
    }
  });

  /**
   * The mark is the only thing on screen saying which column is sorted and
   * which way, so it has to differ in all three states — and a swap of
   * icons could put ascending and descending the wrong way round without
   * the sort tests above noticing, since those assert row order only.
   */
  it("marks only the sorted column, and follows its direction", () => {
    const { container } = render(<KindTable elements={junctions} />);
    const [idHeader, invertHeader] = [...container.querySelectorAll("th")];
    const neutral = idHeader?.querySelector("svg")?.innerHTML;
    expect(neutral).toBeTruthy();

    if (!idHeader) throw new Error("no ID header");
    // The button inside, not the cell: a sortable header is a real
    // control, so it is keyboard-focusable and Enter/Space toggle it.
    // Clicking the `<th>` used to sort because the handler was on the
    // cell, which is exactly what made it unreachable without a mouse.
    const sortById = () => fireEvent.click(sortButton(idHeader));
    sortById();
    const ascending = idHeader.querySelector("svg")?.innerHTML;
    expect(ascending).not.toBe(neutral);
    // The column that is not sorted keeps the neutral mark.
    expect(invertHeader?.querySelector("svg")?.innerHTML).toBe(neutral);

    sortById();
    const descending = idHeader.querySelector("svg")?.innerHTML;
    expect(descending).not.toBe(ascending);
    expect(descending).not.toBe(neutral);
  });

  // Selecting a row is not a reveal request. Only the token clears the
  // search — otherwise every click would wipe the filter under the user.
  it("leaves the search alone when no reveal is requested", () => {
    const { rerender } = render(<KindTable elements={junctions} />);
    fireEvent.change(screen.getByLabelText("Search ids"), {
      target: { value: "J2" },
    });
    rerender(<KindTable elements={junctions} activeId="J2" />);
    expect(screen.queryByText("J1")).toBeNull();
    expect(screen.getByText("J2")).toBeDefined();
  });
});

/**
 * Editing is engine-authored twice over: whether an attribute may be
 * written at all is the backend's answer, carried per column, and
 * whether *this* element has a value for it is the cell's. This file
 * never asks which engine it is drawing.
 */
describe("KindTable editing", () => {
  const editable: KindElements = {
    ids: ["J1", "J2"],
    columns: [
      {
        key: "invert",
        label: "Invert",
        editable: true,
        kind: NUMBER,
        values: [12, 4],
      },
      {
        key: "shape",
        label: "Shape",
        editable: false,
        kind: { type: "text", default: null },
        values: ["CIRCULAR", null],
      },
    ],
    positions: [],
  } as KindElements;

  it("offers an input only where the column says it may be written", () => {
    render(<KindTable elements={editable} onEdit={() => {}} />);
    expect(screen.getByLabelText("J1 Invert")).toBeDefined();
    expect(screen.queryByLabelText("J1 Shape")).toBeNull();
    // The unwritable column still reads — editing arriving must not cost
    // a column its value.
    expect(screen.getByText("CIRCULAR")).toBeDefined();
  });

  it("reads only when no one is listening for writes", () => {
    // A column may declare itself writable while the surface drawing it
    // has nowhere to send the write. Two separate facts, and the table
    // needs both.
    render(<KindTable elements={editable} />);
    expect(screen.queryByLabelText("J1 Invert")).toBeNull();
    expect(screen.getByText(/12/)).toBeDefined();
  });

  it("addresses the write by id and schema key", () => {
    const onEdit = vi.fn();
    render(<KindTable elements={editable} onEdit={onEdit} />);
    const cell = screen.getByLabelText("J2 Invert");
    fireEvent.change(cell, { target: { value: "7" } });
    fireEvent.blur(cell);
    // The second row, not the first: the row index and the id have to
    // stay married through sorting and filtering.
    // The value the cell was showing travels with the write, so the
    // edit can be undone without re-reading the model.
    expect(onEdit).toHaveBeenCalledWith("J2", "invert", 7, 4);
  });

  it("still addresses the right element after a sort", () => {
    // Rows are indices into columnar arrays, and sorting reorders the
    // indices — so a cell that captured its row position rather than its
    // id would write to the wrong element the moment a heading is
    // clicked.
    const onEdit = vi.fn();
    const { container } = render(
      <KindTable elements={editable} onEdit={onEdit} />,
    );
    const [, invertHeader] = [...container.querySelectorAll("th")];
    if (!invertHeader) throw new Error("no Invert header");
    // The header's button — see the sort-mark test above.
    fireEvent.click(sortButton(invertHeader)); // ascending: J2 first
    const first = container.querySelector("tbody tr");
    expect(first?.querySelector("td")?.textContent).toBe("J2");
    const cell = screen.getByLabelText("J2 Invert");
    fireEvent.change(cell, { target: { value: "5" } });
    fireEvent.blur(cell);
    expect(onEdit).toHaveBeenCalledWith("J2", "invert", 5, 4);
  });

  it("shows a dash rather than an empty field where an element has no value", () => {
    // The table serves a column for every attribute the kind declares,
    // including ones a given element has none of. An input there would
    // invite creating a value the model never had.
    const sparse: KindElements = {
      ids: ["J1"],
      columns: [
        {
          key: "initDepth",
          label: "Initial depth",
          editable: true,
          kind: NUMBER,
          values: [null],
        },
      ],
      positions: [],
    } as KindElements;
    render(<KindTable elements={sparse} onEdit={() => {}} />);
    expect(screen.queryByLabelText("J1 Initial depth")).toBeNull();
    expect(screen.getByText("—")).toBeDefined();
  });
});

/**
 * The point of virtualising, and the thing that had no test because
 * until now a virtualised list rendered nothing at all under jsdom.
 *
 * A drainage model has thousands of conduits. This table used to mount
 * every one of them.
 */
describe("KindTable virtualisation", () => {
  const many: KindElements = {
    ids: Array.from({ length: 5000 }, (_, i) => `C${i + 1}`),
    columns: [
      {
        key: "length",
        label: "Length",
        editable: false,
        kind: NUMBER,
        values: Array.from({ length: 5000 }, (_, i) => i),
      },
    ],
    positions: [],
  } as KindElements;

  it("mounts a windowful of rows, not the whole model", () => {
    const { container } = render(<KindTable elements={many} />);
    const rows = container.querySelectorAll(
      "tbody tr[data-selected], tbody tr",
    );
    // Spacer rows are counted here too, which is why this is a bound
    // rather than an exact number — what matters is the order of
    // magnitude, and 5000 rows is what it used to be.
    expect(rows.length).toBeLessThan(200);
    expect(rows.length).toBeGreaterThan(1);
  });

  it("keeps the scrollbar the height of the whole model", () => {
    // The spacer rows above and below the window stand in for the rows
    // that are not mounted. Without them the table is as tall as its
    // window and the scrollbar reaches the end after one screen.
    const { container } = render(<KindTable elements={many} />);
    const spacers = [...container.querySelectorAll("tbody tr[aria-hidden]")];
    const total = spacers.reduce(
      (sum, tr) => sum + Number.parseInt((tr as HTMLElement).style.height, 10),
      0,
    );
    // 5000 rows at 30px, less the mounted window.
    expect(total).toBeGreaterThan(100_000);
  });

  it("reveals a row that is nowhere near the top", async () => {
    const { container, rerender } = render(
      <KindTable elements={many} activeId="C4000" revealToken={1} />,
    );
    const scroller = container.querySelector<HTMLElement>(
      "div[style*='overflow']",
    );
    if (!scroller) throw new Error("no scroll container");
    rerender(<KindTable elements={many} activeId="C4000" revealToken={2} />);
    // Scrolled by arithmetic, because the row is not mounted and cannot
    // be scrolled into view by asking it to. On the next frame, so that
    // the rows the token cleared the search for have been laid out.
    await vi.waitFor(() => expect(scroller.scrollTop).toBeGreaterThan(100_000));
  });
});

/**
 * The columns that answer the question this whole contract was written
 * for: a drainage junction's position is a line in a section the engine
 * preserves verbatim and models not at all, so it appears in no
 * attribute schema — and a table that could only show attributes could
 * not show where anything was. The water-distribution tables have had X
 * and Y columns all along, because that engine happens to store them as
 * fields.
 */
describe("KindTable positions", () => {
  const placed: KindElements = {
    ids: ["J1", "J2"],
    columns: [
      {
        key: "invert",
        label: "Invert",
        editable: false,
        kind: NUMBER,
        values: [12, 4],
      },
    ],
    positions: [[10, 20], null],
  } as KindElements;

  it("shows a column per axis when the kind is somewhere", () => {
    const { container } = render(<KindTable elements={placed} />);
    const headers = [...container.querySelectorAll("th")].map((h) =>
      (h.textContent ?? "").trim(),
    );
    expect(headers[1]).toContain("X");
    expect(headers[2]).toContain("Y");
  });

  it("shows no such columns for a kind that is nowhere", () => {
    const { container } = render(<KindTable elements={junctions} />);
    const headers = [...container.querySelectorAll("th")].map((h) =>
      (h.textContent ?? "").trim(),
    );
    expect(headers.some((h) => h.startsWith("X"))).toBe(false);
  });

  it("says nowhere rather than zero", () => {
    // The origin is a place. An element the model places nowhere has to
    // read as unplaced, or someone will believe the 0.
    render(<KindTable elements={placed} onMove={() => {}} />);
    expect(screen.queryByLabelText("J2 X")).toBeNull();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("moves an element by the axis that changed, keeping the other", () => {
    const onMove = vi.fn();
    render(<KindTable elements={placed} onMove={onMove} />);
    const y = screen.getByLabelText("J1 Y");
    fireEvent.change(y, { target: { value: "99" } });
    fireEvent.blur(y);
    // A move is one operation taking both coordinates, so editing Y has
    // to carry X along unchanged rather than sending a partial position.
    expect(onMove).toHaveBeenCalledWith("J1", 10, 99);
  });

  it("reads only when nothing is listening for a move", () => {
    render(<KindTable elements={placed} />);
    expect(screen.queryByLabelText("J1 X")).toBeNull();
    expect(screen.getByText("10")).toBeDefined();
  });

  it("sorts by a position column", () => {
    const { container } = render(<KindTable elements={placed} />);
    const [, xHeader] = [...container.querySelectorAll("th")];
    if (!xHeader) throw new Error("no X header");
    fireEvent.click(sortButton(xHeader));
    // The unplaced element sorts as absent, not as the origin.
    const first = container.querySelector("tbody tr td");
    expect(first?.textContent).toBe("J2");
  });
});

/**
 * A table of elements is where a reader finds one, so what they then
 * want to do to it belongs on the row rather than behind a selection
 * and a trip elsewhere. Each action appears only when its handler is
 * given, so an engine that cannot do one shows nothing for it.
 */
describe("KindTable row actions", () => {
  it("offers only the actions it was given", () => {
    render(<KindTable elements={junctions} onDelete={() => {}} />);
    expect(screen.getAllByLabelText("Delete").length).toBeGreaterThan(0);
    expect(screen.queryByLabelText("Rename")).toBeNull();
    expect(screen.queryByLabelText("Show on map")).toBeNull();
  });

  it("spends no column on actions when there are none", () => {
    // A permanently empty trailing column is width taken from the data
    // for nothing.
    const { container } = render(<KindTable elements={junctions} />);
    expect(container.querySelector('th[aria-label="Actions"]')).toBeNull();
  });

  it("acts on the row it sits in", () => {
    const onRename = vi.fn();
    render(<KindTable elements={junctions} onRename={onRename} />);
    const buttons = screen.getAllByLabelText("Rename");
    fireEvent.click(buttons[1]);
    expect(onRename).toHaveBeenCalledWith("J2");
  });

  it("does not select the row it acts on", () => {
    // Clicking an action is not choosing the row: the click stops at
    // the button, or every delete would also move the selection to the
    // element being removed.
    const onSelect = vi.fn();
    const onDelete = vi.fn();
    render(
      <KindTable
        elements={junctions}
        onSelect={onSelect}
        onDelete={onDelete}
      />,
    );
    fireEvent.click(screen.getAllByLabelText("Delete")[0]);
    expect(onDelete).toHaveBeenCalledWith("J1");
    expect(onSelect).not.toHaveBeenCalled();
  });
});

/**
 * A column whose value names another element (§4.5.1.1) offers the ids
 * that exist. Without it a reference is a box to type a name into,
 * where the names are the model's own and a typo produces a reference
 * to nothing.
 */
describe("KindTable references", () => {
  const withRef: KindElements = {
    ids: ["J1"],
    columns: [
      {
        key: "demandPattern",
        label: "Demand pattern",
        editable: true,
        kind: { type: "text", default: null },
        references: "pattern",
        values: ["P1"],
      },
    ],
    positions: [],
  } as KindElements;

  it("offers the ids of the kind the column names", () => {
    const { container } = render(
      <KindTable
        elements={withRef}
        onEdit={() => {}}
        referenceIds={{ pattern: ["P1", "P2"] }}
      />,
    );
    const options = [...container.querySelectorAll("datalist option")].map(
      (o) => o.getAttribute("value"),
    );
    expect(options).toEqual(["P1", "P2"]);
    expect(
      screen.getByLabelText("J1 Demand pattern").getAttribute("list"),
    ).toBeTruthy();
  });

  it("stays a plain field when there is nothing to offer", () => {
    // A reference with no list is still typeable, and the engine still
    // refuses a name that means nothing.
    const { container } = render(
      <KindTable elements={withRef} onEdit={() => {}} />,
    );
    expect(container.querySelector("datalist")).toBeNull();
    expect(
      screen.getByLabelText("J1 Demand pattern").getAttribute("list"),
    ).toBeNull();
  });

  it("drops the list rather than truncating it when it is too long", () => {
    // A shortened list silently hides valid ids while still looking
    // authoritative, and the browser's own filter is the bottleneck at
    // that size anyway.
    const many = Array.from({ length: 5001 }, (_, i) => `P${i}`);
    const { container } = render(
      <KindTable
        elements={withRef}
        onEdit={() => {}}
        referenceIds={{ pattern: many }}
      />,
    );
    expect(container.querySelector("datalist")).toBeNull();
  });

  it("draws one list per column, not one per row", () => {
    // A copy per row is tens of thousands of option nodes rebuilt on
    // every scroll, which hangs the tab outright at model scale.
    const rows: KindElements = {
      ...withRef,
      ids: ["J1", "J2", "J3"],
      columns: [{ ...withRef.columns[0], values: ["P1", "P2", "P1"] }],
    };
    const { container } = render(
      <KindTable
        elements={rows}
        onEdit={() => {}}
        referenceIds={{ pattern: ["P1", "P2"] }}
      />,
    );
    expect(container.querySelectorAll("datalist")).toHaveLength(1);
  });
});
