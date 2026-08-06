// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CanvasSlider } from "./CanvasSlider";

/**
 * Dragging a canvas slider used to highlight text elsewhere on the page.
 *
 * The track sets `user-select: none`, so a selection cannot start on it —
 * but `setPointerCapture` retargets only pointer events. Native selection
 * runs off the mouse events beneath, which still reach whatever the cursor
 * passes over, so a drag that wandered off the narrow track dragged a
 * highlight across the panel behind it.
 *
 * What is asserted here is the boundary, not the mechanism: while a drag is
 * in progress the page cannot begin a selection, and the moment it ends the
 * page can again. A fix that suppressed selection permanently would satisfy
 * the first half and is exactly the failure worth guarding against.
 */

// jsdom implements neither, and the component is not under test for them.
if (!Element.prototype.setPointerCapture) {
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn(() => true);
}

function renderSlider() {
  const view = render(
    <CanvasSlider
      value={50}
      onChange={() => {}}
      label="Node size"
      readout="scaled to the network"
      hint="Drag up for larger nodes"
      topGlyph={<span>▲</span>}
      bottomGlyph={<span>▼</span>}
    />,
  );
  return { view, track: screen.getByRole("slider") };
}

/** Did the page allow a selection to begin? */
function selectionAllowed(): boolean {
  const e = new Event("selectstart", { bubbles: true, cancelable: true });
  document.body.dispatchEvent(e);
  return !e.defaultPrevented;
}

describe("dragging a canvas slider", () => {
  it("leaves the page selectable before a drag", () => {
    renderSlider();
    expect(selectionAllowed()).toBe(true);
  });

  it("stops the page selecting while the drag is in progress", () => {
    const { track } = renderSlider();
    fireEvent.pointerDown(track, { clientY: 40, pointerId: 1 });
    expect(selectionAllowed()).toBe(false);
  });

  it("gives selection back when the pointer is released", () => {
    const { track } = renderSlider();
    fireEvent.pointerDown(track, { clientY: 40, pointerId: 1 });
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(selectionAllowed()).toBe(true);
  });

  /**
   * A release can go missing — the pointer leaves the window, the OS takes
   * over, the gesture is cancelled. That must not leave a page nothing can
   * be selected on until reload.
   */
  it("gives selection back when the drag is cancelled", () => {
    const { track } = renderSlider();
    fireEvent.pointerDown(track, { clientY: 40, pointerId: 1 });
    fireEvent.pointerCancel(window, { pointerId: 1 });
    expect(selectionAllowed()).toBe(true);
  });

  /** The listener is the document's, so leaving the page must take it away. */
  it("leaves nothing behind when unmounted mid-drag", () => {
    const { view, track } = renderSlider();
    fireEvent.pointerDown(track, { clientY: 40, pointerId: 1 });
    view.unmount();
    expect(selectionAllowed()).toBe(true);
  });
});
