/**
 * @vitest-environment jsdom
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SettingsSkeleton } from "./SettingsSkeleton";

describe("SettingsSkeleton", () => {
  /**
   * The skeleton exists to hold the layout still, which it can only do if
   * it mirrors the real rows. It duplicates that list deliberately — the
   * alternative was flattening a set of bespoke controls into a
   * data-driven lowest common denominator — so this pins the duplication
   * against the content's own section headings.
   */
  it("mirrors the content's sections, in order", () => {
    render(<SettingsSkeleton />);
    const headings = screen.getAllByRole("heading", { level: 2 });
    expect(headings.map((h) => h.textContent)).toEqual([
      "General",
      "Appearance",
      "Accessibility",
      "About",
    ]);
  });

  /**
   * Real labels rather than grey bars: a reader who can already see "Text
   * size" arriving knows the drawer opened on the right thing, where a row
   * of bars only says "something is coming".
   */
  it("names the rows it is standing in for", () => {
    render(<SettingsSkeleton />);
    for (const label of [
      "Reopen last project on launch",
      "Theme",
      "Default display units",
      "Text size",
      "Reduce motion",
      "High-contrast mode",
    ]) {
      expect(screen.getByText(label)).toBeDefined();
    }
  });

  /** Nothing here is interactive — it is a picture of controls, and a
   * placeholder that took focus or a click would be a trap. */
  it("offers nothing to click or focus", () => {
    const { container } = render(<SettingsSkeleton />);
    expect(container.querySelectorAll("button")).toHaveLength(0);
    expect(container.querySelectorAll("input, select")).toHaveLength(0);
  });
});
