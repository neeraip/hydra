/**
 * Where a dropdown hangs.
 *
 * Two menus got this wrong in opposite directions — one ran off the right
 * of the window, the other off the left — which is what makes it one
 * decision rather than two placements. Both failures are pinned here, and
 * neither was visible to a component test, because jsdom performs no
 * layout and answers every question about width with a zero.
 */
import { describe, expect, it } from "vitest";
import { clampedMenuLeft, MENU_GAP } from "./menuPlacement";

describe("clampedMenuLeft", () => {
  it("hangs from the anchor's right edge when there is room", () => {
    // 300px menu under a control ending at x=500, in a wide window: it
    // sits at 200 and nothing has to move.
    expect(clampedMenuLeft(500, 300, 1000)).toBe(200);
  });

  it("does not run off the left when the anchor is near it", () => {
    // The new-project button in a narrow window. Right-aligned would put
    // this at -60, and clamping the *offset it measured from* left that
    // untouched — the menu overflowed the edge it was not looking at.
    expect(clampedMenuLeft(200, 260, 900)).toBe(MENU_GAP);
  });

  it("does not run off the right when the anchor is near it", () => {
    // The history menus at the right end of the top bar. This is the
    // direction that fails when a menu hangs from its *left* edge: at
    // x=940 a 300px menu would end at 1240 in a 1000px window.
    expect(clampedMenuLeft(1240, 300, 1000)).toBe(1000 - 300 - MENU_GAP);
  });

  it("pins a menu wider than the window to the near edge", () => {
    // No placement fits, so show the beginning of every line rather than
    // the middle of it — the labels start on the left.
    expect(clampedMenuLeft(400, 900, 500)).toBe(MENU_GAP);
  });

  it("measures from what the menu is, not what it declared", () => {
    // The case that kept overflowing: a `minWidth` under-states a menu
    // whose longest item is wider than it. Same anchor, two widths, two
    // answers — so passing the declared one is a real mistake and not a
    // rounding difference.
    expect(clampedMenuLeft(300, 260, 900)).not.toBe(
      clampedMenuLeft(300, 380, 900),
    );
  });
});
