import { describe, expect, it } from "vitest";
import { reselectsCurrentView } from "./projectViewNav";

/**
 * Asking for a project view means two things, and the second one surprises
 * every caller that meets it: from a nav button, asking for the view you
 * are already on collapses the rail. That is right for a tab and wrong for
 * anything that navigates in order to do something else.
 *
 * It has caught three callers now — `focusInEditor` carries a comment about
 * working around it, and both routes into the element finder collapsed the
 * network list for anyone who used them from the canvas, which is where you
 * would use them.
 *
 * So the condition has a name and a test rather than being re-derived at
 * each call site.
 */

describe("asking for a project view", () => {
  /** The rail gesture: the same view, on the project page. */
  it("is a reselect when you are already there", () => {
    expect(reselectsCurrentView("project", "canvas", "canvas")).toBe(true);
  });

  it("is navigation when you are somewhere else in the project", () => {
    expect(reselectsCurrentView("project", "editor", "canvas")).toBe(false);
  });

  /**
   * The load-bearing one. A command that wants the canvas so it can search
   * it is not asking about the rail — and the case it hits is precisely
   * this one, because the canvas is where you run it from.
   */
  it("is the case a navigate-then-act caller lands in", () => {
    expect(reselectsCurrentView("project", "canvas", "canvas")).toBe(true);
    expect(reselectsCurrentView("project", "analysis", "canvas")).toBe(false);
  });

  /**
   * Off the project page there is no rail to gesture at, so a matching view
   * name means nothing. The projects list keeps a `projectView` from
   * whichever project was last open.
   */
  it("is never a reselect away from the project page", () => {
    expect(reselectsCurrentView("projects", "canvas", "canvas")).toBe(false);
    expect(reselectsCurrentView("home", "canvas", "canvas")).toBe(false);
  });

  it("holds for every view, not just the canvas", () => {
    for (const view of ["canvas", "editor", "analysis", "overview"]) {
      expect(reselectsCurrentView("project", view, view)).toBe(true);
      expect(reselectsCurrentView("project", view, "canvas")).toBe(
        view === "canvas",
      );
    }
  });
});
