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

  it("reads the preference of the project being navigated to", () => {
    // History can cross projects; each keeps its own rail preference.
    const saved = (id: string) => id === "p1";
    expect(railOpenForLocation(loc({ activeProjectId: "p1" }), saved)).toBe(
      true,
    );
    expect(railOpenForLocation(loc({ activeProjectId: "p2" }), saved)).toBe(
      false,
    );
  });

  it("closes the rail on pages that have none", () => {
    for (const page of ["home", "projects", "settings"] as const) {
      expect(railOpenForLocation(loc({ page }), () => true)).toBe(false);
    }
  });

  it("opens by default for a project location with no id", () => {
    expect(
      railOpenForLocation(loc({ activeProjectId: null }), () => false),
    ).toBe(true);
  });
});
