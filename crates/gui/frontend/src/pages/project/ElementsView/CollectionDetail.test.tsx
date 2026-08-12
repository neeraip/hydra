/**
 * @vitest-environment jsdom
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CollectionDetail as Detail } from "../../../hooks";
import { CollectionDetail } from "./CollectionDetail";

const empty: Detail = { columns: [], quantities: [], rows: [], lines: [] };

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
});
