import { describe, expect, it } from "vitest";
import {
  backgroundPickerShown,
  CANVAS_BACKGROUND_OVERRIDES,
  CANVAS_BACKGROUNDS,
  canvasBackgroundStyle,
  DEFAULT_CANVAS_BACKGROUND,
  effectiveCanvasBackground,
  GROUND_LABEL,
  readCanvasBackground,
} from "./canvasBackground";

/**
 * The canvas ground already followed the app's theme — `.canvas-bg` paints
 * `--bg-app`, and the deck canvas over it is transparent. What this adds is
 * the override, on the shape the unit system already uses: a default that
 * tracks a setting made elsewhere, and an explicit choice that pins against
 * it.
 *
 * The distinction those two carry is the whole feature, and it is the easy
 * thing to lose: "Match theme" is not "whichever value the theme has right
 * now". The first keeps tracking, the second stops.
 */

describe("what to paint the ground", () => {
  /**
   * The load-bearing one. Resolving `theme` to a colour here would freeze
   * it: the stylesheet re-answers on a theme change, including one the OS
   * makes while the app is open, and nothing in this module would hear
   * about that.
   */
  it("leaves the tracking case to the stylesheet", () => {
    expect(canvasBackgroundStyle("theme")).toBeUndefined();
  });

  it("pins the two overrides", () => {
    expect(canvasBackgroundStyle("dark")).toBe("var(--bg-app-dark)");
    expect(canvasBackgroundStyle("light")).toBe("var(--bg-app-light)");
  });

  /**
   * Tokens, not literals, so a reader who pins to Light gets exactly the
   * ground the light theme would have given them — not a second opinion
   * about what light means that drifts the first time the palette moves.
   */
  it("names a token rather than a colour", () => {
    for (const b of CANVAS_BACKGROUNDS) {
      const style = canvasBackgroundStyle(b);
      if (style != null) expect(style).toMatch(/^var\(--/);
    }
  });

  it("has something to paint for every choice but the tracking one", () => {
    for (const b of CANVAS_BACKGROUNDS) {
      expect(canvasBackgroundStyle(b) == null).toBe(b === "theme");
    }
  });
});

describe("reading a stored preference", () => {
  it("keeps a value it understands", () => {
    for (const b of CANVAS_BACKGROUNDS) {
      expect(readCanvasBackground(b)).toBe(b);
    }
  });

  /** Prefs written before this existed have no value at all. */
  it("falls back to tracking the theme", () => {
    expect(readCanvasBackground(undefined)).toBe(DEFAULT_CANVAS_BACKGROUND);
    expect(readCanvasBackground(null)).toBe("theme");
    expect(readCanvasBackground("sepia")).toBe("theme");
    expect(readCanvasBackground(3)).toBe("theme");
  });
});

describe("which picker holds the slot", () => {
  /**
   * They are alternatives, never both. A basemap *is* the ground when there
   * is one, so a background colour beside it would be a choice with no
   * effect; and where no basemap is possible the picker sat disabled in
   * prime toolbar space saying nothing.
   */
  it("takes the basemap picker's place exactly where it is dead", () => {
    expect(backgroundPickerShown(true)).toBe(true);
    expect(backgroundPickerShown(false)).toBe(false);
  });
});

/**
 * The menu is built to the unit picker's shape: a Default group whose one
 * row names what the setting resolves to, then an Override group that stays
 * put when the setting moves. That only works if the tracking case can be
 * *named*, which is what this resolves — for the label, never for the paint.
 */
describe("which ground is on screen", () => {
  it("is the theme's answer while tracking", () => {
    expect(effectiveCanvasBackground("theme", "light")).toBe("light");
    expect(effectiveCanvasBackground("theme", "dark")).toBe("dark");
  });

  /** The point of pinning: the theme moves and this does not. */
  it("is the override whatever the theme says", () => {
    expect(effectiveCanvasBackground("dark", "light")).toBe("dark");
    expect(effectiveCanvasBackground("light", "dark")).toBe("light");
  });
});

describe("the picker's labels", () => {
  it("names both grounds", () => {
    for (const b of CANVAS_BACKGROUND_OVERRIDES) {
      expect(GROUND_LABEL[b]).toBeTruthy();
    }
  });

  /**
   * `theme` has no label of its own on purpose. It wears whichever ground
   * it resolves to, and a name like "Match theme" sitting in the same list
   * as Dark and Light would read as a third colour rather than as deference
   * — which is precisely the distinction the Default/Override split exists
   * to draw.
   */
  it("offers only the two real grounds as overrides", () => {
    expect([...CANVAS_BACKGROUND_OVERRIDES].sort()).toEqual(["dark", "light"]);
    expect(CANVAS_BACKGROUNDS).toContain("theme");
    expect(CANVAS_BACKGROUND_OVERRIDES).not.toContain(
      "theme" as unknown as "dark",
    );
  });
});
