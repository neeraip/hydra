/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
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
