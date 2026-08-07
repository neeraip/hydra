// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { act } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { useRootAttribute, useRootFlag } from "./useRootAttribute";

/**
 * The app's display settings live on the document element, where the
 * stylesheet reads them. Anything painting outside the cascade — a canvas
 * drawing into a GL context — has to read them itself, and has to hear
 * about a change without a reload.
 *
 * Three hooks had a copy of this apiece. They agreed, which is the only
 * reason it never showed.
 */

function Probe({ name }: { name: string }) {
  return <span data-testid="v">{String(useRootAttribute(name))}</span>;
}

function FlagProbe({ name }: { name: string }) {
  return <span data-testid="f">{String(useRootFlag(name))}</span>;
}

afterEach(() => {
  document.documentElement.removeAttribute("data-probe");
});

async function setAttr(value: string | null) {
  await act(async () => {
    if (value === null) document.documentElement.removeAttribute("data-probe");
    else document.documentElement.setAttribute("data-probe", value);
    // Mutation records are delivered as a microtask.
    await Promise.resolve();
  });
}

describe("reading a root attribute", () => {
  it("starts with whatever is already there", () => {
    document.documentElement.setAttribute("data-probe", "dark");
    render(<Probe name="data-probe" />);
    expect(screen.getByTestId("v").textContent).toBe("dark");
  });

  it("reports null when the attribute is absent", () => {
    render(<Probe name="data-probe" />);
    expect(screen.getByTestId("v").textContent).toBe("null");
  });

  /**
   * The load-bearing one, and the reported defect: a setting changed in the
   * drawer did nothing on the canvas until the page was reloaded.
   */
  it("follows a change without a remount", async () => {
    render(<Probe name="data-probe" />);
    await setAttr("light");
    expect(screen.getByTestId("v").textContent).toBe("light");
    await setAttr("dark");
    expect(screen.getByTestId("v").textContent).toBe("dark");
  });

  it("follows the attribute being removed", async () => {
    document.documentElement.setAttribute("data-probe", "true");
    render(<Probe name="data-probe" />);
    await setAttr(null);
    expect(screen.getByTestId("v").textContent).toBe("null");
  });

  /** Other attributes are none of its business, or every class change on
   *  the root would re-render every consumer. */
  it("ignores attributes it was not asked about", async () => {
    render(<Probe name="data-probe" />);
    await act(async () => {
      document.documentElement.setAttribute("data-other", "x");
      await Promise.resolve();
    });
    expect(screen.getByTestId("v").textContent).toBe("null");
    document.documentElement.removeAttribute("data-other");
  });
});

describe("reading a root flag", () => {
  it("is on only for the literal true", async () => {
    render(<FlagProbe name="data-probe" />);
    expect(screen.getByTestId("f").textContent).toBe("false");
    await setAttr("true");
    expect(screen.getByTestId("f").textContent).toBe("true");
    // The settings drawer writes String(false), not an absent attribute.
    await setAttr("false");
    expect(screen.getByTestId("f").textContent).toBe("false");
  });
});
