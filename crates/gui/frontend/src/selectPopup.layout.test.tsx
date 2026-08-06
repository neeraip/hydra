import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll } from "./layoutTest";

/**
 * What an open `<select>` renders its options on.
 *
 * The popup is drawn by the browser. On Windows it takes a system
 * background regardless of `color-scheme` while still applying the
 * `color` its `<select>` carries — so the timeline's speed picker, which
 * is styled with a muted foreground, drew grey text on a white sheet in
 * dark mode, legible only under the hover highlight.
 *
 * This is worth a test precisely because it is invisible where it was
 * written: macOS draws that popup natively and looked correct throughout.
 * The rule is a handful of CSS lines, but the failure it guards against —
 * someone swapping the opaque token for the translucent one while tidying
 * — reappears only on a platform no developer here is looking at.
 *
 * Real Chromium, because the whole question is what the cascade computes.
 */

afterEach(() => {
  unmountAll();
  document.documentElement.removeAttribute("data-theme");
});

/** `rgb(r, g, b)` / `rgba(...)` → its parts, alpha defaulting to 1. */
function parseColor(value: string) {
  const parts = value.match(/[\d.]+/g);
  if (!parts || parts.length < 3) throw new Error(`unparsable colour ${value}`);
  const [r, g, b] = parts.map(Number);
  return { r, g, b, alpha: parts.length > 3 ? Number(parts[3]) : 1 };
}

/** Rough perceptual lightness, 0 (black) to 1 (white). */
function lightness(value: string): number {
  const { r, g, b } = parseColor(value);
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}

async function optionStyle(theme: "dark" | "light") {
  document.documentElement.setAttribute("data-theme", theme);
  // The speed picker specifically: its muted `color` is what leaked into
  // the options and produced the reported grey.
  const host = await mount(
    <select className="tl-speed">
      <option data-opt value="1">
        1×
      </option>
    </select>,
  );
  const option = host.querySelector("[data-opt]");
  if (!option) throw new Error("no option");
  const style = getComputedStyle(option);
  return { background: style.backgroundColor, text: style.color };
}

describe("an option in a native select popup", () => {
  /**
   * The bug as reported: dark theme, light sheet. The option has to bring
   * its own background rather than inherit the platform's.
   */
  it("sits on a dark sheet in the dark theme", async () => {
    const { background, text } = await optionStyle("dark");
    expect(lightness(background)).toBeLessThan(0.5);
    expect(lightness(text)).toBeGreaterThan(0.5);
  });

  it("sits on a light sheet in the light theme", async () => {
    const { background, text } = await optionStyle("light");
    expect(lightness(background)).toBeGreaterThan(0.5);
    expect(lightness(text)).toBeLessThan(0.5);
  });

  /**
   * Opaque, in both themes. The panel token is translucent by design, and
   * there is nothing of ours behind this sheet to composite against — only
   * the desktop. Swapping the token would look identical everywhere the
   * popup is drawn by the OS and wrong on the one platform where it is
   * not, which is the whole reason this file exists.
   */
  it("is fully opaque, whichever theme is set", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { background } = await optionStyle(theme);
      expect(parseColor(background).alpha).toBe(1);
    }
  });

  /**
   * And the text is legible against it. Stated as a contrast gap rather
   * than an exact colour so that retuning the palette does not fail this,
   * but losing the rule does.
   */
  it("keeps its text clear of its background", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { background, text } = await optionStyle(theme);
      expect(Math.abs(lightness(background) - lightness(text))).toBeGreaterThan(
        0.4,
      );
    }
  });
});
