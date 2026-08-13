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
    render(
      <CollectionDetail
        elementId="ST1"
        detail={{ ...empty, columns: ["Depth"], rows: [[0]] }}
      />,
    );
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
  it("shows the engine's reason when there is nothing to show", () => {
    render(
      <CollectionDetail
        elementId="TS1"
        detail={{ ...empty, note: "This series is read from 'rain.dat'." }}
      />,
    );
    expect(screen.getByText(/read from 'rain.dat'/)).toBeDefined();
  });

  // Six drainage kinds have no contents at all — a pollutant, a land use
  // and an aquifer are their attributes; a LID control's layers cannot be
  // read yet. Each of them drew this panel, under a sentence written for
  // the external-series case: "this entry's contents are held outside the
  // model file", which sent readers looking for a file that never existed.
  it("draws nothing for a kind that has no contents", () => {
    const { container } = render(
      <CollectionDetail elementId="TSS" detail={empty} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("still draws an empty table that can be added to", () => {
    // The headings and the add button are how the first row is entered,
    // so this empty table is an offer rather than a blank.
    render(
      <CollectionDetail
        elementId="C9"
        detail={{ ...empty, columns: ["Depth", "Area"], editable: true }}
        onWrite={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Add row")).toBeDefined();
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
    //
    // Showing it is also the whole of handling it. This test passed for a
    // while against a `send` that showed the reason and then rethrew, and
    // the rejection reached no one — every caller is an event handler that
    // drops the promise — so vitest failed the run while reporting every
    // test in it green. The assertion below is unchanged; what changed is
    // that the suite now exits zero.
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
