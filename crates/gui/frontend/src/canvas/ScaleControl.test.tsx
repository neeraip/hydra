// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ScaleControl } from "./legend-primitives";
import { scaleOptions } from "./scaleOptions";

/**
 * The scale row is a segmented control — one of these is what the colours
 * are measured against.
 *
 * It used to carry a route to the criteria editor beside those options,
 * because criteria were shown on the canvas and authored on another page
 * with nothing joining the two. The project toolbar owns that route now,
 * from every view rather than only this one, so the row is scales alone
 * again — and must stay that way: an action flush against a segmented
 * control reads as a fourth scale.
 */

const OPTIONS = scaleOptions(true, true);

describe("the scale row", () => {
  it("leaves every scale option reachable", () => {
    render(<ScaleControl value="run" options={OPTIONS} onChange={() => {}} />);
    for (const { label } of OPTIONS) {
      expect(screen.getByText(label)).toBeTruthy();
    }
  });

  it("offers nothing but scales", () => {
    render(<ScaleControl value="run" options={OPTIONS} onChange={() => {}} />);
    expect(screen.queryByLabelText("Edit criteria")).toBeNull();
    // Every control in the row is one of the scales.
    expect(screen.getAllByRole("button")).toHaveLength(OPTIONS.length);
  });

  it("still reports the scale a reader picks", () => {
    const onChange = vi.fn();
    render(<ScaleControl value="run" options={OPTIONS} onChange={onChange} />);
    const other = OPTIONS.find((o) => o.mode !== "run");
    if (other) {
      screen.getByText(other.label).click();
      expect(onChange).toHaveBeenCalledWith(other.mode);
    }
  });
});
