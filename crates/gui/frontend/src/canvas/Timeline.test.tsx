// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Timeline } from "./Timeline";

/**
 * The timeline's buttons are icon buttons, all of them.
 *
 * The loop button was a bare `⟳` character sitting in a row of 14×14 SVGs.
 * A text glyph in that row is not the same object: it takes its size from
 * the font rather than the icon rule, it is not centred by the same
 * mechanism, and which shape it draws is up to whatever font the platform
 * resolves — so it rendered at a different weight and size to its
 * neighbours, and on some platforms as an emoji.
 *
 * The assertion is over every button rather than that one, so the next
 * button added to this bar has to be an icon too. That is the decision
 * worth pinning; the loop button was only where it first went wrong.
 */

const NOOP = () => {};

function renderTimeline(loop = false) {
  return render(
    <Timeline
      currentHour={0}
      setCurrentHour={NOOP}
      isPlaying={false}
      setIsPlaying={NOOP}
      speed={1}
      setSpeed={NOOP}
      loop={loop}
      setLoop={NOOP}
      maxStep={24}
    />,
  );
}

describe("the timeline's buttons", () => {
  it("draw an icon rather than a text glyph", () => {
    const { container } = renderTimeline();
    const buttons = Array.from(container.querySelectorAll("button"));
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(
        button.querySelector("svg"),
        `"${button.getAttribute("data-tooltip")}" has no icon`,
      ).not.toBeNull();
      // And nothing but the icon: a stray character beside an SVG would
      // pass the check above while still rendering the glyph.
      expect(button.textContent?.trim()).toBe("");
    }
  });

  /**
   * The loop button reports its state through the tooltip, since the icon
   * itself does not change — so the state has to survive the icon swap.
   */
  it("says whether looping is on", () => {
    const { unmount } = renderTimeline(false);
    expect(screen.getByRole("button", { name: /loop off/i })).toBeTruthy();
    unmount();

    renderTimeline(true);
    expect(screen.getByRole("button", { name: /loop on/i })).toBeTruthy();
  });
});
