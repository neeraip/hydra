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
 * from every view rather than only this one — and an *action* flush
 * against a segmented control still reads as one more scale, so the row
 * must never grow one back.
 *
 * That includes the criteria switch, which briefly sat here: as a fourth
 * rectangle in the same row it read as a fourth range. It belongs to the
 * variable it judges, and lives beside that variable's ramp.
 */

const OPTIONS = scaleOptions(true);

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

describe("the criteria checkbox", () => {
  it("is not in this row at all", () => {
    // It belongs to a variable, not to the map: both engines band
    // variables in two classes, and one switch could not say "judge the
    // pressures, show velocity as a magnitude". It lives beside the ramp
    // it applies to — see `CriteriaCheckbox` and the legend's tests.
    render(<ScaleControl value="run" options={OPTIONS} onChange={() => {}} />);
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(OPTIONS.length);
  });
});
