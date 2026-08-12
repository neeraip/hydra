/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { KindElements } from "../../hooks";
import { KindTable } from "./KindTable";

const junctions: KindElements = {
  ids: ["J1", "J2"],
  columns: [{ key: "invert", label: "Invert", values: [12, 4] }],
} as KindElements;

const conduits: KindElements = {
  ids: ["C1", "C2"],
  columns: [{ key: "length", label: "Length", values: [30, 10] }],
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
    render(<KindTable elements={{ ids: [], columns: [] } as KindElements} />);
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
    fireEvent.click(idHeader);
    const ascending = idHeader.querySelector("svg")?.innerHTML;
    expect(ascending).not.toBe(neutral);
    // The column that is not sorted keeps the neutral mark.
    expect(invertHeader?.querySelector("svg")?.innerHTML).toBe(neutral);

    fireEvent.click(idHeader);
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
      { key: "invert", label: "Invert", editable: true, values: [12, 4] },
      {
        key: "shape",
        label: "Shape",
        editable: false,
        values: ["CIRCULAR", null],
      },
    ],
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
    expect(onEdit).toHaveBeenCalledWith("J2", "invert", 7);
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
    fireEvent.click(invertHeader); // ascending: J2 (4) first
    const first = container.querySelector("tbody tr");
    expect(first?.querySelector("td")?.textContent).toBe("J2");
    const cell = screen.getByLabelText("J2 Invert");
    fireEvent.change(cell, { target: { value: "5" } });
    fireEvent.blur(cell);
    expect(onEdit).toHaveBeenCalledWith("J2", "invert", 5);
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
          values: [null],
        },
      ],
    } as KindElements;
    render(<KindTable elements={sparse} onEdit={() => {}} />);
    expect(screen.queryByLabelText("J1 Initial depth")).toBeNull();
    expect(screen.getByText("—")).toBeDefined();
  });
});
