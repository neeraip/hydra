/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CollectionDetail as Detail } from "../../../hooks";
import { CollectionDetail } from "./CollectionDetail";

const empty: Detail = {
  columns: [],
  quantities: [],
  rows: [],
  lines: [],
  editable: false,
};

describe("CollectionDetail", () => {
  // The engine names the axes because what they *are* depends on the
  // container: a storage curve relates depth to area, a rating curve head
  // to discharge. "X" and "Y" would be two anonymous magnitudes.
  it("shows the engine's column names with their units", () => {
    render(
      <CollectionDetail
        elementId="ST1"
        detail={{
          ...empty,
          columns: ["Depth", "Surface area"],
          quantities: [
            { key: "depth", siLabel: "m", usLabel: "ft" },
            { key: "area", siLabel: "ha", usLabel: "ac" },
          ] as Detail["quantities"],
          rows: [[0, 100]],
        }}
      />,
    );
    expect(screen.getByText("Depth (m)")).toBeDefined();
    expect(screen.getByText("Surface area (ha)")).toBeDefined();
  });

  it("names the container it is showing", () => {
    render(<CollectionDetail elementId="ST1" detail={empty} />);
    expect(screen.getByText("ST1")).toBeDefined();
  });

  it("renders language content verbatim rather than as a table", () => {
    const { container } = render(
      <CollectionDetail
        elementId="R1"
        detail={{
          ...empty,
          lines: ["IF NODE J1 DEPTH > 2", "THEN PUMP P1 STATUS = ON"],
        }}
      />,
    );
    expect(container.querySelector("table")).toBeNull();
    expect(screen.getByText(/IF NODE J1 DEPTH/)).toBeDefined();
  });

  // An external time series' contents live in a file the engine never
  // reads. That is an answer, not a failure, and must not read as one.
  it("says plainly when there is nothing to show", () => {
    render(<CollectionDetail elementId="TS1" detail={empty} />);
    expect(screen.getByText(/Nothing to show/)).toBeDefined();
  });

  const curve: Detail = {
    ...empty,
    columns: ["Depth", "Surface area"],
    quantities: [null, null],
    rows: [
      [0, 100],
      [1, 150],
    ],
    editable: true,
  };

  it("sends the whole table when one cell changes", () => {
    // The rule §4.5.2.2 exists for: rows are ordered and interdependent,
    // so a per-cell write would have to be valid mid-sequence and half
    // of the useful edits are not.
    const onWrite = vi.fn(() => Promise.resolve());
    render(
      <CollectionDetail elementId="ST1" detail={curve} onWrite={onWrite} />,
    );
    const cell = screen.getByLabelText(
      "ST1 row 2 Surface area",
    ) as HTMLInputElement;
    fireEvent.change(cell, { target: { value: "175" } });
    fireEvent.blur(cell);
    expect(onWrite).toHaveBeenCalledWith([
      [0, 100],
      [1, 175],
    ]);
  });

  it("adds and removes a row by sending the table that results", () => {
    const onWrite = vi.fn(() => Promise.resolve());
    const { rerender } = render(
      <CollectionDetail elementId="ST1" detail={curve} onWrite={onWrite} />,
    );
    // A new row lands at the end, not in sorted position: where a point
    // belongs is the modeller's judgement.
    fireEvent.click(screen.getByLabelText("Add row"));
    expect(onWrite).toHaveBeenCalledWith([
      [0, 100],
      [1, 150],
      [0, 0],
    ]);

    rerender(
      <CollectionDetail elementId="ST1" detail={curve} onWrite={onWrite} />,
    );
    fireEvent.click(screen.getAllByLabelText("Remove row")[0]);
    expect(onWrite).toHaveBeenLastCalledWith([[1, 150]]);
  });

  it("reads only when the engine did not mark the contents editable", () => {
    // A control rule is language. Rewriting it means parsing that
    // language with the engine's own reader, so it is shown to be read.
    const onWrite = vi.fn();
    render(
      <CollectionDetail
        elementId="ST1"
        detail={{ ...curve, editable: false }}
        onWrite={onWrite}
      />,
    );
    expect(screen.queryByLabelText("ST1 row 1 Depth")).toBeNull();
    expect(screen.queryByLabelText("Add row")).toBeNull();
    // Still read, though: the values are shown, just not offered.
    expect(screen.getByText(/100/)).toBeDefined();
  });

  it("shows a refusal beside the table it is about", async () => {
    // Not a toast: "a curve's first column has to increase" is about the
    // column above it, and a notification that slides away takes the
    // reason with it.
    const onWrite = vi.fn(() =>
      Promise.reject("a curve's first column has to increase"),
    );
    render(
      <CollectionDetail elementId="ST1" detail={curve} onWrite={onWrite} />,
    );
    fireEvent.click(screen.getByLabelText("Add row"));
    expect(
      await screen.findByText(/first column has to increase/),
    ).toBeDefined();
  });
});
