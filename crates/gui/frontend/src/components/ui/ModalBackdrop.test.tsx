/** @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ModalBackdrop } from "./ModalBackdrop";

/**
 * A full-window overlay must belong to the window, not to whatever opened
 * it. The licences panel proved why: opened from a row inside the settings
 * drawer — a fixed, scrolling, 680px column — it rendered inside that
 * column, so "fixed, inset 0" meant the column. The backdrop covered the
 * drawer instead of the window and the centred panel hung off both edges
 * of it, first tab included.
 */

describe("ModalBackdrop", () => {
  it("renders into the body, not into the box that opened it", () => {
    const { container } = render(
      <div style={{ overflow: "auto" }} data-testid="drawer">
        <ModalBackdrop zIndex={210}>
          <div>panel</div>
        </ModalBackdrop>
      </div>,
    );
    const panel = screen.getByText("panel");
    const backdrop = panel.parentElement;
    expect(backdrop?.parentElement).toBe(document.body);
    // …and nothing of the overlay is left behind inside the trigger's box.
    expect(container.querySelector("[style*='position: fixed']")).toBeNull();
  });

  it("still hands its children to the caller's tree in React terms", () => {
    // A portal moves the DOM node, not the React tree: context and event
    // bubbling still reach the component that wrote the modal, which is
    // what lets a modal be declared where it belongs.
    render(
      <ModalBackdrop zIndex={1}>
        <button type="button">inside</button>
      </ModalBackdrop>,
    );
    expect(screen.getByRole("button", { name: "inside" })).toBeTruthy();
  });
});
