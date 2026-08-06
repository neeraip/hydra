/**
 * Mounting helper for the layout tests.
 *
 * These run in a real browser because jsdom performs no layout: it answers
 * every question about width, height or overflow with a zero, so the whole
 * class of bug they guard against is invisible to it.
 *
 * React Testing Library is not used here. Its queries are about what a
 * user can find, which is the right question for the jsdom component
 * tests and the wrong one for these — what is under test is the geometry
 * of an arrangement, so the test wants a node and its box, nothing more.
 */

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";

const mounted: Array<{ root: Root; host: HTMLElement }> = [];

/** Render `ui` into the document and return its host element. */
export async function mount(ui: React.ReactNode): Promise<HTMLElement> {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = createRoot(host);
  mounted.push({ root, host });
  await act(async () => {
    root.render(ui);
  });
  return host;
}

/**
 * Unmount everything a test rendered.
 *
 * Layout is measured against the live document, so a leftover tree is not
 * merely clutter — it occupies space that the next test's measurements are
 * taken in.
 */
export function unmountAll(): void {
  for (const { root, host } of mounted.splice(0)) {
    root.unmount();
    host.remove();
  }
}

/** The rendered width of the first element matching `selector`. */
export function widthOf(host: HTMLElement, selector: string): number {
  const el = host.querySelector(selector);
  if (!el) throw new Error(`no element matching ${selector}`);
  return el.getBoundingClientRect().width;
}
