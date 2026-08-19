/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ScenarioRow } from "./RunModal";

function renderRow(over: {
  isResumable?: boolean;
  onResume?: () => void;
  onToggle?: () => void;
  errorCount?: number;
}) {
  render(
    <ScenarioRow
      scenario={{ id: null, label: "Base model", state: "not-run" }}
      isChecked={false}
      isActive={false}
      isLast
      errorCount={over.errorCount ?? 0}
      onToggle={over.onToggle ?? (() => {})}
      isResumable={over.isResumable ?? false}
      onResume={over.onResume ?? (() => {})}
    />,
  );
}

describe("RunModal target row — continuing an interrupted run", () => {
  it("offers to continue the base model, which no other surface can", () => {
    // The scenarios panel has no run action for the base model and so no
    // continue action either. This row is the only place it is offered.
    const onResume = vi.fn();
    renderRow({ isResumable: true, onResume });
    fireEvent.click(screen.getByText("Continue"));
    expect(onResume).toHaveBeenCalledTimes(1);
  });

  it("stops the click reaching the row it sits in", () => {
    // The action sits inside the row's own label, so a click a browser
    // forwards to the checkbox would toggle the target and queue a fresh
    // run of the very thing the person asked to continue.
    //
    // Asserted through the event rather than through `onToggle`: jsdom
    // does not forward a label click from a nested button at all, so a
    // test that watched the toggle passed with the guard deleted.
    const onResume = vi.fn();
    renderRow({ isResumable: true, onResume });
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    screen.getByText("Continue").dispatchEvent(click);
    expect(onResume).toHaveBeenCalledTimes(1);
    expect(click.defaultPrevented).toBe(true);
  });

  it("offers nothing when there is no interrupted run", () => {
    renderRow({ isResumable: false });
    expect(screen.queryByText("Continue")).toBeNull();
  });

  it("offers nothing for a target the solver would reject", () => {
    // A model with errors cannot run at all, so continuing it is not a
    // choice worth showing.
    renderRow({ isResumable: true, errorCount: 3 });
    expect(screen.queryByText("Continue")).toBeNull();
  });
});
