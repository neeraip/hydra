import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll } from "../../layoutTest";
import { PrimaryButton } from "./PrimaryButton";

/**
 * What the run button actually paints, in each theme.
 *
 * `[data-theme="light"] .btn-run` outweighs `.btn-run--stale` and
 * `.btn-run--outline` — an attribute plus a class against a single class —
 * so in the light theme it repainted both of them with the filled blue
 * background while leaving their own foregrounds alone. The outline button
 * became a blue fill still carrying blue text, and the amber "edited since
 * last run" warning stopped being amber, which meant a state the user is
 * supposed to notice simply did not show.
 *
 * A cascade question, so it is asked of a real browser rather than of the
 * stylesheet.
 */

afterEach(() => {
  unmountAll();
  document.documentElement.removeAttribute("data-theme");
});

function parse(value: string) {
  const [r, g, b] = (value.match(/[\d.]+/g) ?? []).map(Number);
  return { r, g, b };
}

/** Rough perceptual lightness, 0 (black) to 1 (white). */
function lightness(value: string) {
  const { r, g, b } = parse(value);
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}

async function paint(theme: "dark" | "light", className?: string) {
  document.documentElement.setAttribute("data-theme", theme);
  const host = await mount(
    <PrimaryButton size="sm" className={className}>
      Simulate
    </PrimaryButton>,
  );
  const button = host.querySelector("button");
  if (!button) throw new Error("no button");
  const style = getComputedStyle(button);
  return {
    text: style.color,
    fill: style.backgroundImage,
    background: style.backgroundColor,
  };
}

describe("the run button", () => {
  /** The filled default is white-on-blue in both themes. */
  it("keeps its text clear of its fill when filled", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { text, fill } = await paint(theme);
      expect(fill).toContain("linear-gradient");
      expect(lightness(text)).toBeGreaterThan(0.9);
    }
  });

  /**
   * The reported bug. A button whose text is a mid blue must not also be
   * given a mid blue fill — stated as the contrast between the two rather
   * than as either colour, so retuning the palette cannot quietly
   * reintroduce it.
   */
  it("does not fill the outline variant, in either theme", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { fill, background } = await paint(theme, "btn-run--outline");
      expect(fill, `${theme} outline was given a fill`).toBe("none");
      // Transparent, or at least not a painted sheet behind blue text.
      expect(background).toMatch(/rgba\(0, 0, 0, 0\)|transparent/);
    }
  });

  /**
   * The second, unreported half: an amber warning that renders blue is not
   * a warning. Asserted by hue — amber has far more red than blue, and the
   * fill it was losing to has the opposite.
   */
  it("keeps the stale warning amber rather than blue", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { fill } = await paint(theme, "btn-run--stale");
      const { r, b } = parse(fill.slice(fill.indexOf("rgb")));
      expect(r, `${theme} stale button lost its amber`).toBeGreaterThan(b);
    }
  });

  /**
   * And the outline's text carries enough contrast against the surface it
   * sits on. The dark-theme blue is chosen against a dark panel and lands
   * near 3:1 on white, which is under AA for text this size.
   */
  it("darkens the outline's text for the light theme", async () => {
    const dark = await paint("dark", "btn-run--outline");
    const light = await paint("light", "btn-run--outline");
    expect(lightness(light.text)).toBeLessThan(lightness(dark.text));
  });
});
