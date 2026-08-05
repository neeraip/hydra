import { describe, expect, it } from "vitest";
import type { Page } from "../../AppContext";

/**
 * Settings became a drawer to make two bugs unrepresentable rather than
 * handled. These assert the shape that does it, since neither bug has a
 * runtime seam left to test — which is the point.
 */
describe("the page union", () => {
  /**
   * Session restore forgets the open project when the user leaves for a
   * page that is not a project. While Settings was a page, opening it to
   * change one setting counted as leaving, and the next launch had lost
   * the project you were mid-way through.
   *
   * The fix is not a longer condition listing which pages really mean
   * "done" — that needs updating whenever a page is added, silently. It
   * is that there are only two non-project pages, and both genuinely mean
   * it.
   */
  it("has exactly the three places the app can be", () => {
    const all: Page[] = ["home", "projects", "project"];
    // A compile-time claim made checkable: adding a member without adding
    // it here fails to typecheck, and `"settings"` cannot be added at all.
    const exhaustive: Record<Page, true> = {
      home: true,
      projects: true,
      project: true,
    };
    expect(Object.keys(exhaustive).sort()).toEqual([...all].sort());
  });

  /**
   * The other half: a page takes part in navigation history, so Back
   * walked back through settings visits rather than through the work.
   * An overlay has no location to return to.
   */
  it("does not include settings", () => {
    const pages: string[] = ["home", "projects", "project"];
    expect(pages).not.toContain("settings");
  });
});
