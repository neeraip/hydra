/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
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

describe("ModalBackdrop focus", () => {
  it("moves focus into the dialog when it opens", () => {
    render(
      <ModalBackdrop zIndex={1}>
        <div>
          <button type="button">first</button>
          <button type="button">second</button>
        </div>
      </ModalBackdrop>,
    );
    expect(document.activeElement?.textContent).toBe("first");
  });

  it("keeps Tab inside the dialog", () => {
    // Tabbing out of a modal leaves the reader operating controls the
    // backdrop is covering: the focus ring is out there, the page is not.
    render(
      <ModalBackdrop zIndex={1}>
        <div>
          <button type="button">first</button>
          <button type="button">last</button>
        </div>
      </ModalBackdrop>,
    );
    const last = screen.getByRole("button", { name: "last" });
    last.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement?.textContent).toBe("first");
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement?.textContent).toBe("last");
  });

  it("gives focus back to whatever opened it", () => {
    const opener = document.createElement("button");
    opener.textContent = "opener";
    document.body.appendChild(opener);
    opener.focus();

    const view = render(
      <ModalBackdrop zIndex={1}>
        <button type="button">inside</button>
      </ModalBackdrop>,
    );
    expect(document.activeElement?.textContent).toBe("inside");
    view.unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("leaves Tab to the topmost dialog when two are open", () => {
    // A confirmation over a panel: both would otherwise pull the ring
    // back to themselves and it would bounce between the two.
    render(
      <>
        <ModalBackdrop zIndex={1}>
          <button type="button">under</button>
        </ModalBackdrop>
        <ModalBackdrop zIndex={2}>
          <button type="button">over</button>
        </ModalBackdrop>
      </>,
    );
    screen.getByRole("button", { name: "over" }).focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement?.textContent).toBe("over");
  });
});
