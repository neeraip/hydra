/**
 * "Only one of these may be open at a time" — the coordination behind row
 * menus, kept as plain functions so the rule can be tested without a DOM.
 *
 * Dismiss-on-outside-click cannot carry this on its own. A menu inside a
 * modal never sees the click that opened a sibling: the modal panels spread
 * `stopBackdropEvents`, whose `onMouseDown` stops propagation, and React
 * attaches its handlers at the root container — so the native event dies
 * before reaching any listener above it. Every menu in the Scenarios modal
 * could therefore be opened at once. Exclusivity is a property of menus, not
 * a side effect of event plumbing, so it is stated directly here.
 */

/** The currently open participant's close callback, if any. */
let active: (() => void) | null = null;

/**
 * Become the open participant, closing whoever held the slot.
 *
 * `close` identifies the caller, so it must be stable across renders —
 * a fresh closure each render would leave `release` unable to recognise
 * its own entry and strand the slot.
 */
export function claimExclusive(close: () => void): void {
  if (active !== null && active !== close) active();
  active = close;
}

/**
 * Give up the slot, but only if the caller still holds it. The guard matters
 * on unmount: a menu that was already superseded must not clear the slot of
 * the menu that replaced it.
 */
export function releaseExclusive(close: () => void): void {
  if (active === close) active = null;
}

/** Test seam: whether anyone currently holds the slot. */
export function hasExclusiveHolder(): boolean {
  return active !== null;
}
