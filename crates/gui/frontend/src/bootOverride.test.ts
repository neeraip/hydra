import { describe, expect, it } from "vitest";
import { bootOverride } from "./bootOverride";

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
