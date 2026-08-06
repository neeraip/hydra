import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll } from "../../layoutTest";
import { PrimaryButton } from "./PrimaryButton";

/**
 * What the run button actually paints, in each theme.
 *
 * Two defects live here. The button was `#4a90d9` — the water distribution
 * engine's own identity colour — everywhere it appeared, so a drainage
 * project's Simulate button claimed an engine it had nothing to do with.
 * And `[data-theme="light"] .btn-run` outweighed the modifier classes, so
 * in the light theme it repainted the outline and stale variants with the
 * filled background while leaving their foregrounds alone: the outline
 * became a fill still carrying its own text colour, and the amber "edited
 * since last run" warning stopped being amber, which meant a state the
 * user is supposed to notice did not show at all.
 *
 * Cascade questions, so they are asked of a real browser rather than of
 * the stylesheet.
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

/**
 * How far a colour is from grey, 0 (grey) to 1.
 *
 * The spread between channels against the full range, not against the
 * brightest channel. Measured the second way a near-grey navy reads as
 * heavily saturated purely because it is dark: `#2a3140` is 22 apart on a
 * maximum of 64, which is 0.34, while the same 22 on 255 is 0.09 — and it
 * is 0.09 that matches what the eye calls grey.
 */
function chroma(value: string) {
  const { r, g, b } = parse(value);
  return (Math.max(r, g, b) - Math.min(r, g, b)) / 255;
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
  /**
   * With no engine in scope the fill is the achromatic accent, and its
   * text is whatever can be read on that — which inverts between themes,
   * since the accent is near-white in one and near-black in the other.
   */
  it("is achromatic outside a project, and legible in both themes", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { text, background } = await paint(theme);
      expect(chroma(background)).toBeLessThan(0.12);
      expect(Math.abs(lightness(background) - lightness(text))).toBeGreaterThan(
        0.5,
      );
    }
  });

  /**
   * The point of the whole change: inside a project the fill is that
   * project's engine colour, so the blue only ever appears where blue is
   * true.
   */
  it("takes the engine's colour where one is in scope", async () => {
    const host = await mount(
      <div style={{ "--engine-accent": "#7a6ff0" } as React.CSSProperties}>
        <PrimaryButton size="sm">Simulate</PrimaryButton>
      </div>,
    );
    const button = host.querySelector("button");
    if (!button) throw new Error("no button");
    const { r, g, b } = parse(getComputedStyle(button).backgroundColor);
    expect([r, g, b]).toEqual([122, 111, 240]);
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
  /**
   * Amber survives the achromatic rule because it is a status rather than
   * an identity: colour still means one thing. Asserted by hue, so a
   * palette retune cannot quietly turn the warning back into a fill that
   * says nothing.
   */
  it("keeps the stale warning amber", async () => {
    for (const theme of ["dark", "light"] as const) {
      const { background } = await paint(theme, "btn-run--stale");
      const { r, b } = parse(background);
      expect(r, `${theme} stale button lost its amber`).toBeGreaterThan(b);
      expect(chroma(background)).toBeGreaterThan(0.3);
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

  /**
   * The Settings drawer and anything else drawn over a project without an
   * engine to speak for must stay achromatic. They are siblings of the
   * project subtree rather than children, so they never inherit the
   * variable — asserted here as the absence of inheritance rather than as
   * a rule each surface has to remember.
   */
  it("stays achromatic where no engine variable reaches it", async () => {
    const host = await mount(
      <div style={{ "--engine-accent": "#7a6ff0" } as React.CSSProperties}>
        <span
          data-outside
          style={{ "--engine-accent": "initial" } as React.CSSProperties}
        >
          <PrimaryButton size="sm">Save</PrimaryButton>
        </span>
      </div>,
    );
    const button = host.querySelector("[data-outside] button");
    if (!button) throw new Error("no button");
    expect(chroma(getComputedStyle(button).backgroundColor)).toBeLessThan(0.12);
  });
});

/**
 * A button's width must not depend on which variant it is.
 *
 * The Simulate button is filled until a scenario has been simulated and
 * outlined afterwards. `.btn-run` had no border and `.btn-run--outline`
 * had a real one, so the outlined form was three pixels wider and the
 * toolbar shifted as you moved between scenarios.
 */
describe("the run button's footprint", () => {
  async function width(className?: string) {
    document.documentElement.setAttribute("data-theme", "dark");
    const host = await mount(
      <div data-wrap style={{ width: 400 }}>
        <PrimaryButton size="sm" className={className}>
          Simulate
        </PrimaryButton>
      </div>,
    );
    const el = host.querySelector("[data-wrap] button");
    if (!el) throw new Error("no button");
    return el.getBoundingClientRect().width;
  }

  it("is the same filled, outlined or stale", async () => {
    const filled = await width();
    expect(await width("btn-run--outline")).toBe(filled);
    expect(await width("btn-run--stale")).toBe(filled);
  });

  it("has a width to compare", async () => {
    expect(await width()).toBeGreaterThan(40);
  });
});
