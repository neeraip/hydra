// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
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
 * The criteria toggle is a different thing and is allowed: it is a state,
 * not an action, and it is the answer to a question the segments do not
 * ask. Its own tests are below.
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

describe("the criteria toggle beside the ranges", () => {
  it("is absent when the caller offers none", () => {
    // A model with nothing to judge: the row is ranges alone.
    render(<ScaleControl value="run" options={OPTIONS} onChange={() => {}} />);
    expect(screen.queryByText("Criteria")).toBeNull();
    expect(screen.getAllByRole("button")).toHaveLength(OPTIONS.length);
  });

  it("reports its own state rather than a range", () => {
    // The bug this design replaces: picking criteria used to *deselect*
    // the range, so a reader could not scale to the step and judge at the
    // same time even though the two are separate questions.
    const onChange = vi.fn();
    const onCriteria = vi.fn();
    render(
      <ScaleControl
        value="step"
        options={OPTIONS}
        onChange={onChange}
        criteria={{ on: false, onChange: onCriteria }}
      />,
    );
    fireEvent.click(screen.getByText("Criteria"));
    expect(onCriteria).toHaveBeenCalledWith(true);
    // The range was not touched.
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByText("Step").style.color).toBe("var(--accent)");
  });

  it("says whether it is on", () => {
    render(
      <ScaleControl
        value="run"
        options={OPTIONS}
        onChange={() => {}}
        criteria={{ on: true, onChange: () => {} }}
      />,
    );
    expect(screen.getByText("Criteria").getAttribute("aria-pressed")).toBe(
      "true",
    );
  });
});
