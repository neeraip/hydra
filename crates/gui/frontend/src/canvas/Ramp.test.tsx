/** @vitest-environment jsdom */
import { render } from "@testing-library/react";
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
