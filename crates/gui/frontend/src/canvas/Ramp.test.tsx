/** @vitest-environment jsdom */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Ramp } from "./legend-primitives";

/**
 * The bar's own statement that the canvas is moving.
 *
 * Asserted as a class rather than as motion, because the movement is a CSS
 * animation and jsdom runs none — what this layer can hold is that the
 * class is applied exactly when the caller says the variable animates, and
 * that the gradient underneath is untouched either way. The colours are
 * the data: a sheen may pass over them, and nothing may move them.
 */

const bar = (el: HTMLElement) =>
  el.querySelector(".legend-ramp") as HTMLElement;

describe("Ramp", () => {
  it("marks the bar while its variable is animating", () => {
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min={0}
        max={1}
        animating
      />,
    );
    expect(bar(container).className).toContain("legend-ramp--animating");
  });

  it("leaves it unmarked otherwise, including by default", () => {
    const { container } = render(
      <Ramp gradient="linear-gradient(90deg, red, blue)" min={0} max={1} />,
    );
    expect(bar(container).className).not.toContain("legend-ramp--animating");
  });

  it("paints the same gradient either way", () => {
    // The sheen is an overlay. If animating changed the bar's own
    // background, the legend would be showing values the map does not hold.
    const gradient = "linear-gradient(90deg, red, blue)";
    const still = render(<Ramp gradient={gradient} min={0} max={1} />);
    const moving = render(
      <Ramp gradient={gradient} min={0} max={1} animating />,
    );
    expect(bar(moving.container).style.background).toBe(
      bar(still.container).style.background,
    );
  });
});

describe("Ramp readout", () => {
  /** jsdom lays nothing out, so the bar is given a box to be measured in. */
  function withWidth(el: HTMLElement, left: number, width: number) {
    el.getBoundingClientRect = () => ({ left, width }) as DOMRect;
  }

  it("reads the value under the pointer", () => {
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="40"
        max="100"
        readAt={(t) => `${40 + t * 60} L/s`}
      />,
    );
    const el = bar(container);
    withWidth(el, 100, 200);
    fireEvent.mouseMove(el, { clientX: 200 });
    expect(screen.getByText("70 L/s")).toBeTruthy();
  });

  it("shows nothing where a position does not name a value", () => {
    // A criteria-banded bar: equal-width segments that stand for bands, so
    // the caller answers null and the chip must not appear at all.
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0"
        max="5"
        readAt={() => null}
      />,
    );
    const el = bar(container);
    withWidth(el, 0, 100);
    fireEvent.mouseMove(el, { clientX: 50 });
    expect(container.textContent).not.toContain("L/s");
    expect(container.querySelectorAll("div").length).toBeLessThan(5);
  });

  it("sits to the right of the pointer, wherever the pointer is", () => {
    // Centred, the chip ran off the left of the canvas at the start of the
    // bar. Flipping sides at the halfway mark fixed that and cost more:
    // the chip jumped mid-drag, in the one gesture meant to be a steady
    // read. It is anchored the same way at both ends now.
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0"
        max="10"
        readAt={(t) => `${t}`}
      />,
    );
    const el = bar(container);
    withWidth(el, 0, 100);

    fireEvent.mouseMove(el, { clientX: 10 });
    const nearLeft = screen.getByText("0.1").style;
    expect(nearLeft.left).toBe("10%");
    expect(nearLeft.right).toBe("");
    expect(nearLeft.transform).not.toContain("-50%");

    fireEvent.mouseMove(el, { clientX: 90 });
    const nearRight = screen.getByText("0.9").style;
    expect(Number.parseFloat(nearRight.left)).toBeCloseTo(90);
    expect(nearRight.right).toBe("");
  });

  it("clears when the pointer leaves", () => {
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0"
        max="10"
        readAt={(t) => `${t * 10}`}
      />,
    );
    const el = bar(container);
    withWidth(el, 0, 100);
    fireEvent.mouseMove(el, { clientX: 50 });
    expect(screen.getByText("5")).toBeTruthy();
    fireEvent.mouseLeave(el);
    expect(screen.queryByText("5")).toBeNull();
  });

  it("has no readout at all when the caller offers none", () => {
    const { container } = render(
      <Ramp gradient="linear-gradient(90deg, red, blue)" min="0" max="1" />,
    );
    const el = bar(container);
    withWidth(el, 0, 100);
    fireEvent.mouseMove(el, { clientX: 50 });
    expect(el.style.cursor).toBe("");
  });
});

describe("Ramp labels", () => {
  it("labels the two ends of a data-range bar", () => {
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0.00"
        max="8.6"
      />,
    );
    expect(container.textContent).toContain("0.00");
    expect(container.textContent).toContain("8.6");
  });

  it("labels a banded bar at its seams instead", () => {
    // The reported bug: a bar of velocity bands sat under "0.00" and
    // "8.599" — the run's range — while the hover readout said "≥ 9.843".
    // All three were correct and two of them belonged to another axis.
    const { container } = render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0.00"
        max="8.599"
        boundaries={[
          { at: 1 / 3, label: "1.969" },
          { at: 2 / 3, label: "9.843" },
        ]}
      />,
    );
    expect(container.textContent).toContain("1.969");
    expect(container.textContent).toContain("9.843");
    expect(container.textContent).not.toContain("8.599");
    expect(container.textContent).not.toContain("0.00");
  });

  it("puts each seam label where its colour changes", () => {
    render(
      <Ramp
        gradient="linear-gradient(90deg, red, blue)"
        min="0"
        max="1"
        boundaries={[
          { at: 1 / 3, label: "a" },
          { at: 2 / 3, label: "b" },
        ]}
      />,
    );
    const first = screen.getByText("a") as HTMLElement;
    // A third along, centred on the seam — not at an end.
    expect(Number.parseFloat(first.style.left)).toBeCloseTo(33.3, 0);
    expect(first.style.transform).toContain("-50%");
  });
});
