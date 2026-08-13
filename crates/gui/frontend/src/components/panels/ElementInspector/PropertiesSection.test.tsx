/**
 * @vitest-environment jsdom
 */
/**
 * The inspector's Properties rows, which fell behind the Editor's table
 * twice over.
 *
 * The water-distribution body wrote its own rows out of the network
 * snapshot — hardcoded labels, hardcoded units, read-only — so a
 * junction offered every property for editing in the table and none of
 * them here. The drainage body did read the schema, but decided
 * editability with a rule that said text is never editable, so a tag and
 * an outlet took an input in one surface and not the other.
 *
 * Both are the same defect: one value giving two answers about itself.
 * These assert the rows this section actually offers, which is the only
 * place that shows.
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ElementAttribute } from "../../../hooks";
import { PropertiesSection } from "./PropertiesSection";

const NUMBER = { type: "number", default: null, min: null, max: null } as const;
const TEXT = { type: "text", default: null } as const;

const ROWS: ElementAttribute[] = [
  {
    key: "invert",
    label: "Invert",
    editable: true,
    number: 12.5,
    kind: NUMBER,
  },
  { key: "tag", label: "Tag", editable: true, text: "Zone A", kind: TEXT },
  {
    key: "outlet",
    label: "Outlet",
    editable: true,
    text: "J1",
    kind: TEXT,
    references: ["junction", "subcatchment"],
  },
  {
    key: "shape",
    label: "Shape",
    editable: false,
    text: "CIRCULAR",
    kind: TEXT,
  },
];

vi.mock("../../../AppContext", () => ({
  useActiveProject: () => ({ project: { id: "p1", engine: "uds" } }),
  useAppState: () => ({ activeScenarioId: null }),
}));
vi.mock("../../../hooks", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useElementAttributes: () => [],
  useReferenceIds: () => ({ junction: ["J1", "J2"], subcatchment: ["S1"] }),
  getElementDetails: () => Promise.resolve(null),
}));
vi.mock("../../../hooks/useAttributeWrite", () => ({
  useElementAttributeWrite: () => vi.fn(() => Promise.resolve()),
}));
vi.mock("../../../units", () => ({ useUnitSystem: () => "si" }));
// The records section fetches on its own; it has its own tests.
vi.mock("./RecordSets", () => ({
  RecordSets: () => null,
  useElementRecords: () => ({ sets: [], refetch: () => {} }),
}));

describe("PropertiesSection", () => {
  it("offers a field for every kind of value the schema says is writable", () => {
    render(<PropertiesSection rows={ROWS} elementId="J1" />);
    // A number, and — the part that used to be refused — text.
    expect(screen.getByLabelText("Invert")).toBeDefined();
    expect(screen.getByLabelText("Tag")).toHaveProperty("value", "Zone A");
    expect(screen.getByLabelText("Outlet")).toHaveProperty("value", "J1");
  });

  it("does not offer one for a row the engine will not write", () => {
    render(<PropertiesSection rows={ROWS} elementId="J1" />);
    expect(screen.queryByLabelText("Shape")).toBeNull();
    // Still read, though: editing arriving must not cost a row its value.
    expect(screen.getByText("CIRCULAR")).toBeDefined();
  });

  it("offers the ids of every kind a reference may name", () => {
    // The §4.5.1.1 widening reaching the panel: a subcatchment's outlet
    // names a conveyance node or another subcatchment, and a list that
    // showed one of the two would look complete while hiding the rest.
    const { container } = render(
      <PropertiesSection rows={ROWS} elementId="J1" />,
    );
    const lists = [...container.querySelectorAll("datalist")];
    expect(lists).toHaveLength(1);
    expect(
      [...lists[0].querySelectorAll("option")].map((o) => o.value),
    ).toEqual(["J1", "J2", "S1"]);
    expect(screen.getByLabelText("Outlet").getAttribute("list")).toBe(
      lists[0].id,
    );
    // The tag is text but not a reference, so it gets no list at all.
    expect(screen.getByLabelText("Tag").getAttribute("list")).toBeNull();
  });

  it("reads only when there is no element to address", () => {
    // What a caller with nothing to write to gets — every value shown,
    // no input anywhere.
    render(<PropertiesSection rows={ROWS} />);
    expect(screen.queryByLabelText("Invert")).toBeNull();
    expect(screen.getByText("Zone A")).toBeDefined();
  });
});
