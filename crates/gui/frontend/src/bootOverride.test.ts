import { describe, expect, it } from "vitest";
import { bootOverride, launchSession } from "./bootOverride";

describe("bootOverride", () => {
  it("is inert outside dev builds, whatever the environment says", () => {
    // The screenshot driver must never be able to steer a release
    // binary; DEV is inlined false there and that alone switches it off.
    expect(
      bootOverride({
        DEV: false,
        VITE_HYDRA_BOOT_PROJECT: "0a350c15-6a29-4b21-9c22-30ae4b0f0b60",
        VITE_HYDRA_BOOT_VIEW: "canvas",
      }),
    ).toBeNull();
  });

  it("is inert when no project is named", () => {
    expect(bootOverride({ DEV: true })).toBeNull();
    expect(
      bootOverride({ DEV: true, VITE_HYDRA_BOOT_PROJECT: "  " }),
    ).toBeNull();
  });

  it("returns the project and a validated view", () => {
    expect(
      bootOverride({
        DEV: true,
        VITE_HYDRA_BOOT_PROJECT: "0a350c15-6a29-4b21-9c22-30ae4b0f0b60",
        VITE_HYDRA_BOOT_VIEW: "analysis",
      }),
    ).toEqual({
      projectId: "0a350c15-6a29-4b21-9c22-30ae4b0f0b60",
      view: "analysis",
    });
  });

  it("falls back to the stored view when the requested one is not a view", () => {
    // null, not a guess: the caller layers it over readProjectView(id).
    for (const requested of [undefined, "", "results", "Canvas"]) {
      expect(
        bootOverride({
          DEV: true,
          VITE_HYDRA_BOOT_PROJECT: "p",
          VITE_HYDRA_BOOT_VIEW: requested,
        }),
      ).toEqual({ projectId: "p", view: null });
    }
  });
});

describe("launchSession", () => {
  const noStoredView = () => null;

  it("opens the stored project and lets the session be remembered", () => {
    expect(launchSession(null, "p1", noStoredView)).toEqual({
      projectId: "p1",
      view: "canvas",
      remember: true,
    });
  });

  it("lands on Home with nothing stored, and still allows remembering", () => {
    // `remember` is about whether writes are permitted, not about
    // whether there is anything to write yet: opening a project during
    // this session has to be stored.
    expect(launchSession(null, null, noStoredView)).toEqual({
      projectId: null,
      view: "canvas",
      remember: true,
    });
  });

  it("prefers the project's own stored view", () => {
    expect(launchSession(null, "p1", () => "analysis").view).toBe("analysis");
  });

  it("opens the staged project and refuses to remember it", () => {
    // The whole point of the override: it borrows the real profile, so
    // the session already in it must survive the run.
    expect(
      launchSession(
        { projectId: "staged", view: "overview" },
        "p1",
        noStoredView,
      ),
    ).toEqual({ projectId: "staged", view: "overview", remember: false });
  });

  it("takes the staged project's stored view when the override names none", () => {
    expect(
      launchSession({ projectId: "staged", view: null }, "p1", (id) =>
        id === "staged" ? "editor" : "canvas",
      ),
    ).toEqual({ projectId: "staged", view: "editor", remember: false });
  });

  it("never opens the stored project when an override is present", () => {
    // Two questions, one answer each: what is on screen is the staged
    // project, what may be written is nothing.
    const launch = launchSession(
      { projectId: "staged", view: null },
      "the-real-session",
      noStoredView,
    );
    expect(launch.projectId).toBe("staged");
    expect(launch.remember).toBe(false);
  });
});
