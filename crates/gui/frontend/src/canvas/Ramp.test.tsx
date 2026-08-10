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

  it("anchors beside the pointer, never centred on it", () => {
    // Centred, the chip ran off the left of the canvas at the start of the
    // bar — the legend floats near the edge, so there was nothing to clamp
    // against. It sits to the right of the pointer until the halfway mark,
    // then flips to the left of it.
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
    // Floating point: the anchor is 1 − 0.9, so assert the side rather
    // than the exact percentage.
    expect(Number.parseFloat(nearRight.right)).toBeCloseTo(10);
    expect(nearRight.left).toBe("");
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
