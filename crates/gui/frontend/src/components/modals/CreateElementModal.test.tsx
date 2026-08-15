/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ElementKindInfo } from "../../hooks";
import { clearAllStacks, getUndoStacks, stackKey } from "../../hooks/undoStack";
import { CreateElementModal, offeredKinds } from "./CreateElementModal";

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
    group: "Nodes",
    role: "conveyance",
    creatable: true,
  },
  // A family that crosses classes, which is the case the dialog has to
  // follow the *choice* for: both are put at a coordinate, and only one
  // of them is a point.
  {
    id: "raingage",
    label: "Rain gage",
    labelPlural: "Rain gages",
    class: "point",
    badge: "RG",
    group: "Rainfall and runoff",
    creatable: true,
  },
  {
    id: "subcatchment",
    label: "Subcatchment",
    labelPlural: "Subcatchments",
    class: "region",
    badge: "SC",
    group: "Rainfall and runoff",
    creatable: true,
  },
  {
    id: "tank",
    label: "Tank",
    labelPlural: "Tanks",
    class: "point",
    badge: "TK",
    group: "Nodes",
    role: "boundary",
    creatable: true,
  },
  {
    id: "curve",
    label: "Curve",
    labelPlural: "Curves",
    class: "collection",
    badge: "C",
    creatable: true,
  },
  {
    id: "rule",
    label: "Rule",
    labelPlural: "Rules",
    class: "collection",
    badge: "R",
    creatable: false,
  },
  {
    id: "subcatchment",
    label: "Subcatchment",
    labelPlural: "Subcatchments",
    class: "region",
    badge: "SC",
    creatable: true,
  },
  {
    id: "pipe",
    label: "Pipe",
    labelPlural: "Pipes",
    class: "polyline",
    badge: "P",
    creatable: true,
  },
  {
    id: "pump",
    label: "Pump",
    labelPlural: "Pumps",
    class: "polyline",
    badge: "PU",
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
  subcatchment: [
    {
      key: "raingage",
      label: "Rain gage",
      editable: true,
      kind: { type: "text", default: null },
      references: ["raingage"],
    },
    { key: "area", label: "Area", editable: true, kind: NUMBER },
    {
      key: "grateType",
      label: "Grate type",
      editable: true,
      kind: {
        type: "choice",
        default: "CURVED_VANE",
        items: [
          { value: "P_BAR-50", label: "P_BAR-50" },
          { value: "CURVED_VANE", label: "CURVED_VANE" },
        ],
      },
    },
  ],
  pipe: [{ key: "length", label: "Length", editable: true, kind: NUMBER }],
  pump: [{ key: "power", label: "Power", editable: true, kind: NUMBER }],
};

const showToast = vi.fn();
const markEdited = vi.fn();
const persistOrSay = vi.fn(
  (_id: string, _scenarioId: string | null, _toast: (m: string) => void) =>
    Promise.resolve(),
);

vi.mock("../../AppContext", () => ({
  useActiveProject: () => ({ project: { id: "p1", engine: "wds" } }),
  useAppState: () => ({ activeScenarioId: null, showToast }),
}));
vi.mock("../../hooks/NetworkVersionContext", () => ({
  useNetworkVersion: () => ({ markEdited }),
}));
vi.mock("../../hooks/projects", () => ({
  persistOrSay: (
    id: string,
    scenarioId: string | null,
    toast: (m: string) => void,
  ) => persistOrSay(id, scenarioId, toast),
}));
// Partial: `EditableNumber` reaches through the same barrel for the
// format/parse pair, and mocking those would be mocking the thing under
// test's arithmetic.
vi.mock("../../hooks", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  createElement: vi.fn(() => Promise.resolve()),
  useElementKinds: () => KINDS,
  useElementAttributes: (_engine: string, kind: string) => SCHEMA[kind] ?? [],
  // Faithful to the real hook, which answers only for the kinds it was
  // asked about — a mock that returned everything let a link's end list
  // silently include rain gages.
  useReferenceIds: (_p: unknown, _s: unknown, kinds: string[]) =>
    Object.fromEntries(
      Object.entries({
        junction: ["J1", "J2"],
        tank: ["T1"],
        raingage: ["G1"],
      }).filter(([k]) => kinds.includes(k)),
    ),
}));
vi.mock("../../units", () => ({ useUnitSystem: () => "si" }));

const props = {
  open: true,
  suggestId: (kind: string) => `${kind[0].toUpperCase()}1`,
  position: null,
  onCreated: vi.fn(),
  onCancel: vi.fn(),
};

