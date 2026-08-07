// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CanvasBackgroundPicker } from "./CanvasBackgroundPicker";

/**
 * Built to the unit picker's shape, and for the same reason: the value
 * follows a setting made elsewhere until this project pins it, and those
 * two states look identical whenever they agree.
 *
 * That is the whole difficulty. With a dark theme and the default, the
 * closed control says "Dark" — and so does an override of Dark. What
 * separates them is the grouping, the description, and which row carries
 * the tick. These assert that separation survives, because a menu that
 * loses it silently becomes a menu with a duplicate entry.
 */

function setTheme(theme: "dark" | "light") {
  document.documentElement.setAttribute("data-theme", theme);
}

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
});

function open(value: "theme" | "dark" | "light", onChange = vi.fn()) {
  render(<CanvasBackgroundPicker value={value} onChange={onChange} />);
  // The only button until the menu opens.
  fireEvent.click(screen.getByRole("button"));
  return onChange;
}

describe("the closed control", () => {
  it("shows the ground in effect, not the preference", () => {
    setTheme("light");
    render(<CanvasBackgroundPicker value="theme" onChange={() => {}} />);
    expect(screen.getByRole("button").textContent).toContain("Light");
  });

  /**
   * Why it is what it is, without opening anything — and in the word the
   * menu uses for it. The marker names a group, so it has to be that
   * group's name; the unit picker marks the same state the same way, which
   * is what makes it a pattern rather than a per-control quirk.
   */
  it("marks a ground that is being inherited", () => {
    setTheme("dark");
    render(<CanvasBackgroundPicker value="theme" onChange={() => {}} />);
    expect(screen.getByRole("button").textContent).toContain("Default");
  });

  it("marks it with a word the menu also uses", () => {
    setTheme("dark");
    const marker = "Default";
    render(<CanvasBackgroundPicker value="theme" onChange={() => {}} />);
    expect(screen.getByRole("button").textContent).toContain(marker);
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText(marker)).toBeTruthy();
  });

  it("does not mark a pinned one", () => {
    setTheme("dark");
    render(<CanvasBackgroundPicker value="dark" onChange={() => {}} />);
    const text = screen.getByRole("button").textContent ?? "";
    expect(text).toContain("Dark");
    expect(text).not.toContain("Default");
  });
});

describe("the menu", () => {
  it("groups the tracking row apart from the pinned ones", () => {
    setTheme("dark");
    open("theme");
    expect(screen.getByText("Default")).toBeTruthy();
    expect(screen.getByText("Override")).toBeTruthy();
    expect(screen.getByText("Follows your app theme")).toBeTruthy();
  });

  /**
   * The load-bearing one. With a dark theme, the Default row and the Dark
   * override read the same word — so the tick is the only thing saying
   * which is in force, and it must be on exactly one of them.
   */
  it("ticks the tracking row and not the ground it resolves to", () => {
    setTheme("dark");
    open("theme");
    const checked = screen
      .getAllByRole("menuitemradio")
      .filter((el) => el.getAttribute("aria-checked") === "true");
    expect(checked).toHaveLength(1);
    expect(checked[0].textContent).toContain("Follows your app theme");
  });

  it("ticks the override once pinned, though the words are unchanged", () => {
    setTheme("dark");
    open("dark");
    const checked = screen
      .getAllByRole("menuitemradio")
      .filter((el) => el.getAttribute("aria-checked") === "true");
    expect(checked).toHaveLength(1);
    expect(checked[0].textContent).not.toContain("Follows your app theme");
  });

  it("names what the theme currently resolves to", () => {
    setTheme("light");
    open("theme");
    const tracking = screen
      .getAllByRole("menuitemradio")
      .find((el) => el.textContent?.includes("Follows your app theme"));
    expect(tracking?.textContent).toContain("Light");
  });

  it("offers both grounds to pin to", () => {
    setTheme("dark");
    open("theme");
    expect(screen.getAllByRole("menuitemradio")).toHaveLength(3);
  });
});

describe("choosing", () => {
  it("pins a ground", () => {
    setTheme("dark");
    const onChange = open("theme");
    const light = screen
      .getAllByRole("menuitemradio")
      .filter((el) => el.textContent?.includes("Light"));
    fireEvent.click(light[light.length - 1]);
    expect(onChange).toHaveBeenCalledWith("light");
  });

  /** Going back to tracking has to be reachable, or pinning is one-way. */
  it("goes back to following the theme", () => {
    setTheme("dark");
    const onChange = open("light");
    const tracking = screen
      .getAllByRole("menuitemradio")
      .find((el) => el.textContent?.includes("Follows your app theme"));
    if (tracking) fireEvent.click(tracking);
    expect(onChange).toHaveBeenCalledWith("theme");
  });
});

/**
 * Rows lift on hover, and settle back to the colour their state deserves.
 *
 * The trap is the restore: the checked row is accent-coloured, so resetting
 * every row to the unselected colour on mouse-out quietly un-highlights the
 * one that is in force — in a menu whose two visible states already read
 * alike, losing the tick's colour costs the reader the only other cue.
 */
describe("hovering a row", () => {
  it("lifts it", () => {
    setTheme("dark");
    open("theme");
    const row = screen.getAllByRole("menuitemradio")[1];
    const before = row.style.background;
    fireEvent.mouseEnter(row);
    expect(row.style.background).not.toBe(before);
  });

  it("settles an unselected row back to its own colour", () => {
    setTheme("dark");
    open("theme");
    const rows = screen.getAllByRole("menuitemradio");
    const unselected = rows.find(
      (el) => el.getAttribute("aria-checked") === "false",
    );
    if (!unselected) throw new Error("expected an unselected row");
    const before = unselected.style.color;
    fireEvent.mouseEnter(unselected);
    fireEvent.mouseLeave(unselected);
    expect(unselected.style.color).toBe(before);
  });

  /** The one the restore is easy to get wrong on. */
  it("settles the selected row back to the selected colour", () => {
    setTheme("dark");
    open("theme");
    const selected = screen
      .getAllByRole("menuitemradio")
      .find((el) => el.getAttribute("aria-checked") === "true");
    if (!selected) throw new Error("expected a selected row");
    const before = selected.style.color;
    fireEvent.mouseEnter(selected);
    fireEvent.mouseLeave(selected);
    expect(selected.style.color).toBe(before);
    expect(selected.style.color).toContain("accent");
  });
});
