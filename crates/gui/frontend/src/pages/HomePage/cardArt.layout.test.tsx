import { afterEach, describe, expect, it } from "vitest";
import { NetworkSketch } from "../../components/ui/NetworkSketch";
import { placeholderSketch } from "../../components/ui/placeholderSketch";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { CARD_ART, CARD_ART_INNER } from "../HomePage";

afterEach(unmountAll);

/**
 * The frame a card's drawing sits in, measured in a real browser.
 *
 * Cards sit in a grid, so a frame whose height depends on what it holds
 * makes the row step up and down: a card with a full drawing stood taller
 * than one with a small mark in the middle of it. The frame takes its
 * height from its ratio, and its contents are out of flow so they cannot
 * add to it.
 *
 * jsdom answers every height with zero, so none of this is visible there.
 */

function heightOf(host: HTMLElement, selector: string): number {
  const el = host.querySelector(selector);
  if (!el) throw new Error(`no element matching ${selector}`);
  return el.getBoundingClientRect().height;
}

async function art(children: React.ReactNode) {
  return mount(
    <div data-wrap style={{ width: 260 }}>
      <span data-art style={CARD_ART}>
        <span style={CARD_ART_INNER}>{children}</span>
      </span>
    </div>,
  );
}

describe("a card's art frame", () => {
  /** The reported defect, stated directly. */
  it("is the same height whatever it holds", async () => {
    const full = heightOf(
      await art(<NetworkSketch sketch={placeholderSketch("wds") as never} />),
      "[data-art]",
    );
    const small = heightOf(await art(<span>WD</span>), "[data-art]");
    const nothing = heightOf(await art(null), "[data-art]");
    expect(full).toBe(small);
    expect(nothing).toBe(small);
  });

  /**
   * And the height is the ratio's, not the content's. Without this the
   * first assertion could pass on two frames that were both wrong in the
   * same way.
   */
  it("takes its height from its ratio", async () => {
    const host = await art(<span>WD</span>);
    expect(widthOf(host, "[data-art]")).toBe(260);
    expect(heightOf(host, "[data-art]")).toBeCloseTo((260 * 9) / 16, 0);
  });
});
