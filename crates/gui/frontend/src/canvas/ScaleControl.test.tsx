// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ScaleControl } from "./legend-primitives";
import { scaleOptions } from "./scaleOptions";

/**
 * The scale row is a segmented control — one of these is what the colours
 * are measured against. The criteria route is not one of those: it changes
 * the ruler rather than choosing between rulers, so it has to read as an
 * action and not as a fourth scale.
 *
 * These assert what a user can reach, which is the part that was missing:
 * criteria were shown on the canvas and authored on another page, with
 * nothing joining the two.
 */

const OPTIONS = scaleOptions(true, true);

describe("the scale row", () => {
  it("offers no criteria route when the host gives none", () => {
    render(<ScaleControl value="run" options={OPTIONS} onChange={() => {}} />);
    expect(screen.queryByLabelText("Edit criteria")).toBeNull();
  });

  it("offers one when it does", () => {
    render(
      <ScaleControl
        value="run"
        options={OPTIONS}
        onChange={() => {}}
        onEditCriteria={() => {}}
      />,
    );
    expect(screen.getByLabelText("Edit criteria")).toBeTruthy();
  });

  it("opens the editor when pressed", () => {
    const onEdit = vi.fn();
    render(
      <ScaleControl
        value="run"
        options={OPTIONS}
        onChange={() => {}}
        onEditCriteria={onEdit}
      />,
    );
    fireEvent.click(screen.getByLabelText("Edit criteria"));
    expect(onEdit).toHaveBeenCalledTimes(1);
  });

  /**
   * It must not join the scale group. A press that also changed the scale
   * would move the colours out from under the reader on the way to the
   * editor, and the two mean different things.
   */
  it("does not change the scale on the way", () => {
    const onChange = vi.fn();
    render(
      <ScaleControl
        value="run"
        options={OPTIONS}
        onChange={onChange}
        onEditCriteria={() => {}}
      />,
    );
    fireEvent.click(screen.getByLabelText("Edit criteria"));
    expect(onChange).not.toHaveBeenCalled();
  });

  /** And the scale options are still all there beside it. */
  it("leaves every scale option reachable", () => {
    render(
      <ScaleControl
        value="run"
        options={OPTIONS}
        onChange={() => {}}
        onEditCriteria={() => {}}
      />,
    );
    for (const { label } of OPTIONS) {
      expect(screen.getByText(label)).toBeTruthy();
    }
  });
});
