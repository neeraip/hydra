/**
 * What a dialog owes the keyboard.
 *
 * Three behaviours, none of which the app's modals had. Focus goes into
 * the dialog when it opens, so the first Tab lands inside it rather than
 * somewhere in the page behind. Tab stays inside while it is open, because
 * a dialog you can Tab out of leaves the reader operating controls they
 * cannot see — the backdrop hides them, the focus ring does not. And focus
 * returns to whatever opened the dialog when it closes, so dismissing a
 * panel puts the reader back at the button they pressed rather than at the
 * top of the document.
 *
 * Kept as plain functions over a container element rather than a hook over
 * a ref, so each decision can be tested against real DOM without mounting
 * anything.
 */

/**
 * Elements inside `container` that can take focus, in tab order.
 *
 * Disabled controls, `tabindex="-1"`, and anything inside a `hidden`
 * subtree are excluded — they are not stops on the way round, and treating
 * them as the last one would trap focus on a control that cannot hold it.
 */
export function focusableWithin(container: HTMLElement): HTMLElement[] {
  const selector = [
    "a[href]",
    "button:not([disabled])",
    "input:not([disabled])",
    "select:not([disabled])",
    "textarea:not([disabled])",
    '[tabindex]:not([tabindex="-1"])',
  ].join(",");
  return Array.from(container.querySelectorAll<HTMLElement>(selector)).filter(
    (el) =>
      !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true",
  );
}

/**
 * Where focus should go when the dialog opens, or null to leave it alone.
 *
 * Null when something inside is already focused, which is the case that
 * matters: several dialogs deliberately focus a particular control on
 * mount — the delete confirmations focus Cancel so that Enter is the safe
 * answer — and a container that always grabbed the first focusable would
 * quietly overrule them, turning Enter into "delete".
 */
export function initialFocus(
  container: HTMLElement,
  active: Element | null,
): HTMLElement | null {
  if (active && container.contains(active)) return null;
  return focusableWithin(container)[0] ?? null;
}

/**
 * Where a Tab keypress should send focus, or null to let the browser do
 * its normal thing.
 *
 * Only the two ends of the ring are answered here: forward from the last
 * stop wraps to the first, backward from the first wraps to the last, and
 * everything in between is the browser's own tab order, which is better
 * at this than any list we could maintain. Focus that has escaped the
 * container entirely is pulled back to the near end, which is what
 * happens when something outside was focused before the dialog opened and
 * the reader tabs.
 */
export function nextFocus(
  container: HTMLElement,
  active: Element | null,
  shiftKey: boolean,
): HTMLElement | null {
  const stops = focusableWithin(container);
  if (stops.length === 0) return null;
  const first = stops[0];
  const last = stops[stops.length - 1];
  if (!active || !container.contains(active)) return shiftKey ? last : first;
  if (!shiftKey && active === last) return first;
  if (shiftKey && active === first) return last;
  return null;
}

/**
 * Give focus back to `opener` if it can still take it.
 *
 * An opener that has left the document — a row removed by the very action
 * the dialog performed — is skipped rather than focused, because focusing
 * a detached node silently moves focus to the body and there is nothing to
 * gain by doing that deliberately.
 */
export function restoreFocus(opener: Element | null): void {
  if (!opener?.isConnected) return;
  if (opener instanceof HTMLElement) opener.focus();
}
