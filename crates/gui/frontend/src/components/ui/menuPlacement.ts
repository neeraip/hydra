// ── Where a dropdown hangs, so that it stays on screen ───────────────────────
//
// Two menus got this wrong in two opposite directions, which is what says
// it is one decision rather than two placements. The history menus in the
// top bar hung from their left edge and ran off the *right* of the window,
// because "left" reads as correct for the left-hand button until you
// remember the whole group is against the edge. The new-project menu hung
// from its right edge and ran off the *left*, because it clamped the
// offset it was measuring from and not the edge that actually overflowed.
//
// One rule covers both: prefer the alignment the design asks for, then
// clamp the result into the viewport. It is the rule `TooltipPortal`
// already applies to every tooltip, which is why no tooltip has ever had
// this bug.

/** The margin a menu keeps from the window edge when it has to be moved. */
export const MENU_GAP = 8;

/**
 * The viewport `left` for a menu that would rather hang from its right
 * edge, in the coordinate space `position: fixed` uses.
 *
 * `anchorRight` is the right edge of whatever the menu belongs to, and
 * `menuWidth` is what the menu actually measured — not what it declared.
 * A `minWidth` under-states a menu whose longest item is wider than it,
 * which is the case that kept overflowing: the number that decides this
 * has to be the rendered one.
 */
export function clampedMenuLeft(
  anchorRight: number,
  menuWidth: number,
  viewportWidth: number,
  gap: number = MENU_GAP,
): number {
  // Right-aligned to the anchor: the menu is usually wider than the
  // control that opened it, and hanging it off the left edge pushes it
  // into whatever sits to the right.
  const preferred = anchorRight - menuWidth;
  // A menu wider than the window has no placement that fits. Pinning it
  // to the near edge shows its beginning, which is where the labels
  // start; the alternative shows the middle of every line.
  if (menuWidth + 2 * gap >= viewportWidth) return gap;
  return Math.max(gap, Math.min(preferred, viewportWidth - menuWidth - gap));
}
