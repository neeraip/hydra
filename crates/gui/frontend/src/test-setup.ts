/**
 * Vitest setup for DOM tests.
 *
 * Unmounts anything a test rendered once it finishes. Without this the
 * document accumulates every render in the file, and queries start finding
 * the previous test's elements — which surfaces as "found multiple
 * elements" at best, and as a test asserting against a stale tree at worst.
 */
import { cleanup } from "@testing-library/react";
import { afterEach, beforeAll } from "vitest";

afterEach(cleanup);

/**
 * Give every element a plausible size.
 *
 * jsdom performs no layout, so `getBoundingClientRect` answers zero for
 * everything. Mostly that is harmless — a test asserting a width belongs
 * in the layout project, which runs in real Chromium. It is not harmless
 * for a **virtualised list**: the virtualizer measures its scroll
 * container, is told it is zero pixels tall, concludes no row is
 * visible, and mounts none. Every such list rendered an empty `<tbody>`
 * here, which is why the network list, the water-distribution editor
 * tables and the curve points table had no row-level tests at all.
 *
 * A box, not a layout: this reports one size for every element and does
 * not compute anything. It is enough for a list to decide how many rows
 * fit, and deliberately not enough to assert a width against — a test
 * that wants a real number still has to go to the layout project, and
 * the numbers here are odd enough to be recognised if one leaks into an
 * assertion.
 */
beforeAll(() => {
  // `Element` is absent under the node environment, where most of these
  // tests run — pure logic needs no DOM and gets none.
  if (typeof Element === "undefined") return;
  const W = 1024;
  const H = 721;
  const box = {
    width: W,
    height: H,
    top: 0,
    left: 0,
    right: W,
    bottom: H,
    x: 0,
    y: 0,
  };
  Element.prototype.getBoundingClientRect = function rect(): DOMRect {
    return { ...box, toJSON: () => box } as DOMRect;
  };
  for (const [prop, value] of [
    ["clientHeight", H],
    ["clientWidth", W],
    // What the virtualizer actually measures its scroll container with.
    // It reads `offsetHeight`, not the bounding rect — stubbing only the
    // rect leaves it believing the container is zero pixels tall, which
    // is indistinguishable from not stubbing anything at all.
    ["offsetHeight", H],
    ["offsetWidth", W],
  ] as const) {
    Object.defineProperty(HTMLElement.prototype, prop, {
      configurable: true,
      get: () => value,
    });
  }
});
