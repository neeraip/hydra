/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SimResultColumn } from "../../canvas/selection-context";
import type { ValueColumnHeading } from "./NetworkList";
import { NetworkListRow, type Row } from "./NetworkListRow";

/**
 * The gestures are decided by two pure functions with tests of their own,
 * but nothing asserted that the row still *asks* them. Deleting either
 * call left every one of those tests green, which is the gap this file
 * exists to close: the decisions are tested here through the markup a
 * person actually clicks.
 */

const HEADING: ValueColumnHeading = {
  perRowUnits: false,
  unitWidth: 0,
} as ValueColumnHeading;

const KIND_LABEL = new Map([
  ["junction", "Junction"],
  ["conduit", "Conduit"],
]);

function row(over: Partial<Row> = {}): Row {
  return {
    id: "J-401",
    kind: "junction",
    cls: "point",
    context: "",
    value: null,
    format: null,
    canZoom: true,
    ...over,
  };
}

function draw(over: Partial<Row> = {}, props: Record<string, unknown> = {}) {
  const onSelect = vi.fn();
  const onZoom = vi.fn();
  const onHover = vi.fn();
  const onClearHover = vi.fn();
  const view = render(
    <NetworkListRow
      row={row(over)}
      isActive={false}
      zoomable={true}
      searching={false}
      sys="si"
      valueHeading={HEADING}
      kindLabel={KIND_LABEL}
      onSelect={onSelect}
      onZoom={onZoom}
      onHover={onHover}
      onClearHover={onClearHover}
      {...props}
    />,
  );
  // The row is the first button; the zoom control is its sibling.
  const buttons = [...view.container.querySelectorAll("button")];
  return {
    ...view,
    onSelect,
    onZoom,
    onHover,
    onClearHover,
    rowButton: buttons[0],
  };
}

describe("NetworkListRow", () => {
  it("shows the element's id and its kind's badge", () => {
    draw();
    expect(screen.getByText("J-401")).toBeDefined();
    // The interface rule: a kind is shown as its glyph, never as a word
    // alone. "J" is the junction badge; "Junction" is only its tooltip.
    expect(screen.getByText("J")).toBeDefined();
    expect(screen.queryByText("Junction")).toBeNull();
  });

  it("names the kind on the badge lane's tooltip", () => {
    const { container } = draw();
    const lane = container.querySelector("[data-tooltip='Junction']");
    expect(lane).not.toBeNull();
  });

  it("falls back to the kind's own id when no label was given", () => {
    const { container } = draw({ kind: "conduit" }, { kindLabel: new Map() });
    expect(container.querySelector("[data-tooltip='conduit']")).not.toBeNull();
  });
});

describe("what a row shows for its value", () => {
  it("shows a dash before a run rather than a zero", () => {
    draw();
    expect(screen.getByText("—")).toBeDefined();
  });

  it("shows a coded value by its label, not its number", () => {
    const format = {
      label: "Status",
      codes: { 0: { label: "Closed", severity: 2 } },
    } as unknown as SimResultColumn;
    draw({ value: 0, format });
    expect(screen.getByText("Closed")).toBeDefined();
    expect(screen.queryByText("0")).toBeNull();
  });

  it("keeps the unit lane when a row has no value, so columns do not shift", () => {
    const heading = { perRowUnits: true, unitWidth: 4 } as ValueColumnHeading;
    const { container } = draw({}, { valueHeading: heading });
    const lanes = container.querySelectorAll("span[style*='width: 4ch']");
    expect(lanes.length).toBe(1);
    expect(lanes[0]?.textContent).toBe("");
  });
});

describe("the row's second line", () => {
  it("shows what an element connects to only while searching", () => {
    draw({ context: "J-1 → J-2" }, { searching: true });
    expect(screen.getByText("J-1 → J-2")).toBeDefined();
  });

  it("hides it when not searching, even if the row has one", () => {
    draw({ context: "J-1 → J-2" }, { searching: false });
    expect(screen.queryByText("J-1 → J-2")).toBeNull();
  });

  it("shows nothing when searching a row with no context", () => {
    const { container } = draw({ context: "" }, { searching: true });
    // The id line is the only line inside the text column.
    const text = container.querySelectorAll("span > span");
    expect([...text].some((s) => s.textContent === "")).toBe(false);
  });
});

describe("the zoom control", () => {
  it("names the element it zooms to", () => {
    const { onZoom } = draw();
    const zoom = screen.getByLabelText("Zoom to J-401");
    fireEvent.click(zoom);
    expect(onZoom).toHaveBeenCalledTimes(1);
  });

  it("is absent on a row that cannot be zoomed to", () => {
    draw({}, { zoomable: false });
    expect(screen.queryByLabelText("Zoom to J-401")).toBeNull();
  });
});

describe("click and double-click reach the row's handlers", () => {
  it("selects on the first click of a burst", () => {
    const { rowButton, onSelect } = draw();
    fireEvent.click(rowButton, { detail: 1 });
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("ignores the second click of a burst, which used to undo the first", () => {
    const { rowButton, onSelect } = draw();
    fireEvent.click(rowButton, { detail: 2 });
    expect(onSelect).not.toHaveBeenCalled();
  });

  /**
   * The real sequence, which a frozen prop does not model: the first
   * click toggles selection, the list re-renders the row with the new
   * `isActive`, and only then does the double-click land. Rendering both
   * events against one static prop asserts the opposite of the guarantee.
   */
  it("ends a double-click selected and zoomed, from either starting state", () => {
    for (const startedSelected of [true, false]) {
      const onSelect = vi.fn();
      const onZoom = vi.fn();
      let selected = startedSelected;
      const element = (active: boolean) => (
        <NetworkListRow
          row={row()}
          isActive={active}
          zoomable={true}
          searching={false}
          sys="si"
          valueHeading={HEADING}
          kindLabel={KIND_LABEL}
          onSelect={() => {
            selected = !selected;
            onSelect();
          }}
          onZoom={onZoom}
          onHover={vi.fn()}
          onClearHover={vi.fn()}
        />
      );
      const { container, rerender } = render(element(selected));
      const button = container.querySelector("button") as HTMLButtonElement;

      fireEvent.click(button, { detail: 1 });
      rerender(element(selected));
      fireEvent.doubleClick(
        container.querySelector("button") as HTMLButtonElement,
      );

      const from = `started ${startedSelected ? "" : "un"}selected`;
      expect(selected, from).toBe(true);
      expect(onZoom, from).toHaveBeenCalledTimes(1);
      expect(onSelect.mock.calls.length, from).toBe(startedSelected ? 2 : 1);
    }
  });

  it("does not zoom on a double-click when the row cannot be zoomed to", () => {
    const { rowButton, onZoom } = draw({}, { zoomable: false });
    fireEvent.doubleClick(rowButton);
    expect(onZoom).not.toHaveBeenCalled();
  });
});

describe("hover", () => {
  it("reports the row on pointer entry and on focus", () => {
    const { rowButton, onHover } = draw();
    fireEvent.mouseEnter(rowButton);
    expect(onHover).toHaveBeenCalledTimes(1);
    fireEvent.focus(rowButton);
    expect(onHover).toHaveBeenCalledTimes(2);
  });

  it("clears it on exit and on blur", () => {
    const { rowButton, onClearHover } = draw();
    fireEvent.mouseLeave(rowButton);
    expect(onClearHover).toHaveBeenCalledTimes(1);
    fireEvent.blur(rowButton);
    expect(onClearHover).toHaveBeenCalledTimes(2);
  });
});
