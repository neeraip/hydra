import { describe, expect, it } from "vitest";
import { railOpenForLocation } from "./AppContext";

const loc = (
  over: Partial<Parameters<typeof railOpenForLocation>[0]> = {},
) => ({
  page: "project" as const,
  projectView: "canvas" as const,
  activeProjectId: "p1",
  activeScenarioId: null,
  ...over,
});

describe("railOpenForLocation", () => {
  // The defect: leaving the project page forces railOpen false, and
  // navigating back used to inherit that false — so the network list came
  // back collapsed however the user had left it.
  it("restores the target project's saved preference", () => {
    expect(railOpenForLocation(loc(), () => true)).toBe(true);
    expect(railOpenForLocation(loc(), () => false)).toBe(false);
  });

  /**
   * The preference is one preference, not one per project. Kept per
   * project, stepping through projects from the breadcrumb made the panel
   * open and close on its own — which reads as the app doing something
   * rather than as a choice being honoured. Whether the panel is open is a
   * fact about how someone is working, like an editor's sidebar.
   */
  it("gives every project the same answer", () => {
    const saved = () => true;
    for (const id of ["p1", "p2", "p3"]) {
      expect(railOpenForLocation(loc({ activeProjectId: id }), saved)).toBe(
        true,
      );
    }
  });

  /**
   * What stays per *location* is whether a rail exists at all. That is a
   * different question from whether the reader wants one, and collapsing
   * the two is what made leaving the project page overwrite the
   * preference.
   */
  it("still says no rail on a page that has none, whatever the preference", () => {
    expect(railOpenForLocation(loc({ page: "home" }), () => true)).toBe(false);
  });

  // Settings is absent because it is no longer a page — it opens as a
  // drawer over whatever is underneath, whose rail is none of its business.
  it("closes the rail on pages that have none", () => {
    for (const page of ["home", "projects"] as const) {
      expect(railOpenForLocation(loc({ page }), () => true)).toBe(false);
    }
  });

  it("opens by default for a project location with no id", () => {
    expect(
      railOpenForLocation(loc({ activeProjectId: null }), () => false),
    ).toBe(true);
  });
});
