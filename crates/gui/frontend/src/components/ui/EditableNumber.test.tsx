/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ElementAttributeQuantity } from "../../hooks";
import { EditableNumber } from "./EditableNumber";

/** Metres shown as feet, to two places — the conversion that makes the
 * display round trip lossy, which is the whole reason these rules exist. */
const LENGTH: ElementAttributeQuantity = {
  key: "length",
  siLabel: "m",
  usLabel: "ft",
  siToUsScale: 3.28084,
  siToUsOffset: 0,
  siDecimals: 3,
  usDecimals: 2,
};

function field() {
  return screen.getByLabelText("Invert") as HTMLInputElement;
}

describe("EditableNumber", () => {
  it("shows the number without its unit", () => {
    // The unit belongs beside the field, labelling it. A field that
    // displays "12.500 m" and expects "12.5" back punishes reading it.
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={() => {}}
      />,
    );
    expect(field().value).toBe("12.5");
  });

  it("commits on blur, in the unit it was given", () => {
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    fireEvent.change(field(), { target: { value: "13" } });
    fireEvent.blur(field());
    expect(onCommit).toHaveBeenCalledWith(13);
  });

  it("converts what was typed back to the served unit", () => {
    // The field displays feet; the model takes metres. A scale applied
    // one way and not the other stores a value three times out.
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={1}
        quantity={LENGTH}
        sys="us"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    expect(field().value).toBe("3.28");
    fireEvent.change(field(), { target: { value: "6.56" } });
    fireEvent.blur(field());
    // Within one step of the two decimals the field offers — the entered
    // number is exact, but "6.56 ft" only names a metre value that
    // precisely.
    expect(onCommit.mock.calls[0][0]).toBeCloseTo(2, 2);
  });

  // The rule that keeps the lossy display round trip harmless: 1 m shows
  // as 3.28 ft and reads back as 0.99974 m, so a field that wrote on
  // every blur would erode every value the user merely looked at.
  it("does not write when the draft still matches what was shown", () => {
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={1}
        quantity={LENGTH}
        sys="us"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    fireEvent.focus(field());
    fireEvent.blur(field());
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("treats a half-typed value as no value", () => {
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    for (const draft of ["", "-", "1e", "abc"]) {
      fireEvent.change(field(), { target: { value: draft } });
      fireEvent.blur(field());
    }
    expect(onCommit).not.toHaveBeenCalled();
    // And the field returns to the value the model holds rather than
    // stranding the user on text that means nothing.
    expect(field().value).toBe("12.5");
  });

  it("abandons the edit on Escape", () => {
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    fireEvent.change(field(), { target: { value: "99" } });
    fireEvent.keyDown(field(), { key: "Escape" });
    fireEvent.blur(field());
    expect(onCommit).not.toHaveBeenCalled();
    expect(field().value).toBe("12.5");
  });

  it("restores the shown value when the write is refused", async () => {
    const onCommit = vi.fn(() => Promise.reject(new Error("refused")));
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    fireEvent.change(field(), { target: { value: "13" } });
    fireEvent.blur(field());
    await vi.waitFor(() => expect(field().value).toBe("12.5"));
  });

  it("follows the value when the model changes underneath it", () => {
    // The field redraws from a refetch after every write, and the unit
    // system can change under it; a draft pinned to the first render
    // would strand the user on a stale number.
    const { rerender } = render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={() => {}}
      />,
    );
    rerender(
      <EditableNumber
        value={20}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={() => {}}
      />,
    );
    expect(field().value).toBe("20");
  });

  it("commits on Enter", () => {
    const onCommit = vi.fn();
    render(
      <EditableNumber
        value={12.5}
        quantity={LENGTH}
        sys="si"
        label="Invert"
        onCommit={onCommit}
      />,
    );
    fireEvent.change(field(), { target: { value: "13" } });
    // Enter blurs, and the blur commits — asserted through the real
    // path rather than by calling commit directly, because the two were
    // separate handlers before and could stop agreeing again.
    fireEvent.keyDown(field(), { key: "Enter" });
    fireEvent.blur(field());
    expect(onCommit).toHaveBeenCalledWith(13);
  });
});
