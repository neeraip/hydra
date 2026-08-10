/** @vitest-environment jsdom */
import { afterEach, describe, expect, it } from "vitest";
import {
  focusableWithin,
  initialFocus,
  nextFocus,
  restoreFocus,
} from "./dialogFocus";

/**
 * The decisions behind a focus trap, tested apart from any dialog.
 *
 * Each is a question with a wrong answer that is invisible to look at: a
 * ring that leaves the dialog, a deliberate initial focus quietly
 * overruled, focus restored to a node that no longer exists.
 */

function mount(html: string): HTMLElement {
  const host = document.createElement("div");
  host.innerHTML = html;
  document.body.appendChild(host);
  return host;
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("focusableWithin", () => {
  it("lists the stops in tab order", () => {
    const host = mount(`
      <a href="#a">a</a><button>b</button><input /><select></select>
    `);
    expect(focusableWithin(host).map((el) => el.tagName)).toEqual([
      "A",
      "BUTTON",
      "INPUT",
      "SELECT",
    ]);
  });

  it("skips what cannot take focus", () => {
    // A disabled control as the last "stop" would trap the ring on a
    // control that refuses focus — the reader presses Tab and nothing
    // visibly happens.
    const host = mount(`
      <button disabled>no</button>
      <div tabindex="-1">no</div>
      <button hidden>no</button>
      <button aria-hidden="true">no</button>
      <button>yes</button>
    `);
    expect(focusableWithin(host).map((el) => el.textContent)).toEqual(["yes"]);
  });
});

describe("initialFocus", () => {
  it("takes the first stop when focus is outside", () => {
    const host = mount("<button>first</button><button>second</button>");
    expect(initialFocus(host, document.body)?.textContent).toBe("first");
  });

  it("leaves a deliberate initial focus alone", () => {
    // The delete confirmations focus Cancel on mount so that Enter is the
    // safe answer. A container that always grabbed the first stop would
    // silently make Enter mean "delete".
    const host = mount("<button>Delete</button><button>Cancel</button>");
    const cancel = host.lastElementChild as HTMLElement;
    expect(initialFocus(host, cancel)).toBeNull();
  });

  it("has nothing to offer an empty dialog", () => {
    expect(initialFocus(mount("<p>text</p>"), document.body)).toBeNull();
  });
});

describe("nextFocus", () => {
  const html = "<button>one</button><button>two</button><button>three</button>";

  it("wraps forward off the end", () => {
    const host = mount(html);
    const last = host.lastElementChild as HTMLElement;
    expect(nextFocus(host, last, false)?.textContent).toBe("one");
  });

  it("wraps backward off the start", () => {
    const host = mount(html);
    const first = host.firstElementChild as HTMLElement;
    expect(nextFocus(host, first, true)?.textContent).toBe("three");
  });

  it("leaves the middle of the ring to the browser", () => {
    // The browser's own tab order is better at this than any list we
    // could maintain; only the seam needs an answer.
    const host = mount(html);
    const middle = host.children[1] as HTMLElement;
    expect(nextFocus(host, middle, false)).toBeNull();
    expect(nextFocus(host, middle, true)).toBeNull();
  });

  it("pulls focus back in when it is outside the dialog", () => {
    // The state after opening a dialog while focus sat somewhere in the
    // page behind: the first Tab must land inside, not carry on through
    // controls the backdrop is covering.
    const host = mount(html);
    expect(nextFocus(host, document.body, false)?.textContent).toBe("one");
    expect(nextFocus(host, document.body, true)?.textContent).toBe("three");
  });

  it("says nothing about a dialog with nothing to focus", () => {
    expect(nextFocus(mount("<p>text</p>"), document.body, false)).toBeNull();
  });
});

describe("restoreFocus", () => {
  it("returns focus to the control that opened the dialog", () => {
    const host = mount("<button>opener</button>");
    const opener = host.firstElementChild as HTMLElement;
    restoreFocus(opener);
    expect(document.activeElement).toBe(opener);
  });

  it("does nothing for an opener the dialog's own action removed", () => {
    // Deleting a project from its row's menu detaches the row. Focusing a
    // detached node moves focus to the body, which is where it already
    // is — so this is about not pretending to have restored anything.
    const opener = document.createElement("button");
    expect(() => restoreFocus(opener)).not.toThrow();
    expect(document.activeElement).toBe(document.body);
  });

  it("tolerates never having had one", () => {
    expect(() => restoreFocus(null)).not.toThrow();
  });
});
