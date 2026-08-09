import { describe, expect, it } from "vitest";
import { TEXT_MARKER } from "./SectionList";

/**
 * A marker standing inside a section's name is sized from that name, not
 * from the row's control-cluster icon size. It shipped at the cluster's
 * 12px, which beside 13px text out-measures the capitals it stands next
 * to and reads as an emoji dropped into the title.
 *
 * This pins the decision — "the marker tracks the text" — rather than the
 * pixels: only a browser knows what a capital actually measures, and the
 * layout layer deliberately stays small.
 */

describe("the in-title marker", () => {
  it("is sized in em, so it follows the title rather than a fixed size", () => {
    expect(TEXT_MARKER.width).toMatch(/em$/);
    expect(TEXT_MARKER.height).toBe(TEXT_MARKER.width);
  });

  it("is smaller than the em box, since a capital does not fill one", () => {
    const em = Number.parseFloat(String(TEXT_MARKER.width));
    expect(em).toBeGreaterThan(0.5);
    expect(em).toBeLessThan(1);
  });

  it("resolves its em against the title's size, not the row's", () => {
    // The marker is a sibling of the title span, so without its own
    // font-size the em would follow whatever the row inherited.
    expect(TEXT_MARKER.fontSize).toBe("var(--text-lg)");
  });
});