/** One catalog entry, as the engine publishes it. */
function k(
  id: string,
  label: string,
  klass: ElementKindInfo["class"],
  group: string,
): ElementKindInfo {
  return {
    id,
    label,
    labelPlural: `${label}s`,
    class: klass,
    badge: label.slice(0, 2),
    group,
    creatable: true,
  } as ElementKindInfo;
}

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

  it("opens on the kind the caller was looking at", () => {
    // Pressing Add on the weirs table and being handed a conduit is the
    // dialog answering a question nobody asked.
    // A pump is not the first polyline in this catalog — a pipe is — so
    // the assertion fails if the dialog falls back to the first.
    render(<CreateElementModal {...props} klass="polyline" kind="pump" />);
    expect(screen.getByLabelText("Power")).toBeDefined();
    expect(screen.queryByLabelText("Length")).toBeNull();
  });

  it("opens on the first when nobody said which", () => {
    // A click on empty map has said nothing, so the first of the class
    // is the right answer.
    render(<CreateElementModal {...props} />);
    expect(screen.getByLabelText("Elevation")).toBeDefined();
  });

  it("ignores a kind of another class", () => {
    // The class decides what the dialog is offering; a stale kind from
    // another table must not select nothing at all.
    render(<CreateElementModal {...props} klass="polyline" kind="junction" />);
    expect(screen.getByLabelText("Length")).toBeDefined();
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

  it("asks for a reference the kind requires, with the model's own ids", () => {
    // The change that unlocked half a catalog. A subcatchment must name a
    // rain gage and an outlet, and the model holds both as indices with
    // no value meaning "not yet chosen" — so unlike a number they cannot
    // be left to the edit that follows the create.
    render(<CreateElementModal {...props} klass="region" />);
    const gage = screen.getByLabelText("Rain gage") as HTMLInputElement;
    expect(gage).toBeDefined();
    const list = document.getElementById(gage.getAttribute("list") ?? "");
    expect(
      [...(list?.querySelectorAll("option") ?? [])].map((o) => o.value),
    ).toEqual(["G1"]);
    // Numbers still appear beside it.
    expect(screen.getByLabelText("Area")).toBeDefined();
  });

  it("asks for a choice, starting at the default the engine declared", () => {
    // The reasoning that once excluded choices — a choice always has a
    // value, so a create can leave it alone — holds for an element that
    // exists and fails for one being built: a drainage inlet's grate
    // family decides what the engine constructs.
    render(<CreateElementModal {...props} klass="region" />);
    const family = screen.getByLabelText("Grate type") as HTMLSelectElement;
    expect(family.value).toBe("CURVED_VANE");
    expect([...family.options].map((o) => o.value)).toEqual([
      "P_BAR-50",
      "CURVED_VANE",
    ]);
  });

  it("asks for nothing but a name when adding a container", () => {
    // A curve is not anywhere and runs between nothing. Its contents are
    // the point of it, and those are edited in the panel below the
    // table — which is what made creating one possible at all.
    render(<CreateElementModal {...props} klass="collection" />);
    expect(screen.getByText("Curve")).toBeDefined();
    expect(screen.queryByLabelText("X")).toBeNull();
    expect(screen.queryByLabelText("From")).toBeNull();
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
    // One list per end, each offering every point that is part of the
    // flow network, whatever kind it is — a pipe may run to a tank as
    // readily as to a junction. A rain gage is a point and is not one of
    // them: a link cannot reach it, and offering `G1` here was offering
    // a choice the create always refused.
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

  it("switches what it asks for when the chosen kind is placed differently", () => {
    // A family may cross classes — a rain gage is a point and a
    // subcatchment a region, and both are put at a coordinate — so the
    // dialog follows the choice rather than the class it was opened
    // with. Reading the placing from the prop would have left a
    // subcatchment chosen here sending a point's payload.
    render(<CreateElementModal {...props} klass="point" kind="raingage" />);
    expect(screen.getByText("Rain gage")).toBeDefined();
    expect(screen.getByText("Subcatchment")).toBeDefined();
    // Both are placed at a coordinate, so the coordinate is asked for
    // either way and no end fields appear.
    expect(screen.getByLabelText("X")).toBeDefined();
    expect(screen.queryByLabelText("From")).toBeNull();

    fireEvent.click(screen.getByText("Subcatchment"));
    expect(screen.getByLabelText("X")).toBeDefined();
    expect(screen.queryByLabelText("From")).toBeNull();
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

describe("offeredKinds", () => {
  // The engine's own catalog, in the shape the complaint is about: a
  // rain gage is a point, and it belongs with the subcatchments rather
  // than with the nodes.
  const CATALOG: ElementKindInfo[] = [
    k("junction", "Junction", "point", "Nodes"),
    k("outfall", "Outfall", "point", "Nodes"),
    k("storage", "Storage unit", "point", "Nodes"),
    k("raingage", "Rain gage", "point", "Rainfall and runoff"),
    k("subcatchment", "Subcatchment", "region", "Rainfall and runoff"),
    k("conduit", "Conduit", "polyline", "Links"),
  ];

  it("offers the family the dialog opened on, not the geometry", () => {
    // The complaint: a shared class is not a family. The rail lists a
    // rain gage under Rainfall and runoff and the type row put it beside
    // Junction, which is two organising principles for one catalog.
    expect(
      offeredKinds(CATALOG, "point", "junction").map((o) => o.value),
    ).toEqual(["junction", "outfall", "storage"]);
  });

  it("offers a family whole where it can place it the same way", () => {
    // A gage is a point and a subcatchment a region, and both are put at
    // a coordinate — so the rail's own heading implies one dialog and
    // this is it. Reached from either side.
    expect(
      offeredKinds(CATALOG, "point", "raingage").map((o) => o.value),
    ).toEqual(["raingage", "subcatchment"]);
    expect(
      offeredKinds(CATALOG, "region", "subcatchment").map((o) => o.value),
    ).toEqual(["raingage", "subcatchment"]);
  });

  it("never mixes two ways of placing something", () => {
    // A form that asked for a coordinate and two ends at once would be
    // asking about two different things. The conduit shares no group
    // here, but the rule is about the placing rather than the group.
    const linked = [
      ...CATALOG,
      k("gutter", "Gutter", "polyline", "Rainfall and runoff"),
    ];
    expect(
      offeredKinds(linked, "point", "raingage").map((o) => o.value),
    ).toEqual(["raingage", "subcatchment"]);
    expect(
      offeredKinds(linked, "polyline", "gutter").map((o) => o.value),
    ).toEqual(["gutter"]);
  });

  it("offers everything placeable when nothing named a family", () => {
    // A click on the map names a place and not a family, so the class is
    // all there is to go on.
    expect(offeredKinds(CATALOG, "point").map((o) => o.value)).toEqual([
      "junction",
      "outfall",
      "storage",
      "raingage",
    ]);
  });

  it("leaves out what cannot be created", () => {
    const withRefusal = [
      ...CATALOG,
      {
        ...k("rule", "Control rule", "collection", "Controls"),
        creatable: false,
      },
    ];
    expect(
      offeredKinds(withRefusal, "collection", "rule").map((o) => o.value),
    ).toEqual([]);
  });
});

/**
 * What pressing Add actually does.
 *
 * Nothing here reached the submit before, and that is how the Editor's
 * Add shipped writing the element into the loaded model and doing
 * nothing else: no save, so it was gone at the next open, and no stale
 * mark, so the results on screen went on describing a model that no
 * longer existed. The canvas's Add did both, in its own caller, where
 * the other caller could not inherit them — and did not.
 *
 * All four consequences belong to the write, so they are asserted on the
 * dialog that performs it rather than on either surface that opens it.
 */
describe("pressing Add", () => {
  beforeEach(() => {
    showToast.mockClear();
    markEdited.mockClear();
    persistOrSay.mockClear();
    clearAllStacks();
  });

  async function add() {
    render(<CreateElementModal {...props} klass="point" kind="junction" />);
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    // The submit is a chain of awaits; let it drain.
    await act(async () => {});
  }

  it("saves the model to disk", async () => {
    // The one that loses work. A create that only reaches the loaded
    // model is gone at the next open, and nothing on screen says so.
    await add();
    expect(persistOrSay).toHaveBeenCalledWith("p1", null, showToast);
  });

  it("marks the results stale", async () => {
    // The model on disk and the results beside it now disagree, and a
    // result that does not say it is out of date reads as current.
    await add();
    expect(markEdited).toHaveBeenCalledWith("p1", null);
  });

  it("captures the addition for undo", async () => {
    await add();
    const { undo } = getUndoStacks(stackKey("p1", null));
    expect(undo).toHaveLength(1);
    expect(undo[0].label).toBe("Added J1");
    expect(undo[0].undo.ops).toEqual([
      { op: "remove", kind: "junction", id: "J1" },
    ]);
  });

  it("tells the surface what was added, so it can be selected", async () => {
    await add();
    expect(props.onCreated).toHaveBeenCalledWith("junction", "J1");
  });
});
