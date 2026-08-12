/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CreateElementModal } from "./CreateElementModal";

/**
 * The water-distribution Add button did nothing. The button asked the
 * catalog whether the kind could be created and the catalog said yes,
 * while the dialog behind it was hardcoded to one engine's two kinds
 * and one field — so for every other engine there was nothing to open.
 *
 * Everything the dialog offers comes from the catalogs now, which is
 * what these pin: kinds from §4.2, fields from §4.4.
 */

const KINDS = [
  {
    id: "junction",
    label: "Junction",
    labelPlural: "Junctions",
    class: "point",
    badge: "J",
    creatable: true,
  },
  {
    id: "tank",
    label: "Tank",
    labelPlural: "Tanks",
    class: "point",
    badge: "TK",
    creatable: true,
  },
  {
    id: "curve",
    label: "Curve",
    labelPlural: "Curves",
    class: "collection",
    badge: "C",
    creatable: false,
  },
  {
    id: "pipe",
    label: "Pipe",
    labelPlural: "Pipes",
    class: "polyline",
    badge: "P",
    creatable: true,
  },
];

const NUMBER = { type: "number", default: null, min: null, max: null };

const SCHEMA: Record<string, unknown[]> = {
  junction: [
    { key: "elevation", label: "Elevation", editable: true, kind: NUMBER },
    {
      key: "demandPattern",
      label: "Demand pattern",
      editable: true,
      kind: { type: "text", default: null },
    },
  ],
  tank: [{ key: "diameter", label: "Diameter", editable: true, kind: NUMBER }],
  pipe: [{ key: "length", label: "Length", editable: true, kind: NUMBER }],
};

vi.mock("../../AppContext", () => ({
  useActiveProject: () => ({ project: { id: "p1", engine: "wds" } }),
  useAppState: () => ({ activeScenarioId: null }),
}));
// Partial: `EditableNumber` reaches through the same barrel for the
// format/parse pair, and mocking those would be mocking the thing under
// test's arithmetic.
vi.mock("../../hooks", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createElement: vi.fn(() => Promise.resolve()),
  useElementKinds: () => KINDS,
  useElementAttributes: (_engine: string, kind: string) => SCHEMA[kind] ?? [],
  useReferenceIds: () => ({ junction: ["J1", "J2"], tank: ["T1"] }),
}));
vi.mock("../../units", () => ({ useUnitSystem: () => "si" }));

const props = {
  open: true,
  suggestId: (kind: string) => `${kind[0].toUpperCase()}1`,
  position: null,
  onCreated: vi.fn(),
  onCancel: vi.fn(),
};

describe("CreateElementModal", () => {
  it("offers the kinds the catalog says can be created", () => {
    render(<CreateElementModal {...props} />);
    expect(screen.getByRole("button", { name: "Junction" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Tank" })).toBeDefined();
    // Not creatable, and not a class a position places.
    expect(screen.queryByRole("button", { name: "Curve" })).toBeNull();
    // A polyline is a different class and is added by naming its two
    // ends; this dialog is showing the point kinds.
    expect(screen.queryByRole("button", { name: "Pipe" })).toBeNull();
  });

  it("asks for the chosen kind's own editable numbers", () => {
    render(<CreateElementModal {...props} />);
    expect(screen.getByLabelText("Elevation")).toBeDefined();
    // Text and choices keep the engine's defaults and are changed
    // afterwards in the table, where they have their proper editors.
    expect(screen.queryByLabelText("Demand pattern")).toBeNull();
  });

  it("asks the next kind's fields when the kind changes", () => {
    render(<CreateElementModal {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Tank" }));
    expect(screen.getByLabelText("Diameter")).toBeDefined();
    expect(screen.queryByLabelText("Elevation")).toBeNull();
  });

  it("asks where to put it when nobody could say", () => {
    render(<CreateElementModal {...props} />);
    expect(screen.getByLabelText("X")).toBeDefined();
    expect(screen.getByLabelText("Y")).toBeDefined();
  });

  it("does not ask again when a click already answered", () => {
    render(<CreateElementModal {...props} position={[5, 6]} />);
    expect(screen.queryByLabelText("X")).toBeNull();
  });

  it("asks for the two ends when adding a link", () => {
    // The reason a link can be added from a table at all: the ends are
    // named rather than drawn, with the model's own ids offered.
    render(<CreateElementModal {...props} klass="polyline" />);
    // One creatable polyline in this catalog, so it is stated rather
    // than offered as a row of one button.
    expect(screen.getByText("Pipe")).toBeDefined();
    expect(screen.getByLabelText("From")).toBeDefined();
    expect(screen.getByLabelText("To")).toBeDefined();
    // One list per end, each offering every point in the model whatever
    // kind it is — a pipe may run to a tank as readily as to a junction.
    const lists = [...document.querySelectorAll("datalist")];
    expect(lists).toHaveLength(2);
    for (const list of lists) {
      const options = [...list.querySelectorAll("option")].map((o) =>
        o.getAttribute("value"),
      );
      expect(options).toEqual(["J1", "J2", "T1"]);
    }
    // A link is placed by its ends, not by a coordinate.
    expect(screen.queryByLabelText("X")).toBeNull();
  });

  it("starts a drawn link's length at the distance drawn", () => {
    // The one number the gesture itself measured. Losing it when the
    // link modal became the shared dialog would have made every pipe
    // drawn on the map start at zero length.
    render(
      <CreateElementModal
        {...props}
        klass="polyline"
        fromNodeId="J1"
        toNodeId="J2"
        spanLength={42.5}
      />,
    );
    expect(screen.getByLabelText("Length")).toHaveProperty("value", "42.5");
  });

  it("does not let a drawn line's ends be changed", () => {
    // The line on screen has answered this; an editable field would
    // invite disagreeing with it.
    render(
      <CreateElementModal
        {...props}
        klass="polyline"
        fromNodeId="J1"
        toNodeId="J2"
      />,
    );
    expect(screen.getByLabelText("From")).toHaveProperty("readOnly", true);
    expect(screen.getByLabelText("From")).toHaveProperty("value", "J1");
  });
});
