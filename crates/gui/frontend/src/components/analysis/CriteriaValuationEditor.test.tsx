/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CriteriaValuationEditor } from "./CriteriaValuationEditor";
import type { Criterion } from "./criteria";

/**
 * The descriptor-driven editor: what renders comes from the catalog, and
 * what an edit emits is SI — the two halves of the contract an engine's
 * criteria travel through.
 */

const CATALOG: Criterion[] = [
  {
    key: "freeboard",
    label: "Freeboard",
    help: "Clearance below the rim.",
    quantity: {
      key: "depth",
      siLabel: "m",
      usLabel: "ft",
      siToUsScale: 3.28084,
      siToUsOffset: 0,
      siDecimals: 2,
      usDecimals: 2,
    },
    kind: { type: "value", default: 0.3 },
  },
  {
    key: "velocity",
    label: "Velocity",
    help: "Self-cleansing to erosive.",
    quantity: {
      key: "velocity",
      siLabel: "m/s",
      usLabel: "ft/s",
      siToUsScale: 3.28084,
      siToUsOffset: 0,
      siDecimals: 2,
      usDecimals: 2,
    },
    kind: {
      type: "band",
      cuts: [
        { key: "selfCleansing", label: "Self-cleansing", default: 0.6 },
        { key: "erosive", label: "Erosive", default: 3 },
      ],
    },
  },
];

const VALUES = { freeboard: 0.3, velocity: [0.6, 3] };

describe("CriteriaValuationEditor", () => {
  it("renders every cataloged criterion with its unit and cut labels", () => {
    render(
      <CriteriaValuationEditor
        catalog={CATALOG}
        values={VALUES}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByText(/Freeboard \(m\)/)).toBeTruthy();
    expect(screen.getByText(/Velocity \(m\/s\)/)).toBeTruthy();
    expect(screen.getByText("Self-cleansing")).toBeTruthy();
    expect(screen.getByText("Erosive")).toBeTruthy();
    expect(screen.getAllByRole("spinbutton")).toHaveLength(3);
  });

  it("edits emit SI values, bands one cut at a time", () => {
    const onChange = vi.fn();
    render(
      <CriteriaValuationEditor
        catalog={CATALOG}
        values={VALUES}
        onChange={onChange}
      />,
    );
    const inputs = screen.getAllByRole("spinbutton");
    fireEvent.change(inputs[0], { target: { value: "0.5" } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ freeboard: 0.5 }),
    );
    fireEvent.change(inputs[2], { target: { value: "4" } });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ velocity: [0.6, 4] }),
    );
  });

  it("offers a reset only when the standard deviates from the defaults", () => {
    const { rerender } = render(
      <CriteriaValuationEditor
        catalog={CATALOG}
        values={VALUES}
        onChange={vi.fn()}
      />,
    );
    expect(screen.queryByText("Reset all")).toBeNull();
    const onChange = vi.fn();
    rerender(
      <CriteriaValuationEditor
        catalog={CATALOG}
        values={{ freeboard: 1, velocity: [0.6, 3] }}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("Reset all"));
    expect(onChange).toHaveBeenCalledWith(VALUES);
  });
});
